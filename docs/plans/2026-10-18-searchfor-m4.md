# searchfor M4 (v0.4: object files, documents with --docs, --pre and -z) Implementation Plan

> **For agentic workers:** each chunk below is implemented by one subagent in its own git
> worktree, following `test-driven-development`: every task writes its tests first, runs them
> and sees them fail (RED), implements, and runs them green (GREEN). Steps use checkbox
> (`- [ ]`) syntax. Read `CONTEXT.md` (vocabulary), `docs/specs/2026-10-16-searchfor-design.md`
> (the spec; §3 items 3 and 4, §4.1, §4.3, §5.7, §5.8, §5.9 and §11 #2, #6, #7 and #22–#27 are
> M4's) and this plan's «Design notes» before starting. The spec wins over this plan if they
> disagree, and the disagreement is reported. Everything is written in English: code,
> comments, docs, commit messages. Commit messages carry no attribution lines.

**Goal:** `sfor` v0.4, which describes the Matches of ELF, PE and Mach-O files
(`ELF x86-64 › .rodata › main.version+0x10`, at the hex offset of the Match), searches PDF,
Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite files with `--docs` through built-in
Extractors whose text carries its structure (`1 Intro › page 2`, `Budget › B12`,
`users › row 12 › email`), and implements ripgrep's `--pre`, `--pre-glob` and `-z`.

**Architecture:** the M3 pipeline stays: search first, describe later. Two stages join it.
Before the search, a file may be read *through* something (`preprocess::Through`): ripgrep's
`--pre` command or `-z` decompressor, as ripgrep does, or, with `--docs`, an Extractor
(`docs::extract`), which returns the document's text and an Outline of it; the text is then
searched as stdin is, and its Trails come from that Outline. After the search, a binary file
with a Match whose first bytes are an object Format's magic is searched again whole, and each
printable string with Matches becomes a row with the Trail of the `object` Processor (goblin).
Nothing changes for a file that is neither: no document Format is even detected without
`--docs`, and an object file is recognised only once it has a Match (8385).

**Tech stack:** Rust 1.90 (edition 2024), as M3. New dependencies (verified, «Verified for
this plan»): `goblin` 0.10 (default features off: `std`, `elf32`, `elf64`, `mach32`, `mach64`,
`pe32`, `pe64`, `te`, `endian_fd`), `rustc-demangle` 0.1, `zip` 9 (default features off,
`deflate-flate2`) and `lopdf` 0.45 (default features off). No XML crate: the Office,
OpenDocument and EPUB parts are read by a reader of our own (D12), and SQLite by a reader of
our own (D15). tree-sitter stays at `=0.24.7` and every ripgrep 14.1.1 pin of ADR 0001 stays
where it is.

**Reference ripgrep:** 14.1.1 at `/usr/bin/rg` (another `rg`, such as 15.x, may come first on
`PATH`), as M3 (M3 plan, D33).

**Branch:** work branch `t-768-searchfor-m4`, worktree
`/home/sebas/robotin/apps/mdgrep/.workspace/t-768-mdgrep/wt/m4`, which holds M3's final code
since `191dbaf` (the merge of `t-768-searchfor` at `0126a3f`; M1 and M2 are on `main`). The
diffs below are against `191dbaf`. Each chunk branches `t-768-searchfor-m4-c<N>` from the work
branch and is merged back after its tanda, as `ciclo` stage 7 describes
(`using-git-worktrees`, `finishing-a-development-branch`). Line numbers in the diffs are those
of `191dbaf` and only hints: locate code by the names given. The code of this plan was built
and tested in a probe copy of the repository, frozen as
`/tmp/subtask/m4-plan/m4tree-48fe5e8` (see «Verified for this plan»).

---

## Why M4, and what stays for M5–M6

M3 closed the Formats written as text. M4 is the milestone of «what ripgrep reads as bytes»:
the object files of grill #12, the documents Sebastián asked to search in 8393–8395 (PDF,
Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB, SQLite) and ripgrep's `--pre`. The milestone table
of the M3 plan, with M4's content as this plan decides it (D1):

| Milestone | Release | Content |
|---|---|---|
| M1 (done) | v0.1 | Source, Detector, Format Definitions, Processor contract, Markdown, core ripgrep flags, tree / one-line / JSON output, Fragments, `--within`, `--expand`, `--in-headings`, skill, crates.io |
| M2 (done) | v0.2 | `keypath` (YAML, JSON, TOML); `treesitter` with eight compiled-in Grammars; Helper Processors (`pool`, `concurrent`, `oneshot`); `processor = "external"`; `--install-grammar`, `--list-grammars` |
| M3 (done) | v0.3 | `markup` (HTML, XML), `csv` (CSV, TSV), `subtitles` (SubRip, WebVTT); control characters escaped in printed names; M3 benchmark, perf ceilings and fuzz targets |
| **M4 (this plan)** | v0.4 | `object` (ELF, PE, Mach-O, fat Mach-O); `--docs` with Extractors for PDF, Word, Excel, PowerPoint, OpenDocument text, spreadsheet and presentation, EPUB and SQLite; ripgrep's `--pre`, `--pre-glob`, `--no-pre`, `-z`, `--no-search-zip`; M4 benchmark, perf ceilings and fuzz targets |
| M5 | v1.0 | MCP server, the rest of ripgrep's flags, `dist` (GitHub Releases, Homebrew tap, npm), Windows and macOS in CI, Grammar Helper binaries in the index, signed releases, the External Processor guide, streaming the Trails of dense files |
| M6 | v1.x | The other Formats of Questionnaire 49 that 8385 accepted, among them image/audio metadata, WebAssembly and Java `.class` (offset decorations, as the `object` Processor's), and the candidates M4 leaves (below) |

**What M4 leaves out, and why:**

- **Image and audio metadata, WebAssembly and Java `.class`.** 8390 keeps them «as decoration
  of the Match offset», and the M3 table named «the binary Formats kept by 8390» in M4's row;
  but 8385 had moved every Format of Questionnaire 49 except PDF, SQLite and subtitles to M6,
  and the same M3 table lists these four under M6. M4 builds the mechanism they need (object
  rows at hex offsets, detection by magic in files with a Match), so each is a Definition and a
  small parser in M6 (D1). SQLite, which 8390 also named, is a `--docs` Format since 8395: without
  `--docs` it is searched as ripgrep searches it.
- **The binary Office formats** (`.doc`, `.xls`, `.ppt`): their text lives in OLE compound
  files with formats of their own (Word's piece tables, BIFF8); M6 candidates (D18).
- **Headers, footers, footnotes, comments, charts and embedded objects of Office documents**:
  the body of the document, the cell values and the slides with their notes are what 8393 asked
  to find; the other parts are each a reader more, later (D18).
- **OCR of scanned PDFs**, PDF form fields and annotations: no text in the content streams.
- **`--in-headings` over section and symbol names of object files** and over the names of
  pages, sheets, slides, tables and chapters, which are not in the extracted text (D11).
- **Streaming the Trails of dense files** stays for M5, as the M3 plan left it.

## Grill decisions this plan implements

| # | Decision | Where in M4 |
|---|---|---|
| 1 | Binaries ELF, PE and Mach-O: section, offset and symbol | C2, C5, C11 |
| 5 | Magic bytes decide a Format (object files, and documents without an extension) | C1 |
| 6 | Embedded TOML Definitions, user directory, `--export-formats` (the twelve new ones export too) | C1 |
| 12 | Format and architecture, section, hex offset, nearest symbol, the printable string around the Match | C2, C5 |
| 8375 | Everything in English | all chunks |
| 8381 | Coloured output: object rows print their offset in the colour of line numbers | C5 |
| 8385 | Search as fast as ripgrep; extra cost only in files with Matches; a Match with no structure prints as ripgrep prints it | C1 (no detection without `--docs`), C11 (objects only once they have a Match), C13 (benchmark) |
| 8390 | Decorate what ripgrep already finds; archive Formats out; a zip is never opened | C2, C11 (D4), C1 (no `zip` Definition) |
| 8393–8395 | `--docs`: built-in Extractors in Rust, no external programs, text plus structure, independent of `-z`; without `--docs` as ripgrep; `--pre` | C1, C3, C4, C6–C10, C11 |
| 8612 | The release binary may weigh up to 23 MB | C13 (measured), «Verified» |

Decisions 2–4, 7–11, 13–21 and the others of 8355–8459 were implemented by M1–M3 or belong to
M5–M6.

## Decisions delegated to this plan

Sebastián left these open; each one is taken here, with its reason, and the spec carries the
ones it describes marked «M4 plan».

| Id | Decision | Reason |
|---|---|---|
| D1 | M4 is the object files, the nine `--docs` Formats, `--pre`/`--pre-glob`/`--no-pre` and `-z`/`--no-search-zip`; image/audio metadata, WebAssembly and Java `.class` stay in M6 | 8385 moved Questionnaire 49's Formats to M6 and the M3 table lists those four there too; M4 builds the offset decoration they will reuse. SQLite without `--docs` is ripgrep's (8395 overrides 8390 for it) |
| D2 | `-z` comes with `--pre` in M4, and the one given last wins, as in ripgrep 14.1.1 | ripgrep reads both through the same reader stage (`grep-cli`'s `CommandReader` and `DecompressionReader`); the spec must say how `--docs` and `-z` combine, which cannot be tested without `-z`; measured: `rg -z --pre X` uses `--pre`, `rg --pre X -z` decompresses |
| D3 | goblin 0.10 for object files, and rustc-demangle for Rust symbols; C++ names print mangled | The spec named goblin; it reads ELF, PE, Mach-O and fat Mach-O with one API and resolves with the pins (verified). rustc-demangle is BurntSushi-small and Rust symbols are the common case in a Rust tree; a C++ demangler is a far larger crate, left for later |
| D4 | Object rows only where ripgrep finds a Match: a file named on the command line, `--binary`, or `-a`. A walked object file without `--binary` prints nothing, as rg; `-v`, `-c`, `-l`, `-q` and `--no-trail` print what rg prints | 8390: decorate «as long as rg already finds things in them»; 8385: a walked binary costs what it costs rg (it stops at the first NUL) |
| D5 | Object Formats name no extension and are told by their magic in every file with a Match, read only once the file has one (`Detector::object`) | `libc.so.6`, `ls` and `app.bin` say nothing by their name; reading 16 bytes of a file that already has a Match costs nothing that 8385 forbids |
| D6 | A row is the printable string around its Matches, as `strings` reads it (printable ASCII, tab, valid UTF-8 non-controls, at most 4096 bytes on each side), located by the hex offset of its first Match where a line number goes; Matches joined by printable bytes are one row however far apart (the 4096 bytes bound only what is shown around them); a control byte of a Match prints as `.`; in `--json` a row keeps rg -a's `line_number` and the string's `absolute_offset` | #12 asked for the offset in hex and the printable string around the Match; a line of a binary is meaningless, and the JSON stays valid against rg's schema; two Matches 5 000 bytes apart in one string made two rows, and `-m1` dropped the second (r2, Codex #5) |
| D7 | The symbol is the closest at or before the Match in its section; an ELF symbol's `st_size` bounds it; PE uses exports and COFF symbols (no section symbols, no `.`-names); Mach-O skips stabs and the `l`/`L` local labels; a fat Mach-O is one image per architecture | The spec's «closest preceding symbol», refined by what the probe showed on real binaries: `another needle` after `version_string` got `version_string+0x1d` from gcc's ELF, `.rdata+0xe` from MinGW's PE and `ltmp1+0xe` from clang's arm64 Mach-O |
| D8 | Object rows have no context lines and no `--`; a row is one string, printed once, at its first Match and with that Match's Trail, its Matches highlighted together (the JSON keeps each Match's Trail); `--within` keeps the Matches it keeps, with their Trails, and drops a row with none (`filter::retain_rows`); `-m` after `--within` counts rows; `--expand` prints the rows | A binary's Section is not text; the M1 `retain` turns a dropped Match on the same line as a kept one into a context line, and two rows of one binary often share an `rg -a` line, so `-m` by line would keep them all; a Trail names the distance into its symbol, so rows grouped by Trail would print a string once per Match (code review of this plan r1, Codex #7–#9) |
| D9 | A file with an object magic that goblin does not read prints as rg prints it, without a warning | A text file named `x` that starts with `MZ` is not a broken PE |
| D10 | Documents by extension, or for a name without an extension by `%PDF-`, `SQLite format 3\0` or the OpenDocument/EPUB `mimetype` at byte 30; only a regular file is a document: stdin, `/dev/fd/N` or a FIFO is searched as rg, read once; a file whose bytes do not start as its Format's (`Kind::looks_like`, the same bytes as the magic) is searched as rg, silently; one the Extractor cannot read warns once (`sfor: <path>: <format>: <reason>`) and is searched as rg; the exit code does not change | Office files have no fixed signature, and an extension-less zip must not be opened to find out (8390); a `.db` that is not SQLite is normal; a broken document is the user's to know about, but must not hide ripgrep's Matches in its bytes; a stream read for its first bytes is gone for the search (r1, Codex #3); `%PDF-` anywhere in the first 1024 bytes made a PDF a document by its name and not without one (r1, Fable #12) |
| D11 | `--in-headings` finds in documents only the names that are in the text (headings); object files give no names | Pages, sheets, slides, tables and chapters have no text to map a Match to (the M3 rule for `cue `); section and symbol names live in string tables, an M6 refinement |
| D12 | An XML pull reader of our own (`docs::xml`) for the Office, OpenDocument and EPUB parts, instead of quick-xml | Those parts are written by programs and well formed; the reader is 200 lines with entity decoding, and one dependency less; at a construct it cannot read the part ends there, as M3's XML scanner ends its structure |
| D13 | lopdf 0.45 reads PDF objects, fonts, streams and encryption; a font's strings decode through its ToUnicode map when it has one that reads, else through its `/Encoding`; the content streams and the ToUnicode maps are read by our code (D29); the text is laid out by our code from the content streams: a new line when a string moves half its size up or down, a space for a gap of 0.15 of its size (only when its font gave widths), and for a `TJ` kerning past 0.1 em; ligatures expanded | lopdf's own `extract_text` joins positioned strings without spaces or line breaks; measured against `pdftotext` on six PDFs from five producers: word recall 0.93–1.00, bigram recall 0.875–1.00, and faster than pdftotext on the two long ones (0.41 s against 2.5 s, 0.14 s against 0.35 s). lopdf reads `/Encoding` before `/ToUnicode`, and a subset font's codes mean what its map says (r1, Codex #5) |
| D14 | A PDF bookmark is a chapter that holds pages; it starts at the line of its page that starts with its title, or at the top of the page; a page split by a bookmark is a `page N` Segment under each chapter it holds part of; `page N` is the 1-based position | A Trail goes from the root down, and a chapter holds pages; LibreOffice's PDFs point every bookmark of a one-page document at page 1, which at page granularity put the whole page under the last bookmark |
| D15 | A SQLite reader of our own: `sqlite_schema`, then each table's b-tree (interior and leaf, table and index pages, overflow pages); `row N` is the rowid (the position for `WITHOUT ROWID`); values as the `sqlite3` shell prints them, in the order of `CREATE TABLE` (a `WITHOUT ROWID` table stores its PRIMARY KEY columns first, in the key's order) | No C compiler in the build and no C library in the binary (rusqlite bundles SQLite's C); read-only access to the file format is a few hundred lines; the rowid is what `SELECT … WHERE rowid = 12` uses. It replaces the M3 plan's forward reference (its D5 said M4's `row 12` would be the 12th row). The key's order was measured with SQLite: `t(payload TEXT, k TEXT PRIMARY KEY) WITHOUT ROWID` stores `k, payload` (r1, Codex #4, Fable #2) |
| D16 | SQLite: indexes, views and triggers are not read, nor a `-wal` file | Tables hold the data; a WAL is a second file, and changes in it reach the database at the next checkpoint |
| D17 | Limits: 512 MiB of text per document (cut, with a warning; a PDF page stops being drawn when its text reaches what is left); 256 MiB per zip part and 1 GiB for all the parts one document reads; 64 MiB per PDF stream (pages, Forms, fonts' ToUnicode maps, object streams) and 1 GiB for the pages', Forms' and maps' together; 8 000 000 units of Form work per PDF (16 per Form drawn plus one per operation it runs); refused with one warning, `… is over the N MiB limit` or `its Forms run more than 8000000 operations`, while a stream that does not decompress draws nothing. Round 2 made the work budget the content streams', pages' included: 8 000 000 plus 20 000 per page, one per operation read and 16 per Form drawn (`its content streams run more than N operations`); 256 open `q`; 65 536 operands to an operation; 200 000 entries to a ToUnicode map, a range counting one; 8 bytes of text for each byte of a string; 64 MiB and 2 000 000 tokens for the object streams of a document, counted before lopdf parses them (`its object streams hold more than 2000000 tokens`), and 1 MiB per stream while an encrypted PDF loads; 100 000 bookmarks; forms nested 8 deep; a repeated ODF cell or row with a value written at most 1000 times, and a repeat past the largest number an error; b-trees 40 deep | Decompression bombs and pathological files must neither fill the memory nor hang sfor. Round 1 of this plan's review found a ToUnicode map read without a limit (791 MiB for a 714 KB PDF), a page of Forms drawing Forms (16⁷ draws), twelve 200 MiB sheets each under the part limit (4.7 s), and ODF repeats that overflow (Codex #1, #2, #10; Fable #1, #3); the perf tests bound a 300 MiB zip bomb, a 300 MiB ToUnicode map and a workbook of six 200 MiB sheets. Round 2 found that bytes are not what fills the memory: lopdf's parsed operations and objects are (6 GB for a page of 63 MiB of `0 0 m`, 2 GB for an object stream of 32 MiB of integers, a ToUnicode range listed code by code, 512 MiB of text from a 5 KB file; r2, Codex #1–#3, Fable #1); its perf tests bound each of those. 10 000 pages each drawing five Forms of 200 operations (Fable's `many-forms.pdf`) fit (r2, Fable #2); a content stream several pages share counts once in the 1 GiB (r2, Fable #3) |
| D18 | Office documents: the body, the cell values as stored (Excel: dates as serial numbers, formulas' cached values; ODS: by `office:value-type`, the number of a float, percentage or currency, the date, time or boolean value, else the text) and the slides with their notes; not headers, footers, footnotes, comments, charts, embedded objects, nor the 97–2003 binary formats | What 8393 asked to find; each other part is a reader more, M6 candidates. An ODS cell shown as `$1.50` stores `1.5`, as an XLSX one does (r1, Codex #6) |
| D19 | Test documents and object files are written by code in the repository (`docs::fixture`, `processors::object::fixture`, `#[doc(hidden)] pub` so the integration tests use them), the SQLite ones by `scripts/gen-sqlite-fixtures.py` (Python's standard library) and committed; all MIT OR Apache-2.0 | No third-party file, so no licence question; a writer documents exactly what each byte is |
| D20 | Trails: Word/ODT `heading › heading › table N › row R › Column` (the first row names the columns, 40 characters); Excel/ODS `Sheet › B12`; PowerPoint/ODP `slide N › notes`; EPUB `chapter › headings`; PDF `chapters › page N`; SQLite `table › row ROWID › column` | Each says where a person finds the text in the program that opens the document; a cell reference is what a spreadsheet shows |
| D21 | Order: `--pre` (when its globs take the file), then `-z` (when the extension has a decompressor), then `--docs`; a file `--pre` or `-z` reads is never extracted; stdin is never transformed; `--pre`'s output gets the Trails of the Format its path says; what a transform outputs is never an object file nor a document, whatever its bytes, `--format` or `-a` say: no object rows, no `object` or `docs` Processor (Plain) | ripgrep's order for the first two; a decompressed stream has no name of its own to say what it is; `--pre cat` over an ELF printed object rows before (r1, Codex #11) |
| D22 | `--docs` also changes `--no-trail`, `-c`, `-l` and `-q`: they work on the extracted text, in ripgrep's layout | `--docs` is sfor's flag, not ripgrep's; a script that counts Matches with `--docs -c` wants the documents' |
| D23 | Benchmark (`scripts/bench-m4.sh`) and ceilings (`tests/perf.rs`) as in §11 #26: 1.1 × rg where M4 adds nothing; objects at 100 MB/s; documents at 2 MB/s of text per thread on top of rg's search of their text; per-format rates in `tests/perf.rs` about a third of the probe's | The rates measured in the probe (MB/s of text): docx 41, odt 36, epub 66, PDF 23 (generated) and 1.2–1.8 (real), xlsx 5.6, ods 3.4, pptx 4.0, SQLite 16–17; a third leaves room for a loaded machine, as M3's ceilings did. Memory: rg's plus six times the text, plus 400 bytes per cell Segment of a spreadsheet, plus 64 MiB (the probe: 228 MiB for the 600 000 cells of an XLSX with 18 MB of text) |
| D24 | The binary's growth is measured and reported: 16.6 MB (16 598 640 bytes at `191dbaf`) → 19.1 MB (19 064 256 bytes), under 23 MB (8612) | Spec §11 #22 |
| D25 | Extracted text never holds a control character (line breaks inside a cell, tabs inside a paragraph, NUL become spaces) and has no empty lines; a row writes a tab before each cell but the first, empty cells included, and a cell's spaces as they are | A row of a table stays one line, and the searcher never sees binary data in a document; a SQLite row `(NULL, 'needle')` is `\tneedle`, so a search anchored at a column finds it (r1, Codex #12, Fable #11) |
| D26 | The JSON statistics of an object file count the whole search (`bytes_searched` = file size, `matched_lines` = rows, `binary_offset` null), documented in `docs/rg-compat.md` | rg stops at the first Match of a binary; sfor reads on to describe each one |
| D27 | Without `--docs` the document Formats are never detected (`Detector::with_docs`); `--list-formats` lists them; `--format pdf` without `--docs` prints the file as Plain | 8395 «without --docs those files are treated as ripgrep does», at no cost |
| D28 | The fuzz mutates documents (bytes changed, cut, repeated; and the XML inside the zip, zipped again) and object files; nothing may panic, an extraction ends within 2 s whatever it returns, and every Segment and name lies inside its text. The documents run in a child process the test watches: one that prints nothing for 2 s is killed and reaped, and the cases after it run in a new child | It found a heading name with a NUL (fixed in `Builder`), and a panic in `xml::unescape` at `&` before a multi-byte letter (fixed); a slow extraction that ended in an error passed, and a hang hung the run (r1, Codex #13); a thread left running a hung case went on using memory and CPU (r2, Codex #7) |
| D29 | PDF content streams and ToUnicode CMaps are read by a reader of our own, about 500 lines: `Ops` gives `run` one operation at a time, its operands borrowing the stream's bytes, and spends the work budget as it reads; `ToUnicode` keeps a `bfrange` as a range and decodes a string only as far as the text has room. lopdf keeps reading the file's objects, encryption, fonts' dictionaries and the encodings of fonts without a map; a load filter counts the tokens of each object stream before lopdf parses it | One cause, three findings: lopdf did unbounded work before sfor could measure it (`many0(operation)` over a whole stream, a reverse map of every code of every range, `decode_text` into two UTF-16 buffers). Pre-validating the bytes would need the same lexer to count operators past strings and inline images, and would still leave lopdf's parse of what passes; the reader replaces both. Measured with the release binary: Fable's `page-ops-bomb.pdf` 6 264 620 KiB and 9.14 s → 73 700 KiB and 1.12 s, `form-ops-bomb.pdf` 6 265 792 KiB → 73 552 KiB, `objstm-bomb-32.pdf` 2 021 992 KiB → 57 220 KiB; a range of 2³² codes out of memory → 8 MiB; a 4 KB map of long codes 1 796 208 KiB and 9.13 s → 92 040 KiB and 0.68 s. The six real PDFs of D13 and `many-forms.pdf` extract as before (to verify in chunk 9: the D13 recall over them, the probe ran only the unit and perf tests) |

## Verified for this plan (2026-10-17 and 2026-10-18)

Outputs in `/tmp/subtask/m4-plan/outputs/`. The probe `/tmp/subtask/m4-plan/m4v2` is a git
repository of its own with five commits: the repository at `1610a50`, the repository at
`191dbaf`, M4's code over the latter (`57edf0f`, cherry-picked from the first probe, see «Rebase
on M3's final code»), and the fixes of the first and second reviews of this plan (`eea167a`
and `48fe5e8`, see «Review round 1» and «Review round 2»). The frozen copy `/tmp/subtask/m4-plan/m4tree-48fe5e8` is that probe at its commit
`48fe5e8`; every code block below was generated from it (`/tmp/subtask/m4-plan/plan/gen.py`,
diffs against its commit `05699bc`, which is `191dbaf`). The working probe may change with the
reviews; the frozen one does not.

- `rustc 1.90.0`, `cargo 1.90.0`, `/usr/bin/rg` 14.1.1 (also first on `PATH` here), 20 cores;
  for the probes, Python 3.13.3 with SQLite 3.46.1, pdftotext (poppler) 25.03.0, qpdf 12.0.0,
  LibreOffice 25.2.7.2, gcc, the MinGW cross gcc and clang.
- **Dependencies.** Adding the four of Task 1.1 to the whole `Cargo.toml` of `1610a50` resolves
  and builds with rustc 1.90 (their `rust-version`: goblin 1.85, zip 1.88, lopdf 1.88); the lock
  gains the 31 packages listed in Task 1.1 and loses none; `regex` 1.10.6, `regex-automata`
  0.4.7, `regex-syntax` 0.8.4, `memchr` 2.7.4, `encoding_rs` 0.8.34, `aho-corasick` 1.1.3 and
  `bstr` 1.10.0 stay. Without a lock (`cargo generate-lockfile`, what `cargo install searchfor`
  does) the same pins and tree-sitter 0.24.7 resolve, with newer `libc`, `js-sys`,
  `wasm-bindgen` and `ureq-proto`, as any build without the lock gets. With the lock,
  `cargo check --offline --workspace --all-targets` passes from the local cache. quick-xml
  0.42.0, the M3 plan's candidate, also resolved, and was dropped (D12).
- **Binary.** `target/release/sfor` of `191dbaf`: 16 598 640 bytes (15 381 072 stripped); of the
  frozen copy: 19 064 256 bytes (17 657 616 stripped), 2.5 MB more, under 23 MB (8612); before
  the fixes of review round 1, 19 015 120, and of round 2, 19 041 208. Over `1610a50` the probe had measured 16 583 992 and
  19 000 368.
- **Object files.** Over binaries built from one C file (`version_string` and a second string
  literal): gcc's ELF gives `ELF x86-64 › .rodata › version_string+0xe` and, for the second
  string, past the symbol's size, `ELF x86-64 › .rodata`; the stripped ELF, `ELF x86-64 ›
  .rodata` for both; MinGW's PE, `PE x86-64 › .rdata › version_string+0xe`; clang's Mach-O
  objects, `Mach-O x86-64 › __TEXT,__const › _version_string+0xe` and `Mach-O arm64 › …` (before
  D7: `version_string+0x1d`, `.rdata+0xe` and `ltmp1+0xe`). `sfor --flat -m3 'GLIBC_2\.3'
  /lib/x86_64-linux-gnu/libc.so.6` and `/bin/ls` give `ELF x86-64 › .dynstr` rows.
- **Documents.** Over a report (headings, a table, accents), a budget and a deck that
  LibreOffice converted to DOCX, ODT, PDF, XLSX, ODS, PPTX and ODP, every Extractor gives the
  Trails of `m4_06`; before D14 the PDF's text sat under its last bookmark, and before the
  `Heading_20_N` rule the ODT's level-1 headings were paragraphs. PDF against `pdftotext` (D13):
  word recall / precision / bigram recall 0.968 / 0.942 / 0.944 (Apple pstopdf), 1.000 / 1.000 /
  0.984 (Apache FOP), 0.997 / 0.996 / 0.979 (pdfTeX, 296 pages), 0.934 / 0.953 / 0.875
  (Ghostscript, Type 1 fonts without `/Widths`), 1.000 for both LibreOffice files. An AES-256 PDF
  with an empty user password is read; with the password `secret` it is `encrypted`; a PDF cut at
  3 000 bytes is `broken`. SQLite: the row counts of every table (27) of five browser databases up
  to 86 MB equal Python's `count(*)`.
- **Extraction speed** (release without LTO, one thread, text written per second): docx 41 MB/s
  (200 000 paragraphs), odt 36, epub 66, PDF 23 (2 000 generated pages) and 1.2–1.8 (two real
  ones, faster than pdftotext), xlsx 5.6 and ods 3.4 (600 000 cells), pptx 4.0 then 5.3 (5 000
  slides, after the relationships by id; 20 000 slides took 1.7 s before), SQLite 16–17
  (16 and 86 MB databases).
- **Every tanda builds** (`/tmp/subtask/m4-plan/states/build.py`, over `191dbaf` with the code
  of the frozen copy): the repository with only what tanda 1, then 1–2, 1–3 and 1–4 write (the
  later files as their stubs, and the `offsets: false` literals) passes `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo test --lib` (409, 449, 492 and 492 tests)
  and `cargo test --test cli --test cli_m2 --test cli_m3` (42, 16, 19), and after tanda 4 also
  `--test cli_m4` (15). The first run of this check, in round 0, found that the shared writer
  helpers, `pub(crate)` then, are dead code to clippy while the format writers are stubs; they
  are `pub` now (Task 4.3).
- **Frozen copy, whole suite:** `cargo build --workspace`, `cargo clippy --workspace
  --all-targets -- -D warnings` (no findings), `cargo fmt --check` and
  `SFOR_REQUIRE_REFERENCE=1 cargo test --workspace` pass: 492 unit tests and 2 ignored, `cli` 42,
  `cli_m2` 16 (with `m2_07` updated), `cli_m3` 19, `cli_m4` 15, `fuzz` 5, `helpers` 22,
  `rg_compat` 37, and the helpers' own.
- **Rebase on M3's final code.** The probe was first built over `1610a50` (frozen then as
  `m4tree-7134553`, removed). Its changes were cherry-picked onto `191dbaf`: every diff applied
  but those of `src/output/{mod,tree,stream}.rs`, where M3's final code had turned `Row::bytes`
  into a shared `Row::line` (`LineText`) and split `tree::write_row_text` out of `write_row`;
  the resolution keeps M3's structure and adds `Row::offset` beside `line` and an `offset`
  argument to `write_row_text`. Only the diff of `src/output/tree.rs` differed from the earlier
  plan's.
- **Review round 1** (`reviews/m4-plan-r1/{codex,fable}.md`). Each fix was written test first
  and seen RED over the round-0 code (`57edf0f`), then GREEN; the PDF, SQLite, ODF and zip
  findings with adversarial fixtures written by the tests themselves, and Fable's own files
  (`/tmp/subtask/m4-plan-r1-fable/exp/`) run through the release binary: `tounicode-bomb.pdf`
  (714 KB, a 700 MiB ToUnicode map) took 791 016 KiB and 2.56 s in round 0 and is refused in
  0.09 s with 75 224 KiB; `multipart-bomb.xlsx` (twelve 200 MiB sheets) took 4.68 s and is
  refused in 1.79 s with 230 800 KiB; `wr.sqlite` (`PRIMARY KEY(c)` and `PRIMARY KEY(y, x)`
  of `WITHOUT ROWID` tables) gave `t › row 1 › b │ valC	valA needle	valB` and gives
  `t › row 1 › a │ valA needle	valB	valC`.
  What changed, by chunk: C1 `magic_at` and `may_be_document` (D10, Fable #5, #12); C2 the
  `/bin/ls` test on Linux only (Fable #7); C4 the Builder's cells (D25), `remaining` and
  `mark_cut`, `looks_like` at byte 0 (D10), the zip's budget per document (D17) and three SQLite
  tables; C5 one row per object row (D8); C6 a saturating row number; C7 checked repeats and
  stored values (D17, D18); C9 the PDF Reader (D13, D17); C10 the key order of `WITHOUT ROWID`
  (D15); C11 `m4_12`–`m4_14`, `m4_06` and the SQLite snapshots of `m4_01`, `m4_10`'s pattern,
  documents only for regular files, `retain_rows` with `-m`, no object or document from a
  transform (D8, D10, D21); C12 the watchdog (D28); C13 the two bomb tests and `docs-none`
  over the object files too (D17, Fable #5). No chunk or tanda was added or moved and no
  dependency changed; C1 gains `src/formats/definition.rs`. The decisions of 8385, 8390,
  8393–8395, 8612 and #12 hold as they were.
- **Review round 2** (`reviews/m4-plan-r2/{codex,fable}.md`). Each fix written test first
  and seen RED over the round-1 code (`eea167a`), then GREEN. The three PDF findings of Codex
  and Fable's MAJOR share one cause, lopdf doing unbounded work before sfor can measure it,
  and one fix, D29, chosen over pre-validating the bytes by the numbers in D29's row. Fable's
  files (`/tmp/subtask/m4-plan-r2-fable/exp/`) through the release binary: `page-ops-bomb.pdf`
  refused in 1.12 s with 73 520 KiB (6 264 620 KiB and 9.14 s in round 1), `form-ops-bomb.pdf`
  in 1.11 s with 73 236 KiB, `objstm-bomb-32.pdf` in 0.14 s with 57 880 KiB (2 021 992 KiB),
  `many-forms.pdf` read whole in 1.49 s (refused in round 1), `shared-content.pdf` read in
  2.94 s (refused). What changed, by chunk: C2 rows by string (D6); C4 a fourth SQLite table
  (`odd`); C7 repeats past the largest number (D17); C9 the reader of D29, the work budget of
  pages, `MAX_GROWTH`, the object-stream filter, a content stream counted once, a page of one
  stream decompressed without lopdf's copy (D13, D17, D29); C10 the SQL tokens (D15); C11
  `m4_15`, `m4_06` and the SQLite snapshots; C12 the child-process supervisor (D28); C13 five
  PDF perf tests (D17). No chunk or tanda was added or moved and no dependency changed.
  Unverified, to verify in chunk 9: an encrypted PDF's object streams are bounded only per
  stream (1 MiB each while it loads): lopdf parses them while it decrypts, where the load
  filter is not called, and the probe built no encrypted PDF with object streams.
- **RED** over the stubs of C1 and C4 with each module's tests (`/tmp/subtask/m4-plan/red/run.py`,
  over the frozen copy): object `mod.rs` does not compile (`demangle`), `rows.rs` 7 FAIL,
  `preprocess.rs` 2 FAIL, `ooxml.rs` does not compile (4 functions), `odf.rs` 5 FAIL and 1 passes
  (an error either way), `epub.rs` 2 FAIL and 1 passes (likewise), `pdf.rs` and `sqlite.rs` do
  not compile. In round 0, `tests/cli_m4.rs` over the state after tanda 3 (no pipeline yet): 10
  of its 11 tests FAIL, and `m4_11` (without `--docs`, as rg) passes, as it must. The two fuzz
  findings (D28) FAILED before their fixes, as the inputs in Task 12.1 show.
- **Fuzz:** default count, 5 tests in 34 s (debug); `SFOR_FUZZ_N=20000` (release), the whole
  file in 213 s, the 55 000 documents in a supervised child process, 20 000 object files, no
  failure. In round 0 the two M4 tests took 69 s.
- **Perf ceilings** (release, `SFOR_REQUIRE_REFERENCE=1`, ten tests): 50 MB ELF 0.052 s and
  57 MiB against `rg -a` 0.106 s and 56 MiB; with 200 000 symbols 0.134 s and 94 MiB against
  0.105 s and 62 MiB; zip bomb (298 KiB, 300 MiB inside) refused in 0.33 s with 263 MiB;
  workbook of six 200 MiB sheets (1 196 KiB) refused in 1.77 s with 223 MiB; PDF bomb (299 KiB,
  a 300 MiB ToUnicode map) refused in 0.10 s with 71 MiB; a page of 63 MiB of `h` refused in
  0.38 s with 71 MiB; an object stream of 63 MiB of `1 ` refused in 0.27 s with 71 MiB; a
  ToUnicode range of 2³² codes read in 0.009 s with 8 MiB; a 4 MiB string of a code of 256
  letters read in 0.72 s with 121 MiB; documents: docx 0.40 s (15 MB of text, budget 2.27 s,
  47 MiB), odt 0.45 s (2.52 s, 39 MiB), epub 0.28 s (1.77 s, 36 MiB), PDF 0.42 s (7 MB, 2.08 s,
  26 MiB), xlsx 1.45 s (18 MB, 10.38 s, 228 MiB), ods 1.97 s (19.75 s, 253 MiB), pptx 0.52 s
  (1.7 MB, 2.33 s, 36 MiB; 1.68 s before the relationships by id), rg over each text at most
  0.015 s.
- **Benchmark** (`scripts/bench-m4.sh`, 1 080 documents, 300 ELF files from `/usr/bin` and
  `/usr/lib`, 20 000 text files, 12 threads, 40 runs per case): none 1.01 × rg, sparse 1.06 ×,
  `--no-trail` 1.01 ×, `--docs` without documents (text and object files) 1.09 ×, `--binary`
  1.03 × (extra 0.001 s, budget 0.054 s), `--docs` over the documents 0.075 s against rg's
  0.012 s over their texts (extra 0.063 s, budget 0.780 s for 16 MB of text). `docs-none` is
  near its ceiling for sfor's start, not for `--docs`: over the same files `hyperfine -N`
  (80 runs) gives rg 43.1 ms, sfor 45.1 ms, sfor `--docs` 45.2 ms, and with the first bytes of
  the 300 object files not read at all 46.4 ms against 46.0 ms, so reading them from the
  search's own descriptor (Fable #5's other fix) would save nothing measurable; before
  `magic_at` and `may_be_document`, `--docs` cost 1.3 ms there. With 15 runs per case the
  ratio of two 45 ms medians moved by ±3 % between runs (1.04–1.13), hence 40. With 3 000 text
  files the corpus gave, in round 0, 1.10 × and, for `--docs` without documents, 1.14 ×: a
  14–20 ms run is decided by process start, as the M3 plan saw with 5 004 files.

**To verify** (each in the chunk named): the C1 and C5 RED runs (chunks 1, 5); a real EPUB
(chunk 8); M1's `scripts/bench.sh`, M2's `scripts/bench-code.sh` and M3's `scripts/bench-m3.sh`
on the merged code (chunk 13); a RED run of each ceiling (chunk 13); the README texts, which the
probe did not write (chunk 14); that the diffs apply after the last M3 rounds merged into the
work branch (chunk 1). Every other statement above was measured.

## M4 acceptance criteria

M4 is done when all of these hold; each names the test or script that measures it.

1. `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and
   `SFOR_REQUIRE_REFERENCE=1 cargo test --workspace` pass, every M1–M3 test included
   (`m2_07` and the Detector's `myfy` fixture updated, C1).
2. Spec §11 #2 for the twelve new Formats: `m4_01` snapshots their tree, one-line and JSON
   output, JSON valid against `docs/json-schema.json`.
3. Spec §11 #6: `m4_02` (ELF with and without symbols, PE, fat Mach-O) and `m4_03` (walked
   objects, `--binary`, and `-v`, `-c`, `--no-trail` equal to rg 14.1.1).
4. Spec §11 #7: `m4_05` (PDF chapters with `--docs`, rg's output without it, a zip never opened).
5. Spec §11 #23 and #24: `m4_06` (every document Format's Trails) and `m4_07` (an encrypted PDF
   and a broken DOCX warn once each and are searched as rg; `--no-messages`).
6. Spec §11 #25: `m4_08` compares `--pre`, `--pre-glob`, a failing and a missing command, `-z`
   and both orders of `-z`/`--pre` with rg 14.1.1, stdout, stderr and exit code, run as
   `SFOR_REQUIRE_REFERENCE=1 cargo test --test cli_m4` so it cannot skip.
7. Spec §11 #26: `scripts/bench-m4.sh` exits 0, and `PATH=/usr/bin:$PATH scripts/bench.sh`,
   `scripts/bench-code.sh` and `scripts/bench-m3.sh` still exit 0 (C13).
8. The ten M4 tests of `tests/perf.rs` pass in release with the reference required:
   `SFOR_REQUIRE_REFERENCE=1 cargo test --release --test perf -- --ignored object_ elf_
   documents_ zip_bomb pdf_`. A run that printed `skipping:` does not count.
9. Spec §11 #27: the M4 fuzz of `tests/fuzz.rs` passes with its default count and with
   `SFOR_FUZZ_N=20000` once before the merge.
10. `Cargo.lock` gains only the packages listed in Task 1.1; `cargo tree -i regex-automata`,
    `-i regex-syntax`, `-i memchr` and `-i encoding_rs` show only 0.4.7, 0.8.4, 2.7.4 and 0.8.34.
11. Spec §11 #22: `target/release/sfor` is at most 23 MB (measured in C13 and reported; 19.0 MB
    in the probe).

## Design notes the chunks share

1. **The Processor contract stays.** `object` implements `Processor` (`trails` from its `Map`,
   no `names`); the pipeline calls `object::Map` and `object::rows` directly for its rows.
   `docs` is a Processor name Definitions may use (`Processors::known`), but `for_definition`
   gives None for it: a document's Trails come from its Extractor's Outline (note 4).
2. **Search first, describe later**, as M1–M3. An object file is recognised after its search
   found a Match: by its Format when its name decides one, else by `Detector::object` over its
   first 16 bytes, read then (`head_of`, in `plain_file`), or over the Source's head in
   `describe`. Only then is the whole file searched again (`Ctx::object`).
3. **Reading through something** (`preprocess::Through`). `Ctx::target` sets `through` (`Pre`,
   `Zip`, or `Docs` for a regular file only) and makes such a Target not `regular`, so every path that opens a file
   treats it as a pipe: it is read once, kept by a `Tee` where needed, and never memory-mapped
   or searched in memory as a file. `Ctx::open_input` gives each path its reader: stdin, a
   document's text, `--pre`'s or the decompressor's output, or the file. `disk_of` gives None
   for such a Target, so a Helper Processor gets a temporary file of what was searched. What
   `Pre` or `Zip` gives is never described as an object file or a document (D21).
4. **Documents.** `Ctx::prepare`, called by the walk for each Target, reads a `Through::Docs`
   file, checks its first bytes (`Kind::looks_like`) and extracts it (`docs::extract`), keeping
   the `Extracted` (text, Outline, `cut`) in `Target::doc`; on failure it warns and turns the
   Target back into an ordinary file. The search then reads `doc.text`, and `describe` takes
   the Trails from `doc.outline` (`outline_trails`); `names_of` takes the names from it.
   Extractors write through `docs::Builder`: `line`, `heading` (in the text, searchable),
   `chapter` (not in the text), `open`/`close` for pages, sheets, slides, tables and notes, and
   `start_row`/`cell`/`end_row` for rows of cells separated by tabs; `docs::table::Table` turns
   a table's rows into rows whose first one names the columns.
5. **Object rows.** `Described::offsets` marks them. `output::match_rows` makes one `Row` of
   each, with the Trail of its first submatch and the hex offset of that submatch in
   `Row::offset`; flat's `Head` and tree's `write_row` print it in place of the line number;
   `Rows` puts no `--` between rows. The filter keeps each row's Matches with `retain_rows`,
   which also applies `-m` by row (D8).
6. **Stubs keep every tanda building.** C1 writes every `mod` line of its tanda, the
   dependencies, the Definitions, the Detector, the CLI flags, `Described::offsets` and a stub
   for `processors/object/{mod,rows,fixture}.rs` and `preprocess.rs`; C4 writes the shared
   modules of `docs` and a stub for each Extractor and each document writer. Each stub has the
   final signatures, so a chunk fills its own files and never shares one, and after each tanda the
   whole workspace builds, passes clippy and passes its tests (measured for the four
   intermediate states, «Verified»). C11 alone wires the modules into `app.rs`.
7. **Snapshots** as before: insta, `INSTA_UPDATE=always cargo test`, every new `.snap` read in
   full and checked by eye before committing; the chunk report pastes one snapshot per Format.
8. After each tanda: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D
   warnings`, `cargo test --workspace`; `cargo fmt` before every commit.

## File structure

```
Cargo.toml, Cargo.lock                         goblin, rustc-demangle, zip, lopdf (C1 only)
formats/{elf,pe,macho}.toml                    object Definitions (C1)
formats/{pdf,docx,xlsx,pptx,odt,ods,odp,epub,sqlite}.toml   document Definitions (C1)
src/formats/{builtin,definition,detector,mod}.rs   embedding, magic_at, with_docs, object (C1)
src/cli/{mod,rg_flags}.rs, docs/rg-compat.md   -z, --pre, --pre-glob, --docs (C1)
src/model.rs, src/processors/mod.rs            Described::offsets, object, docs (C1)
src/lib.rs                                     `preprocess` (C1), `docs` (C4)
src/app.rs, src/filter.rs, src/output/{mod,stream}.rs   `offsets: false` in their literals (C1)
src/processors/object/{mod,rows,fixture}.rs    object Processor and writers (stubs C1, C2)
src/preprocess.rs                              --pre and -z (stub C1, C3)
src/docs/{mod,table,xml,zipped}.rs             shared Extractor code (C4)
src/docs/fixture/mod.rs                        shared document writers (C4)
src/docs/{ooxml,odf,epub,pdf,sqlite}.rs        Extractors (stubs C4, C6–C10)
src/docs/fixture/{ooxml,odf,epub,pdf}.rs       their writers (stubs C4, C6–C9)
scripts/gen-sqlite-fixtures.py, tests/fixtures/docs/*.sqlite   SQLite fixtures (C4)
src/output/{mod,flat,tree,stream}.rs           object rows, labels of the new kinds (C5)
src/app.rs, src/filter.rs                      the pipeline (C11)
tests/cli_m4.rs, tests/snapshots/cli_m4__*     end-to-end tests (C11)
tests/cli_m2.rs                                m2_07's count (C1)
tests/fuzz.rs                                  the M4 targets (C12)
tests/perf.rs, scripts/bench-m4.sh, scripts/gen-m4-corpus.py   ceilings and benchmark (C13)
README.md, README.es.md, skills/searchfor/SKILL.md, docs/json-schema.json, the spec   (C14)
```

## Chunk graph

| Chunk | Content | Depends on | Files and folders (exclusive within its tanda) | Tanda |
|---|---|---|---|---|
| C1 | Dependencies, Definitions, Detector, CLI flags, `Described::offsets`, `object` in the Processors, stubs of `object` and `preprocess` | — | `Cargo.toml`, `Cargo.lock`, `formats/*.toml` (the 12 new), `src/formats/{builtin,definition,detector,mod}.rs`, `src/cli/{mod,rg_flags}.rs`, `docs/rg-compat.md`, `src/model.rs`, `src/lib.rs`, `src/processors/mod.rs`, `src/processors/object/**` (stubs), `src/preprocess.rs` (stub), `src/app.rs`, `src/filter.rs`, `src/output/{mod,stream}.rs` (the `Described` literals only), `tests/cli_m2.rs` | 1 |
| C2 | `object`: `Map`, Trails, rows, and the object writers | C1 | `src/processors/object/{mod,rows,fixture}.rs` | 2 |
| C3 | `--pre`, `--pre-glob`, `-z` | C1 | `src/preprocess.rs` | 2 |
| C4 | The shared modules of `docs` (Builder, tables, XML, zip), the shared writers, the stubs of the Extractors and their writers, the SQLite fixtures | C1 | `src/lib.rs`, `src/docs/**`, `scripts/gen-sqlite-fixtures.py`, `tests/fixtures/docs/**` | 2 |
| C5 | Object rows and the new kinds in the printers | C1 | `src/output/{mod,flat,tree,stream}.rs` | 2 |
| C6 | Word, Excel, PowerPoint | C4 | `src/docs/ooxml.rs`, `src/docs/fixture/ooxml.rs` | 3 |
| C7 | OpenDocument | C4 | `src/docs/odf.rs`, `src/docs/fixture/odf.rs` | 3 |
| C8 | EPUB | C4 | `src/docs/epub.rs`, `src/docs/fixture/epub.rs` | 3 |
| C9 | PDF | C4 | `src/docs/pdf.rs`, `src/docs/fixture/pdf.rs` | 3 |
| C10 | SQLite | C4 | `src/docs/sqlite.rs` | 3 |
| C11 | The pipeline; end-to-end tests and snapshots | C2, C3, C5–C10 | `src/app.rs`, `src/filter.rs`, `tests/cli_m4.rs`, `tests/snapshots/cli_m4__*.snap` | 4 |
| C12 | Fuzz targets | C11 | `tests/fuzz.rs` | 5 |
| C13 | Perf ceilings, benchmark, binary size; M1–M3 benchmarks rerun | C11 | `tests/perf.rs`, `scripts/bench-m4.sh`, `scripts/gen-m4-corpus.py` | 5 |
| C14 | README, README.es, skill, JSON schema, spec reconciliation | C11 | `README.md`, `README.es.md`, `skills/searchfor/SKILL.md`, `docs/json-schema.json`, `docs/specs/2026-10-16-searchfor-design.md` | 5 |

Fourteen chunks in five tandas. No file is shared within a tanda. A file edited by two chunks
is edited in different tandas: `src/lib.rs` by C1 and C4, `src/output/{mod,stream}.rs` by C1
(one line per literal) and C5, `src/app.rs` and `src/filter.rs` by C1 (the literals) and C11,
`src/processors/object/**` and `src/preprocess.rs` by C1 (stubs) and C2, C3, and each
Extractor and writer by C4 (stubs) and its chunk of tanda 3. The Extractors wait for tanda 3
because they write through C4's Builder; `preprocess::Through::Docs` holds the Extractor's
name, not `docs::Kind`, so C3 does not depend on C4. C12–C14 test, measure and document the
merged result in parallel; C14 pastes real output of the merged binary.

---
## Chunk 1: dependencies, Definitions, Detector, flags and stubs

### Task 1.1: the dependencies

**Files:** `Cargo.toml`, `Cargo.lock`. This is the only task of M4 that edits them.

- [ ] **Step 1:** add the four dependencies after `csv-core` in `[dependencies]`, keeping every
  pinned line:

```diff
diff --git a/Cargo.toml b/Cargo.toml
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -69,6 +69,13 @@ ureq = "3"
 tempfile = "3"
 # CSV and TSV fields with their byte offsets (spec §5.5; M3 plan, D3).
 csv-core = "0.1"
+# Object files, ELF, PE and Mach-O, and Rust symbol names (spec §5.7; M4 plan, D3).
+goblin = { version = "0.10", default-features = false, features = ["std", "elf32", "elf64", "mach32", "mach64", "pe32", "pe64", "te", "endian_fd"] }
+rustc-demangle = "0.1"
+# --docs (spec §5.8; M4 plan, D12, D13): the zip of Office, OpenDocument and EPUB files, and
+# PDF's objects, fonts, streams and encryption. Their XML and SQLite are read by our own code.
+zip = { version = "9", default-features = false, features = ["deflate-flate2"] }
+lopdf = { version = "0.45", default-features = false }
 
 # Ctrl-C with Helper Processors running (spec §4.4, Lifetime; M2 plan, D42).
 [target.'cfg(unix)'.dependencies]
```

- [ ] **Step 2:** `cargo build`. Use `cargo build`, never `cargo update` or
  `cargo generate-lockfile`, so the locked versions stay. Check: `git diff Cargo.lock` adds only
  these packages (versions of 2026-10-18): aes 0.9.3, alloc-no-stdlib 2.0.4, alloc-stdlib
  0.2.4, block-padding 0.4.2, brotli-decompressor 5.0.3, cbc 0.2.1, cfg_aliases 0.2.2, chacha20
  0.10.2, cipher 0.5.2, cpubits 0.1.1, ecb 0.2.1, goblin 0.10.7, inout 0.2.4, lopdf 0.45.0,
  md-5 0.11.0, nix 0.31.3, plain 0.2.3, rand 0.10.3, rand_core 0.10.1, rangemap 1.8.0,
  rustc-demangle 0.1.28, scroll 0.13.0, scroll_derive 0.13.2, stringprep 0.1.5, tinyvec 1.13.3,
  typed-path 0.12.3, unicode-bidi 0.3.18, unicode-normalization 0.1.26, unicode-properties
  0.1.4, weezl 0.2.1, zip 9.0.5, and removes none; `cargo tree -i regex-automata`,
  `-i regex-syntax`, `-i memchr`, `-i encoding_rs` and `-i bstr` show only 0.4.7, 0.8.4, 2.7.4,
  0.8.34 and 1.10.0. Record the versions in the report; a newer release of a new dependency is
  accepted when these checks still hold.

### Task 1.2: the failing tests

**Files:** `src/formats/mod.rs`, `src/cli/mod.rs`, `src/processors/mod.rs`,
`src/cli/rg_flags.rs` (tests), `tests/cli_m2.rs`.

- [ ] **Step 1:** in the `tests` module of `src/formats/mod.rs`, before
  `the_m3_definitions_load_and_detect`:

```rust
    /// M4 plan, D5 and D10: document Formats only with --docs; object Formats by their magic
    /// whatever the name.
    #[test]
    fn the_m4_definitions_load_and_detect() {
        let (r, warnings) = Registry::load(None, &known);
        assert!(warnings.is_empty(), "{warnings:?}");
        let plain = detector::Detector::new(&r, &[]).unwrap();
        let docs = detector::Detector::new(&r, &[]).unwrap().with_docs(true);
        let p = std::path::Path::new;
        for (path, format) in [
            ("r.pdf", "pdf"),
            ("r.DOCX", "docx"),
            ("b.xlsm", "xlsx"),
            ("d.ppsx", "pptx"),
            ("t.ott", "odt"),
            ("s.ods", "ods"),
            ("p.odp", "odp"),
            ("b.epub", "epub"),
            ("x.sqlite3", "sqlite"),
            ("app.db", "sqlite"),
        ] {
            assert_eq!(
                docs.detect(p(path), b"").map(|d| d.name.as_str()),
                Some(format),
                "{path}"
            );
            assert!(
                plain.detect(p(path), b"").is_none(),
                "{path} without --docs"
            );
        }
        let mut odt = b"PK\x03\x04".to_vec();
        odt.resize(30, 0);
        odt.extend(b"mimetypeapplication/vnd.oasis.opendocument.text");
        for (head, format) in [
            (&b"%PDF-1.7\n"[..], "pdf"),
            (b"SQLite format 3\0", "sqlite"),
            (&odt, "odt"),
        ] {
            assert_eq!(
                docs.detect(p("noext"), head).map(|d| d.name.as_str()),
                Some(format)
            );
            assert!(plain.detect(p("noext"), head).is_none());
        }
        for (head, format) in [
            (&b"\x7fELF\x02\x01"[..], "elf"),
            (b"MZ\x90\0", "pe"),
            (b"\xcf\xfa\xed\xfe", "macho"),
            (b"\xca\xfe\xba\xbe", "macho"),
        ] {
            assert_eq!(plain.object(head).map(|d| d.name.as_str()), Some(format));
            // Without an extension the magic decides as for any Format.
            assert_eq!(
                plain.detect(p("ls"), head).map(|d| d.name.as_str()),
                Some(format)
            );
        }
        assert!(plain.object(b"#!/bin/sh\n").is_none());
        // `libc.so.6`: its extension decides nothing, so only `object` tells it.
        assert!(plain.detect(p("libc.so.6"), b"\x7fELF").is_none());
        assert_eq!(
            plain.object(b"\x7fELF").map(|d| d.name.as_str()),
            Some("elf")
        );
    }
```

- [ ] **Step 2:** in the `tests` module of `src/cli/mod.rs`, after `fn p`:

```rust
    /// M4: ripgrep's -z and --pre override each other, the last one given winning, and an
    /// empty --pre is none (rg 14.1.1).
    #[test]
    fn z_and_pre_override_each_other() {
        let a = p(&["sfor", "x", "-z", "--pre", "cat"]);
        assert_eq!(
            (a.search_zip, a.pre.as_deref()),
            (false, Some(std::ffi::OsStr::new("cat")))
        );
        let a = p(&["sfor", "x", "--pre", "cat", "-z"]);
        assert_eq!((a.search_zip, a.pre), (true, None));
        assert_eq!(p(&["sfor", "x", "--pre", ""]).pre, None);
        assert_eq!(p(&["sfor", "x", "--pre", "cat", "--no-pre"]).pre, None);
        assert!(!p(&["sfor", "x", "-z", "--no-search-zip"]).search_zip);
        let a = p(&[
            "sfor",
            "x",
            "--pre-glob",
            "*.pdf",
            "--pre-glob",
            "!a.pdf",
            "--docs",
        ]);
        assert_eq!(a.pre_globs, ["*.pdf", "!a.pdf"]);
        assert!(a.docs);
    }
```

- [ ] **Step 3:** in the `tests` module of `src/processors/mod.rs`, before
  `by_name_knows_the_m3_built_ins`:

```rust
    /// M4: `object` is a stateless built-in; `docs` is known to Definitions but serves no file
    /// as a Processor (its Extractors run before the search: M4 plan, design note 4).
    #[test]
    fn object_is_built_in_and_docs_is_known_but_describes_nothing() {
        assert!(by_name("object").is_some());
        assert!(Processors::known("object") && Processors::known("docs"));
        assert!(by_name("docs").is_none());
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        let pdf = def("name='pdf'\nprocessor='docs'\n[params]\nextractor='pdf'");
        assert!(ps.for_definition(&pdf).is_none());
        assert!(!ps.describes(&pdf));
        assert!(ps.available(&pdf));
        assert!(seen.lock().unwrap().is_empty());
    }
```

- [ ] **Step 4:** in `implemented_flags_are_the_m1_set` of `src/cli/rg_flags.rs`, add to
  `expected`, after `"word-regexp",`:

```rust
            // M4: ripgrep's input transforms (spec §8).
            "pre",
            "no-pre",
            "pre-glob",
            "search-zip",
            "no-search-zip",
```

  and in `m2_07_list_and_export_the_new_formats` of `tests/cli_m2.rs`:

```diff
diff --git a/tests/cli_m2.rs b/tests/cli_m2.rs
--- a/tests/cli_m2.rs
+++ b/tests/cli_m2.rs
@@ -181,8 +181,9 @@ fn m2_07_list_and_export_the_new_formats() {
     );
     assert_eq!(
         std::fs::read_dir(&target).unwrap().count(),
-        // Markdown, M2's Formats, and M3's html, xml, csv, tsv, srt and vtt.
-        1 + FORMATS.len() + 6
+        // Markdown, M2's Formats, M3's html, xml, csv, tsv, srt and vtt, and M4's elf, pe,
+        // macho and nine document Formats.
+        1 + FORMATS.len() + 6 + 12
     );
 }
 
```

- [ ] **Step 5:** `cargo test --lib -- the_m4_definitions z_and_pre object_is_built_in
  implemented_flags`: RED, since `with_docs`, `object` and the field `search_zip` do not exist
  yet (to verify in chunk 1: the probe wrote these tests together with their code).

### Task 1.3: the twelve Definitions

**Files:** `formats/{elf,pe,macho,pdf,docx,xlsx,pptx,odt,ods,odp,epub,sqlite}.toml`,
`src/formats/builtin.rs`.

- [ ] **Step 1:** the object Definitions (spec §5.7, D5): no extensions, their magic.

```toml
# formats/elf.toml — ELF object files (spec §5.7): `ELF x86-64 › .rodata › main.version+0x10`.
# An object Format is told by its magic in every binary file with a Match, whatever its name
# (`libc.so.6`, `/bin/ls`): spec §4.1, M4 plan D5.
name      = "elf"
magic     = [{ hex = "7f454c46" }]
processor = "object"
```

```toml
# formats/pe.toml — PE images and objects (spec §5.7): `PE x86-64 › .rdata › version+0xe`.
name      = "pe"
magic     = [{ hex = "4d5a" }]
processor = "object"
```

```toml
# formats/macho.toml — Mach-O files, also fat ones (spec §5.7): `Mach-O arm64 › __TEXT,__cstring`.
name      = "macho"
magic     = [{ hex = "feedface" }, { hex = "feedfacf" }, { hex = "cefaedfe" }, { hex = "cffaedfe" }, { hex = "cafebabe" }]
processor = "object"
```

- [ ] **Step 2:** the document Definitions (spec §5.8, D10). The magic of `odt`, `ods`, `odp` and
  `epub` is `mimetype` and the media type, at byte 30 of the zip (the first member, stored),
  in hex:

```toml
# formats/pdf.toml — PDF with --docs (spec §5.8): `3 Results › 3.2 Costs › page 42`.
name       = "pdf"
extensions = ["pdf"]
magic      = [{ hex = "255044462d" }]
processor  = "docs"

[params]
extractor = "pdf"
```

```toml
# formats/docx.toml — Word documents: `2 Costs › table 1 › row 3 › Price` with --docs (spec §5.8).
name       = "docx"
extensions = ["docx", "docm", "dotx", "dotm"]
processor  = "docs"

[params]
extractor = "docx"
```

```toml
# formats/xlsx.toml — Excel workbooks: `Budget › B12` with --docs (spec §5.8).
name       = "xlsx"
extensions = ["xlsx", "xlsm", "xltx", "xltm"]
processor  = "docs"

[params]
extractor = "xlsx"
```

```toml
# formats/pptx.toml — PowerPoint presentations: `slide 3 › notes` with --docs (spec §5.8).
name       = "pptx"
extensions = ["pptx", "pptm", "potx", "potm", "ppsx", "ppsm"]
processor  = "docs"

[params]
extractor = "pptx"
```

```toml
# formats/odt.toml — OpenDocument texts: `2 Costs › table 1 › row 3 › Price` with --docs (spec §5.8).
name       = "odt"
extensions = ["odt", "ott"]
magic      = [{ offset = 30, hex = "6d696d65747970656170706c69636174696f6e2f766e642e6f617369732e6f70656e646f63756d656e742e74657874" }]
processor  = "docs"

[params]
extractor = "odt"
```

```toml
# formats/ods.toml — OpenDocument spreadsheets: `Budget › B12` with --docs (spec §5.8).
name       = "ods"
extensions = ["ods", "ots"]
magic      = [{ offset = 30, hex = "6d696d65747970656170706c69636174696f6e2f766e642e6f617369732e6f70656e646f63756d656e742e7370726561647368656574" }]
processor  = "docs"

[params]
extractor = "ods"
```

```toml
# formats/odp.toml — OpenDocument presentations: `slide 3 › notes` with --docs (spec §5.8).
name       = "odp"
extensions = ["odp", "otp"]
magic      = [{ offset = 30, hex = "6d696d65747970656170706c69636174696f6e2f766e642e6f617369732e6f70656e646f63756d656e742e70726573656e746174696f6e" }]
processor  = "docs"

[params]
extractor = "odp"
```

```toml
# formats/epub.toml — EPUB books: `3. The Storm › The harbour` with --docs (spec §5.8).
name       = "epub"
extensions = ["epub"]
magic      = [{ offset = 30, hex = "6d696d65747970656170706c69636174696f6e2f657075622b7a6970" }]
processor  = "docs"

[params]
extractor = "epub"
```

```toml
# formats/sqlite.toml — SQLite databases with --docs (spec §5.8): `users › row 12 › email`.
name       = "sqlite"
extensions = ["sqlite", "sqlite3", "db", "db3", "s3db", "sl3"]
magic      = [{ hex = "53514c69746520666f726d6174203300" }]
processor  = "docs"

[params]
extractor = "sqlite"
```

- [ ] **Step 3:** embed them:

```diff
diff --git a/src/formats/builtin.rs b/src/formats/builtin.rs
--- a/src/formats/builtin.rs
+++ b/src/formats/builtin.rs
@@ -25,4 +25,16 @@ pub const BUILTIN: &[(&str, &str)] = &[
     ("tsv.toml", include_str!("../../formats/tsv.toml")),
     ("srt.toml", include_str!("../../formats/srt.toml")),
     ("vtt.toml", include_str!("../../formats/vtt.toml")),
+    ("elf.toml", include_str!("../../formats/elf.toml")),
+    ("pe.toml", include_str!("../../formats/pe.toml")),
+    ("macho.toml", include_str!("../../formats/macho.toml")),
+    ("pdf.toml", include_str!("../../formats/pdf.toml")),
+    ("docx.toml", include_str!("../../formats/docx.toml")),
+    ("xlsx.toml", include_str!("../../formats/xlsx.toml")),
+    ("pptx.toml", include_str!("../../formats/pptx.toml")),
+    ("odt.toml", include_str!("../../formats/odt.toml")),
+    ("ods.toml", include_str!("../../formats/ods.toml")),
+    ("odp.toml", include_str!("../../formats/odp.toml")),
+    ("epub.toml", include_str!("../../formats/epub.toml")),
+    ("sqlite.toml", include_str!("../../formats/sqlite.toml")),
 ];
```

### Task 1.4: the Detector

**Files:** `src/formats/definition.rs`, `src/formats/detector.rs`.

- [ ] **Step 1:** `FormatDefinition::magic_at`, which compares a magic with a file's first
  bytes as it reads its hex, in place of `magic_bytes`, which allocated each magic for each
  file: with `--docs`, every file named without an extension is detected before its search, and
  the allocations made `--docs` over the benchmark's 300 object files cost 2.1 ms of CPU more
  than without it, 0.8 ms after (review r1, Fable #5). Test first (`magic_decodes` asserts
  `magic_at` instead of `magic_bytes`), RED: `magic_at` does not exist:

```diff
diff --git a/src/formats/definition.rs b/src/formats/definition.rs
--- a/src/formats/definition.rs
+++ b/src/formats/definition.rs
@@ -70,18 +70,20 @@ impl FormatDefinition {
         Ok(d)
     }
 
-    /// Each magic entry as (offset, bytes). `parse` has already checked the hex strings.
-    pub fn magic_bytes(&self) -> Vec<(usize, Vec<u8>)> {
-        self.magic
-            .iter()
-            .map(|m| {
-                let bytes = (0..m.hex.len())
-                    .step_by(2)
-                    .map(|i| u8::from_str_radix(&m.hex[i..i + 2], 16).unwrap())
-                    .collect();
-                (m.offset, bytes)
-            })
-            .collect()
+    /// True when the bytes of one magic entry are at its offset in `head`. Compared as they
+    /// are read, so a file's detection allocates nothing (M4 plan r1, Fable #5: --docs looks
+    /// at the first bytes of every file named without an extension).
+    pub fn magic_at(&self, head: &[u8]) -> bool {
+        self.magic.iter().any(|m| {
+            let hex = m.hex.as_bytes();
+            let want = (0..hex.len() / 2).map(|i| {
+                std::str::from_utf8(&hex[2 * i..2 * i + 2])
+                    .ok()
+                    .and_then(|h| u8::from_str_radix(h, 16).ok())
+            });
+            head.get(m.offset..m.offset + hex.len() / 2)
+                .is_some_and(|got| got.iter().zip(want).all(|(b, w)| w == Some(*b)))
+        })
     }
 }
 
@@ -126,6 +128,8 @@ mod tests {
             known,
         )
         .unwrap();
-        assert_eq!(d.magic_bytes(), vec![(0, vec![0x7f, b'E', b'L', b'F'])]);
+        assert!(d.magic_at(b"\x7fELF\x02"));
+        assert!(!d.magic_at(b"\x7fEL"));
+        assert!(!d.magic_at(b"\x7fELG"));
     }
 }
```

- [ ] **Step 2:** `with_docs`, `active`, `object` and `may_be_document`, with `detect` and
  `object` comparing through `magic_at`. `with_docs` gathers the names and extensions the
  document Formats claim, so `may_be_document` tells with one set lookup that `a.txt` is no
  document, before `detect` walks every Definition: `--docs` asks it of every file, and over
  the benchmark's 20 000 text files the walk cost 23 ms of CPU and 1.3 ms of wall time (review
  r1, Fable #5; test `only_some_names_may_be_documents`). And the
  test fixture `elfy` renamed `myfy` with a magic that is not ELF's, which the built-in `elf`
  claims now (its tests would detect `elf` instead):

```diff
diff --git a/src/formats/detector.rs b/src/formats/detector.rs
--- a/src/formats/detector.rs
+++ b/src/formats/detector.rs
@@ -1,6 +1,7 @@
 //! The Detector (spec §4.1): override, exact name, extension, magic bytes, shebang.
 use super::{Registry, definition::FormatDefinition};
 use globset::{Glob, GlobMatcher};
+use std::collections::HashSet;
 use std::path::Path;
 
 enum Override {
@@ -11,6 +12,13 @@ enum Override {
 pub struct Detector<'r> {
     registry: &'r Registry,
     overrides: Vec<Override>,
+    /// `--docs`: without it the document Formats (`processor = "docs"`) are never detected,
+    /// so their files are Plain, as ripgrep reads them (spec §5.8).
+    docs: bool,
+    /// With `--docs`, the exact names and the extensions (lowercase) the document Formats
+    /// claim (`may_be_document`).
+    doc_names: HashSet<String>,
+    doc_extensions: HashSet<String>,
 }
 
 impl<'r> Detector<'r> {
@@ -33,9 +41,59 @@ impl<'r> Detector<'r> {
         Ok(Self {
             registry,
             overrides,
+            docs: false,
+            doc_names: HashSet::new(),
+            doc_extensions: HashSet::new(),
         })
     }
 
+    /// Detects the document Formats too (`--docs`).
+    pub fn with_docs(mut self, on: bool) -> Self {
+        self.docs = on;
+        if on {
+            for d in self.registry.all().iter().filter(|d| d.processor == "docs") {
+                self.doc_names.extend(d.names.iter().cloned());
+                self.doc_extensions
+                    .extend(d.extensions.iter().map(|e| e.to_ascii_lowercase()));
+            }
+        }
+        self
+    }
+
+    /// False when `path` cannot be a document, which a set lookup tells for most files, before
+    /// `detect` walks every Definition: --docs asks it of every file (M4 plan r1, Fable #5).
+    /// True with an override, for a name without an extension (its first bytes decide), and
+    /// for a name or extension a document Format claims.
+    pub fn may_be_document(&self, path: &Path) -> bool {
+        if !self.docs {
+            return false;
+        }
+        let (Some(name), Some(ext)) = (
+            path.file_name().and_then(|n| n.to_str()),
+            path.extension().and_then(|e| e.to_str()),
+        ) else {
+            return true;
+        };
+        !self.overrides.is_empty()
+            || self.doc_names.contains(name)
+            || self.doc_extensions.contains(&ext.to_ascii_lowercase())
+    }
+
+    /// Whether `d` takes part in detection: a document Format only with `--docs`.
+    fn active(&self, d: &FormatDefinition) -> bool {
+        self.docs || d.processor != "docs"
+    }
+
+    /// The object Format whose magic bytes start `head`, whatever the file's name: binaries are
+    /// often named without a telling extension (`libc.so.6`, `ls`), so for a binary file with a
+    /// Match the magic decides (spec §4.1, M4 plan D5).
+    pub fn object(&self, head: &[u8]) -> Option<&'r FormatDefinition> {
+        self.registry
+            .all()
+            .iter()
+            .find(|d| d.processor == "object" && d.magic_at(head))
+    }
+
     /// `detect` before reading the file: Some when the path alone decides (an override, an
     /// exact name, an extension, a name with an extension no Format claims, or no Format that
     /// looks at the content), None when the Format may depend on the first bytes (magic bytes
@@ -51,7 +109,7 @@ impl<'r> Detector<'r> {
             .registry
             .all()
             .iter()
-            .any(|d| !d.magic_bytes().is_empty() || !d.shebangs.is_empty());
+            .any(|d| self.active(d) && (!d.magic.is_empty() || !d.shebangs.is_empty()));
         (!by_content).then_some(None)
     }
 
@@ -69,14 +127,16 @@ impl<'r> Detector<'r> {
         }
         // A file name that is not UTF-8 skips the name and extension signals only.
         if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
-            && let Some(d) = defs.iter().find(|d| d.names.iter().any(|n| n == file_name))
+            && let Some(d) = defs
+                .iter()
+                .find(|d| self.active(d) && d.names.iter().any(|n| n == file_name))
         {
             return Some(d);
         }
         if let Some(ext) = path.extension().and_then(|e| e.to_str())
-            && let Some(d) = defs
-                .iter()
-                .find(|d| d.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
+            && let Some(d) = defs.iter().find(|d| {
+                self.active(d) && d.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
+            })
         {
             return Some(d);
         }
@@ -84,11 +144,7 @@ impl<'r> Detector<'r> {
         if path.extension().is_some() {
             return None;
         }
-        if let Some(d) = defs.iter().find(|d| {
-            d.magic_bytes()
-                .iter()
-                .any(|(off, bytes)| head.get(*off..off + bytes.len()) == Some(bytes.as_slice()))
-        }) {
+        if let Some(d) = defs.iter().find(|d| self.active(d) && d.magic_at(head)) {
             return Some(d);
         }
         let interp = shebang_interpreter(head)?;
@@ -115,11 +171,35 @@ fn shebang_interpreter(head: &[u8]) -> Option<&str> {
 mod tests {
     use super::*;
 
+    #[test]
+    fn only_some_names_may_be_documents() {
+        let r = registry();
+        let d = Detector::new(&r, &[]).unwrap().with_docs(true);
+        let p = Path::new;
+        for (path, may) in [
+            ("a.txt", false),
+            ("src/main.rs", false),
+            ("r.pdf", true),
+            ("R.PDF", true),
+            ("app.db", true),
+            ("Makefile", true),
+            ("bin/ls", true),
+        ] {
+            assert_eq!(d.may_be_document(p(path)), may, "{path}");
+        }
+        assert!(!Detector::new(&r, &[]).unwrap().may_be_document(p("r.pdf")));
+        let forced = Detector::new(&r, &["*.txt=pdf".into()])
+            .unwrap()
+            .with_docs(true);
+        assert!(forced.may_be_document(p("a.txt")));
+    }
+
     fn known(p: &str) -> bool {
         crate::processors::Processors::known(p)
     }
 
-    /// The built-ins plus `bashy` (shebang), `elfy` (magic) and `makey` (exact name).
+    /// The built-ins plus `bashy` (shebang), `myfy` (magic) and `makey` (exact name). `myfy`'s
+    /// magic is not ELF's, which the built-in `elf` claims since M4.
     fn registry() -> Registry {
         let d = tempfile::tempdir().unwrap();
         std::fs::write(
@@ -128,8 +208,8 @@ mod tests {
         )
         .unwrap();
         std::fs::write(
-            d.path().join("elfy.toml"),
-            "name='elfy'\nmagic=[{hex='7f454c46'}]\nprocessor='markdown'",
+            d.path().join("myfy.toml"),
+            "name='myfy'\nmagic=[{hex='7f4d5946'}]\nprocessor='markdown'",
         )
         .unwrap();
         std::fs::write(
@@ -155,8 +235,8 @@ mod tests {
             "makey"
         );
         assert_eq!(
-            det.detect(Path::new("prog"), b"\x7fELF\x02").unwrap().name,
-            "elfy"
+            det.detect(Path::new("prog"), b"\x7fMYF\x02").unwrap().name,
+            "myfy"
         );
         assert_eq!(
             det.detect(Path::new("run"), b"#!/usr/bin/env -S fish -e\n")
@@ -174,7 +254,7 @@ mod tests {
         let name = |p: &str| det.detect_by_path(Path::new(p)).map(|d| d.map(|d| &d.name));
         assert_eq!(name("a.md"), Some(Some(&"markdown".to_string())));
         assert_eq!(name("Makefile"), Some(Some(&"makey".to_string())));
-        // elfy (magic) and bashy (shebang) need the first bytes of a name without extension.
+        // myfy (magic) and bashy (shebang) need the first bytes of a name without extension.
         assert_eq!(name("data"), None);
         let (builtin, _) = Registry::load(None, &known);
         let det = Detector::new(&builtin, &[]).unwrap();
@@ -198,12 +278,12 @@ mod tests {
             det.detect(Path::new("run.txt"), b"#!/usr/bin/env fish\n")
                 .is_none()
         );
-        assert!(det.detect(Path::new("prog.bin"), b"\x7fELF\x02").is_none());
+        assert!(det.detect(Path::new("prog.bin"), b"\x7fMYF\x02").is_none());
         assert_eq!(
-            det.detect(Path::new(".bashrc_like"), b"\x7fELF")
+            det.detect(Path::new(".bashrc_like"), b"\x7fMYF")
                 .unwrap()
                 .name,
-            "elfy"
+            "myfy"
         );
         // So a file with an unclaimed extension is Plain from its path alone.
         assert_eq!(det.detect_by_path(Path::new("data.txt")), Some(None));
@@ -216,11 +296,11 @@ mod tests {
         let det = Detector::new(&reg, &[]).unwrap();
         // Exact name beats extension, extension beats magic, magic beats shebang.
         assert_eq!(
-            det.detect(Path::new("Makefile"), b"\x7fELF").unwrap().name,
+            det.detect(Path::new("Makefile"), b"\x7fMYF").unwrap().name,
             "makey"
         );
         assert_eq!(
-            det.detect(Path::new("x.md"), b"\x7fELF").unwrap().name,
+            det.detect(Path::new("x.md"), b"\x7fMYF").unwrap().name,
             "markdown"
         );
         assert_eq!(
@@ -250,7 +330,7 @@ mod tests {
         let det = Detector::new(&reg, &["makey".into()]).unwrap();
         assert_eq!(det.detect(Path::new("x.md"), b"").unwrap().name, "makey");
         assert_eq!(
-            det.detect(Path::new("data.bin"), b"\x7fELF").unwrap().name,
+            det.detect(Path::new("data.bin"), b"\x7fMYF").unwrap().name,
             "makey"
         );
     }
@@ -263,8 +343,8 @@ mod tests {
         let det = Detector::new(&reg, &[]).unwrap();
         let name = std::ffi::OsStr::from_bytes(b"\xff\xfe");
         assert_eq!(
-            det.detect(Path::new(name), b"\x7fELF").unwrap().name,
-            "elfy"
+            det.detect(Path::new(name), b"\x7fMYF").unwrap().name,
+            "myfy"
         );
         assert_eq!(
             det.detect(Path::new(name), b"#!/usr/bin/fish\n")
```

### Task 1.5: the flags

**Files:** `src/cli/mod.rs`, `src/cli/rg_flags.rs`, `docs/rg-compat.md`.

- [ ] **Step 1:** `-z`, `--no-search-zip`, `--pre`, `--no-pre`, `--pre-glob` and `--docs`, and
  the last of `-z` and `--pre` winning (D2). `pre` is an `OsString`, as clap's `PathBuf` parser
  refuses the empty value that turns `--pre` off. The diff also shows the test
  `z_and_pre_override_each_other`, which Task 1.2 wrote: skip that hunk:

```diff
diff --git a/src/cli/mod.rs b/src/cli/mod.rs
--- a/src/cli/mod.rs
+++ b/src/cli/mod.rs
@@ -179,6 +179,22 @@ pub struct Args {
     pub binary_choice: BinaryChoice,
     #[arg(short = 'q', long)]
     pub quiet: bool,
+    /// ripgrep's -z: search compressed files through their decompressors on PATH (gzip,
+    /// bzip2, xz, LZ4, LZMA, Brotli, Zstd). -z and --pre override each other (last wins).
+    #[arg(short = 'z', long, overrides_with = "no_search_zip")]
+    pub search_zip: bool,
+    #[arg(long, hide = true, overrides_with = "search_zip")]
+    pub no_search_zip: bool,
+    /// ripgrep's --pre: search the output of `COMMAND PATH` instead of PATH; an empty COMMAND
+    /// turns it off.
+    #[arg(long, value_name = "COMMAND", overrides_with = "no_pre")]
+    pub pre: Option<OsString>,
+    #[arg(long, hide = true, overrides_with = "pre")]
+    pub no_pre: bool,
+    /// ripgrep's --pre-glob: run --pre only on files these globs match (gitignore syntax, `!`
+    /// excludes).
+    #[arg(long = "pre-glob", value_name = "GLOB", action = ArgAction::Append)]
+    pub pre_globs: Vec<String>,
     #[arg(long, overrides_with = "messages")]
     pub no_messages: bool,
     #[arg(long, hide = true, overrides_with = "no_messages")]
@@ -204,6 +220,10 @@ pub struct Args {
     /// Print exactly what ripgrep would print.
     #[arg(long)]
     pub no_trail: bool,
+    /// Search PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite files through the
+    /// built-in Extractors (spec §5.8).
+    #[arg(long)]
+    pub docs: bool,
     /// Fragment width when there is no terminal.
     #[arg(long, default_value_t = 200)]
     pub fragment_width: usize,
@@ -282,6 +302,18 @@ pub fn parse(args: Vec<OsString>) -> Result<Args, CliError> {
     a.no_ignore_resolved = last_by_index(&m, &[("no_ignore", true), ("ignore", false)], u(0), true);
     a.hidden_resolved = last_by_index(&m, &[("hidden", true), ("no_hidden", false)], u(1), true);
     a.binary_choice = binary_choice(&m);
+    // -z and --pre override each other, the last one given wins, as in ripgrep 14.1.1; an
+    // empty --pre is no --pre.
+    if a.pre.as_ref().is_some_and(|p| p.is_empty()) {
+        a.pre = None;
+    }
+    if a.search_zip && a.pre.is_some() {
+        if m.index_of("search_zip") > m.index_of("pre") {
+            a.pre = None;
+        } else {
+            a.search_zip = false;
+        }
+    }
     a.type_selections = type_selections(&m);
     a.pattern_sources = pattern_sources(&m);
     // `-M0` turns the limit off, as in ripgrep 14.1.1 (code review r2, Codex #3).
@@ -519,6 +551,33 @@ mod tests {
         parse(os(v)).unwrap()
     }
 
+    /// M4: ripgrep's -z and --pre override each other, the last one given winning, and an
+    /// empty --pre is none (rg 14.1.1).
+    #[test]
+    fn z_and_pre_override_each_other() {
+        let a = p(&["sfor", "x", "-z", "--pre", "cat"]);
+        assert_eq!(
+            (a.search_zip, a.pre.as_deref()),
+            (false, Some(std::ffi::OsStr::new("cat")))
+        );
+        let a = p(&["sfor", "x", "--pre", "cat", "-z"]);
+        assert_eq!((a.search_zip, a.pre), (true, None));
+        assert_eq!(p(&["sfor", "x", "--pre", ""]).pre, None);
+        assert_eq!(p(&["sfor", "x", "--pre", "cat", "--no-pre"]).pre, None);
+        assert!(!p(&["sfor", "x", "-z", "--no-search-zip"]).search_zip);
+        let a = p(&[
+            "sfor",
+            "x",
+            "--pre-glob",
+            "*.pdf",
+            "--pre-glob",
+            "!a.pdf",
+            "--docs",
+        ]);
+        assert_eq!(a.pre_globs, ["*.pdf", "!a.pdf"]);
+        assert!(a.docs);
+    }
+
     #[test]
     fn max_columns_zero_is_no_limit() {
         assert_eq!(p(&["sfor", "-M0", "x"]).max_columns, None);
```

- [ ] **Step 2:** the five ripgrep flags implemented, and the deviations of object files and
  documents in `docs/rg-compat.md` (D26). The diff also shows the five names Task 1.2 added to
  `expected`: skip that hunk:

````diff
diff --git a/src/cli/rg_flags.rs b/src/cli/rg_flags.rs
--- a/src/cli/rg_flags.rs
+++ b/src/cli/rg_flags.rs
@@ -120,9 +120,9 @@ pub const RG_FLAGS: &[RgFlag] = &[
     RgFlag { long: "no-one-file-system", short: None, takes_value: false, status: NotYet },
     RgFlag { long: "no-pcre2", short: None, takes_value: false, status: NotYet },
     RgFlag { long: "no-pcre2-unicode", short: None, takes_value: false, status: NotYet },
-    RgFlag { long: "no-pre", short: None, takes_value: false, status: NotYet },
+    RgFlag { long: "no-pre", short: None, takes_value: false, status: Implemented },
     RgFlag { long: "no-require-git", short: None, takes_value: false, status: NotYet },
-    RgFlag { long: "no-search-zip", short: None, takes_value: false, status: NotYet },
+    RgFlag { long: "no-search-zip", short: None, takes_value: false, status: Implemented },
     RgFlag { long: "no-sort-files", short: None, takes_value: false, status: NotYet },
     RgFlag { long: "no-stats", short: None, takes_value: false, status: NotYet },
     RgFlag { long: "no-text", short: None, takes_value: false, status: Implemented },
@@ -137,15 +137,15 @@ pub const RG_FLAGS: &[RgFlag] = &[
     RgFlag { long: "pcre2", short: Some('P'), takes_value: false, status: NotYet },
     RgFlag { long: "pcre2-unicode", short: None, takes_value: false, status: NotYet },
     RgFlag { long: "pcre2-version", short: None, takes_value: false, status: NotYet },
-    RgFlag { long: "pre", short: None, takes_value: true, status: NotYet },
-    RgFlag { long: "pre-glob", short: None, takes_value: true, status: NotYet },
+    RgFlag { long: "pre", short: None, takes_value: true, status: Implemented },
+    RgFlag { long: "pre-glob", short: None, takes_value: true, status: Implemented },
     RgFlag { long: "pretty", short: Some('p'), takes_value: false, status: NotYet },
     RgFlag { long: "quiet", short: Some('q'), takes_value: false, status: Implemented },
     RgFlag { long: "regex-size-limit", short: None, takes_value: true, status: NotYet },
     RgFlag { long: "regexp", short: Some('e'), takes_value: true, status: Implemented },
     RgFlag { long: "replace", short: Some('r'), takes_value: true, status: NotYet },
     RgFlag { long: "require-git", short: None, takes_value: false, status: NotYet },
-    RgFlag { long: "search-zip", short: Some('z'), takes_value: false, status: NotYet },
+    RgFlag { long: "search-zip", short: Some('z'), takes_value: false, status: Implemented },
     RgFlag { long: "smart-case", short: Some('S'), takes_value: false, status: Implemented },
     RgFlag { long: "sort", short: None, takes_value: true, status: Implemented },
     RgFlag { long: "sort-files", short: None, takes_value: false, status: NotYet },
@@ -284,6 +284,29 @@ searched before the read that brings the NUL. For UTF-16 that cut cannot follow
 through the decoder, so it is the larger of the last reported line and `bytes_searched`: with a
 context (`-A`, `-B`, `-C`) on a UTF-16 file that holds U+0000, the names of the last lines of
 that buffer can be missing (fix after code review r9).
+
+### Object files
+
+rg 14.1.1 prints `binary file matches (found \"\\0\" byte around offset N)` for an ELF, PE or
+Mach-O file that it searches with a Match (a file named on the command line, or any with
+`--binary`), and stops at its first Match. sfor prints one row per printable string that holds
+Matches instead, at the hex offset of its first Match and with its Trail (spec §5.7), and so
+searches the whole file: in `--json`, `bytes_searched` is the file's size, `matched_lines` the
+rows and `matches` the Matches, and `binary_offset` is null. `-c`, `-l`, `-q` and `--no-trail`
+print what rg prints. With `-v` the file is printed as rg prints it. A walked object file
+without `--binary` stops at its first NUL, as in rg, and prints nothing (M4 plan, D4):
+
+```sh
+sfor --flat version /usr/bin/ls      # rows; rg: binary file matches
+sfor --flat --binary version /usr/bin    # the same for every object file with a Match
+```
+
+### Documents with `--docs`
+
+`--docs` has no ripgrep counterpart: a PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB or SQLite
+file is searched in the text its Extractor takes from it (spec §5.8), also with `--no-trail`,
+`-c`, `-l` and `-q`, which print what rg would print for that text. `bytes_searched` counts the
+text. Without `--docs` these files are searched as rg searches them.
 ";
 
 #[cfg(test)]
@@ -452,6 +475,12 @@ mod tests {
             "version",
             "with-filename",
             "word-regexp",
+            // M4: ripgrep's input transforms (spec §8).
+            "pre",
+            "no-pre",
+            "pre-glob",
+            "search-zip",
+            "no-search-zip",
         ];
         expected.sort();
         assert_eq!(implemented, expected);
````

- [ ] **Step 3:** `cargo test --lib rg_compat_doc -- --ignored` regenerates `docs/rg-compat.md`;
  `git diff docs/rg-compat.md` shows the five rows turned to `yes` and the two new sections.

### Task 1.6: `Described::offsets` and the Processors

**Files:** `src/model.rs`, `src/processors/mod.rs`, `src/app.rs`, `src/filter.rs`,
`src/output/mod.rs`, `src/output/stream.rs` (the `Described` literals only).

- [ ] **Step 1:** the field (design note 5):

```diff
diff --git a/src/model.rs b/src/model.rs
--- a/src/model.rs
+++ b/src/model.rs
@@ -134,6 +134,9 @@ pub struct Described {
     /// With `--expand`, `sections[i][k]`: the Section of submatch `k` of Match `i`
     /// (1-based inclusive lines), aligned with `trails`.
     pub sections: Vec<Vec<Option<(u64, u64)>>>,
+    /// The rows are an object file's printable strings (spec §5.7): tree and one-line output
+    /// print each at the hex offset of its first Match, where they print a line number.
+    pub offsets: bool,
 }
 
 /// What a Processor receives about one file (design note 6). External Processors (M5) need
```

- [ ] **Step 2:** add `offsets: false,` after `sections: …,` in every `Described { … }` literal:
  the two of `src/app.rs` (`describe` and `names_of`), and those of the tests of
  `src/filter.rs`, `src/output/mod.rs` (`fixture::described`) and `src/output/stream.rs`. The
  compiler lists them.

- [ ] **Step 3:** `object` as a built-in, `docs` as a known name that serves no file (design
  note 1). The diff also shows the test
  `object_is_built_in_and_docs_is_known_but_describes_nothing`, which Task 1.2 wrote: skip that
  hunk:

```diff
diff --git a/src/processors/mod.rs b/src/processors/mod.rs
--- a/src/processors/mod.rs
+++ b/src/processors/mod.rs
@@ -5,6 +5,7 @@ pub mod frontmatter;
 pub mod keypath;
 pub mod markdown;
 pub mod markup;
+pub mod object;
 pub mod subtitles;
 pub mod treesitter;
 
@@ -74,6 +75,7 @@ pub fn by_name(name: &str) -> Option<&'static dyn Processor> {
         "markup" => Some(&markup::Markup),
         "csv" => Some(&csv::Csv),
         "subtitles" => Some(&subtitles::Subtitles),
+        "object" => Some(&object::Object),
         _ => None,
     }
 }
@@ -194,7 +196,7 @@ impl Processors {
 
     /// The Processor names a Format Definition may use (`Registry::load`).
     pub fn known(name: &str) -> bool {
-        by_name(name).is_some() || matches!(name, "treesitter" | "external")
+        by_name(name).is_some() || matches!(name, "treesitter" | "external" | "docs")
     }
 
     /// Whether files of `def` may get Trails, without building anything or warning: for
@@ -204,6 +206,9 @@ impl Processors {
         match def.processor.as_str() {
             "external" => !self.disabled.lock().unwrap().contains(&def.name),
             "treesitter" => self.has_grammar(Self::grammar_of(def)),
+            // A document's Trails come from its Extractor, before the search (M4 plan,
+            // design note 4); as a Processor, `docs` describes nothing.
+            "docs" => false,
             p => by_name(p).is_some(),
         }
     }
@@ -240,6 +245,7 @@ impl Processors {
                 );
                 None
             }
+            "docs" => None,
             p => by_name(p).map(ProcessorRef::BuiltIn),
         }
     }
@@ -360,6 +366,22 @@ mod tests {
         assert_eq!(n[0].pieces, vec![(0..1, 2..3)]);
     }
 
+    /// M4: `object` is a stateless built-in; `docs` is known to Definitions but serves no file
+    /// as a Processor (its Extractors run before the search: M4 plan, design note 4).
+    #[test]
+    fn object_is_built_in_and_docs_is_known_but_describes_nothing() {
+        assert!(by_name("object").is_some());
+        assert!(Processors::known("object") && Processors::known("docs"));
+        assert!(by_name("docs").is_none());
+        let (w, seen) = collecting();
+        let ps = Processors::new(4, w);
+        let pdf = def("name='pdf'\nprocessor='docs'\n[params]\nextractor='pdf'");
+        assert!(ps.for_definition(&pdf).is_none());
+        assert!(!ps.describes(&pdf));
+        assert!(ps.available(&pdf));
+        assert!(seen.lock().unwrap().is_empty());
+    }
+
     #[test]
     fn by_name_knows_the_m3_built_ins() {
         for p in ["markup", "csv", "subtitles"] {
```

### Task 1.7: the stubs

**Files:** `src/lib.rs`, `src/processors/object/{mod,rows,fixture}.rs`, `src/preprocess.rs`.

Their signatures are the final ones; tanda 2 replaces each file whole.

- [ ] **Step 1:** in `src/lib.rs`, after `pub mod output;`:

```rust
pub mod preprocess;
```

  (`processors/mod.rs` already declares `pub mod object;`, Task 1.6.)

- [ ] **Step 2:** `src/processors/object/mod.rs`:

```rust
//! The `object` Processor (spec §5.7). Stub of C1; Chunk 2 writes it.
#[doc(hidden)]
pub mod fixture;
pub mod rows;

use crate::model::{Input, Name, Trail};
use crate::trail::LineIndex;

pub struct Object;

impl super::Processor for Object {
    fn trails(
        &self,
        _input: &Input,
        _params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        Ok(vec![Vec::new(); anchors.len()])
    }

    fn names(&self, _input: &Input, _params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(Vec::new())
    }
}

/// What the `object` Processor knows of a file. The stub reads nothing.
pub struct Map;

impl Map {
    pub fn parse(_bytes: &[u8]) -> Option<Map> {
        None
    }

    pub fn trail(&self, _at: usize, _lines: &LineIndex) -> Trail {
        Vec::new()
    }
}
```

  `src/processors/object/rows.rs`:

```rust
//! The rows of an object file (spec §5.7). Stub of C1; Chunk 2 writes it.
use crate::model::Line;
use crate::trail::LineIndex;
use std::ops::Range;

pub const SIDE: usize = 4096;

pub fn rows(_bytes: &[u8], _hits: &[Range<usize>], _limit: Option<u64>, _lines: &LineIndex) -> Vec<Line> {
    Vec::new()
}
```

  and `src/processors/object/fixture.rs`, which C2 writes:

```rust
//! ELF, PE and Mach-O writers (M4 plan, D19). Stub of C1; Chunk 2 writes it.
```

- [ ] **Step 3:** `src/preprocess.rs`, whose `Through` is already the final one (D21: `Docs`
  holds the document Format's name and its Extractor's, so `preprocess` needs nothing of
  `docs`):

```rust
//! What a file is searched through (M4 plan, design note 3). Stub of C1; Chunk 3 writes it.
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Through {
    Pre,
    Zip,
    Docs(String, String),
}

pub struct Preprocess;

impl Preprocess {
    pub fn new(_args: &crate::cli::Args, _cwd: &Path) -> anyhow::Result<Self> {
        Ok(Preprocess)
    }

    pub fn through(&self, _path: &Path) -> Option<Through> {
        None
    }

    pub fn open(&self, _how: &Through, _path: &Path) -> std::io::Result<Box<dyn Read + Send>> {
        Err(std::io::Error::other("--pre and -z are written in Chunk 3"))
    }
}
```

### Task 1.8: green, and the commit

- [ ] **Step 1:** `cargo test --lib -- the_m4_definitions z_and_pre object_is_built_in
  implemented_flags rg_compat_doc_is_current` and `cargo test --test cli_m2`: GREEN.
- [ ] **Step 2:** the whole suite (the probe's state after C1, measured over `191dbaf`: 409 unit
  tests and 2 ignored, `cli` 42, `cli_m2` 16, `cli_m3` 19), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt`, and one
  commit per task (`git add -- <files>`).

## Chunk 2: the `object` Processor

### Task 2.1: the object writers (src/processors/object/fixture.rs)

**Files:** `src/processors/object/fixture.rs`.

- [ ] **Step 1:** the writers (D19), which the tests of this chunk and C11–C13 use. Each lays out
  its sections after its headers, so a test knows where every byte lands (`Layout::offset`); the
  probe checked their output with goblin, `file(1)` and the trails of Task 2.2:

```rust
//! Small object files written byte by byte, for the tests of the `object` Processor, the
//! end-to-end tests, the fuzz and the perf ceilings (M4 plan, D19). Not used by `sfor` itself.
//! Each writer lays out its sections in order after the headers, so a test knows where every
//! byte lands: `Spec::offset` gives the file offset of a byte of a section.

/// What to write: sections (name, bytes) and symbols (name, section index, offset in it, size;
/// only ELF writes the size, 0 meaning unknown).
#[derive(Debug, Clone, Default)]
pub struct Spec {
    pub sections: Vec<(String, Vec<u8>)>,
    pub symbols: Vec<(String, usize, u64, u64)>,
    /// The machine: an ELF `e_machine`, a PE `Machine` or a Mach-O `cputype`; 0 picks x86-64.
    pub machine: u32,
}

impl Spec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn section(mut self, name: &str, bytes: &[u8]) -> Self {
        self.sections.push((name.into(), bytes.to_vec()));
        self
    }

    pub fn symbol(self, name: &str, section: usize, offset: u64) -> Self {
        self.sized(name, section, offset, 0)
    }

    pub fn sized(mut self, name: &str, section: usize, offset: u64, size: u64) -> Self {
        self.symbols.push((name.into(), section, offset, size));
        self
    }

    pub fn machine(mut self, m: u32) -> Self {
        self.machine = m;
        self
    }
}

fn pad(out: &mut Vec<u8>, align: usize) {
    while !out.len().is_multiple_of(align) {
        out.push(0);
    }
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}
fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}
fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend(v.to_le_bytes());
}
fn name16(out: &mut Vec<u8>, name: &str) {
    let mut n = [0u8; 16];
    n[..name.len().min(16)].copy_from_slice(&name.as_bytes()[..name.len().min(16)]);
    out.extend(n);
}

/// Where each section's bytes start in the file, as the writers place them.
pub struct Layout {
    pub file: Vec<u8>,
    pub starts: Vec<usize>,
}

impl Layout {
    /// The file offset of byte `at` of section `section`.
    pub fn offset(&self, section: usize, at: usize) -> usize {
        self.starts[section] + at
    }
}

/// ELF64 little-endian shared object (`ET_DYN`), its sections loaded at 0x400000 plus their
/// file offset, a `.symtab` when `spec.symbols` is not empty, and `.shstrtab` last.
pub fn elf(spec: &Spec) -> Layout {
    const BASE: u64 = 0x40_0000;
    let machine = if spec.machine == 0 {
        62
    } else {
        spec.machine as u16
    };
    let mut out = vec![0u8; 64];
    let mut starts = Vec::new();
    for (_, data) in &spec.sections {
        pad(&mut out, 16);
        starts.push(out.len());
        out.extend(data);
    }
    // .strtab and .symtab
    let mut strtab = vec![0u8];
    let mut symtab = vec![0u8; 24];
    for (name, sec, off, size) in &spec.symbols {
        let st_name = strtab.len() as u32;
        strtab.extend(name.as_bytes());
        strtab.push(0);
        put32(&mut symtab, st_name);
        symtab.push(0x11); // STB_GLOBAL, STT_OBJECT
        symtab.push(0);
        put16(&mut symtab, *sec as u16 + 1);
        put64(&mut symtab, BASE + (starts[*sec] as u64) + off);
        put64(&mut symtab, *size);
    }
    let with_syms = !spec.symbols.is_empty();
    let mut shstrtab = vec![0u8];
    let mut names = Vec::new();
    let mut all: Vec<&str> = spec.sections.iter().map(|(n, _)| n.as_str()).collect();
    if with_syms {
        all.extend([".symtab", ".strtab"]);
    }
    all.push(".shstrtab");
    for n in &all {
        names.push(shstrtab.len() as u32);
        shstrtab.extend(n.as_bytes());
        shstrtab.push(0);
    }
    let mut extra = Vec::new(); // (offset, size) of symtab, strtab, shstrtab
    if with_syms {
        pad(&mut out, 8);
        extra.push((out.len(), symtab.len()));
        out.extend(&symtab);
        extra.push((out.len(), strtab.len()));
        out.extend(&strtab);
    }
    extra.push((out.len(), shstrtab.len()));
    out.extend(&shstrtab);
    pad(&mut out, 8);
    let shoff = out.len();
    let n = spec.sections.len();
    let shnum = 1 + all.len();
    out.extend([0u8; 64]); // SHT_NULL
    let header = |out: &mut Vec<u8>,
                  name: u32,
                  ty: u32,
                  flags: u64,
                  addr: u64,
                  off: usize,
                  size: usize,
                  link: u32,
                  info: u32,
                  ent: u64| {
        put32(out, name);
        put32(out, ty);
        put64(out, flags);
        put64(out, addr);
        put64(out, off as u64);
        put64(out, size as u64);
        put32(out, link);
        put32(out, info);
        put64(out, 1);
        put64(out, ent);
    };
    for (i, (_, data)) in spec.sections.iter().enumerate() {
        let off = starts[i];
        header(
            &mut out,
            names[i],
            1,
            2,
            BASE + off as u64,
            off,
            data.len(),
            0,
            0,
            0,
        );
    }
    let mut k = n;
    if with_syms {
        let (o, s) = extra[0];
        header(&mut out, names[k], 2, 0, 0, o, s, (n + 2) as u32, 1, 24);
        k += 1;
        let (o, s) = extra[1];
        header(&mut out, names[k], 3, 0, 0, o, s, 0, 0, 0);
        k += 1;
    }
    let (o, s) = *extra.last().unwrap();
    header(&mut out, names[k], 3, 0, 0, o, s, 0, 0, 0);
    let mut h = Vec::new();
    h.extend(b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0");
    put16(&mut h, 3); // ET_DYN
    put16(&mut h, machine);
    put32(&mut h, 1);
    put64(&mut h, 0); // entry
    put64(&mut h, 0); // phoff
    put64(&mut h, shoff as u64);
    put32(&mut h, 0);
    put16(&mut h, 64);
    put16(&mut h, 56);
    put16(&mut h, 0);
    put16(&mut h, 64);
    put16(&mut h, shnum as u16);
    put16(&mut h, (shnum - 1) as u16);
    out[..64].copy_from_slice(&h);
    Layout { file: out, starts }
}

/// PE32+ image: DOS header, PE header, sections aligned to 0x200 in the file and 0x1000 in
/// memory, and a COFF symbol table (as MinGW leaves in its images) when `spec.symbols` is not
/// empty.
pub fn pe(spec: &Spec) -> Layout {
    let machine = if spec.machine == 0 {
        0x8664
    } else {
        spec.machine as u16
    };
    let n = spec.sections.len();
    let headers = 0x40 + 4 + 20 + 240 + 40 * n;
    let mut starts = Vec::new();
    let mut at = headers.div_ceil(0x200) * 0x200;
    for (_, d) in &spec.sections {
        starts.push(at);
        at += d.len().div_ceil(0x200).max(1) * 0x200;
    }
    let symtab_at = at;
    let mut out = Vec::new();
    out.extend(b"MZ");
    out.resize(0x3c, 0);
    put32(&mut out, 0x40);
    out.extend(b"PE\0\0");
    put16(&mut out, machine);
    put16(&mut out, n as u16);
    put32(&mut out, 0);
    put32(
        &mut out,
        if spec.symbols.is_empty() {
            0
        } else {
            symtab_at as u32
        },
    );
    put32(&mut out, spec.symbols.len() as u32);
    put16(&mut out, 240);
    put16(&mut out, 0x22);
    // Optional header (PE32+).
    put16(&mut out, 0x20b);
    out.extend([14, 0]);
    put32(&mut out, 0); // SizeOfCode
    put32(&mut out, 0);
    put32(&mut out, 0);
    put32(&mut out, 0x1000); // AddressOfEntryPoint
    put32(&mut out, 0x1000); // BaseOfCode
    put64(&mut out, 0x1_4000_0000);
    put32(&mut out, 0x1000);
    put32(&mut out, 0x200);
    put16(&mut out, 6);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, 6);
    put16(&mut out, 0);
    put32(&mut out, 0);
    put32(&mut out, (0x1000 * (n + 1)) as u32); // SizeOfImage
    put32(&mut out, starts.first().copied().unwrap_or(0x200) as u32); // SizeOfHeaders
    put32(&mut out, 0);
    put16(&mut out, 3);
    put16(&mut out, 0);
    for _ in 0..4 {
        put64(&mut out, 0x1000);
    }
    put32(&mut out, 0);
    put32(&mut out, 16);
    out.extend([0u8; 128]);
    for (i, (name, d)) in spec.sections.iter().enumerate() {
        let mut nm = [0u8; 8];
        nm[..name.len().min(8)].copy_from_slice(&name.as_bytes()[..name.len().min(8)]);
        out.extend(nm);
        put32(&mut out, d.len() as u32);
        put32(&mut out, (0x1000 * (i + 1)) as u32);
        put32(&mut out, (d.len().div_ceil(0x200).max(1) * 0x200) as u32);
        put32(&mut out, starts[i] as u32);
        put32(&mut out, 0);
        put32(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put32(&mut out, 0x4000_0040); // initialized data, readable
    }
    for (i, (_, d)) in spec.sections.iter().enumerate() {
        out.resize(starts[i], 0);
        out.extend(d);
    }
    out.resize(symtab_at, 0);
    if !spec.symbols.is_empty() {
        let mut strings = Vec::new();
        for (name, sec, off, _) in &spec.symbols {
            if name.len() <= 8 {
                let mut nm = [0u8; 8];
                nm[..name.len()].copy_from_slice(name.as_bytes());
                out.extend(nm);
            } else {
                put32(&mut out, 0);
                put32(&mut out, 4 + strings.len() as u32);
                strings.extend(name.as_bytes());
                strings.push(0);
            }
            put32(&mut out, *off as u32);
            out.extend((*sec as i16 + 1).to_le_bytes());
            put16(&mut out, 0);
            out.push(2); // IMAGE_SYM_CLASS_EXTERNAL
            out.push(0);
        }
        put32(&mut out, 4 + strings.len() as u32);
        out.extend(&strings);
    }
    Layout { file: out, starts }
}

/// Mach-O 64-bit little-endian executable: one `LC_SEGMENT_64` per segment name, in the order
/// the sections first name them (`__TEXT,__cstring` names segment `__TEXT`, section
/// `__cstring`), and an `LC_SYMTAB` when `spec.symbols` is not empty. Sections are loaded at
/// 0x1_0000_0000 plus their file offset.
pub fn macho(spec: &Spec) -> Layout {
    const BASE: u64 = 0x1_0000_0000;
    let cputype = if spec.machine == 0 {
        0x0100_0007
    } else {
        spec.machine
    };
    let mut segs: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, (name, _)) in spec.sections.iter().enumerate() {
        let seg = name.split(',').next().unwrap_or("").to_string();
        match segs.iter_mut().find(|s| s.0 == seg) {
            Some(s) => s.1.push(i),
            None => segs.push((seg, vec![i])),
        }
    }
    let with_syms = !spec.symbols.is_empty();
    let cmds: usize =
        segs.iter().map(|s| 72 + 80 * s.1.len()).sum::<usize>() + if with_syms { 24 } else { 0 };
    let mut at = 32 + cmds;
    let mut starts = vec![0; spec.sections.len()];
    for (_, idx) in &segs {
        for &i in idx {
            at = at.div_ceil(16) * 16;
            starts[i] = at;
            at += spec.sections[i].1.len();
        }
    }
    let symoff = at.div_ceil(8) * 8;
    let mut strtab = vec![b' ', 0];
    let mut syms = Vec::new();
    // n_sect is 1-based over every section in load-command order.
    let order: Vec<usize> = segs.iter().flat_map(|s| s.1.iter().copied()).collect();
    for (name, sec, off, _) in &spec.symbols {
        put32(&mut syms, strtab.len() as u32);
        strtab.extend(name.as_bytes());
        strtab.push(0);
        syms.push(0x0f); // N_SECT | N_EXT
        syms.push(order.iter().position(|&s| s == *sec).unwrap() as u8 + 1);
        put16(&mut syms, 0);
        put64(&mut syms, BASE + starts[*sec] as u64 + off);
    }
    let stroff = symoff + syms.len();
    let mut out = Vec::new();
    put32(&mut out, 0xfeed_facf);
    put32(&mut out, cputype);
    put32(&mut out, 3);
    put32(&mut out, 2); // MH_EXECUTE
    put32(&mut out, (segs.len() + usize::from(with_syms)) as u32);
    put32(&mut out, cmds as u32);
    put32(&mut out, 0);
    put32(&mut out, 0);
    for (seg, idx) in &segs {
        let lo = idx.iter().map(|&i| starts[i]).min().unwrap_or(0);
        let hi = idx
            .iter()
            .map(|&i| starts[i] + spec.sections[i].1.len())
            .max()
            .unwrap_or(0);
        put32(&mut out, 0x19);
        put32(&mut out, (72 + 80 * idx.len()) as u32);
        name16(&mut out, seg);
        put64(&mut out, BASE + lo as u64);
        put64(&mut out, (hi - lo) as u64);
        put64(&mut out, lo as u64);
        put64(&mut out, (hi - lo) as u64);
        put32(&mut out, 5);
        put32(&mut out, 5);
        put32(&mut out, idx.len() as u32);
        put32(&mut out, 0);
        for &i in idx {
            let (name, d) = &spec.sections[i];
            let sect = name.split(',').nth(1).unwrap_or(name);
            name16(&mut out, sect);
            name16(&mut out, seg);
            put64(&mut out, BASE + starts[i] as u64);
            put64(&mut out, d.len() as u64);
            put32(&mut out, starts[i] as u32);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
        }
    }
    if with_syms {
        put32(&mut out, 0x2);
        put32(&mut out, 24);
        put32(&mut out, symoff as u32);
        put32(&mut out, spec.symbols.len() as u32);
        put32(&mut out, stroff as u32);
        put32(&mut out, strtab.len() as u32);
    }
    for &i in &order {
        out.resize(starts[i], 0);
        out.extend(&spec.sections[i].1);
    }
    if with_syms {
        out.resize(symoff, 0);
        out.extend(&syms);
        out.extend(&strtab);
    }
    Layout { file: out, starts }
}

/// A fat (universal) Mach-O holding `arches`, each `(cputype, file)`, at 4 KiB boundaries.
/// Returns the file and where each arch starts.
pub fn fat(arches: &[(u32, Vec<u8>)]) -> (Vec<u8>, Vec<usize>) {
    let mut out = Vec::new();
    out.extend(0xcafe_babeu32.to_be_bytes());
    out.extend((arches.len() as u32).to_be_bytes());
    let mut at = 4096;
    let mut starts = Vec::new();
    for (cpu, f) in arches {
        starts.push(at);
        for v in [*cpu, 3, at as u32, f.len() as u32, 12] {
            out.extend(v.to_be_bytes());
        }
        at = (at + f.len()).div_ceil(4096) * 4096;
    }
    for ((_, f), s) in arches.iter().zip(&starts) {
        out.resize(*s, 0);
        out.extend(f);
    }
    (out, starts)
}
```

### Task 2.2: the Map and its Trails (src/processors/object/mod.rs)

**Files:** `src/processors/object/mod.rs`.

- [ ] **Step 1:** replace the stub's body by its tests, keeping the stub's `Map` and
  `Object` so it compiles:

```rust
#[cfg(test)]
mod tests {
    use super::fixture::{self, Spec};
    use super::*;

    fn names(t: &Trail) -> Vec<&str> {
        t.iter().map(|s| s.name.as_str()).collect()
    }

    fn spec() -> Spec {
        Spec::new()
            .section(".text", b"\x55\x48\x89\xe5\xc3\0\0\0")
            .section(".rodata", b"\0\0\0\0hello\0version=1.2.3-needle\0tail\0")
            .symbol("main.greeting", 1, 4)
            .symbol("main.version", 1, 10)
    }

    fn trail_at(file: &[u8], at: usize) -> Vec<String> {
        let m = Map::parse(file).expect("parses");
        names(&m.trail(at, &LineIndex::new(file)))
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn elf_trail_has_image_section_and_symbol_offset() {
        let l = fixture::elf(&spec());
        let at = l.offset(1, 10 + 14); // "needle" in "version=1.2.3-needle"
        assert_eq!(&l.file[at..at + 6], b"needle");
        assert_eq!(
            trail_at(&l.file, at),
            ["ELF x86-64", ".rodata", "main.version+0xe"]
        );
        // On the symbol's first byte the offset is left out.
        assert_eq!(
            trail_at(&l.file, l.offset(1, 10)),
            ["ELF x86-64", ".rodata", "main.version"]
        );
        // Before the first symbol of the section: no symbol Segment.
        assert_eq!(trail_at(&l.file, l.offset(1, 1)), ["ELF x86-64", ".rodata"]);
        // A symbol of another section never names a Match in this one.
        assert_eq!(trail_at(&l.file, l.offset(0, 2)), ["ELF x86-64", ".text"]);
        // In the ELF header: the image only.
        assert_eq!(trail_at(&l.file, 3), ["ELF x86-64"]);
    }

    /// D7: an ELF symbol whose size ends before the Match does not name it.
    #[test]
    fn an_elf_symbol_names_only_its_own_bytes() {
        let s = Spec::new()
            .section(".rodata", b"version=1.2.3\0another needle\0")
            .sized("version", 0, 0, 14);
        let l = fixture::elf(&s);
        assert_eq!(
            trail_at(&l.file, l.offset(0, 2)),
            ["ELF x86-64", ".rodata", "version+0x2"]
        );
        assert_eq!(
            trail_at(&l.file, l.offset(0, 22)),
            ["ELF x86-64", ".rodata"]
        );
    }

    #[test]
    fn a_stripped_elf_has_no_symbol_segment() {
        let mut s = spec();
        s.symbols.clear();
        let l = fixture::elf(&s);
        assert_eq!(
            trail_at(&l.file, l.offset(1, 24)),
            ["ELF x86-64", ".rodata"]
        );
    }

    #[test]
    fn elf_machines_are_named() {
        let l = fixture::elf(&spec().machine(183));
        assert_eq!(trail_at(&l.file, 0)[0], "ELF aarch64");
        let l = fixture::elf(&spec().machine(0x9999));
        assert_eq!(trail_at(&l.file, 0)[0], "ELF machine 0x9999");
    }

    #[test]
    fn pe_trail_uses_the_coff_symbols() {
        let s = Spec::new()
            .section(".text", b"\xc3\0\0\0")
            .section(".rdata", b"\0\0abc\0version=1.2.3-needle\0")
            .symbol("_version_string", 1, 6);
        let l = fixture::pe(&s);
        let at = l.offset(1, 6 + 14);
        assert_eq!(&l.file[at..at + 6], b"needle");
        assert_eq!(
            trail_at(&l.file, at),
            ["PE x86-64", ".rdata", "_version_string+0xe"]
        );
        // The padding after a section's used bytes is in no section.
        assert_eq!(trail_at(&l.file, l.offset(1, 100)), ["PE x86-64"]);
    }

    #[test]
    fn macho_sections_are_segment_comma_section() {
        let s = Spec::new()
            .section("__TEXT,__text", b"\xc3\0\0\0")
            .section("__TEXT,__cstring", b"hi\0version=1.2.3-needle\0")
            .symbol("_version", 1, 3);
        let l = fixture::macho(&s);
        let at = l.offset(1, 3 + 14);
        assert_eq!(&l.file[at..at + 6], b"needle");
        assert_eq!(
            trail_at(&l.file, at),
            ["Mach-O x86-64", "__TEXT,__cstring", "_version+0xe"]
        );
    }

    /// MinGW's section symbols (`.rdata`) and the assembler's `ltmp`/`L` labels are not
    /// symbols a person looks for.
    #[test]
    fn section_symbols_and_local_labels_are_left_out() {
        let s = Spec::new()
            .section(".rdata", b"version=1.2.3-needle\0")
            .symbol(".rdata", 0, 0)
            .symbol("version", 0, 0);
        let l = fixture::pe(&s);
        assert_eq!(trail_at(&l.file, l.offset(0, 14))[2], "version+0xe");
        let s = Spec::new()
            .section("__TEXT,__const", b"version=1.2.3-needle\0")
            .symbol("ltmp1", 0, 0)
            .symbol("_version", 0, 0);
        let l = fixture::macho(&s);
        assert_eq!(trail_at(&l.file, l.offset(0, 14))[2], "_version+0xe");
    }

    #[test]
    fn a_fat_macho_names_each_architecture() {
        let s = Spec::new()
            .section("__TEXT,__cstring", b"hi\0version=1.2.3-needle\0")
            .symbol("_version", 0, 3);
        let x86 = fixture::macho(&s);
        let arm = fixture::macho(&s.clone().machine(0x0100_000c));
        let (fat, starts) = fixture::fat(&[
            (0x0100_0007, x86.file.clone()),
            (0x0100_000c, arm.file.clone()),
        ]);
        let in_arm = starts[1] + arm.offset(0, 17);
        assert_eq!(&fat[in_arm..in_arm + 6], b"needle");
        assert_eq!(
            trail_at(&fat, in_arm),
            ["Mach-O arm64", "__TEXT,__cstring", "_version+0xe"]
        );
        let in_x86 = starts[0] + x86.offset(0, 17);
        assert_eq!(trail_at(&fat, in_x86)[0], "Mach-O x86-64");
        // The fat header belongs to no architecture.
        assert!(trail_at(&fat, 2).is_empty());
    }

    #[test]
    fn rust_symbols_are_demangled_without_their_hash() {
        assert_eq!(
            demangle("_ZN4core3fmt5write17h0123456789abcdefE"),
            "core::fmt::write"
        );
        assert_eq!(demangle("_ZN3foo3barEv"), "_ZN3foo3barEv"); // C++: kept
        assert_eq!(demangle("main.version"), "main.version");
    }

    #[test]
    fn what_goblin_cannot_read_is_none() {
        assert!(Map::parse(b"").is_none());
        assert!(Map::parse(b"\x7fELF garbage").is_none());
        assert!(Map::parse(b"MZ but not a PE file at all, just text\n").is_none());
        assert!(Map::parse(b"plain text\n").is_none());
    }

    #[test]
    fn segments_carry_the_lines_rg_a_counts() {
        let s = Spec::new()
            .section(".rodata", b"a\nb\nneedle\n")
            .symbol("s", 0, 4);
        let l = fixture::elf(&s);
        let m = Map::parse(&l.file).unwrap();
        let idx = LineIndex::new(&l.file);
        let at = l.offset(0, 4);
        let t = m.trail(at, &idx);
        let before = memchr::memchr_iter(b'\n', &l.file[..at]).count() as u64;
        assert_eq!(t[2].line, before + 1);
        assert_eq!(t[2].kind, "symbol");
        assert_eq!(t[1].kind, "section");
        assert_eq!(t[0].kind, "object");
    }

    /// Linux's `/bin/ls` is an ELF that links glibc; macOS's is a Mach-O (M4 plan r1, Fable #7).
    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_ls_binary_parses() {
        // The machine's own ELF, when there is one: a smoke test, not a fixture.
        let Ok(bytes) = std::fs::read("/bin/ls") else {
            return;
        };
        let m = Map::parse(&bytes).expect("/bin/ls parses");
        let at = memchr::memmem::find(&bytes, b"GLIBC_2.").unwrap();
        let t = m.trail(at, &LineIndex::new(&bytes));
        assert!(t[0].name.starts_with("ELF "), "{t:?}");
        assert!(t.len() >= 2, "{t:?}");
    }
}
```

- [ ] **Step 2:** `cargo test --lib processors::object::tests` does not compile: `demangle`
  does not exist (measured over the stub): RED.

- [ ] **Step 3:** the module (D3, D5, D7, D9):

```rust
//! The `object` Processor (spec §5.7): the Trail of a Match in an ELF, PE or Mach-O file is
//! `ELF x86-64 › .rodata › main.version+0x10`: the image, the section the Match's bytes are in,
//! and the closest symbol of that section at or before them. Parsed with goblin (M4 plan, D3).
#[doc(hidden)]
pub mod fixture;
pub mod rows;

use crate::model::{Input, Name, Segment, Trail};
use crate::trail::LineIndex;
use std::ops::Range;

pub struct Object;

impl super::Processor for Object {
    fn trails(
        &self,
        input: &Input,
        _params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let Some(map) = Map::parse(input.bytes) else {
            return Ok(vec![Vec::new(); anchors.len()]);
        };
        let lines = LineIndex::new(input.bytes);
        Ok(anchors.iter().map(|&a| map.trail(a, &lines)).collect())
    }

    /// Section and symbol names are not searched by `--in-headings` in M4 (M4 plan, D11).
    fn names(&self, _input: &Input, _params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(Vec::new())
    }
}

/// One section: its name, the bytes of the file it holds, and its symbols, sorted by offset,
/// one per offset.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sect {
    name: String,
    file: Range<usize>,
    symbols: Vec<Sym>,
}

/// A symbol at file offset `at`; `end` is where it ends when the table says (ELF's `st_size`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sym {
    at: usize,
    end: Option<usize>,
    name: String,
}

/// One image: a whole ELF or PE file, or one architecture of a fat Mach-O.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Image {
    label: String,
    range: Range<usize>,
    /// Sorted by start; sections without bytes in the file are left out.
    sections: Vec<Sect>,
}

/// What the `object` Processor knows of a file: its images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    images: Vec<Image>,
}

impl Map {
    /// None when goblin does not read `bytes` as an ELF, PE or Mach-O file (the file is then
    /// Plain, without a warning: M4 plan, D9).
    pub fn parse(bytes: &[u8]) -> Option<Map> {
        let images = match goblin::Object::parse(bytes).ok()? {
            goblin::Object::Elf(e) => vec![elf(&e, bytes.len())],
            goblin::Object::PE(p) => vec![pe(&p, bytes)],
            goblin::Object::Mach(goblin::mach::Mach::Binary(m)) => vec![macho(&m, 0..bytes.len())],
            goblin::Object::Mach(goblin::mach::Mach::Fat(f)) => {
                let mut v = Vec::new();
                for (i, arch) in f.iter_arches().enumerate() {
                    let arch = arch.ok()?;
                    let start = arch.offset as usize;
                    let end = start.checked_add(arch.size as usize)?.min(bytes.len());
                    if let Ok(goblin::mach::SingleArch::MachO(m)) = f.get(i) {
                        let mut img = macho(&m, start..end);
                        for s in &mut img.sections {
                            s.file = s.file.start + start..s.file.end + start;
                            for sym in &mut s.symbols {
                                sym.at += start;
                                sym.end = sym.end.map(|e| e + start);
                            }
                        }
                        v.push(img);
                    }
                }
                v
            }
            _ => return None,
        };
        let mut images: Vec<Image> = images
            .into_iter()
            .map(|mut img| {
                img.sections.retain(|s| {
                    s.file.start < s.file.end
                        && s.file.end <= img.range.end.max(s.file.end).min(bytes.len())
                });
                img.sections
                    .sort_by_key(|s| (s.file.start, std::cmp::Reverse(s.file.end)));
                img
            })
            .collect();
        images.sort_by_key(|i| i.range.start);
        Some(Map { images })
    }

    /// The Trail of the byte at file offset `at`: the image, the section and the symbol, each
    /// present when `at` is inside one.
    pub fn trail(&self, at: usize, lines: &LineIndex) -> Trail {
        let mut t = Vec::new();
        let Some(img) = self.images.iter().find(|i| i.range.contains(&at)) else {
            return t;
        };
        t.push(segment("object", &img.label, &img.range, lines));
        // The last section that starts at or before `at` and holds it.
        let Some(sect) = img
            .sections
            .iter()
            .rev()
            .find(|s| s.file.start <= at && at < s.file.end)
        else {
            return t;
        };
        t.push(segment("section", &sect.name, &sect.file, lines));
        let i = sect.symbols.partition_point(|s| s.at <= at);
        // The closest symbol at or before `at`, unless its size ends before `at` (D7).
        if let Some(sym) = i.checked_sub(1).map(|i| &sect.symbols[i])
            && sym.end.is_none_or(|e| at < e)
        {
            let next = sect.symbols.get(i).map_or(sect.file.end, |s| s.at);
            let end = sym.end.map_or(next, |e| e.min(next).max(sym.at + 1));
            let shown = if at == sym.at {
                sym.name.clone()
            } else {
                format!("{}+0x{:x}", sym.name, at - sym.at)
            };
            t.push(segment("symbol", &shown, &(sym.at..end), lines));
        }
        t
    }
}

fn segment(kind: &str, name: &str, range: &Range<usize>, lines: &LineIndex) -> Segment {
    Segment {
        kind: kind.into(),
        name: name.into(),
        line: lines.line_of(range.start),
        end_line: lines.line_of(range.end.saturating_sub(1).max(range.start)),
        level: None,
        signature: None,
    }
}

/// A Rust symbol demangled without its hash (`{:#}`); any other name as the table has it.
fn demangle(name: &str) -> String {
    match rustc_demangle::try_demangle(name) {
        Ok(d) => format!("{d:#}"),
        Err(_) => name.to_string(),
    }
}

/// A symbol as a table gives it: the index of its section in `sections`, its address minus the
/// section's address, its size when known (and not zero), and its name.
type RawSym = (usize, u64, Option<u64>, String);

/// Adds the symbols to the sections they fall in.
fn place(sections: &mut [Sect], syms: Vec<RawSym>) {
    for (i, delta, size, name) in syms {
        let Some(s) = sections.get_mut(i) else {
            continue;
        };
        let Ok(delta) = usize::try_from(delta) else {
            continue;
        };
        let Some(at) = s.file.start.checked_add(delta) else {
            continue;
        };
        if name.is_empty() || at >= s.file.end {
            continue;
        }
        let end = size
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| at.checked_add(n));
        s.symbols.push(Sym {
            at,
            end,
            name: demangle(&name),
        });
    }
    for s in sections {
        // Stable: of the symbols at one offset, the first in the symbol tables is kept.
        s.symbols.sort_by_key(|s| s.at);
        s.symbols.dedup_by_key(|s| s.at);
    }
}

fn elf_machine(m: u16, is_64: bool) -> String {
    use goblin::elf::header::*;
    match m {
        EM_X86_64 => "x86-64".into(),
        EM_386 => "i386".into(),
        EM_AARCH64 => "aarch64".into(),
        EM_ARM => "arm".into(),
        EM_RISCV if is_64 => "riscv64".into(),
        EM_RISCV => "riscv32".into(),
        EM_PPC64 => "ppc64".into(),
        EM_PPC => "ppc".into(),
        EM_MIPS => "mips".into(),
        EM_S390 => "s390x".into(),
        258 => "loongarch64".into(),
        m => format!("machine 0x{m:x}"),
    }
}

fn elf(e: &goblin::elf::Elf, len: usize) -> Image {
    use goblin::elf::section_header::{SHT_NOBITS, SHT_NULL};
    let mut sections = Vec::new();
    // ELF section index → index in `sections`.
    let mut index = vec![usize::MAX; e.section_headers.len()];
    let mut addr = vec![0u64; e.section_headers.len()];
    for (i, sh) in e.section_headers.iter().enumerate() {
        if sh.sh_type == SHT_NULL || sh.sh_type == SHT_NOBITS || sh.sh_size == 0 {
            continue;
        }
        let start = sh.sh_offset as usize;
        let end = start.saturating_add(sh.sh_size as usize).min(len);
        let name = e.shdr_strtab.get_at(sh.sh_name).unwrap_or("").to_string();
        index[i] = sections.len();
        addr[i] = sh.sh_addr;
        sections.push(Sect {
            name,
            file: start..end,
            symbols: Vec::new(),
        });
    }
    let mut syms = Vec::new();
    for (tab, strs) in [(&e.syms, &e.strtab), (&e.dynsyms, &e.dynstrtab)] {
        for s in tab.iter() {
            use goblin::elf::sym::{STT_FILE, STT_SECTION};
            let sh = s.st_shndx;
            if s.st_type() == STT_SECTION || s.st_type() == STT_FILE || sh >= index.len() {
                continue;
            }
            if index[sh] == usize::MAX || s.st_value < addr[sh] {
                continue;
            }
            let name = strs.get_at(s.st_name).unwrap_or("").to_string();
            let size = (s.st_size > 0).then_some(s.st_size);
            syms.push((index[sh], s.st_value - addr[sh], size, name));
        }
    }
    place(&mut sections, syms);
    Image {
        label: format!("ELF {}", elf_machine(e.header.e_machine, e.is_64)),
        range: 0..len,
        sections,
    }
}

fn pe(p: &goblin::pe::PE, bytes: &[u8]) -> Image {
    let machine = match p.header.coff_header.machine {
        0x8664 => "x86-64".to_string(),
        0x14c => "i386".into(),
        0xaa64 => "aarch64".into(),
        0x1c0 | 0x1c4 => "arm".into(),
        m => format!("machine 0x{m:x}"),
    };
    let mut sections = Vec::new();
    let mut rvas = Vec::new();
    for s in &p.sections {
        let start = s.pointer_to_raw_data as usize;
        // The raw data is padded to the file alignment; the virtual size says what is used.
        let used = if s.virtual_size > 0 {
            s.virtual_size.min(s.size_of_raw_data)
        } else {
            s.size_of_raw_data
        };
        let end = start.saturating_add(used as usize).min(bytes.len());
        sections.push(Sect {
            name: s.name().unwrap_or("").to_string(),
            file: start..end,
            symbols: Vec::new(),
        });
        rvas.push((
            s.virtual_address as u64,
            s.virtual_size.max(s.size_of_raw_data) as u64,
        ));
    }
    let mut syms = Vec::new();
    for x in &p.exports {
        let (Some(name), rva) = (x.name, x.rva as u64) else {
            continue;
        };
        if let Some(i) = rvas.iter().position(|(a, n)| *a <= rva && rva < a + n) {
            syms.push((i, rva - rvas[i].0, None, name.to_string()));
        }
    }
    // A COFF symbol table, which MinGW leaves in images that were not stripped.
    let coff = &p.header.coff_header;
    if let (Ok(Some(table)), Ok(Some(strings))) = (coff.symbols(bytes), coff.strings(bytes)) {
        for (_, _, s) in table.iter() {
            // External and static symbols, except the static ones that define a section
            // (`.rdata`, with an auxiliary record) and `.`-names.
            if s.section_number <= 0 || s.storage_class != 2 && s.storage_class != 3 {
                continue;
            }
            if s.is_section_definition() {
                continue;
            }
            if let Ok(name) = s.name(&strings)
                && !name.starts_with('.')
            {
                syms.push((
                    s.section_number as usize - 1,
                    s.value as u64,
                    None,
                    name.to_string(),
                ));
            }
        }
    }
    place(&mut sections, syms);
    Image {
        label: format!("PE {machine}"),
        range: 0..bytes.len(),
        sections,
    }
}

fn macho_cpu(cpu: u32) -> String {
    match cpu {
        0x0100_0007 => "x86-64".into(),
        7 => "i386".into(),
        0x0100_000c => "arm64".into(),
        0x0200_000c => "arm64_32".into(),
        12 => "arm".into(),
        0x0100_0012 => "ppc64".into(),
        18 => "ppc".into(),
        c => format!("cpu 0x{c:x}"),
    }
}

/// `range` is the slice of the file this Mach-O is; its offsets are relative to it.
fn macho(m: &goblin::mach::MachO, range: Range<usize>) -> Image {
    let len = range.len();
    let mut sections = Vec::new();
    let mut addrs = Vec::new();
    for seg in m.segments.iter() {
        let Ok(list) = seg.sections() else { continue };
        for (s, _) in list {
            const S_ZEROFILL: u32 = 0x1;
            const S_GB_ZEROFILL: u32 = 0xc;
            const S_THREAD_LOCAL_ZEROFILL: u32 = 0x12;
            let zero = matches!(
                s.flags & 0xff,
                S_ZEROFILL | S_GB_ZEROFILL | S_THREAD_LOCAL_ZEROFILL
            );
            let start = s.offset as usize;
            let end = if zero || start == 0 {
                start
            } else {
                start.saturating_add(s.size as usize).min(len)
            };
            let name = format!("{},{}", s.segname().unwrap_or(""), s.name().unwrap_or(""));
            // n_sect counts every section, also the ones without bytes in the file.
            addrs.push(s.addr);
            sections.push(Sect {
                name,
                file: start..end,
                symbols: Vec::new(),
            });
        }
    }
    let mut syms = Vec::new();
    for (name, nl) in m.symbols().flatten() {
        const N_STAB: u8 = 0xe0;
        const N_TYPE: u8 = 0x0e;
        const N_SECT: u8 = 0x0e;
        if nl.n_type & N_STAB != 0 || nl.n_type & N_TYPE != N_SECT || nl.n_sect == 0 {
            continue;
        }
        // `l…` and `L…` are the assembler's local labels (`ltmp1`, `L_.str`), not symbols.
        if name.starts_with('l') || name.starts_with('L') {
            continue;
        }
        let i = nl.n_sect - 1;
        let Some(&a) = addrs.get(i) else { continue };
        if nl.n_value < a {
            continue;
        }
        syms.push((i, nl.n_value - a, None, name.to_string()));
    }
    place(&mut sections, syms);
    Image {
        label: format!("Mach-O {}", macho_cpu(m.header.cputype)),
        range,
        sections,
    }
}
```

- [ ] **Step 4:** `cargo test --lib processors::object::tests`: 12 tests pass.
  `the_real_ls_binary_parses` reads `/bin/ls` when the machine has one and passes vacuously
  otherwise; it is a smoke test, not a fixture, and runs on Linux only: macOS's `/bin/ls` is a
  Mach-O without `GLIBC_2.`, and M5 brings CI to macOS (review r1, Fable #7).

### Task 2.3: the rows (src/processors/object/rows.rs)

**Files:** `src/processors/object/rows.rs`.

- [ ] **Step 1:** the tests, below the stub:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Each row's text, submatches and offset.
    type Shown = Vec<(String, Vec<(usize, usize)>, u64)>;

    fn shown(bytes: &[u8], hits: &[Range<usize>]) -> Shown {
        rows(bytes, hits, None, &LineIndex::new(bytes))
            .into_iter()
            .map(|l| {
                (
                    String::from_utf8_lossy(l.owned.as_deref().unwrap()).into_owned(),
                    l.submatches.iter().map(|s| (s.start, s.end)).collect(),
                    l.absolute_offset,
                )
            })
            .collect()
    }

    fn find(hay: &[u8], needle: &[u8]) -> Vec<Range<usize>> {
        memchr::memmem::find_iter(hay, needle)
            .map(|i| i..i + needle.len())
            .collect()
    }

    #[test]
    fn a_row_is_the_printable_string_around_its_matches() {
        let b = b"\x01\x02version=1.2.3-needle\0\xffneedle and needle\0";
        let r = shown(b, &find(b, b"needle"));
        assert_eq!(
            r,
            [
                ("version=1.2.3-needle".into(), vec![(14, 20)], 2),
                ("needle and needle".into(), vec![(0, 6), (11, 17)], 24),
            ]
        );
    }

    /// Codex r2 #5: Matches more than `SIDE` apart in one string are one row, even with -m1.
    #[test]
    fn a_long_string_is_one_row_whatever_its_matches_distance() {
        let mut b = b"\0needle".to_vec();
        b.extend(vec![b'a'; 5000]);
        b.extend(b"needle\0");
        let hits = find(&b, b"needle");
        let r = rows(&b, &hits, Some(1), &LineIndex::new(&b));
        assert_eq!(r.len(), 1);
        let subs: Vec<(usize, usize)> = r[0].submatches.iter().map(|s| (s.start, s.end)).collect();
        assert_eq!(
            (r[0].absolute_offset, subs),
            (1, vec![(0, 6), (5006, 5012)])
        );
    }

    #[test]
    fn control_bytes_inside_a_match_print_as_dots() {
        let b = b"ab\0cd";
        let r = shown(b, std::slice::from_ref(&(1..4)));
        assert_eq!(r, [("ab.cd".into(), vec![(1, 4)], 0)]);
    }

    #[test]
    fn utf8_text_is_printable_and_stray_high_bytes_are_not() {
        let b = "\u{1}año needle\u{0}".as_bytes();
        let r = shown(b, &find(b, b"needle"));
        assert_eq!(r[0].0, "año needle");
        let b = b"\x80\xfeneedle\x9f";
        assert_eq!(shown(b, &find(b, b"needle"))[0].0, "needle");
    }

    #[test]
    fn long_strings_are_cut_at_side_bytes_each_way() {
        let mut b = vec![b'x'; SIDE * 3];
        b.extend(b"needle");
        b.extend(vec![b'y'; SIDE * 3]);
        let r = rows(&b, &find(&b, b"needle"), None, &LineIndex::new(&b));
        assert_eq!(r[0].len, SIDE * 2 + 6);
        assert_eq!(r[0].submatches[0].start, SIDE);
    }

    #[test]
    fn rows_count_toward_the_limit_and_number_lines_as_rg_a() {
        let b = b"one needle\0two\nneedle\0three needle\0";
        let li = LineIndex::new(b);
        let r = rows(b, &find(b, b"needle"), Some(2), &li);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].line_number, r[1].line_number), (1, 2));
    }

    #[test]
    fn an_empty_match_makes_a_row_of_its_string() {
        let b = b"\0abc\0";
        let r = shown(b, std::slice::from_ref(&(2..2)));
        assert_eq!(r, [("abc".into(), vec![(1, 1)], 1)]);
    }
}
```

- [ ] **Step 2:** `cargo test --lib processors::object::rows`: the 7 tests FAIL (no rows): RED.
  `a_long_string_is_one_row_whatever_its_matches_distance` also failed over the round-1 code,
  which grouped Matches by the 4096-byte window and so cut one string into two rows (review r2,
  Codex #5).

- [ ] **Step 3:** the rows (D6): a Match joins the row before it when the bytes from that row's
  last Match to it are printable, however many:

```rust
//! The rows of an object file (spec §5.7): each printable string that holds Matches, as
//! `strings` reads it, located by the hex offset of its first Match (M4 plan, D6).
use crate::model::{Line, Submatch};
use crate::trail::LineIndex;
use std::ops::Range;

/// The most bytes a row shows on each side of its Matches; longer strings are cut there (the
/// Fragments of spec §6.3 then fit the row to the width).
pub const SIDE: usize = 4096;

/// The end of the printable character that starts at `i`: printable ASCII, a tab, or a valid
/// UTF-8 sequence of a character that is not a control.
fn char_after(b: &[u8], i: usize) -> Option<usize> {
    let c = *b.get(i)?;
    if c == b'\t' || (0x20..0x7f).contains(&c) {
        return Some(i + 1);
    }
    let w = match c {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return None,
    };
    let s = std::str::from_utf8(b.get(i..i + w)?).ok()?;
    (!s.chars().next()?.is_control()).then_some(i + w)
}

/// The start of the printable character that ends at `i` (exclusive), as `char_after` reads it.
fn char_before(b: &[u8], i: usize) -> Option<usize> {
    (1..=4.min(i)).find_map(|w| (char_after(b, i - w) == Some(i)).then_some(i - w))
}

/// The rows of `hits` (absolute byte ranges of Matches in `bytes`, in file order): one `Line`
/// per printable string, its `owned` bytes the string with each control byte of a Match shown
/// as `.`, its submatches relative to the string, and `line_number` the line `rg -a` would
/// give it. At most `limit` rows (`-m`).
pub fn rows(
    bytes: &[u8],
    hits: &[Range<usize>],
    limit: Option<u64>,
    lines: &LineIndex,
) -> Vec<Line> {
    let mut out: Vec<(Range<usize>, Vec<Range<usize>>)> = Vec::new();
    for h in hits {
        let h = h.start.min(bytes.len())..h.end.min(bytes.len());
        // A Match joins the row before it when it starts inside the row's window, or when the
        // bytes from the row's last Match to it are printable: one string is one row, however
        // far apart its Matches are (M4 plan r2, Codex #5).
        if let Some((w, subs)) = out.last_mut()
            && h.start >= w.start
            && (h.start < w.end.max(w.start + 1)
                || printable(bytes, subs.last().map_or(w.start, |s| s.end), h.start))
        {
            if h.end > w.end {
                w.end = extend_right(bytes, h.end);
            }
            subs.push(h);
            continue;
        }
        if limit.is_some_and(|m| out.len() as u64 >= m) {
            break;
        }
        let mut start = h.start;
        while h.start - start < SIDE
            && let Some(s) = char_before(bytes, start)
        {
            start = s;
        }
        // A row never starts inside the previous one.
        if let Some((w, _)) = out.last() {
            start = start.max(w.end);
        }
        out.push((start..extend_right(bytes, h.end), vec![h]));
    }
    out.into_iter()
        .map(|(w, subs)| {
            let mut owned = bytes[w.clone()].to_vec();
            for s in &subs {
                for b in &mut owned[s.start - w.start..s.end - w.start] {
                    if *b < 0x20 && *b != b'\t' || *b == 0x7f {
                        *b = b'.';
                    }
                }
            }
            Line {
                line_number: lines.line_of(subs[0].start),
                absolute_offset: w.start as u64,
                len: w.len(),
                owned: Some(owned.into()),
                submatches: subs
                    .iter()
                    .map(|s| Submatch {
                        start: s.start - w.start,
                        end: s.end - w.start,
                    })
                    .collect(),
            }
        })
        .collect()
}

/// Whether `bytes[from..to]` is printable characters only.
fn printable(bytes: &[u8], from: usize, to: usize) -> bool {
    let mut i = from;
    while i < to {
        match char_after(bytes, i) {
            Some(e) if e <= to => i = e,
            _ => return false,
        }
    }
    true
}

fn extend_right(bytes: &[u8], from: usize) -> usize {
    let mut end = from;
    while end - from < SIDE
        && let Some(e) = char_after(bytes, end)
    {
        end = e;
    }
    end
}
```

- [ ] **Step 4:** GREEN, `cargo clippy`, `cargo fmt`, commit.
## Chunk 3: `--pre`, `--pre-glob` and `-z`

### Task 3.1: src/preprocess.rs

**Files:** `src/preprocess.rs`.

- [ ] **Step 1:** the tests, below the stub:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn pre(args: &[&str]) -> Preprocess {
        let mut v = vec![OsString::from("sfor"), OsString::from("x")];
        v.extend(args.iter().map(OsString::from));
        Preprocess::new(&crate::cli::parse(v).unwrap(), Path::new("/")).unwrap()
    }

    #[test]
    fn pre_globs_choose_the_files_and_z_takes_the_rest() {
        let p = pre(&["--pre", "cat", "--pre-glob", "*.pdf"]);
        assert_eq!(p.through(Path::new("/a/b.pdf")), Some(Through::Pre));
        assert_eq!(p.through(Path::new("/a/b.md")), None);
        let p = pre(&["--pre", "cat", "--pre-glob", "!*.pdf"]);
        assert_eq!(p.through(Path::new("/a/b.pdf")), None);
        assert_eq!(p.through(Path::new("/a/b.md")), Some(Through::Pre));
        let p = pre(&["-z"]);
        assert_eq!(p.through(Path::new("/a/b.txt.gz")), Some(Through::Zip));
        assert_eq!(p.through(Path::new("/a/b.txt")), None);
        // The last of -z and --pre wins.
        assert_eq!(
            pre(&["-z", "--pre", "cat"]).through(Path::new("/a.gz")),
            Some(Through::Pre)
        );
        assert_eq!(
            pre(&["--pre", "cat", "-z"]).through(Path::new("/a.gz")),
            Some(Through::Zip)
        );
        assert_eq!(pre(&["--pre", ""]).through(Path::new("/a")), None);
        assert_eq!(
            pre(&["--pre", "cat", "--no-pre"]).through(Path::new("/a")),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn pre_runs_the_command_with_the_path_and_the_file_on_stdin() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.txt");
        std::fs::write(&f, "abc\n").unwrap();
        let p = pre(&["--pre", "rev"]);
        let mut out = String::new();
        p.open(&Through::Pre, &f)
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        assert_eq!(out, "cba\n");
        let p = pre(&["--pre", "/nonexistent/cmd"]);
        let e = p.open(&Through::Pre, &f).err().unwrap().to_string();
        assert!(
            e.starts_with("preprocessor command could not start: '\"/nonexistent/cmd\" "),
            "{e}"
        );
    }
}
```

- [ ] **Step 2:** `cargo test --lib preprocess`: both FAIL (`through` is None, `open` fails):
  RED.

- [ ] **Step 3:** the module (D2, D21). ripgrep 14.1.1's messages come from its search worker:
  `preprocessor command could not start: '<Command as Debug>': <error>` when the command cannot
  be spawned, `preprocessor command failed: '<Command as Debug>': <error>` around a read error,
  which `grep-cli`'s `CommandReader` gives at the end of the output of a command that failed,
  with its stderr between two rules:

```rust
//! What a file is searched through (M4 plan, design note 3): its own bytes, the output of
//! ripgrep's `--pre` command, its decompressor's output with `-z`, or the text an Extractor
//! takes from it with `--docs`. `--pre` and `-z` behave as in ripgrep 14.1.1 (crates/core/
//! search.rs): `--pre` first, when its globs allow it, then `-z`; stdin is never transformed.
//! `--docs` comes last, so a file `--pre` or `-z` takes is never extracted (M4 plan, D21).
use grep_cli::{CommandReader, CommandReaderBuilder, DecompressionReaderBuilder};
use std::io::Read;
use std::path::{Path, PathBuf};

/// How one file is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Through {
    /// The output of `--pre COMMAND PATH`.
    Pre,
    /// The output of the decompressor `-z` picks for the file's extension.
    Zip,
    /// The text of an Extractor: the document Format's name and its Extractor's (`extractor` in
    /// its Definition, a `docs::Kind` name).
    Docs(String, String),
}

/// The run's `--pre` and `-z`.
pub struct Preprocess {
    pre: Option<PathBuf>,
    /// `--pre-glob`: None when no glob was given (every file goes through `--pre`).
    globs: Option<ignore::overrides::Override>,
    zip: Option<DecompressionReaderBuilder>,
    command: CommandReaderBuilder,
}

impl Preprocess {
    pub fn new(args: &crate::cli::Args, cwd: &Path) -> anyhow::Result<Self> {
        let globs = if args.pre_globs.is_empty() {
            None
        } else {
            let mut b = ignore::overrides::OverrideBuilder::new(cwd);
            for g in &args.pre_globs {
                b.add(g)?;
            }
            Some(b.build()?)
        };
        let mut command = CommandReaderBuilder::new();
        command.async_stderr(true);
        let zip = args.search_zip.then(|| {
            let mut z = DecompressionReaderBuilder::new();
            z.async_stderr(true);
            z
        });
        Ok(Preprocess {
            pre: args.pre.clone().map(PathBuf::from),
            globs,
            zip,
            command,
        })
    }

    /// How the file at `path` is read, when not as it is. Never for stdin.
    pub fn through(&self, path: &Path) -> Option<Through> {
        if self.pre.is_some()
            && self
                .globs
                .as_ref()
                .is_none_or(|g| !g.matched(path, false).is_ignore())
        {
            return Some(Through::Pre);
        }
        if let Some(z) = &self.zip
            && z.get_matcher().has_command(path)
        {
            return Some(Through::Zip);
        }
        None
    }

    /// The reader of a file `through` says to transform (`Pre` or `Zip`). Its errors are
    /// ripgrep's: `preprocessor command could not start: '<command>': <error>` and, while it is
    /// read, `preprocessor command failed: '<command>': <error and stderr>`.
    pub fn open(&self, how: &Through, path: &Path) -> std::io::Result<Box<dyn Read + Send>> {
        match how {
            Through::Pre => {
                let bin = self.pre.as_deref().expect("Through::Pre needs --pre");
                let mut cmd = std::process::Command::new(bin);
                cmd.arg(path)
                    .stdin(std::process::Stdio::from(std::fs::File::open(path)?));
                let shown = format!("{cmd:?}");
                let reader = self.command.build(&mut cmd).map_err(|e| {
                    std::io::Error::other(format!(
                        "preprocessor command could not start: '{shown}': {e}"
                    ))
                })?;
                Ok(Box::new(PreReader {
                    inner: reader,
                    cmd: shown,
                }))
            }
            Through::Zip => {
                let z = self.zip.as_ref().expect("Through::Zip needs -z");
                Ok(Box::new(z.build(path).map_err(std::io::Error::from)?))
            }
            Through::Docs(..) => unreachable!("documents are extracted, not read"),
        }
    }
}

/// `--pre`'s output, whose errors name the command (the stderr of a command that fails comes
/// at the end of its output, when `CommandReader` closes it).
struct PreReader {
    inner: CommandReader,
    cmd: String,
}

impl Read for PreReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf).map_err(|e| {
            std::io::Error::other(format!("preprocessor command failed: '{}': {e}", self.cmd))
        })
    }
}
```

- [ ] **Step 4:** GREEN. The end-to-end comparison with rg 14.1.1, stderr included, is C11's
  `m4_08`.
## Chunk 4: the shared modules of `docs`

### Task 4.1: the Builder (src/docs/mod.rs)

**Files:** `src/lib.rs`, `src/docs/mod.rs`.

- [ ] **Step 1:** in `src/lib.rs`, after `pub mod cli;`:

```rust
pub mod docs;
```

- [ ] **Step 2:** the tests of `docs/mod.rs`, written first into a file that holds only them and
  the `mod` lines of the module's head (the Builder does not exist yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Fable r1 #12: a document is told by the bytes its Format's magic looks at, by name or
    /// not.
    #[test]
    fn documents_look_like_their_magic() {
        let defs = crate::formats::builtin::BUILTIN;
        for (kind, name, head) in [
            (Kind::Pdf, "pdf", &b"%PDF-1.7\n"[..]),
            (Kind::Sqlite, "sqlite", b"SQLite format 3\0"),
            (Kind::Docx, "docx", b"PK\x03\x04"),
        ] {
            let file = format!("{name}.toml");
            let def = defs.iter().find(|d| d.0 == file).map(|d| d.1).unwrap();
            let def = crate::formats::definition::FormatDefinition::parse(def, |_| true).unwrap();
            assert!(kind.looks_like(head), "{name}");
            assert!(def.magic.is_empty() || def.magic_at(head), "{name}");
            let late = [b"junk ".as_slice(), head].concat();
            assert!(!kind.looks_like(&late), "{name}");
        }
    }
    use crate::trail::trail_for;

    fn trail(e: &Extracted, needle: &str) -> Vec<String> {
        let at = memchr::memmem::find(&e.text, needle.as_bytes()).expect(needle);
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn headings_nest_by_level_and_pages_hold_them() {
        let mut b = Builder::new(MAX_TEXT);
        let p = b.open("page", "page 1");
        b.heading(1, "Intro");
        b.line("first needle");
        b.heading(2, "Detail");
        b.line("second needle");
        b.heading(1, "Costs");
        b.line("third needle");
        b.close(p);
        let e = b.finish();
        assert_eq!(
            String::from_utf8(e.text.clone()).unwrap(),
            "Intro\nfirst needle\nDetail\nsecond needle\nCosts\nthird needle\n"
        );
        assert_eq!(trail(&e, "first"), ["page 1", "Intro"]);
        assert_eq!(trail(&e, "second"), ["page 1", "Intro", "Detail"]);
        assert_eq!(trail(&e, "third"), ["page 1", "Costs"]);
        // On the heading line itself, the heading is left out (its label).
        assert_eq!(trail(&e, "Detail"), ["page 1", "Intro"]);
        let names = crate::trail::names_of(&e.outline);
        assert_eq!(
            names.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
            ["Intro", "Detail", "Costs"]
        );
    }

    #[test]
    fn rows_are_lines_of_tab_separated_cells_with_column_segments() {
        let mut b = Builder::new(MAX_TEXT);
        let s = b.open("sheet", "Budget");
        b.start_row(None);
        b.cell("cell", "A1", "Item");
        b.cell("cell", "B1", "Price");
        b.start_row(None);
        b.cell("cell", "A2", "pens needle");
        b.cell("cell", "B2", "1.50");
        b.start_row(None);
        b.cell("cell", "A3", "");
        b.cell("cell", "B3", " ");
        b.end_row();
        b.close(s);
        let t = b.open("table", "table 1");
        b.start_row(Some("row 1"));
        b.cell("column", "Name", "Ana");
        b.cell("column", "Note", "");
        b.cell("column", "Plan", "pro needle");
        b.close(t);
        let e = b.finish();
        assert_eq!(
            String::from_utf8(e.text.clone()).unwrap(),
            "Item\tPrice\npens needle\t1.50\nAna\t\tpro needle\n"
        );
        assert_eq!(trail(&e, "needle"), ["Budget", "A2"]);
        assert_eq!(trail(&e, "1.50"), ["Budget", "B2"]);
        assert_eq!(trail(&e, "pro"), ["table 1", "row 1", "Plan"]);
        // The tabs after a cell belong to that cell, also an empty cell's.
        let ana = memchr::memmem::find(&e.text, b"Ana").unwrap();
        let t: Vec<_> = trail_for(&e.outline, ana + 4)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(t, ["table 1", "row 1", "Name"]);
        // The Section of a cell is its row.
        let pens = memchr::memmem::find(&e.text, b"pens").unwrap();
        let col = trail_for(&e.outline, pens).pop().unwrap();
        assert_eq!((col.line, col.end_line), (2, 2));
    }

    #[test]
    fn chapters_hold_pages_and_headings_and_close_by_level() {
        let mut b = Builder::new(MAX_TEXT);
        b.chapter(1, "1 Intro");
        let p = b.open("page", "page 1");
        b.line("alpha needle");
        b.close(p);
        b.chapter(2, "1.1 Scope");
        let p = b.open("page", "page 2");
        b.line("beta needle");
        b.close(p);
        b.chapter(1, "2 Costs");
        b.heading(1, "Totals");
        b.line("gamma needle");
        b.heading(1, "Taxes");
        b.line("delta needle");
        b.chapter(1, "3 End");
        b.line("omega needle");
        let e = b.finish();
        assert_eq!(trail(&e, "alpha"), ["1 Intro", "page 1"]);
        assert_eq!(trail(&e, "beta"), ["1 Intro", "1.1 Scope", "page 2"]);
        assert_eq!(trail(&e, "gamma"), ["2 Costs", "Totals"]);
        assert_eq!(trail(&e, "delta"), ["2 Costs", "Taxes"]);
        assert_eq!(trail(&e, "omega"), ["3 End"]);
        // Chapter titles are not in the text, so `--in-headings` does not search them.
        let names = crate::trail::names_of(&e.outline);
        assert_eq!(
            names.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
            ["Totals", "Taxes"]
        );
    }

    /// Codex r1 #12: every cell writes its tab, also an empty one before the first value, and
    /// a value keeps its spaces.
    #[test]
    fn empty_cells_keep_their_tabs_and_values_their_spaces() {
        let mut b = Builder::new(MAX_TEXT);
        b.start_row(Some("row 1"));
        b.cell("column", "a", "");
        b.cell("column", "b", "");
        b.cell("column", "c", "  needle ");
        b.cell("column", "d", "");
        b.start_row(Some("row 2"));
        b.cell("column", "a", " ");
        b.cell("column", "b", "");
        b.end_row();
        let e = b.finish();
        assert_eq!(e.text, b"\t\t  needle \t\n");
        assert_eq!(trail(&e, "needle"), ["row 1", "c"]);
        let c = e
            .outline
            .segments
            .iter()
            .find(|s| s.segment.name == "c")
            .unwrap();
        // Up to the line break: the tabs of the empty cell after it are its own.
        assert_eq!(c.range, 2..13);
    }

    #[test]
    fn control_characters_become_spaces_and_empty_lines_are_skipped() {
        let mut b = Builder::new(MAX_TEXT);
        b.line("a\tb\0c\r");
        b.line("   ");
        b.line("d\ne");
        let e = b.finish();
        assert_eq!(e.text, b"a b c\nd e\n");
    }

    #[test]
    fn the_text_stops_at_its_limit_and_says_so() {
        let mut b = Builder::new(11);
        b.line("12345");
        b.line("6789");
        b.line("more");
        assert!(b.full());
        let e = b.finish();
        assert_eq!(e.text, b"12345\n6789\n");
        assert!(e.cut);
    }

    #[test]
    fn column_letters_are_a_spreadsheets() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(701), "ZZ");
        assert_eq!(column_letters(702), "AAA");
    }
}
```

  `cargo test --lib docs::tests` does not compile: RED. Then the module (design note 4). The
  Builder gives an Extractor what is left of the text limit (`remaining`) and lets it say it
  stopped there (`mark_cut`), which the PDF Extractor needs to stop drawing a page (D17); a
  row writes its cells' tabs and spaces as they are (D25); `Kind::looks_like` tests the bytes
  the magic of each Format tests (D10):

```rust
//! The Extractors of `--docs` (spec §5.8): each turns a document ripgrep cannot read into text
//! and an Outline of that text, which the pipeline then searches and describes like a file
//! (M4 plan, design note 4). No external programs: lopdf for PDF, zip and an XML reader of our
//! own (`xml`, D12) for the Office, OpenDocument and EPUB formats, a reader of our own for SQLite.
pub mod epub;
#[doc(hidden)]
pub mod fixture;
pub mod odf;
pub mod ooxml;
pub mod pdf;
pub mod sqlite;
pub mod table;
pub mod xml;
pub mod zipped;

use crate::trail::{Outline, OutlineSegment};

/// The most text one document may extract to (512 MiB): past it the text is cut, with a
/// warning (M4 plan, D17).
pub const MAX_TEXT: usize = 512 << 20;

/// The document Formats, named by `extractor` in their Definition's `[params]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Odt,
    Ods,
    Odp,
    Epub,
    Sqlite,
}

impl Kind {
    pub fn from_name(name: &str) -> Option<Kind> {
        Some(match name {
            "pdf" => Kind::Pdf,
            "docx" => Kind::Docx,
            "xlsx" => Kind::Xlsx,
            "pptx" => Kind::Pptx,
            "odt" => Kind::Odt,
            "ods" => Kind::Ods,
            "odp" => Kind::Odp,
            "epub" => Kind::Epub,
            "sqlite" => Kind::Sqlite,
            _ => return None,
        })
    }

    /// Whether `head`, the first bytes of a file, can be a document of this kind: `%PDF-`, a
    /// zip's local header, SQLite's header string, each where the magic bytes of its Format
    /// look for it, at the start, so a document is one whatever its name (M4 plan r1, Fable
    /// #12). A file whose name
    /// says it is a document but whose bytes do not is not one, and is searched as ripgrep
    /// searches it, without a warning (M4 plan, D10).
    pub fn looks_like(self, head: &[u8]) -> bool {
        match self {
            Kind::Pdf => head.starts_with(b"%PDF-"),
            Kind::Sqlite => head.starts_with(b"SQLite format 3\0"),
            _ => head.starts_with(b"PK\x03\x04"),
        }
    }
}

/// Why a document gave no text. Each is one warning, `sfor: <path>: <format>: <reason>`, and
/// the file is then searched as ripgrep searches it (M4 plan, D10).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocError {
    #[error("encrypted, and the empty password does not open it")]
    Encrypted,
    #[error("{0}")]
    Broken(String),
    #[error("{0} is over the {1} MiB limit")]
    TooLarge(String, usize),
}

impl DocError {
    pub fn broken(e: impl std::fmt::Display) -> Self {
        DocError::Broken(e.to_string())
    }
}

/// What an Extractor returns: the text sfor searches, its Outline, and a warning when the text
/// was cut at `MAX_TEXT`.
#[derive(Debug, Clone, Default)]
pub struct Extracted {
    pub text: Vec<u8>,
    pub outline: Outline,
    pub cut: bool,
}

/// Extracts `bytes`, a whole document of `kind`.
pub fn extract(kind: Kind, bytes: &[u8]) -> Result<Extracted, DocError> {
    extract_with(kind, bytes, MAX_TEXT)
}

/// `extract` with another text limit (tests).
pub fn extract_with(kind: Kind, bytes: &[u8], max_text: usize) -> Result<Extracted, DocError> {
    let mut b = Builder::new(max_text);
    match kind {
        Kind::Pdf => pdf::extract(bytes, &mut b)?,
        Kind::Docx => ooxml::docx(bytes, &mut b)?,
        Kind::Xlsx => ooxml::xlsx(bytes, &mut b)?,
        Kind::Pptx => ooxml::pptx(bytes, &mut b)?,
        Kind::Odt => odf::odt(bytes, &mut b)?,
        Kind::Ods => odf::ods(bytes, &mut b)?,
        Kind::Odp => odf::odp(bytes, &mut b)?,
        Kind::Epub => epub::extract(bytes, &mut b)?,
        Kind::Sqlite => sqlite::extract(bytes, &mut b)?,
    }
    Ok(b.finish())
}

/// The ranks of `Builder::chapter` and `Builder::heading`: a chapter holds headings.
const CHAPTER: u8 = 0;
const HEADING: u8 = 1;

/// Builds the text and Outline of a document as an Extractor walks it. Text arrives a line at
/// a time; Segments open at the end of the text so far and close where it is when they close.
pub struct Builder {
    text: Vec<u8>,
    segments: Vec<OutlineSegment>,
    /// Open Segments: index into `segments`, and for chapters and headings their rank
    /// (`CHAPTER` outside `HEADING`) and level.
    open: Vec<(usize, Option<(u8, u8)>)>,
    max: usize,
    cut: bool,
    /// The row being built.
    row: Option<Row>,
    /// Line breaks in `text[..counted]`, for the line of each row.
    lines: u64,
    counted: usize,
}

struct Row {
    /// The row's Segment, when the row has one.
    seg: Option<usize>,
    line_start: usize,
    /// The cells written so far, empty ones included: each but the first writes its tab first.
    written: usize,
    /// Every cell so far is empty or white space: the row writes nothing.
    blank: bool,
    /// Each cell with text: its Segment's kind and name, and where its text starts.
    cells: Vec<(&'static str, String, usize)>,
}

/// A name on one line: its words, control characters counted as spaces, joined by one space.
fn one_line(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One line of text: control characters (line breaks, tabs, NUL) become spaces, so a line is
/// one line and the text never looks binary to the searcher.
fn clean(s: &str, out: &mut Vec<u8>) {
    for c in s.chars() {
        if c.is_control() {
            out.push(b' ');
        } else {
            let mut buf = [0u8; 4];
            out.extend(c.encode_utf8(&mut buf).as_bytes());
        }
    }
}

impl Builder {
    pub fn new(max: usize) -> Self {
        Builder {
            text: Vec::new(),
            segments: Vec::new(),
            open: Vec::new(),
            max,
            cut: false,
            row: None,
            lines: 0,
            counted: 0,
        }
    }

    /// The text reached its limit: the Extractor may stop walking.
    pub fn full(&self) -> bool {
        self.cut
    }

    /// The bytes of text the document has room for still.
    pub fn remaining(&self) -> usize {
        self.max.saturating_sub(self.text.len())
    }

    /// The Extractor stopped before the end of the document because its text would not fit:
    /// the text is cut.
    pub fn mark_cut(&mut self) {
        self.cut = true;
    }

    fn room(&mut self, more: usize) -> bool {
        if self.text.len() + more > self.max {
            self.cut = true;
        }
        !self.cut
    }

    /// Appends one line (its control characters as spaces). Empty lines are skipped.
    pub fn line(&mut self, s: &str) {
        let s = s.trim_end();
        if s.trim_start().is_empty() || !self.room(s.len() + 1) {
            return;
        }
        clean(s, &mut self.text);
        self.text.push(b'\n');
    }

    /// Opens a Segment that is not in the text (a page, a sheet, a slide, a table): `--within`
    /// finds it by name, `--in-headings` does not (M4 plan, D11). Returns its id for `close`.
    pub fn open(&mut self, kind: &str, name: &str) -> usize {
        let at = self.text.len();
        let id = self.segments.len();
        self.segments
            .push(OutlineSegment::new(kind, name, at..at, at..at));
        self.open.push((id, None));
        id
    }

    /// Closes Segment `id` and every Segment opened after it.
    pub fn close(&mut self, id: usize) {
        self.end_row();
        while let Some((top, _)) = self.open.pop() {
            self.segments[top].range.end = self.text.len();
            if top == id {
                break;
            }
        }
    }

    /// Closes, from the top of the stack, what a new entry of `rank` and `level` ends: entries
    /// of a higher rank (headings, when a chapter opens) and of its rank at its level or deeper.
    /// A Segment from `open` (a page, a table) stops it.
    fn close_levels(&mut self, rank: u8, level: u8) {
        while let Some(&(top, Some((r, l)))) = self.open.last() {
            if r < rank || r == rank && l < level {
                break;
            }
            self.open.pop();
            self.segments[top].range.end = self.text.len();
        }
    }

    /// A chapter of `level` (1 = top) whose title is not in the text, such as a PDF bookmark or
    /// an EPUB table-of-contents entry: closes the open chapters of that level or deeper and
    /// the headings inside them.
    pub fn chapter(&mut self, level: u8, title: &str) {
        let name = one_line(title);
        if name.is_empty() {
            return;
        }
        self.end_row();
        self.close_levels(CHAPTER, level);
        let at = self.text.len();
        let id = self.segments.len();
        let mut s = OutlineSegment::new("chapter", name, at..at, at..at);
        s.segment.level = Some(level);
        self.segments.push(s);
        self.open.push((id, Some((CHAPTER, level))));
    }

    /// A heading of `level` (1 = top): closes the open headings of that level or deeper, and
    /// writes `text` as a line that is the new heading's label and its searchable name, as a
    /// Markdown heading's.
    pub fn heading(&mut self, level: u8, text: &str) {
        let name = one_line(text);
        if name.is_empty() {
            return;
        }
        self.end_row();
        self.close_levels(HEADING, level);
        let start = self.text.len();
        if !self.room(name.len() + 1) {
            return;
        }
        clean(&name, &mut self.text);
        let end = self.text.len();
        self.text.push(b'\n');
        let id = self.segments.len();
        let mut s = OutlineSegment::new("heading", name.clone(), start..start, start..end + 1);
        s.segment.level = Some(level);
        s.name = Some(vec![(0..name.len(), start..end)]);
        s.name_anchor = start;
        self.segments.push(s);
        self.open.push((id, Some((HEADING, level))));
    }

    /// Starts a row of a table, whose cells come with `cell` and are written as one line,
    /// separated by tabs: a Segment `row` named `name`, or no row Segment with None (a
    /// spreadsheet's cells carry the row in their own name, `B12`).
    pub fn start_row(&mut self, name: Option<&str>) {
        self.end_row();
        let at = self.text.len();
        let seg = name.map(|n| {
            self.segments
                .push(OutlineSegment::new("row", n, at..at, at..at));
            self.segments.len() - 1
        });
        self.row = Some(Row {
            seg,
            line_start: at,
            written: 0,
            blank: true,
            cells: Vec::new(),
        });
    }

    /// One cell of the current row, a Segment of `kind` (`column` in a table, `cell` in a
    /// spreadsheet) named `name`, whose Section is the row (M3 plan, D10). A cell holds the tab
    /// after it, so a Match on the tab belongs to the cell before it, as in CSV. Every cell but
    /// the first writes a tab before it, so an empty cell is an empty field (Codex r1 #12); a
    /// value is written as it is, its spaces included; an empty cell has no Segment.
    pub fn cell(&mut self, kind: &'static str, name: &str, text: &str) {
        if self.row.is_none() || !self.room(text.len() + 1) {
            return;
        }
        let row = self.row.as_mut().unwrap();
        if row.written > 0 {
            self.text.push(b'\t');
        }
        row.written += 1;
        let start = self.text.len();
        clean(text, &mut self.text);
        if !text.trim().is_empty() {
            row.blank = false;
        }
        if self.text.len() > start {
            // A cell without a name (a table's first row, which names the columns) has no
            // Segment; its text still makes the row non-empty.
            row.cells.push((kind, name.to_string(), start));
        }
    }

    /// Ends the current row, if any: its line, and a Segment per non-empty cell.
    pub fn end_row(&mut self) {
        let Some(row) = self.row.take() else { return };
        let end = self.text.len();
        if row.blank {
            // A row without text writes nothing (its tabs are removed) and has no Segment.
            self.text.truncate(row.line_start);
            return;
        }
        self.text.push(b'\n');
        self.lines +=
            memchr::memchr_iter(b'\n', &self.text[self.counted..row.line_start]).count() as u64;
        self.counted = row.line_start;
        let line = self.lines + 1;
        if let Some(seg) = row.seg {
            let s = &mut self.segments[seg];
            s.range = row.line_start..end + 1;
            s.segment.line = line;
            s.segment.end_line = line;
        }
        for (i, (kind, name, start)) in row.cells.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            // The cell's bytes up to the next cell, so its tab too; the last one, its line break.
            let stop = row.cells.get(i + 1).map_or(end + 1, |c| c.2);
            let mut s = OutlineSegment::new(kind, name.clone(), *start..stop, *start..*start);
            s.segment.line = line;
            s.segment.end_line = line;
            self.segments.push(s);
        }
    }

    pub fn finish(mut self) -> Extracted {
        self.end_row();
        let end = self.text.len();
        for (i, _) in self.open.drain(..) {
            self.segments[i].range.end = end;
        }
        // Segments that hold no text (an empty page, a sheet without values) are dropped.
        self.segments.retain(|s| s.range.start < s.range.end);
        Extracted {
            outline: Outline::build(self.segments, &self.text),
            text: self.text,
            cut: self.cut,
        }
    }
}

/// `A`, `B`, …, `Z`, `AA`: the letters of 0-based column `i`, as a spreadsheet names it.
pub fn column_letters(mut i: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}
```

  The module declares the modules Tasks 4.2 and 4.3 write, so it compiles only after them:
  `cargo test --lib docs::tests` is GREEN at the end of Task 4.3 (review r1, Fable #8).

### Task 4.2: the XML reader, the zip container and the tables

**Files:** `src/docs/xml.rs`, `src/docs/zipped.rs`, `src/docs/table.rs`.

- [ ] **Step 1:** the XML reader (D12), tests first, then the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_text_and_entities() {
        let x = r#"<?xml version="1.0"?><!-- c --><w:p a="1>2" b='x'><w:t>a &amp; b &#233;&#x41;</w:t><w:br/></w:p><![CDATA[<raw>]]>"#;
        let ev: Vec<Ev> = Pull::new(x).collect();
        assert_eq!(
            ev,
            [
                Ev::Start {
                    name: "w:p",
                    attrs: r#" a="1>2" b='x'"#,
                    empty: false
                },
                Ev::Start {
                    name: "w:t",
                    attrs: "",
                    empty: false
                },
                Ev::Text(Cow::Owned("a & b éA".into())),
                Ev::End { name: "w:t" },
                Ev::Start {
                    name: "w:br",
                    attrs: "",
                    empty: true
                },
                Ev::End { name: "w:p" },
                Ev::Text(Cow::Borrowed("<raw>")),
            ]
        );
        assert_eq!(
            attr(r#" w:val="Heading1" x='&lt;'"#, "val").as_deref(),
            Some("Heading1")
        );
        assert_eq!(
            attr(r#" w:val="Heading1" x='&lt;'"#, "x").as_deref(),
            Some("<")
        );
        assert_eq!(attr(r#" w:val="H""#, "nope"), None);
        assert_eq!(
            prefixed(r#" id="256" r:id="rId2""#, "id").as_deref(),
            Some("rId2")
        );
        assert_eq!(
            attr(r#" id="256" r:id="rId2""#, "id").as_deref(),
            Some("256")
        );
        assert_eq!(local("text:p"), "p");
    }

    #[test]
    fn unknown_entities_stay_and_a_broken_tag_ends_the_part() {
        assert_eq!(unescape("a &bogus; &amp"), "a &bogus; &amp");
        // Found by the fuzz: a lone `&` before multi-byte text.
        assert_eq!(unescape("&eedle é"), "&eedle é");
        let ev: Vec<Ev> = Pull::new("<a>x</a><b attr='unclosed>").collect();
        assert_eq!(ev.len(), 3);
    }
}
```

```rust
//! A pull reader of the XML inside Office, OpenDocument and EPUB files (M4 plan, D12): start
//! and end tags with their attributes, and text with its entities decoded. Those files are
//! written by programs and are well formed; at the first construct this reader cannot read,
//! the part ends there, as M3's XML scanner ends its structure (M3 plan, D12).
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev<'a> {
    /// A start tag: its name (with its prefix), its attributes as written, and whether it is
    /// empty (`<br/>`).
    Start {
        name: &'a str,
        attrs: &'a str,
        empty: bool,
    },
    End {
        name: &'a str,
    },
    Text(Cow<'a, str>),
}

pub struct Pull<'a> {
    s: &'a str,
    at: usize,
}

impl<'a> Pull<'a> {
    pub fn new(s: &'a str) -> Self {
        // A BOM before the declaration.
        Pull {
            s: s.strip_prefix('\u{feff}').unwrap_or(s),
            at: 0,
        }
    }
}

impl<'a> Iterator for Pull<'a> {
    type Item = Ev<'a>;

    fn next(&mut self) -> Option<Ev<'a>> {
        loop {
            let rest = self.s.get(self.at..)?;
            if rest.is_empty() {
                return None;
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                self.at += end;
                return Some(Ev::Text(unescape(&rest[..end])));
            }
            if let Some(r) = rest.strip_prefix("<!--") {
                self.at += 4 + r.find("-->")? + 3;
                continue;
            }
            if let Some(r) = rest.strip_prefix("<![CDATA[") {
                let end = r.find("]]>")?;
                self.at += 9 + end + 3;
                return Some(Ev::Text(Cow::Borrowed(&r[..end])));
            }
            if rest.starts_with("<?") {
                self.at += rest.find("?>")? + 2;
                continue;
            }
            if rest.starts_with("<!") {
                // A doctype; an internal subset ends at `]>`.
                let end = match (rest.find('['), rest.find('>')) {
                    (Some(b), Some(g)) if b < g => rest.find("]>")? + 2,
                    (_, g) => g? + 1,
                };
                self.at += end;
                continue;
            }
            let end = tag_end(rest)?;
            let tag = &rest[1..end];
            self.at += end + 1;
            if let Some(name) = tag.strip_prefix('/') {
                return Some(Ev::End { name: name.trim() });
            }
            let (tag, empty) = match tag.strip_suffix('/') {
                Some(t) => (t, true),
                None => (tag, false),
            };
            let n = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
            if n == 0 {
                return None;
            }
            return Some(Ev::Start {
                name: &tag[..n],
                attrs: &tag[n..],
                empty,
            });
        }
    }
}

/// The `>` that ends the tag at the start of `s`, outside quoted attribute values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

/// The name without its prefix: `w:p` → `p`.
pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The value of the attribute whose local name is `want` (`w:val` and `val` both answer
/// `val`), entities decoded.
pub fn attr<'a>(attrs: &'a str, want: &str) -> Option<Cow<'a, str>> {
    find_attr(attrs, |name| local(name) == want)
}

/// The value of the attribute with a prefix whose local name is `want`: `r:id` for `id`, never
/// an unprefixed `id`.
pub fn prefixed<'a>(attrs: &'a str, want: &str) -> Option<Cow<'a, str>> {
    find_attr(attrs, |name| name.contains(':') && local(name) == want)
}

fn find_attr<'a>(attrs: &'a str, wanted: impl Fn(&str) -> bool) -> Option<Cow<'a, str>> {
    let mut s = attrs;
    loop {
        s = s.trim_start();
        let eq = s.find('=')?;
        let name = s[..eq].trim();
        let after = s[eq + 1..].trim_start();
        let q = after.chars().next()?;
        if q != '"' && q != '\'' {
            return None;
        }
        let end = after[1..].find(q)?;
        let value = &after[1..1 + end];
        if wanted(name) {
            return Some(unescape(value));
        }
        s = &after[end + 2..];
    }
}

/// XML's five entities and character references, and the HTML entities EPUB books use most;
/// an unknown entity is kept as written.
pub fn unescape(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // An entity is short: its `;` comes within 12 bytes (searched as bytes, so a
        // multi-byte character after a lone `&` is never cut).
        let Some(semi) = rest.as_bytes()[..rest.len().min(12)]
            .iter()
            .position(|&b| b == b';')
        else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let c = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "lsquo" => Some('‘'),
            "rsquo" => Some('’'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            "copy" => Some('©'),
            _ => ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}
```

  `cargo test --lib docs::xml` passes. With `unescape` searching the `;` in `&rest[..12]` (a
  `str` slice) instead of its bytes, the case `&eedle é` panics: it is the input the fuzz found
  (D28), kept as a test.

- [ ] **Step 2:** the zip container (D17): 256 MiB per part, and 1 GiB for all the parts one
  document reads (`the_parts_of_a_document_share_a_budget`, RED before `left` exists: it does
  not compile; review r1, Fable #3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Fable r1 #3: every part a document reads spends one budget, so many parts under the
    /// limit of one are refused too.
    #[test]
    fn the_parts_of_a_document_share_a_budget() {
        let parts: Vec<(&str, Vec<u8>)> = ["a.xml", "b.xml", "c.xml"]
            .iter()
            .map(|n| (*n, vec![b' '; 400]))
            .collect();
        let bytes = super::super::fixture::zip(&parts, &[]);
        let mut z = Zip::open(&bytes).unwrap();
        z.left = 1000;
        assert_eq!(z.bytes("a.xml").unwrap().unwrap().len(), 400);
        assert_eq!(z.bytes("b.xml").unwrap().unwrap().len(), 400);
        let e = z.bytes("c.xml").unwrap_err();
        assert!(
            matches!(&e, DocError::TooLarge(what, 1024) if what == "the whole document, decompressed,"),
            "{e}"
        );
    }

    #[test]
    fn hrefs_resolve_against_their_part() {
        assert_eq!(
            resolve("OEBPS/content.opf", "text/ch%201.xhtml#x"),
            "OEBPS/text/ch 1.xhtml"
        );
        assert_eq!(
            resolve("OEBPS/text/nav.xhtml", "../text/c2.xhtml"),
            "OEBPS/text/c2.xhtml"
        );
        assert_eq!(resolve("content.opf", "./c.xhtml"), "c.xhtml");
        assert_eq!(
            resolve("ppt/presentation.xml", "slides/slide1.xml"),
            "ppt/slides/slide1.xml"
        );
        assert_eq!(
            resolve("ppt/slides/slide1.xml", "/ppt/notesSlides/n1.xml"),
            "ppt/notesSlides/n1.xml"
        );
    }
}
```

```rust
//! The zip container of the Office, OpenDocument and EPUB formats: its parts, read whole and
//! never more than a limit each and never more than a limit all together, so a zip bomb fails
//! instead of filling the memory or taking minutes (D17).
//! Only the parts a format names are read; nothing else in the zip is opened (8390).
use super::DocError;
use std::io::Read;

/// The most bytes one part may decompress to (256 MiB).
pub const MAX_PART: u64 = 256 << 20;
/// The most bytes all the parts one document reads may decompress to, together (1 GiB).
pub const MAX_DOCUMENT: u64 = 1 << 30;

pub struct Zip<'a> {
    archive: zip::ZipArchive<std::io::Cursor<&'a [u8]>>,
    /// What is left of `MAX_DOCUMENT`.
    left: u64,
}

impl<'a> Zip<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Zip<'a>, DocError> {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| DocError::broken(format!("not a zip file the reader can open ({e})")))?;
        Ok(Zip {
            archive,
            left: MAX_DOCUMENT,
        })
    }

    /// The part `name`, as text (UTF-8, invalid bytes replaced). None when it is missing.
    pub fn text(&mut self, name: &str) -> Result<Option<String>, DocError> {
        Ok(self.bytes(name)?.map(|b| {
            String::from_utf8(b)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
        }))
    }

    /// The part `name`, decompressed. None when it is missing.
    pub fn bytes(&mut self, name: &str) -> Result<Option<Vec<u8>>, DocError> {
        let mut f = match self.archive.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(
                zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED)
                | zip::result::ZipError::InvalidPassword,
            ) => return Err(DocError::Encrypted),
            Err(e) => return Err(DocError::broken(format!("{name}: {e}"))),
        };
        if f.encrypted() {
            return Err(DocError::Encrypted);
        }
        let mut out = Vec::with_capacity(f.size().min(1 << 20) as usize);
        let read = (&mut f)
            .take(MAX_PART.min(self.left) + 1)
            .read_to_end(&mut out);
        read.map_err(|e| DocError::broken(format!("{name}: {e}")))?;
        let n = out.len() as u64;
        if n > MAX_PART {
            return Err(DocError::TooLarge(
                name.to_string(),
                (MAX_PART >> 20) as usize,
            ));
        }
        if n > self.left {
            return Err(DocError::TooLarge(
                "the whole document, decompressed,".into(),
                (MAX_DOCUMENT >> 20) as usize,
            ));
        }
        self.left -= n;
        Ok(Some(out))
    }
}

/// Resolves `href`, relative to the part `base` (`OEBPS/content.opf`), to a part name:
/// percent escapes decoded, `.` and `..` folded, a fragment dropped.
pub fn resolve(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let href = percent(href);
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map_or(Vec::new(), |(d, _)| d.split('/').collect())
    };
    for p in href.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn percent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
```

- [ ] **Step 3:** the tables of documents (D20):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cut_at_forty_characters() {
        assert_eq!(cut("  Unit   price "), "Unit price");
        assert_eq!(cut(&"x".repeat(45)), format!("{}…", "x".repeat(40)));
    }

    #[test]
    fn the_first_row_names_the_columns() {
        let mut b = Builder::new(super::super::MAX_TEXT);
        let mut t = Table::default();
        for row in [
            &["Item", "", "Price"][..],
            &["pens", "x", "1.50 needle"],
            &["", ""],
        ] {
            for c in row {
                t.push_cell();
                t.add_text(c);
            }
            t.end_row(&mut b);
        }
        let e = b.finish();
        assert_eq!(e.text, b"Item\t\tPrice\npens\tx\t1.50 needle\n");
        let names: Vec<_> = e
            .outline
            .segments
            .iter()
            .map(|s| (s.segment.kind.as_str(), s.segment.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("row", "row 1"),
                ("row", "row 2"),
                ("column", "Item"),
                ("column", "column 2"),
                ("column", "Price")
            ]
        );
    }
}
```

```rust
//! The tables of Word, OpenDocument text and presentations (spec §5.8): `table N › row R ›
//! Column`, the first row naming the columns, as a CSV header row (M4 plan, D20).
use super::Builder;

/// The characters of a table column's name, cut with `…` past them (as M3's `name_chars`).
pub const NAME_CHARS: usize = 40;

pub fn cut(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match s.char_indices().nth(NAME_CHARS) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s,
    }
}

/// A table being read: its number, its rows so far, the names of its columns (its first row's
/// cells), and the cells of the row being read.
#[derive(Default)]
pub struct Table {
    rows: usize,
    names: Vec<String>,
    cells: Vec<String>,
}

impl Table {
    /// Starts a cell of the row being read.
    pub fn push_cell(&mut self) {
        self.cells.push(String::new());
    }

    /// Adds a paragraph's text to the last cell, after a space.
    pub fn add_text(&mut self, s: &str) {
        if let Some(c) = self.cells.last_mut() {
            if !c.is_empty() && !s.trim().is_empty() {
                c.push(' ');
            }
            c.push_str(s.trim());
        }
    }

    /// The text of the row being read, its cells joined by spaces (a nested table's row).
    pub fn take_row(&mut self) -> String {
        std::mem::take(&mut self.cells).join(" ")
    }

    /// Writes the row just read: in the first row the cells name the columns and have no
    /// Segment (they are the columns' names, as a CSV header row); in the others each cell is
    /// a `column` Segment named by its column, or `column K` when that name is empty.
    pub fn end_row(&mut self, b: &mut Builder) {
        let cells = std::mem::take(&mut self.cells);
        if cells.iter().all(|c| c.trim().is_empty()) {
            return;
        }
        self.rows += 1;
        b.start_row(Some(&format!("row {}", self.rows)));
        if self.rows == 1 {
            self.names = cells.iter().map(|c| cut(c)).collect();
            for c in &cells {
                b.cell("column", "", c);
            }
        } else {
            for (k, c) in cells.iter().enumerate() {
                let name = match self.names.get(k) {
                    Some(n) if !n.is_empty() => n.clone(),
                    _ => format!("column {}", k + 1),
                };
                b.cell("column", &name, c);
            }
        }
        b.end_row();
    }
}
```

### Task 4.3: the shared writers and the stubs

**Files:** `src/docs/fixture/{mod,ooxml,odf,epub,pdf}.rs`, `src/docs/{ooxml,odf,epub,pdf,sqlite}.rs`.

- [ ] **Step 1:** the shared document writers (D19): blocks, the zip writer (every part deflated
  but `mimetype`, with 1980 dates, so the bytes do not depend on the clock), `rezip` for the fuzz,
  and the OOXML package parts. Its helpers are `pub`, not `pub(crate)`: with the format writers
  still stubs, a `pub(crate)` helper is dead code and clippy refuses the build (seen in the
  probe's state after tanda 2):

```rust
//! Small documents written by code, for the tests of the Extractors, the end-to-end tests, the
//! fuzz and the perf ceilings (M4 plan, D19). Each is the least a reader of its format needs,
//! written here so no fixture comes from a third party. Not used by `sfor` itself.
use std::io::{Cursor, Write};

mod epub;
mod odf;
mod ooxml;
mod pdf;
pub use epub::*;
pub use odf::*;
pub use ooxml::*;
pub use pdf::*;

/// One block of a text document.
#[derive(Debug, Clone)]
pub enum Block {
    Heading(u8, String),
    Text(String),
    /// Rows of cells.
    Table(Vec<Vec<String>>),
}

pub fn h(level: u8, s: &str) -> Block {
    Block::Heading(level, s.into())
}
pub fn p(s: &str) -> Block {
    Block::Text(s.into())
}
pub fn table(rows: &[&[&str]]) -> Block {
    Block::Table(
        rows.iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect(),
    )
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The parts of a zip, in order, each passed through `f` (name, bytes) and written again as
/// `zip` writes them: for the fuzz, which changes the XML inside documents.
pub fn rezip(bytes: &[u8], mut f: impl FnMut(&str, Vec<u8>) -> Vec<u8>) -> Vec<u8> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = Vec::new();
    for i in 0..z.len() {
        let mut file = z.by_index(i).unwrap();
        let name = file.name().unwrap().to_string();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        let data = f(&name, data);
        parts.push((name, data));
    }
    let refs: Vec<(&str, Vec<u8>)> = parts.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &["mimetype"])
}

/// A zip of `parts` (name, bytes), deflated, except those named in `stored`.
pub fn zip(parts: &[(&str, Vec<u8>)], stored: &[&str]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in parts {
        let method = if stored.contains(name) {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let opts = zip::write::SimpleFileOptions::default().compression_method(method);
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn ct(overrides: &[(&str, &str)]) -> Vec<u8> {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
    );
    for (part, ty) in overrides {
        s.push_str(&format!(
            r#"<Override PartName="/{part}" ContentType="{ty}"/>"#
        ));
    }
    s.push_str("</Types>");
    s.into_bytes()
}

pub fn rels(list: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for (id, ty, target) in list {
        s.push_str(&format!(
            r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{ty}" Target="{target}"/>"#
        ));
    }
    s.push_str("</Relationships>");
    s.into_bytes()
}
```

- [ ] **Step 2:** a stub for each format's writers, with their final signatures, so
  `pub use ooxml::*` and the others re-export something (a glob of an empty module is an unused
  import); `src/docs/fixture/ooxml.rs`:

```rust
//! The Word, Excel and PowerPoint writers (M4 plan, D19). Stub of Chunk 4; Chunk 6 writes it.
use super::Block;

pub fn docx(_blocks: &[Block]) -> Vec<u8> {
    unimplemented!("written in Chunk 6")
}

pub fn xlsx(_sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    unimplemented!("written in Chunk 6")
}

pub fn pptx(_slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    unimplemented!("written in Chunk 6")
}
```

  `odf.rs`, `epub.rs` and `pdf.rs` alike:

```rust
//! The OpenDocument writers (M4 plan, D19). Stub of Chunk 4; Chunk 7 writes it.
use super::Block;

pub fn odt(_blocks: &[Block]) -> Vec<u8> {
    unimplemented!("written in Chunk 7")
}

pub fn ods(_sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    unimplemented!("written in Chunk 7")
}

pub fn odp(_slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    unimplemented!("written in Chunk 7")
}
```

```rust
//! The EPUB writers (M4 plan, D19). Stub of Chunk 4; Chunk 8 writes it.
pub fn epub(_chapters: &[(&str, &str)], _nav: bool) -> Vec<u8> {
    unimplemented!("written in Chunk 8")
}
```

```rust
//! The PDF writers (M4 plan, D19). Stub of Chunk 4; Chunk 9 writes it.
pub fn pdf(_pages: &[&[&str]], _bookmarks: &[(u8, &str, usize)]) -> Vec<u8> {
    unimplemented!("written in Chunk 9")
}

pub fn pdf_encrypted(_pages: &[&[&str]], _user: &str) -> Vec<u8> {
    unimplemented!("written in Chunk 9")
}
```

- [ ] **Step 3:** a stub for each Extractor, returning an error until its chunk writes it;
  `src/docs/ooxml.rs`:

```rust
//! Word, Excel and PowerPoint (spec §5.8). Stub of Chunk 4; Chunk 6 writes it.
use super::{Builder, DocError};

pub fn docx(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 6"))
}

pub fn xlsx(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 6"))
}

pub fn pptx(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 6"))
}
```

  `src/docs/odf.rs`:

```rust
//! OpenDocument (spec §5.8). Stub of Chunk 4; Chunk 7 writes it.
use super::{Builder, DocError};

pub fn odt(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 7"))
}

pub fn ods(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 7"))
}

pub fn odp(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 7"))
}
```

  and `src/docs/epub.rs`, `src/docs/pdf.rs`, `src/docs/sqlite.rs`, with one `pub fn
  extract(bytes, b)` each and «Chunk 8», «Chunk 9», «Chunk 10».

- [ ] **Step 4:** `cargo test --lib docs::`: the tests of Tasks 4.1 and 4.2 are GREEN.

### Task 4.4: the SQLite fixtures

**Files:** `scripts/gen-sqlite-fixtures.py`, `tests/fixtures/docs/{shop,many,utf16}.sqlite`.

- [ ] **Step 1:** the generator (D19):

```python
#!/usr/bin/env python3
"""Writes the SQLite fixtures of tests/fixtures/docs/ (M4 plan, D19). Python's own sqlite3
module, no third-party code; the data is ours (MIT OR Apache-2.0, as the repository).
Usage: scripts/gen-sqlite-fixtures.py [DIR]  (tests/fixtures/docs by default)."""
import os, sqlite3, sys

out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "docs")
os.makedirs(out, exist_ok=True)

def fresh(name, page_size=512, encoding=None):
    path = os.path.join(out, name)
    if os.path.exists(path):
        os.remove(path)
    db = sqlite3.connect(path)
    db.execute(f"PRAGMA page_size={page_size}")
    if encoding:
        db.execute(f"PRAGMA encoding='{encoding}'")
    db.execute("PRAGMA journal_mode=DELETE")
    return db

# shop.sqlite: the end-to-end fixture. `users` row 12 holds the needle in `email`.
db = fresh("shop.sqlite")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT, score REAL)")
for i in range(1, 41):
    email = "ana.needle@example.com" if i == 12 else f"user{i}@example.com"
    db.execute("INSERT INTO users VALUES (?, ?, ?, ?)", (i, f"User {i}", email, i * 1.5))
db.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT, photo BLOB)")
db.execute("INSERT INTO notes VALUES (1, ?, ?)", ("short note", b"\x00\x01binary"))
# Longer than a 512-byte page: its payload continues on overflow pages.
db.execute("INSERT INTO notes VALUES (2, ?, NULL)", ("long " * 300 + "needle at the end",))
db.execute("CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT) WITHOUT ROWID")
for k in ["beta", "alpha", "gamma"]:
    db.execute("INSERT INTO kv VALUES (?, ?)", (k, f"value of {k}" + (" needle" if k == "gamma" else "")))
# A WITHOUT ROWID table stores its primary key first: `late` holds (k, payload) on disk, `comp`
# (c, a, b); the reader must name each value by its declared column (Codex r1 #4).
db.execute("CREATE TABLE late (payload TEXT, k TEXT PRIMARY KEY) WITHOUT ROWID")
db.execute("INSERT INTO late VALUES ('needle payload', 'k1')")
db.execute("CREATE TABLE comp (a TEXT, b REAL, c TEXT, PRIMARY KEY (c, a)) WITHOUT ROWID")
db.execute("INSERT INTO comp VALUES ('a1', 3.0, 'c1')")
# A NULL before a value: its field stays, empty (Codex r1 #12).
db.execute("CREATE TABLE gaps (x TEXT, y TEXT)")
db.execute("INSERT INTO gaps VALUES (NULL, 'needle gap')")
# The schema as SQLite keeps it, its spaces and comments included: `WITHOUT` and `ROWID` on two
# lines, a comment between them, and a key whose quoted name holds a comma (M4 plan r2, Codex #4).
db.execute('CREATE TABLE odd (payload TEXT, "k,key" REAL /* the key */, PRIMARY KEY ("k,key")) WITHOUT\n  -- no rowid\n  ROWID')
db.execute("INSERT INTO odd VALUES ('needle odd', 2)")
db.execute("CREATE INDEX users_email ON users(email)")
db.execute("CREATE VIEW v AS SELECT name FROM users")
db.commit()
db.close()

# many.sqlite: 2 000 rows, enough for interior pages of a table and of a WITHOUT ROWID table.
db = fresh("many.sqlite")
db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, word TEXT)")
db.execute("CREATE TABLE w (k INTEGER PRIMARY KEY, word TEXT) WITHOUT ROWID")
for i in range(1, 2001):
    db.execute("INSERT INTO t VALUES (?, ?)", (i, f"word{i}"))
    db.execute("INSERT INTO w VALUES (?, ?)", (i, f"word{i}"))
db.commit()
db.close()

# utf16.sqlite: text stored as UTF-16le.
db = fresh("utf16.sqlite", encoding="UTF-16le")
db.execute("CREATE TABLE t (name TEXT)")
db.execute("INSERT INTO t VALUES ('año needle')")
db.commit()
db.close()
```

- [ ] **Step 2:** `python3 scripts/gen-sqlite-fixtures.py` writes the three databases (9 728,
  69 632 and 1 024 bytes with Python 3.13's sqlite3, SQLite 3.46.1; the sizes may differ with
  another SQLite, the tests do not depend on them). Commit them: git reads them as binary.
  `shop.sqlite`'s tables `late` (`WITHOUT ROWID`, its key the last column), `comp` (`WITHOUT
  ROWID`, a key of two columns in another order) and `gaps` (a NULL first column) are the cases
  of review r1 (Codex #4 and #12, Fable #2).

### Task 4.5: green, and the commit

- [ ] **Step 1:** the whole suite (the probe's state after tanda 2, measured over `191dbaf`: 448
  unit tests), clippy,
  fmt, one commit per task.

## Chunk 5: object rows and the new kinds in the printers

### Task 5.1: src/output/{mod,flat,tree,stream}.rs

**Files:** `src/output/mod.rs`, `src/output/flat.rs`, `src/output/tree.rs`,
`src/output/stream.rs`.

- [ ] **Step 1:** the tests first: the fixtures `object_rows` and `object_row_twice` (in
  `output::fixture`), `object_rows_print_the_hex_offset_of_their_match` and
  `an_object_row_with_two_matches_is_one_row` (flat), `labels_of_the_m4_kinds`,
  `object_rows_in_the_tree` and `an_object_row_with_two_matches_is_one_row` (tree), as in the
  hunks of their `tests` modules below. `object_row_twice` is one string with two Matches of
  one symbol, `s` and `s+0x7`: before D8's single row it printed twice, once per Trail (measured
  over the frozen copy of round 0; review r1, Codex #9). RED: they do
  not compile before `object_rows` sets `offsets`, and with it the rows print their line number
  `1` and the labels `page N` (to verify in chunk 5: the probe wrote them together with the
  code).

- [ ] **Step 2:** `Row::offset`, filled by `match_rows` when `Described::offsets`, which makes one
  row of each object row (design note 5), no `--` between object rows, flat's `Head` with the offset in hex, tree's `write_row` and
  `write_row_text` (M3's final code split the latter out for `LineText`) with it, the labels of
  the new kinds, and `offset: None` in the three `Row` literals of `stream.rs`. The
  `offsets: false` lines in these diffs are C1's:

```diff
diff --git a/src/output/mod.rs b/src/output/mod.rs
--- a/src/output/mod.rs
+++ b/src/output/mod.rs
@@ -205,6 +205,9 @@ type Subs = Vec<(usize, usize)>;
 pub(crate) struct Row<'a> {
     pub trail: &'a [Segment],
     pub line_number: u64,
+    /// An object file's row (`Described::offsets`): the hex offset printed in place of the
+    /// line number.
+    pub offset: Option<u64>,
     /// The line, shared by the rows of a Match line.
     pub line: Rc<LineText<'a>>,
     pub subs: Vec<(usize, usize)>,
@@ -310,7 +313,10 @@ fn match_rows<'a>(
     o: &RenderOptions,
 ) -> Vec<Row<'a>> {
     let bytes = m.bytes(src);
-    let groups: Vec<(&'a [Segment], Subs)> = if m.submatches.is_empty() || omitted(bytes, o) {
+    // An object file's row is one string: one row, with the Trail of its first Match (spec
+    // §5.7; code review of the M4 plan r1, Codex #9).
+    let one = d.offsets || m.submatches.is_empty() || omitted(bytes, o);
+    let groups: Vec<(&'a [Segment], Subs)> = if one {
         vec![(trail_of(d, i, 0), subs_of(&m.submatches))]
     } else {
         group_by_trail(
@@ -324,6 +330,10 @@ fn match_rows<'a>(
     groups
         .into_iter()
         .map(|(trail, subs)| Row {
+            // At its first Match (spec §5.7).
+            offset: d
+                .offsets
+                .then(|| m.absolute_offset + m.submatches.first().map_or(0, |s| s.start) as u64),
             trail,
             line_number: m.line_number,
             line: line.clone(),
@@ -371,7 +381,8 @@ impl<'a> Rows<'a> {
         let line = match item {
             Item::Match(m) | Item::Context(m) => m,
         };
-        let gap = self.prev.is_some_and(|p| p + 1 != line.line_number);
+        // An object file's rows are strings, not lines: no `--` between them.
+        let gap = !self.d.offsets && self.prev.is_some_and(|p| p + 1 != line.line_number);
         if gap {
             // A group without a Match keeps an empty Trail.
             self.ready.extend(self.pending.drain(..));
@@ -384,6 +395,7 @@ impl<'a> Rows<'a> {
                 let row = Row {
                     trail: self.last.unwrap_or(&[]),
                     line_number: c.line_number,
+                    offset: None,
                     line: Rc::new(LineText::new(c.bytes(self.src))),
                     subs: subs_of(&c.submatches),
                     is_match: false,
@@ -646,6 +658,56 @@ pub(crate) mod fixture {
         (line, bytes)
     }
 
+    /// An object file's rows (spec §5.7): two printable strings of a binary on its first line,
+    /// with context on, as `sfor -C1` gives them.
+    pub fn object_rows() -> (Vec<u8>, Described) {
+        let src = b"\x7fELF\0\0version=needle\0\x01another needle\0".to_vec();
+        let row = |at: usize, text: &[u8], start: usize| {
+            let mut l = Line::owned(1, at as u64, text);
+            l.submatches = vec![Submatch {
+                start,
+                end: start + 6,
+            }];
+            Item::Match(l)
+        };
+        let base = vec![
+            seg("object", "ELF x86-64", 1, 1),
+            seg("section", ".rodata", 1, 1),
+        ];
+        let mut sym = base.clone();
+        sym.push(seg("symbol", "main.version+0x8", 1, 1));
+        let mut d = described(
+            &src,
+            vec![row(6, b"version=needle", 8), row(22, b"another needle", 8)],
+            vec![vec![sym], vec![base]],
+        );
+        d.offsets = true;
+        d.format = Some("elf".into());
+        (src, d)
+    }
+
+    /// An object file's row holding two Matches of one symbol, whose Trails differ in the
+    /// distance into the symbol (code review of the M4 plan r1, Codex #9).
+    pub fn object_row_twice() -> (Vec<u8>, Described) {
+        let src = b"\x7fELF\0\0needle needle\0".to_vec();
+        let mut l = Line::owned(1, 6, b"needle needle");
+        l.submatches = vec![
+            Submatch { start: 0, end: 6 },
+            Submatch { start: 7, end: 13 },
+        ];
+        let at = |n: &str| {
+            vec![
+                seg("object", "ELF x86-64", 1, 1),
+                seg("section", ".rodata", 1, 1),
+                seg("symbol", n, 1, 1),
+            ]
+        };
+        let mut d = described(&src, vec![Item::Match(l)], vec![vec![at("s"), at("s+0x7")]]);
+        d.offsets = true;
+        d.format = Some("elf".into());
+        (src, d)
+    }
+
     /// Line `n` as a Match, with one submatch per occurrence of `needle`.
     pub fn mat(src: &[u8], n: u64) -> Item {
         let (line, bytes) = ranged(src, n);
@@ -695,6 +757,7 @@ pub(crate) mod fixture {
             format: Some("markdown".into()),
             trails,
             sections: Vec::new(),
+            offsets: false,
         }
     }
 }
```

```diff
diff --git a/src/output/flat.rs b/src/output/flat.rs
--- a/src/output/flat.rs
+++ b/src/output/flat.rs
@@ -105,10 +105,13 @@ fn digits(mut n: u64) -> usize {
     d
 }
 
-/// `path:line:` (or `path-line-` for context), each part present when asked for.
+/// `path:line:` (or `path-line-` for context), each part present when asked for; an object
+/// file's row has `path:0x1f3a:`, its hex offset in place of the line (spec §5.7).
 struct Head<'a> {
     file: &'a FileHead,
     line: Option<u64>,
+    /// `line` is an object row's offset, printed in hex.
+    hex: bool,
     sep: &'static str,
 }
 
@@ -122,15 +125,32 @@ impl<'a> Head<'a> {
         Head {
             file,
             line: line_number.filter(|_| o.line_numbers),
+            hex: false,
             sep: if is_match { ":" } else { "-" },
         }
     }
 
+    fn at_offset(mut self, offset: Option<u64>) -> Self {
+        if let (Some(_), Some(off)) = (self.line, offset) {
+            self.line = Some(off);
+            self.hex = true;
+        }
+        self
+    }
+
     fn is_empty(&self) -> bool {
         self.file.path.is_none() && self.line.is_none()
     }
 
     fn width(&self) -> usize {
+        let digits = |l: u64| {
+            if self.hex {
+                // `0x` and the hex digits.
+                2 + (l.checked_ilog2().unwrap_or(0) / 4 + 1) as usize
+            } else {
+                digits(l)
+            }
+        };
         self.file.width + self.line.map_or(0, |l| digits(l) + 1)
     }
 
@@ -143,7 +163,11 @@ impl<'a> Head<'a> {
         }
         if let Some(l) = self.line {
             w.set_color(&style::spec("line"))?;
-            write!(w, "{l}")?;
+            if self.hex {
+                write!(w, "0x{l:x}")?;
+            } else {
+                write!(w, "{l}")?;
+            }
             w.reset()?;
             w.write_all(self.sep.as_bytes())?;
         }
@@ -161,7 +185,7 @@ pub(crate) fn write_row(
     if r.gap_before {
         w.write_all(b"--\n")?;
     }
-    let head = Head::new(file, Some(r.line_number), r.is_match, o);
+    let head = Head::new(file, Some(r.line_number), r.is_match, o).at_offset(r.offset);
     if !r.is_match {
         let prefix = |w: &mut dyn WriteColor| head.write(w);
         let avail = o.width.saturating_sub(head.width());
@@ -305,6 +329,48 @@ mod tests {
         String::from_utf8(w.into_inner()).unwrap()
     }
 
+    /// M4 plan, D6 and D8: an object file's row is at the hex offset of its first Match, and
+    /// rows are strings, not lines: no `--` between them, even with context on.
+    #[test]
+    fn object_rows_print_the_hex_offset_of_their_match() {
+        let (src, d) = object_rows();
+        let o = RenderOptions {
+            context: true,
+            ..flat()
+        };
+        assert_eq!(
+            plain(&d, &src, &o),
+            "docs/a.md:0xe: ELF x86-64 › .rodata › main.version+0x8 │ version=needle\n\
+             docs/a.md:0x1e: ELF x86-64 › .rodata │ another needle\n"
+        );
+        let o = RenderOptions {
+            with_filename: false,
+            ..flat()
+        };
+        assert!(plain(&d, &src, &o).starts_with("0xe: ELF"));
+        // -N hides the offset as it hides line numbers.
+        let o = RenderOptions {
+            line_numbers: false,
+            with_filename: false,
+            ..flat()
+        };
+        assert!(plain(&d, &src, &o).starts_with("ELF x86-64"));
+    }
+
+    /// Codex r1 #9: a string with two Matches is one row, at its first Match and with its
+    /// Trail, both Matches highlighted.
+    #[test]
+    fn an_object_row_with_two_matches_is_one_row() {
+        let (src, d) = object_row_twice();
+        assert_eq!(
+            plain(&d, &src, &flat()),
+            "docs/a.md:0x6: ELF x86-64 › .rodata › s │ needle needle\n"
+        );
+        let colored = ansi(&d, &src, &flat());
+        assert_eq!(colored.lines().count(), 1, "{colored}");
+        assert_eq!(colored.matches("needle\x1b[0m").count(), 2, "{colored}");
+    }
+
     fn ansi(d: &Described, src: &[u8], o: &RenderOptions) -> String {
         let mut w = Ansi::new(Vec::new());
         render(d, src, o, None, &mut w).unwrap();
```

```diff
diff --git a/src/output/tree.rs b/src/output/tree.rs
--- a/src/output/tree.rs
+++ b/src/output/tree.rs
@@ -19,7 +19,10 @@ pub fn label(s: &Segment) -> String {
         "item" => format!("• {name}"),
         "column" => format!("column «{name}»"),
         // M3: markup elements (`tag`) and CSV rows (`row 12`) show their name alone.
-        "document" | "key" | "element" | "tag" | "row" => name.into_owned(),
+        // M4: the image, section and symbol of an object file, and a document's pages,
+        // sheets, slides, tables, chapters, cells and notes show their name alone.
+        "document" | "key" | "element" | "tag" | "row" | "object" | "section" | "symbol"
+        | "page" | "sheet" | "slide" | "table" | "chapter" | "cell" | "notes" => name.into_owned(),
         kind => format!("{kind} {name}"),
     }
 }
@@ -71,38 +74,46 @@ fn write_trail(
     Ok(())
 }
 
-/// One line at `depth`: indentation, line number with `:` or `-`, text.
+/// One line at `depth`: indentation, line number (an object file's row: its hex offset, spec
+/// §5.7) with `:` or `-`, text.
+#[allow(clippy::too_many_arguments)]
 pub(crate) fn write_row(
     w: &mut dyn WriteColor,
     depth: usize,
     line_number: u64,
+    offset: Option<u64>,
     bytes: &[u8],
     subs: &[(usize, usize)],
     is_match: bool,
     o: &RenderOptions,
 ) -> std::io::Result<()> {
     let line = super::LineText::new(bytes);
-    write_row_text(w, depth, line_number, &line, subs, is_match, o)
+    write_row_text(w, depth, line_number, offset, &line, subs, is_match, o)
 }
 
 /// `write_row` for a line whose text the rows before may have worked out already.
+#[allow(clippy::too_many_arguments)]
 fn write_row_text(
     w: &mut dyn WriteColor,
     depth: usize,
     line_number: u64,
+    offset: Option<u64>,
     line: &super::LineText,
     subs: &[(usize, usize)],
     is_match: bool,
     o: &RenderOptions,
 ) -> std::io::Result<()> {
-    let digits = line_number.checked_ilog10().map_or(1, |d| d as usize + 1);
+    let at = match offset {
+        Some(off) => format!("0x{off:x}"),
+        None => line_number.to_string(),
+    };
     let sep = if is_match { ":" } else { "-" };
-    let used = 2 * (depth + 1) + if o.line_numbers { digits + 1 } else { 0 };
+    let used = 2 * (depth + 1) + if o.line_numbers { at.len() + 1 } else { 0 };
     let prefix = |w: &mut dyn WriteColor| -> std::io::Result<()> {
         write_indent(w, depth)?;
         if o.line_numbers {
             w.set_color(&style::spec("line"))?;
-            write!(w, "{line_number}")?;
+            w.write_all(at.as_bytes())?;
             w.reset()?;
             w.write_all(sep.as_bytes())?;
         }
@@ -164,7 +175,16 @@ impl TreeState {
             write_trail(w, &self.printed, r.trail, &self.seen)?;
             self.printed = r.trail.to_vec();
         }
-        write_row_text(w, depth, r.line_number, &r.line, &r.subs, r.is_match, o)?;
+        write_row_text(
+            w,
+            depth,
+            r.line_number,
+            r.offset,
+            &r.line,
+            &r.subs,
+            r.is_match,
+            o,
+        )?;
         if self.seen.len() == SEEN {
             self.seen.pop_front();
         }
@@ -188,7 +208,7 @@ impl TreeState {
         for (n, line) in super::section_lines(src, start, end) {
             let subs = kept.get(&n);
             let s = subs.map_or(&[][..], |v| v.as_slice());
-            write_row(w, trail.len(), n, line, s, subs.is_some(), o)?;
+            write_row(w, trail.len(), n, None, line, s, subs.is_some(), o)?;
         }
         Ok(())
     }
@@ -244,6 +264,35 @@ mod tests {
         assert_eq!(label(&sig), "fn m(\\u{1b})");
     }
 
+    #[test]
+    fn labels_of_the_m4_kinds() {
+        for kind in [
+            "object", "section", "symbol", "page", "sheet", "slide", "table", "chapter", "cell",
+            "notes",
+        ] {
+            assert_eq!(label(&seg(kind, "N", 1, 1)), "N", "{kind}");
+        }
+    }
+
+    #[test]
+    fn object_rows_in_the_tree() {
+        let (src, d) = crate::output::fixture::object_rows();
+        assert_eq!(
+            plain(&d, &src, &opts()),
+            "docs/a.md\n  ELF x86-64\n    .rodata\n      main.version+0x8\n        0xe:version=needle\n      0x1e:another needle\n"
+        );
+    }
+
+    /// Codex r1 #9: a string with two Matches is one row, under its first Match's Trail.
+    #[test]
+    fn an_object_row_with_two_matches_is_one_row() {
+        let (src, d) = crate::output::fixture::object_row_twice();
+        assert_eq!(
+            plain(&d, &src, &opts()),
+            "docs/a.md\n  ELF x86-64\n    .rodata\n      s\n        0x6:needle needle\n"
+        );
+    }
+
     fn plain(d: &Described, src: &[u8], o: &RenderOptions) -> String {
         let mut w = NoColor::new(Vec::new());
         render(d, src, o, &mut w).unwrap();
```

```diff
diff --git a/src/output/stream.rs b/src/output/stream.rs
--- a/src/output/stream.rs
+++ b/src/output/stream.rs
@@ -152,6 +152,7 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
             let subs: Vec<(usize, usize)> =
                 p.line.submatches.iter().map(|s| (s.start, s.end)).collect();
             let row = Row {
+                offset: None,
                 trail: &trail,
                 line_number: p.line.line_number,
                 line: Rc::new(LineText::new(p.line.bytes(src))),
@@ -249,6 +250,7 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
         let text = Rc::new(LineText::new(bytes));
         for (n, (trail, subs)) in groups.iter().enumerate() {
             let row = Row {
+                offset: None,
                 trail,
                 line_number: line.line_number,
                 line: text.clone(),
@@ -297,6 +299,7 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
                 let trail = group.unwrap_or_else(|| self.last.1.clone());
                 let subs = line.submatches.iter().map(|s| (s.start, s.end)).collect();
                 let row = Row {
+                    offset: None,
                     trail: &trail,
                     line_number: line.line_number,
                     line: Rc::new(LineText::new(bytes)),
@@ -557,6 +560,7 @@ mod tests {
                 format: format.map(Into::into),
                 trails,
                 sections: Vec::new(),
+                offsets: false,
             };
             match layout {
                 Layout::Flat => {
```

- [ ] **Step 3:** GREEN (`cargo test --lib output::`), clippy, fmt, commit.

## Chunk 6: Word, Excel and PowerPoint

### Task 6.1: src/docs/ooxml.rs

**Files:** `src/docs/ooxml.rs`, `src/docs/fixture/ooxml.rs`.

- [ ] **Step 1:** the writers of these formats (D19), replacing C4's stub; the tests below use
  them:

```rust
//! The Word, Excel and PowerPoint writers (M4 plan, D19).
use super::{Block, ct, esc, rels, zip};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

/// A Word document. Headings use the styles `Heading1`…`Heading3`, named `heading N` in
/// `styles.xml` as Word names them.
pub fn docx(blocks: &[Block]) -> Vec<u8> {
    let mut body = String::new();
    for b in blocks {
        match b {
            Block::Heading(l, t) => body.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="Heading{l}"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#,
                esc(t)
            )),
            Block::Text(t) => body.push_str(&format!(
                r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                esc(t)
            )),
            Block::Table(rows) => {
                body.push_str("<w:tbl>");
                for r in rows {
                    body.push_str("<w:tr>");
                    for c in r {
                        body.push_str(&format!("<w:tc><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:tc>", esc(c)));
                    }
                    body.push_str("</w:tr>");
                }
                body.push_str("</w:tbl>");
            }
        }
    }
    let doc = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document {W}><w:body>{body}<w:sectPr/></w:body></w:document>"#
    );
    let mut styles = format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:styles {W}>"#);
    for l in 1..=3 {
        styles.push_str(&format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading{l}"><w:name w:val="heading {l}"/></w:style>"#
        ));
    }
    styles.push_str("</w:styles>");
    zip(
        &[
            (
                "[Content_Types].xml",
                ct(&[(
                    "word/document.xml",
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                )]),
            ),
            (
                "_rels/.rels",
                rels(&[("rId1", "officeDocument", "word/document.xml")]),
            ),
            ("word/document.xml", doc.into_bytes()),
            ("word/styles.xml", styles.into_bytes()),
        ],
        &[],
    )
}

/// An Excel workbook: sheets of rows, text cells in the shared strings, numbers inline.
pub fn xlsx(sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    let mut shared: Vec<String> = Vec::new();
    let mut parts = Vec::new();
    let mut book = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>"#,
    );
    let mut brels = Vec::new();
    for (i, (name, rows)) in sheets.iter().enumerate() {
        book.push_str(&format!(
            r#"<sheet name="{}" sheetId="{}" r:id="rId{}"/>"#,
            esc(name),
            i + 1,
            i + 1
        ));
        brels.push((
            format!("rId{}", i + 1),
            format!("worksheets/sheet{}.xml", i + 1),
        ));
        let mut x = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        );
        for (r, row) in rows.iter().enumerate() {
            x.push_str(&format!(r#"<row r="{}">"#, r + 1));
            for (c, v) in row.iter().enumerate() {
                if v.is_empty() {
                    continue;
                }
                let at = format!("{}{}", crate::docs::column_letters(c), r + 1);
                if v.parse::<f64>().is_ok() {
                    x.push_str(&format!(r#"<c r="{at}"><v>{v}</v></c>"#));
                } else {
                    shared.push(v.to_string());
                    x.push_str(&format!(
                        r#"<c r="{at}" t="s"><v>{}</v></c>"#,
                        shared.len() - 1
                    ));
                }
            }
            x.push_str("</row>");
        }
        x.push_str("</sheetData></worksheet>");
        parts.push((format!("xl/worksheets/sheet{}.xml", i + 1), x.into_bytes()));
    }
    book.push_str("</sheets></workbook>");
    let mut sst = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    );
    for s in &shared {
        sst.push_str(&format!("<si><t>{}</t></si>", esc(s)));
    }
    sst.push_str("</sst>");
    let mut r: Vec<(&str, &str, &str)> = brels
        .iter()
        .map(|(i, t)| (i.as_str(), "worksheet", t.as_str()))
        .collect();
    let sst_id = format!("rId{}", sheets.len() + 1);
    r.push((&sst_id, "sharedStrings", "sharedStrings.xml"));
    let mut all = vec![
        ("[Content_Types].xml".to_string(), ct(&[])),
        (
            "_rels/.rels".to_string(),
            rels(&[("rId1", "officeDocument", "xl/workbook.xml")]),
        ),
        ("xl/workbook.xml".to_string(), book.into_bytes()),
        ("xl/_rels/workbook.xml.rels".to_string(), rels(&r)),
        ("xl/sharedStrings.xml".to_string(), sst.into_bytes()),
    ];
    all.extend(parts);
    let refs: Vec<(&str, Vec<u8>)> = all.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &[])
}

/// A PowerPoint presentation: slides of paragraphs, each with optional notes.
pub fn pptx(slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    const NS: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let mut pres =
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><p:presentation {NS}><p:sldIdLst>"#);
    let mut prels = Vec::new();
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let shape = |paras: &[&str], ph: Option<&str>| {
        let mut s =
            String::from("<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"\"/><p:cNvSpPr/><p:nvPr>");
        if let Some(t) = ph {
            s.push_str(&format!(r#"<p:ph type="{t}"/>"#));
        }
        s.push_str("</p:nvPr></p:nvSpPr><p:txBody>");
        for t in paras {
            s.push_str(&format!("<a:p><a:r><a:t>{}</a:t></a:r></a:p>", esc(t)));
        }
        s.push_str("</p:txBody></p:sp>");
        s
    };
    for (i, (paras, notes)) in slides.iter().enumerate() {
        let n = i + 1;
        // The slide's id comes before its relationship id, as PowerPoint writes it.
        pres.push_str(&format!(r#"<p:sldId id="{}" r:id="rId{n}"/>"#, 255 + n));
        prels.push((format!("rId{n}"), format!("slides/slide{n}.xml")));
        let sl = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><p:sld {NS}><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:sld>"#,
            shape(paras, None)
        );
        parts.push((format!("ppt/slides/slide{n}.xml"), sl.into_bytes()));
        if let Some(text) = notes {
            let nx = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><p:notes {NS}><p:cSld><p:spTree>{}{}</p:spTree></p:cSld></p:notes>"#,
                shape(&["slide image"], Some("sldImg")),
                shape(&[text], Some("body"))
            );
            parts.push((
                format!("ppt/notesSlides/notesSlide{n}.xml"),
                nx.into_bytes(),
            ));
            parts.push((
                format!("ppt/slides/_rels/slide{n}.xml.rels"),
                rels(&[(
                    "rId1",
                    "notesSlide",
                    &format!("../notesSlides/notesSlide{n}.xml"),
                )]),
            ));
        }
    }
    pres.push_str("</p:sldIdLst></p:presentation>");
    let r: Vec<(&str, &str, &str)> = prels
        .iter()
        .map(|(i, t)| (i.as_str(), "slide", t.as_str()))
        .collect();
    let mut all = vec![
        ("[Content_Types].xml".to_string(), ct(&[])),
        (
            "_rels/.rels".to_string(),
            rels(&[("rId1", "officeDocument", "ppt/presentation.xml")]),
        ),
        ("ppt/presentation.xml".to_string(), pres.into_bytes()),
        ("ppt/_rels/presentation.xml.rels".to_string(), rels(&r)),
    ];
    all.extend(parts);
    let refs: Vec<(&str, Vec<u8>)> = all.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &[])
}
```

- [ ] **Step 2:** the tests, below the stub (they use C4's and this chunk's writers):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_styles_come_from_names_and_outline_levels() {
        let x = r#"<w:styles><w:style w:type="paragraph" w:styleId="Ttulo1"><w:name w:val="heading 1"/></w:style><w:style w:styleId="Mine"><w:name w:val="Mine"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr></w:style><w:style w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#;
        let s = heading_styles(x);
        assert_eq!(s.get("Ttulo1"), Some(&1));
        assert_eq!(s.get("Mine"), Some(&3));
        assert_eq!(s.get("Normal"), None);
    }

    #[test]
    fn column_references() {
        assert_eq!(column_of("B12"), Some(1));
        assert_eq!(column_of("AA3"), Some(26));
        assert_eq!(column_of("12"), None);
    }

    #[test]
    fn relationship_targets_resolve_against_their_part() {
        let r = r#"<Relationships><Relationship Id="rId1" Type="http://x/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId9" Type="http://x/hyperlink" Target="http://e.com" TargetMode="External"/></Relationships>"#;
        assert_eq!(
            relationships(r, "xl/workbook.xml"),
            [(
                "rId1".into(),
                "http://x/worksheet".into(),
                "xl/worksheets/sheet1.xml".into()
            )]
        );
        assert_eq!(rels_of("xl/workbook.xml"), "xl/_rels/workbook.xml.rels");
    }
}

#[cfg(test)]
mod extract_tests {
    use super::super::fixture::{self, h, p, table};
    use super::super::{Kind, extract};
    use crate::trail::trail_for;

    fn trail(kind: Kind, doc: &[u8], needle: &str) -> Vec<String> {
        let e = extract(kind, doc).unwrap();
        let at = memchr::memmem::find(&e.text, needle.as_bytes())
            .unwrap_or_else(|| panic!("{needle} not in {:?}", String::from_utf8_lossy(&e.text)));
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn docx_headings_and_tables() {
        let d = fixture::docx(&[
            h(1, "1 Intro"),
            p("first needle"),
            h(2, "1.1 Scope"),
            p("second needle"),
            h(1, "2 Costs"),
            table(&[&["Item", "Price"], &["Pens", "1.50 needle"]]),
            p("after the table needle"),
        ]);
        assert_eq!(trail(Kind::Docx, &d, "first"), ["1 Intro"]);
        assert_eq!(trail(Kind::Docx, &d, "second"), ["1 Intro", "1.1 Scope"]);
        assert_eq!(
            trail(Kind::Docx, &d, "1.50"),
            ["2 Costs", "table 1", "row 2", "Price"]
        );
        // The first row names the columns: a Match in it is in no column.
        assert_eq!(
            trail(Kind::Docx, &d, "Item"),
            ["2 Costs", "table 1", "row 1"]
        );
        assert_eq!(trail(Kind::Docx, &d, "after"), ["2 Costs"]);
        let e = extract(Kind::Docx, &d).unwrap();
        assert_eq!(
            String::from_utf8(e.text).unwrap(),
            "1 Intro\nfirst needle\n1.1 Scope\nsecond needle\n2 Costs\nItem\tPrice\nPens\t1.50 needle\nafter the table needle\n"
        );
    }

    #[test]
    fn xlsx_cells_are_named_by_their_reference() {
        let d = fixture::xlsx(&[
            (
                "Budget",
                &[&["Item", "Price"], &["Pens", "1.5"], &["needle", "", "x"]],
            ),
            ("Other", &[&["", "the needle here"]]),
        ]);
        assert_eq!(trail(Kind::Xlsx, &d, "needle"), ["Budget", "A3"]);
        assert_eq!(trail(Kind::Xlsx, &d, "1.5"), ["Budget", "B2"]);
        assert_eq!(trail(Kind::Xlsx, &d, "x"), ["Budget", "C3"]);
        assert_eq!(trail(Kind::Xlsx, &d, "the needle"), ["Other", "B1"]);
    }

    #[test]
    fn pptx_slides_and_their_notes() {
        let d = fixture::pptx(&[
            (
                &["Quarterly results", "a needle margin"],
                Some("say the needle joke"),
            ),
            (&["Next steps", "find the needle"], None),
        ]);
        assert_eq!(trail(Kind::Pptx, &d, "margin"), ["slide 1"]);
        assert_eq!(trail(Kind::Pptx, &d, "joke"), ["slide 1", "notes"]);
        assert_eq!(trail(Kind::Pptx, &d, "find"), ["slide 2"]);
        // The slide-image placeholder of the notes page is left out.
        let e = extract(Kind::Pptx, &d).unwrap();
        assert!(memchr::memmem::find(&e.text, b"slide image").is_none());
    }

    #[test]
    fn a_zip_that_is_not_a_document_is_an_error() {
        let z = fixture::zip(&[("a.txt", b"needle".to_vec())], &[]);
        assert!(extract(Kind::Docx, &z).is_err());
        assert!(extract(Kind::Xlsx, &z).is_err());
        assert!(extract(Kind::Pptx, &z).is_err());
        assert!(extract(Kind::Docx, b"PK\x03\x04 truncated").is_err());
    }
}
```

- [ ] **Step 3:** `cargo test --lib docs::ooxml` does not compile: `column_of`,
  `heading_styles`, `relationships` and `rels_of` do not exist: RED.

- [ ] **Step 4:** the Extractors (D12, D18, D20). `Flow` reads WordprocessingML and DrawingML,
  which share their local names (`p`, `t`, `br`, `tab`, `tbl`, `tr`, `tc`); a nested table's row
  joins the text of the cell that holds the table; `mc:Fallback` is skipped, so a text box
  written twice by Word is read once:

```rust
//! Word, Excel and PowerPoint files (Office Open XML, spec §5.8): `2 Costs › table 1 › row 3 ›
//! Price` in a document, `Budget › B12` in a workbook, `slide 3 › notes` in a presentation.
//! Only the main parts are read: the document body, the sheets, the slides and their notes
//! (headers, footers, footnotes, comments and charts are left for later: M4 plan, D18).
use super::table::Table;
use super::xml::{Ev, Pull, attr, local, prefixed};
use super::zipped::{Zip, resolve};
use super::{Builder, DocError, column_letters};
use std::collections::HashMap;

/// Reads paragraphs and tables of WordprocessingML (`w:`) or DrawingML (`a:`), which share
/// their local names: `p`, `t`, `br`, `tab`, `tbl`, `tr`, `tc`. `heading` gives the level of a
/// paragraph from its `pStyle` value or its `outlineLvl` (Word only).
pub struct Flow<'s> {
    styles: &'s HashMap<String, u8>,
    /// The paragraphs being read, innermost last (a text box's paragraphs are inside one).
    paras: Vec<(String, Option<u8>)>,
    /// Inside `w:t` or `a:t`.
    in_text: bool,
    /// Depth inside elements whose text is not shown (`w:instrText`, `w:delText`,
    /// `mc:Fallback`).
    skip: usize,
    tables: Vec<Table>,
    /// Top-level tables so far.
    count: usize,
    table_seg: Option<usize>,
}

impl<'s> Flow<'s> {
    pub fn new(styles: &'s HashMap<String, u8>) -> Self {
        Flow {
            styles,
            paras: Vec::new(),
            in_text: false,
            skip: 0,
            tables: Vec::new(),
            count: 0,
            table_seg: None,
        }
    }

    pub fn event(&mut self, ev: &Ev, b: &mut Builder) {
        match ev {
            Ev::Start { name, attrs, empty } => {
                let l = local(name);
                if self.skip > 0 || matches!(l, "instrText" | "delText") || *name == "mc:Fallback" {
                    if !empty {
                        self.skip += 1;
                    }
                    return;
                }
                match l {
                    "p" if !empty => self.paras.push((String::new(), None)),
                    "t" if !empty => self.in_text = true,
                    "br" | "cr" => self.push("\n"),
                    "tab" if name.starts_with("w:") && self.paras.last().is_some() => {
                        self.push("\t")
                    }
                    "pStyle" => {
                        if let (Some(v), Some(p)) = (attr(attrs, "val"), self.paras.last_mut()) {
                            p.1 = self.styles.get(v.as_ref()).copied().or(p.1);
                        }
                    }
                    "outlineLvl" => {
                        if let (Some(v), Some(p)) = (attr(attrs, "val"), self.paras.last_mut())
                            && let Ok(n) = v.parse::<u8>()
                            && n < 9
                        {
                            p.1 = Some(n + 1);
                        }
                    }
                    "tbl" if !empty => {
                        if self.tables.is_empty() {
                            self.count += 1;
                            self.table_seg =
                                Some(b.open("table", &format!("table {}", self.count)));
                        }
                        self.tables.push(Table::default());
                    }
                    "tc" if !empty => {
                        if let Some(t) = self.tables.last_mut() {
                            t.push_cell();
                        }
                    }
                    _ => {}
                }
            }
            Ev::End { name } => {
                let l = local(name);
                if self.skip > 0 {
                    self.skip -= 1;
                    return;
                }
                match l {
                    "t" => self.in_text = false,
                    "p" => {
                        let Some((text, level)) = self.paras.pop() else {
                            return;
                        };
                        self.end_paragraph(text, level, b);
                    }
                    "tr" => {
                        if self.tables.len() == 1 {
                            self.tables[0].end_row(b);
                        } else if let Some(t) = self.tables.last_mut() {
                            // A nested table's row: its text joins the cell holding the table.
                            let row = t.take_row();
                            self.add_to_outer_cell(&row);
                        }
                    }
                    "tbl" => {
                        self.tables.pop();
                        if self.tables.is_empty()
                            && let Some(s) = self.table_seg.take()
                        {
                            b.close(s);
                        }
                    }
                    _ => {}
                }
            }
            Ev::Text(t) => {
                if self.in_text && self.skip == 0 {
                    self.push(t);
                }
            }
        }
    }

    fn push(&mut self, s: &str) {
        if let Some(p) = self.paras.last_mut() {
            p.0.push_str(s);
        }
    }

    /// Appends to the innermost table cell being read.
    fn add_to_outer_cell(&mut self, s: &str) {
        let n = self.tables.len();
        // The cell that holds the innermost table is in the table before it.
        if n >= 2 {
            self.tables[n - 2].add_text(s);
        }
    }

    fn end_paragraph(&mut self, text: String, level: Option<u8>, b: &mut Builder) {
        // A text box's paragraph, inside another one, writes its lines before the outer's.
        if let Some(t) = self.tables.last_mut() {
            t.add_text(&text);
            return;
        }
        match level {
            Some(l) => b.heading(l, &text),
            None => {
                for line in text.split('\n') {
                    b.line(line);
                }
            }
        }
    }
}

fn walk(xml: &str, flow: &mut Flow, b: &mut Builder) {
    for ev in Pull::new(xml) {
        flow.event(&ev, b);
        if b.full() {
            break;
        }
    }
}

/// Paragraph styles that are headings, by id: their name is `heading N` (Word writes built-in
/// style names in English whatever the language of its interface), or they carry an outline
/// level.
fn heading_styles(xml: &str) -> HashMap<String, u8> {
    let mut out = HashMap::new();
    let mut id: Option<String> = None;
    for ev in Pull::new(xml) {
        match ev {
            Ev::Start { name, attrs, .. } => match local(name) {
                "style" => id = attr(attrs, "styleId").map(|s| s.into_owned()),
                "name" => {
                    if let (Some(i), Some(v)) = (&id, attr(attrs, "val")) {
                        let v = v.to_ascii_lowercase();
                        if let Some(n) = v
                            .strip_prefix("heading ")
                            .and_then(|n| n.parse::<u8>().ok())
                            && (1..=9).contains(&n)
                        {
                            out.insert(i.clone(), n);
                        }
                    }
                }
                "outlineLvl" => {
                    if let (Some(i), Some(v)) = (&id, attr(attrs, "val"))
                        && let Ok(n) = v.parse::<u8>()
                        && n < 9
                    {
                        out.entry(i.clone()).or_insert(n + 1);
                    }
                }
                _ => {}
            },
            Ev::End { name } if local(name) == "style" => id = None,
            _ => {}
        }
    }
    out
}

pub fn docx(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let main = main_part(&mut z, "word/document.xml")?;
    let doc = z
        .text(&main)?
        .ok_or_else(|| DocError::broken("no document part (word/document.xml)"))?;
    let styles = z
        .text("word/styles.xml")?
        .map(|s| heading_styles(&s))
        .unwrap_or_default();
    let mut flow = Flow::new(&styles);
    walk(&doc, &mut flow, b);
    Ok(())
}

/// The main part named by `_rels/.rels` (`officeDocument`), else `default`.
fn main_part(z: &mut Zip, default: &str) -> Result<String, DocError> {
    let Some(rels) = z.text("_rels/.rels")? else {
        return Ok(default.into());
    };
    Ok(relationships(&rels, "")
        .into_iter()
        .find(|r| r.1.ends_with("/officeDocument"))
        .map_or(default.into(), |r| r.2))
}

/// (Id, Type, target part) of a relationships part; targets resolve against `base`, the
/// part the relationships are of.
fn relationships(xml: &str, base: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for ev in Pull::new(xml) {
        if let Ev::Start { name, attrs, .. } = ev
            && local(name) == "Relationship"
            && attr(attrs, "TargetMode").as_deref() != Some("External")
            && let (Some(id), Some(ty), Some(target)) = (
                attr(attrs, "Id"),
                attr(attrs, "Type"),
                attr(attrs, "Target"),
            )
        {
            out.push((id.into_owned(), ty.into_owned(), resolve(base, &target)));
        }
    }
    out
}

/// `xl/workbook.xml` → `xl/_rels/workbook.xml.rels`.
fn rels_of(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

pub fn xlsx(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let book_part = main_part(&mut z, "xl/workbook.xml")?;
    let book = z
        .text(&book_part)?
        .ok_or_else(|| DocError::broken("no workbook part (xl/workbook.xml)"))?;
    let rels = z.text(&rels_of(&book_part))?.unwrap_or_default();
    let rels = relationships(&rels, &book_part);
    // By id: a workbook may have thousands of sheets, and a lookup per sheet must not scan them.
    let targets: HashMap<&str, String> = rels.iter().map(|r| (r.0.as_str(), r.2.clone())).collect();
    let shared = match rels.iter().find(|r| r.1.ends_with("/sharedStrings")) {
        Some(r) => z
            .text(&r.2)?
            .map(|s| shared_strings(&s))
            .unwrap_or_default(),
        None => Vec::new(),
    };
    for ev in Pull::new(&book) {
        let Ev::Start { name, attrs, .. } = ev else {
            continue;
        };
        if local(name) != "sheet" {
            continue;
        }
        let (Some(sheet), Some(id)) = (attr(attrs, "name"), prefixed(attrs, "id")) else {
            continue;
        };
        let Some(target) = targets.get(id.as_ref()).cloned() else {
            continue;
        };
        let Some(xml) = z.text(&target)? else {
            continue;
        };
        let s = b.open("sheet", &sheet);
        cells(&xml, &shared, b);
        b.close(s);
        if b.full() {
            break;
        }
    }
    Ok(())
}

fn shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut in_t, mut phonetic) = (false, false);
    for ev in Pull::new(xml) {
        match ev {
            Ev::Start { name, empty, .. } => match local(name) {
                "si" => cur.clear(),
                "t" if !empty => in_t = true,
                "rPh" => phonetic = !empty,
                _ => {}
            },
            Ev::End { name } => match local(name) {
                "si" => out.push(std::mem::take(&mut cur)),
                "t" => in_t = false,
                "rPh" => phonetic = false,
                _ => {}
            },
            Ev::Text(t) if in_t && !phonetic => cur.push_str(&t),
            _ => {}
        }
    }
    out
}

/// The rows of a worksheet: each row a line, each cell a `cell` Segment named by its
/// reference (`B12`); a shared string by its text, a boolean as TRUE or FALSE, anything else as
/// its value as stored (a date is its serial number: M4 plan, D18).
fn cells(xml: &str, shared: &[String], b: &mut Builder) {
    let mut row_n = 0u64;
    let mut col = 0usize;
    // The cell being read: its reference, type, and value text.
    let mut cell: Option<(String, String, String)> = None;
    let mut in_v = false;
    for ev in Pull::new(xml) {
        match ev {
            Ev::Start { name, attrs, empty } => match local(name) {
                "row" => {
                    row_n = attr(attrs, "r")
                        .and_then(|r| r.parse().ok())
                        .unwrap_or(row_n.saturating_add(1));
                    col = 0;
                    b.start_row(None);
                    if empty {
                        b.end_row();
                    }
                }
                "c" => {
                    let r = attr(attrs, "r").map(|r| r.into_owned());
                    if let Some(c) = r.as_deref().and_then(column_of) {
                        col = c;
                    }
                    let r = r.unwrap_or_else(|| format!("{}{row_n}", column_letters(col)));
                    col += 1;
                    let ty = attr(attrs, "t").map(|t| t.into_owned()).unwrap_or_default();
                    cell = (!empty).then(|| (r, ty, String::new()));
                }
                "v" | "t" if !empty => in_v = true,
                _ => {}
            },
            Ev::End { name } => match local(name) {
                "v" | "t" => in_v = false,
                "c" => {
                    if let Some((r, ty, v)) = cell.take() {
                        let shown = match ty.as_str() {
                            "s" => v
                                .trim()
                                .parse::<usize>()
                                .ok()
                                .and_then(|i| shared.get(i))
                                .cloned()
                                .unwrap_or_default(),
                            "b" => {
                                if v.trim() == "1" {
                                    "TRUE".into()
                                } else {
                                    "FALSE".into()
                                }
                            }
                            _ => v,
                        };
                        b.cell("cell", &r, &shown);
                    }
                }
                "row" => b.end_row(),
                _ => {}
            },
            Ev::Text(t) if in_v => {
                if let Some(c) = cell.as_mut() {
                    c.2.push_str(&t);
                }
            }
            _ => {}
        }
        if b.full() {
            break;
        }
    }
}

/// The 0-based column of a cell reference: `B12` → 1.
fn column_of(r: &str) -> Option<usize> {
    let letters: Vec<u8> = r.bytes().take_while(u8::is_ascii_alphabetic).collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let n = letters.iter().fold(0usize, |a, c| {
        a * 26 + (c.to_ascii_uppercase() - b'A' + 1) as usize
    });
    Some(n - 1)
}

pub fn pptx(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let pres_part = main_part(&mut z, "ppt/presentation.xml")?;
    let pres = z
        .text(&pres_part)?
        .ok_or_else(|| DocError::broken("no presentation part (ppt/presentation.xml)"))?;
    let rels = z.text(&rels_of(&pres_part))?.unwrap_or_default();
    let rels = relationships(&rels, &pres_part);
    // By id: 20 000 slides searched one by one in the list took 1.7 s (M4 probe).
    let targets: HashMap<&str, String> = rels.iter().map(|r| (r.0.as_str(), r.2.clone())).collect();
    let none = HashMap::new();
    let mut n = 0;
    for ev in Pull::new(&pres) {
        let Ev::Start { name, attrs, .. } = ev else {
            continue;
        };
        if local(name) != "sldId" {
            continue;
        }
        // `<p:sldId id="256" r:id="rId2"/>`: the relationship is the prefixed `id`.
        let Some(rid) = prefixed(attrs, "id") else {
            continue;
        };
        let Some(target) = targets.get(rid.as_ref()).cloned() else {
            continue;
        };
        let Some(xml) = z.text(&target)? else {
            continue;
        };
        n += 1;
        let s = b.open("slide", &format!("slide {n}"));
        let mut flow = Flow::new(&none);
        walk(&xml, &mut flow, b);
        let srels = z.text(&rels_of(&target))?.unwrap_or_default();
        if let Some(notes) = relationships(&srels, &target)
            .into_iter()
            .find(|r| r.1.ends_with("/notesSlide"))
            && let Some(xml) = z.text(&notes.2)?
        {
            let ns = b.open("notes", "notes");
            notes_body(&xml, b);
            b.close(ns);
        }
        b.close(s);
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The text of a notes slide's shapes, except the placeholders of the slide image, its number,
/// the date, the header and the footer.
fn notes_body(xml: &str, b: &mut Builder) {
    let none = HashMap::new();
    // The events of the shape being read, and whether it is kept.
    let mut shape: Option<(Vec<Ev>, bool)> = None;
    for ev in Pull::new(xml) {
        match &ev {
            Ev::Start { name, .. } if local(name) == "sp" => shape = Some((Vec::new(), true)),
            Ev::Start { name, attrs, .. } if local(name) == "ph" => {
                if let Some(s) = shape.as_mut() {
                    let ty = attr(attrs, "type");
                    s.1 = !matches!(
                        ty.as_deref(),
                        Some("sldImg" | "sldNum" | "dt" | "hdr" | "ftr")
                    );
                }
            }
            Ev::End { name } if local(name) == "sp" => {
                if let Some((evs, true)) = shape.take() {
                    let mut flow = Flow::new(&none);
                    for e in &evs {
                        flow.event(e, b);
                    }
                }
                continue;
            }
            _ => {}
        }
        if let Some(s) = shape.as_mut() {
            s.0.push(ev);
        }
    }
}
```

- [ ] **Step 5:** GREEN. Then run it over real files, as the probe did over files LibreOffice
  25.2 wrote from an HTML report, a CSV and a flat ODP (`soffice --headless --convert-to docx|
  xlsx|pptx`): the headings, the table, the cells and the notes come out as in `m4_06`. A
  LibreOffice PPTX writes notes as a text box without a placeholder, which is why
  `notes_body` keeps every shape but the slide image, number, date, header and footer.
## Chunk 7: OpenDocument

### Task 7.1: src/docs/odf.rs

**Files:** `src/docs/odf.rs`, `src/docs/fixture/odf.rs`.

- [ ] **Step 1:** the writers of these formats (D19), replacing C4's stub; the tests below use
  them:

```rust
//! The OpenDocument writers (M4 plan, D19).
use super::{Block, esc, zip};

const ODF_NS: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0""#;

fn odf(mime: &str, body: &str) -> Vec<u8> {
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ODF_NS} office:version="1.3"><office:body>{body}</office:body></office:document-content>"#
    );
    let manifest = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="{mime}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#
    );
    zip(
        &[
            ("mimetype", mime.as_bytes().to_vec()),
            ("content.xml", content.into_bytes()),
            ("META-INF/manifest.xml", manifest.into_bytes()),
        ],
        &["mimetype"],
    )
}

/// An OpenDocument text: headings as `text:h`, tables as `table:table`.
pub fn odt(blocks: &[Block]) -> Vec<u8> {
    let mut body = String::from("<office:text>");
    for b in blocks {
        match b {
            Block::Heading(l, t) => body.push_str(&format!(
                r#"<text:h text:outline-level="{l}">{}</text:h>"#,
                esc(t)
            )),
            Block::Text(t) => body.push_str(&format!("<text:p>{}</text:p>", esc(t))),
            Block::Table(rows) => {
                body.push_str(r#"<table:table table:name="Table1">"#);
                for r in rows {
                    body.push_str("<table:table-row>");
                    for c in r {
                        body.push_str(&format!(
                            "<table:table-cell><text:p>{}</text:p></table:table-cell>",
                            esc(c)
                        ));
                    }
                    body.push_str("</table:table-row>");
                }
                body.push_str("</table:table>");
            }
        }
    }
    body.push_str("</office:text>");
    odf("application/vnd.oasis.opendocument.text", &body)
}

/// An OpenDocument spreadsheet; numbers carry `office:value` as LibreOffice writes them, and
/// each sheet ends with the repeated empty rows LibreOffice adds.
pub fn ods(sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    let mut body = String::from("<office:spreadsheet>");
    for (name, rows) in sheets {
        body.push_str(&format!(r#"<table:table table:name="{}">"#, esc(name)));
        for row in *rows {
            body.push_str("<table:table-row>");
            for v in *row {
                if v.is_empty() {
                    body.push_str("<table:table-cell/>");
                } else if v.parse::<f64>().is_ok() {
                    body.push_str(&format!(r#"<table:table-cell office:value-type="float" office:value="{v}"><text:p>{v}</text:p></table:table-cell>"#));
                } else {
                    body.push_str(&format!(r#"<table:table-cell office:value-type="string"><text:p>{}</text:p></table:table-cell>"#, esc(v)));
                }
            }
            body.push_str(
                r#"<table:table-cell table:number-columns-repeated="16380"/></table:table-row>"#,
            );
        }
        body.push_str(r#"<table:table-row table:number-rows-repeated="1048570"><table:table-cell table:number-columns-repeated="16384"/></table:table-row></table:table>"#);
    }
    body.push_str("</office:spreadsheet>");
    odf("application/vnd.oasis.opendocument.spreadsheet", &body)
}

/// An OpenDocument presentation: pages of paragraphs, each with optional notes.
pub fn odp(slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    let mut body = String::from("<office:presentation>");
    for (i, (paras, notes)) in slides.iter().enumerate() {
        body.push_str(&format!(
            r#"<draw:page draw:name="page{}"><draw:frame><draw:text-box>"#,
            i + 1
        ));
        for t in *paras {
            body.push_str(&format!("<text:p>{}</text:p>", esc(t)));
        }
        body.push_str("</draw:text-box></draw:frame>");
        if let Some(n) = notes {
            body.push_str(&format!(
                "<presentation:notes><draw:frame><draw:text-box><text:p>{}</text:p></draw:text-box></draw:frame></presentation:notes>",
                esc(n)
            ));
        }
        body.push_str("</draw:page>");
    }
    body.push_str("</office:presentation>");
    odf("application/vnd.oasis.opendocument.presentation", &body)
}
```

- [ ] **Step 2:** the tests, below the stub:

```rust
#[cfg(test)]
mod tests {
    use super::super::fixture::{self, h, p, table};
    use super::super::{Kind, extract};
    use crate::trail::trail_for;

    fn trail(kind: Kind, doc: &[u8], needle: &str) -> Vec<String> {
        let e = extract(kind, doc).unwrap();
        let at = memchr::memmem::find(&e.text, needle.as_bytes())
            .unwrap_or_else(|| panic!("{needle} not in {:?}", String::from_utf8_lossy(&e.text)));
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn odt_headings_and_tables() {
        let d = fixture::odt(&[
            h(1, "1 Intro"),
            p("first needle"),
            h(2, "1.1 Scope"),
            table(&[&["Item", "Price"], &["Pens", "1.50 needle"]]),
        ]);
        assert_eq!(trail(Kind::Odt, &d, "first"), ["1 Intro"]);
        assert_eq!(
            trail(Kind::Odt, &d, "1.50"),
            ["1 Intro", "1.1 Scope", "table 1", "row 2", "Price"]
        );
    }

    #[test]
    fn ods_counts_repeated_rows_and_cells_without_writing_them() {
        let d = fixture::ods(&[(
            "Budget",
            &[&["Item", "Price"], &["Pens", "1.5"], &["", "", "needle"]],
        )]);
        let e = extract(Kind::Ods, &d).unwrap();
        // 1 048 570 repeated empty rows and 16 380 empty cells per row write nothing.
        assert!(e.text.len() < 100, "{}", e.text.len());
        assert_eq!(trail(Kind::Ods, &d, "needle"), ["Budget", "C3"]);
        assert_eq!(trail(Kind::Ods, &d, "1.5"), ["Budget", "B2"]);
    }

    fn ods_of(rows: &str) -> Vec<u8> {
        let body = format!(
            r#"<office:spreadsheet><table:table table:name="S">{rows}</table:table></office:spreadsheet>"#
        );
        fixture::zip(
            &[
                ("mimetype", b"application/vnd.oasis.opendocument.spreadsheet".to_vec()),
                ("content.xml", format!("<office:document-content><office:body>{body}</office:body></office:document-content>").into_bytes()),
            ],
            &["mimetype"],
        )
    }

    /// Codex r1 #6: a cell prints its stored value, by its `office:value-type`, not the text a
    /// format made of it; a formula's is its cached value.
    #[test]
    fn ods_cells_print_their_stored_values() {
        let d = ods_of(concat!(
            r#"<table:table-row>"#,
            r#"<table:table-cell office:value-type="currency" office:value="1.5"><text:p>$1.50</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="boolean" office:boolean-value="true"><text:p>VERDADERO</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="date" office:date-value="2026-10-18"><text:p>18/10/2026</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="time" office:time-value="PT01H00M00S"><text:p>01:00</text:p></table:table-cell>"#,
            r#"<table:table-cell table:formula="of:=1+1" office:value-type="float" office:value="2"><text:p>2,00</text:p></table:table-cell>"#,
            r#"<table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell>"#,
            r#"</table:table-row>"#,
        ));
        let e = extract(Kind::Ods, &d).unwrap();
        assert_eq!(e.text, b"1.5\tTRUE\t2026-10-18\tPT01H00M00S\t2\tneedle\n");
        assert_eq!(trail(Kind::Ods, &d, "needle"), ["S", "F1"]);
    }

    /// Codex r1 #10: repeat counts near `usize::MAX` are an error of the document, never a
    /// panic or a wrapped row number.
    #[test]
    fn huge_repeats_are_an_error_not_an_overflow() {
        let max = usize::MAX;
        for rows in [
            format!(
                r#"<table:table-row table:number-rows-repeated="{max}"><table:table-cell/></table:table-row><table:table-row><table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell></table:table-row>"#
            ),
            format!(
                r#"<table:table-row><table:table-cell table:number-columns-repeated="{max}"/><table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell></table:table-row>"#
            ),
            format!(
                r#"<table:table-row table:number-rows-repeated="{max}"><table:table-cell office:value-type="string"><text:p>needle</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell office:value-type="string"><text:p>x</text:p></table:table-cell></table:table-row>"#
            ),
            // Codex r2 #6: a count past the largest number is not one.
            r#"<table:table-row table:number-rows-repeated="18446744073709551616"><table:table-cell/></table:table-row>"#.to_string(),
            r#"<table:table-row><table:table-cell table:number-columns-repeated="18446744073709551616"/></table:table-row>"#.to_string(),
        ] {
            let got = std::panic::catch_unwind(|| extract(Kind::Ods, &ods_of(&rows)));
            assert!(
                matches!(got, Ok(Err(super::super::DocError::Broken(_)))),
                "{rows}"
            );
        }
    }

    #[test]
    fn odp_slides_and_notes() {
        let d = fixture::odp(&[
            (&["Title", "a needle margin"], Some("the needle joke")),
            (&["find the needle"], None),
        ]);
        assert_eq!(trail(Kind::Odp, &d, "margin"), ["slide 1"]);
        assert_eq!(trail(Kind::Odp, &d, "joke"), ["slide 1", "notes"]);
        assert_eq!(trail(Kind::Odp, &d, "find"), ["slide 2"]);
    }

    #[test]
    fn spaces_tabs_and_line_breaks_of_odf_text() {
        let body = r#"<office:text><text:p>a<text:s text:c="3"/>b<text:tab/>c<text:line-break/>d   e</text:p></office:text>"#;
        let d = fixture::zip(
            &[
                (
                    "mimetype",
                    b"application/vnd.oasis.opendocument.text".to_vec(),
                ),
                (
                    "content.xml",
                    format!("<office:document-content>{body}</office:document-content>")
                        .into_bytes(),
                ),
            ],
            &["mimetype"],
        );
        let e = extract(Kind::Odt, &d).unwrap();
        assert_eq!(e.text, b"a   b c\nd e\n");
    }
}
```

- [ ] **Step 3:** `cargo test --lib docs::odf`: 5 tests FAIL («written in Chunk 7»);
  `huge_repeats_are_an_error_not_an_overflow` passes over the stub too, which also errs: RED.

- [ ] **Step 4:** the Extractors. A paragraph is a heading when it is `text:h` or when its
  style, or a parent of it, has an outline level or is `Heading_20_N`: LibreOffice's import of
  HTML writes `<text:p text:style-name="P1">` whose parent is `Heading_20_1`, which carries no
  `default-outline-level` (seen in the probe). White space in ODF text collapses, and `text:s`
  says how many spaces it means. `number-rows-repeated` and `number-columns-repeated` of empty
  cells are only counted, with checked additions, so a repeat of `18446744073709551615` is an
  error and not an overflow (D17; `huge_repeats_are_an_error_not_an_overflow` panicked before:
  review r1, Codex #10), and so is a count that does not fit in a number at all, which round 1
  read as 1 (review r2, Codex #6). A cell is searched by its stored value, by its `office:value-type`
  (D18; `ods_cells_print_their_stored_values` is RED over the round-0 code, which read the
  displayed `<text:p>`, `$1.50` for the stored `1.5`: Codex #6):

```rust
//! OpenDocument text, spreadsheets and presentations (spec §5.8), read from `content.xml`:
//! `2 Costs › table 1 › row 3 › Price`, `Budget › B12`, `slide 3 › notes`.
use super::table::Table;
use super::xml::{Ev, Pull, attr, local};
use super::zipped::Zip;
use super::{Builder, DocError, column_letters};
use std::collections::HashMap;

/// Repeated rows and cells with text are written at most this many times each (a repeat of
/// empty cells is only counted).
const MAX_REPEAT: usize = 1000;

/// `content.xml`, and the paragraph styles of it and of `styles.xml` that are headings.
fn content(bytes: &[u8]) -> Result<(String, HashMap<String, u8>), DocError> {
    let mut z = Zip::open(bytes)?;
    let content = z
        .text("content.xml")?
        .ok_or_else(|| DocError::broken("no content.xml"))?;
    let styles = z.text("styles.xml")?.unwrap_or_default();
    let mut parents = HashMap::new();
    let mut levels = HashMap::new();
    for xml in [&styles, &content] {
        for ev in Pull::new(xml) {
            if let Ev::Start {
                name: "style:style",
                attrs,
                ..
            } = ev
                && let Some(n) = attr(attrs, "name")
            {
                if let Some(p) = attr(attrs, "parent-style-name") {
                    parents.insert(n.to_string(), p.into_owned());
                }
                // The outline level, or the level of a built-in `Heading N` style (whose
                // name is `Heading_20_N` whatever the interface language).
                let level = attr(attrs, "default-outline-level")
                    .and_then(|l| l.parse::<u8>().ok())
                    .or_else(|| n.strip_prefix("Heading_20_").and_then(|d| d.parse().ok()));
                if let Some(l) = level.filter(|l| (1..=10).contains(l)) {
                    levels.insert(n.into_owned(), l);
                }
            }
        }
    }
    // A style is a heading when it or one of its parents has an outline level.
    let mut headings = HashMap::new();
    for name in parents.keys().chain(levels.keys()) {
        let mut cur = name.as_str();
        for _ in 0..16 {
            if let Some(l) = levels.get(cur) {
                headings.insert(name.clone(), *l);
                break;
            }
            match parents.get(cur) {
                Some(p) => cur = p,
                None => break,
            }
        }
    }
    Ok((content, headings))
}

/// Text paragraphs and headings of `office:text`, a slide or a frame: `text:h` with its
/// `outline-level`, `text:p`, spans, `text:s` (spaces), `text:tab`, `text:line-break`; notes
/// (footnotes) inside a paragraph as lines after it; tables as in Word.
struct Flow {
    /// Paragraph styles that make a `text:p` a heading, by name, with their level.
    headings: HashMap<String, u8>,
    paras: Vec<(String, Option<u8>)>,
    tables: Vec<Table>,
    count: usize,
    table_seg: Option<usize>,
    /// Depth inside elements whose text is not shown (a note's citation, an annotation,
    /// tracked deletions).
    skip: usize,
}

impl Flow {
    fn new(headings: HashMap<String, u8>) -> Self {
        Flow {
            headings,
            paras: Vec::new(),
            tables: Vec::new(),
            count: 0,
            table_seg: None,
            skip: 0,
        }
    }

    fn event(&mut self, ev: &Ev, b: &mut Builder) {
        match ev {
            Ev::Start { name, attrs, empty } => {
                let l = local(name);
                if self.skip > 0 || matches!(l, "note-citation" | "annotation" | "tracked-changes")
                {
                    if !empty {
                        self.skip += 1;
                    }
                    return;
                }
                match l {
                    "p" | "h" if !empty => {
                        let level = if l == "h" {
                            Some(
                                attr(attrs, "outline-level")
                                    .and_then(|v| v.parse().ok())
                                    .unwrap_or(1),
                            )
                        } else {
                            attr(attrs, "style-name")
                                .and_then(|s| self.headings.get(s.as_ref()).copied())
                        };
                        self.paras.push((String::new(), level));
                    }
                    "s" => {
                        let n = attr(attrs, "c")
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1);
                        self.push(&" ".repeat(n.min(100)));
                    }
                    "tab" => self.push("\t"),
                    "line-break" => self.push("\n"),
                    "table" if !empty => {
                        if self.tables.is_empty() {
                            self.count += 1;
                            self.table_seg =
                                Some(b.open("table", &format!("table {}", self.count)));
                        }
                        self.tables.push(Table::default());
                    }
                    "table-cell" => {
                        if let Some(t) = self.tables.last_mut() {
                            let n = attr(attrs, "number-columns-repeated")
                                .and_then(|v| v.parse::<usize>().ok())
                                .unwrap_or(1);
                            // Repeated cells are empty but the last, which `p` fills.
                            for _ in 0..n.min(MAX_REPEAT) {
                                t.push_cell();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ev::End { name } => {
                if self.skip > 0 {
                    self.skip -= 1;
                    return;
                }
                match local(name) {
                    "p" | "h" => {
                        let Some((text, level)) = self.paras.pop() else {
                            return;
                        };
                        if let Some(t) = self.tables.last_mut() {
                            t.add_text(&text);
                        } else if let Some(l) = level {
                            b.heading(l, &text);
                        } else {
                            for line in text.split('\n') {
                                b.line(line);
                            }
                        }
                    }
                    "table-row" => {
                        if self.tables.len() == 1 {
                            self.tables[0].end_row(b);
                        } else if let Some(t) = self.tables.last_mut() {
                            let row = t.take_row();
                            let n = self.tables.len();
                            self.tables[n - 2].add_text(&row);
                        }
                    }
                    "table" => {
                        self.tables.pop();
                        if self.tables.is_empty()
                            && let Some(s) = self.table_seg.take()
                        {
                            b.close(s);
                        }
                    }
                    _ => {}
                }
            }
            Ev::Text(t) => {
                if self.skip == 0
                    && let Some(p) = self.paras.last_mut()
                {
                    // White space in ODF text is collapsed; `text:s` writes the spaces it means.
                    let mut last_space = p.0.ends_with(' ');
                    for c in t.chars() {
                        if c.is_whitespace() && c != '\u{a0}' {
                            if !last_space {
                                p.0.push(' ');
                            }
                            last_space = true;
                        } else {
                            p.0.push(c);
                            last_space = false;
                        }
                    }
                }
            }
        }
    }

    fn push(&mut self, s: &str) {
        if let Some(p) = self.paras.last_mut() {
            p.0.push_str(s);
        }
    }
}

pub fn odt(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, headings) = content(bytes)?;
    let mut flow = Flow::new(headings);
    let mut body = false;
    for ev in Pull::new(&xml) {
        match &ev {
            Ev::Start { name, .. } if *name == "office:text" => body = true,
            Ev::End { name } if *name == "office:text" => body = false,
            _ if body => flow.event(&ev, b),
            _ => {}
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

pub fn odp(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, _) = content(bytes)?;
    let mut flow = Flow::new(HashMap::new());
    let (mut n, mut slide, mut notes) = (0, None, None);
    for ev in Pull::new(&xml) {
        match &ev {
            Ev::Start {
                name, empty: false, ..
            } if *name == "draw:page" => {
                n += 1;
                slide = Some(b.open("slide", &format!("slide {n}")));
            }
            Ev::End { name } if *name == "draw:page" => {
                if let Some(s) = slide.take() {
                    b.close(s);
                }
            }
            Ev::Start {
                name, empty: false, ..
            } if *name == "presentation:notes" && slide.is_some() => {
                notes = Some(b.open("notes", "notes"));
            }
            Ev::End { name } if *name == "presentation:notes" => {
                if let Some(s) = notes.take() {
                    b.close(s);
                }
            }
            _ if slide.is_some() => flow.event(&ev, b),
            _ => {}
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The value a spreadsheet cell stores, by its `office:value-type` (Codex r1 #6): a number,
/// percentage or currency as its `office:value`, a date or time as its ISO value, a boolean as
/// TRUE or FALSE, a string as its text (or the `office:string-value` of a formula). A formula's
/// value is its cached one. None: the cell's text, as for a cell without a type.
fn stored(attrs: &str) -> Option<String> {
    let v = |a: &str| attr(attrs, a).map(|s| s.into_owned());
    match attr(attrs, "value-type").as_deref()? {
        "float" | "percentage" | "currency" => v("value"),
        "date" => v("date-value"),
        "time" => v("time-value"),
        "boolean" => v("boolean-value").map(|b| {
            if b == "true" {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }),
        _ => v("string-value"),
    }
}

/// `a + b`, or the error of a document whose repeat counts take a row or column number past
/// what a machine word holds (Codex r1 #10).
fn add(a: usize, b: usize) -> Result<usize, DocError> {
    a.checked_add(b)
        .ok_or_else(|| DocError::broken("a repeated row or column goes past the largest number"))
}

/// A repeat count: 1 when absent or not a number, an error when it is a number past the
/// largest one (M4 plan r2, Codex #6).
fn repeat(attrs: &str, name: &str) -> Result<usize, DocError> {
    let Some(v) = attr(attrs, name) else {
        return Ok(1);
    };
    match v.trim().parse::<usize>() {
        Ok(n) => Ok(n.max(1)),
        Err(_) if !v.trim().is_empty() && v.trim().bytes().all(|c| c.is_ascii_digit()) => Err(
            DocError::broken("a repeated row or column goes past the largest number"),
        ),
        Err(_) => Ok(1),
    }
}

pub fn ods(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let (xml, _) = content(bytes)?;
    let mut sheet = None;
    let mut row = 0usize;
    let mut col = 0usize;
    // The row being read: its repeat count and its cells (column, repeat, text).
    let mut cells: Vec<(usize, usize, String)> = Vec::new();
    let mut repeat_row = 1usize;
    // The cell being read: its column, repeat count, text, stored value.
    let mut cell: Option<(usize, usize, String, Option<String>)> = None;
    let mut para_depth = 0usize;
    for ev in Pull::new(&xml) {
        match ev {
            Ev::Start { name, attrs, empty } => match name {
                "table:table" if !empty => {
                    let n = attr(attrs, "name")
                        .map(|s| s.into_owned())
                        .unwrap_or_default();
                    sheet = Some(b.open("sheet", &n));
                    row = 0;
                }
                "table:table-row" => {
                    repeat_row = repeat(attrs, "number-rows-repeated")?;
                    col = 0;
                    cells.clear();
                    if empty {
                        row = add(row, repeat_row)?;
                    }
                }
                "table:table-cell" | "table:covered-table-cell" => {
                    let rep = repeat(attrs, "number-columns-repeated")?;
                    let value = stored(attrs);
                    if empty {
                        if let Some(v) = value.filter(|v| !v.is_empty()) {
                            cells.push((col, rep, v));
                        }
                        col = add(col, rep)?;
                    } else {
                        cell = Some((col, rep, String::new(), value));
                    }
                }
                "text:p" if !empty => {
                    if let Some(c) = cell.as_mut()
                        && !c.2.is_empty()
                    {
                        c.2.push(' ');
                    }
                    para_depth += 1;
                }
                "text:s" => {
                    if let Some(c) = cell.as_mut() {
                        c.2.push(' ');
                    }
                }
                _ => {}
            },
            Ev::End { name } => match name {
                "text:p" => para_depth = para_depth.saturating_sub(1),
                "table:table-cell" | "table:covered-table-cell" => {
                    if let Some((c, rep, text, value)) = cell.take() {
                        let shown = value.unwrap_or(text);
                        if !shown.trim().is_empty() {
                            cells.push((c, rep, shown));
                        }
                        col = add(c, rep)?;
                    }
                }
                "table:table-row" => {
                    let writes = if cells.is_empty() {
                        0
                    } else {
                        repeat_row.min(MAX_REPEAT)
                    };
                    for k in 0..writes {
                        let r = add(row, k + 1)?;
                        b.start_row(None);
                        for (c, rep, text) in &cells {
                            for j in 0..(*rep).min(MAX_REPEAT) {
                                let at = add(*c, j)?;
                                b.cell("cell", &format!("{}{r}", column_letters(at)), text);
                            }
                        }
                        b.end_row();
                    }
                    row = add(row, repeat_row)?;
                }
                "table:table" => {
                    if let Some(s) = sheet.take() {
                        b.close(s);
                    }
                }
                _ => {}
            },
            Ev::Text(t) if para_depth > 0 => {
                if let Some(c) = cell.as_mut() {
                    c.2.push_str(&t);
                }
            }
            _ => {}
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}
```

- [ ] **Step 5:** GREEN, and the real files of Task 6.1 Step 5 converted to `odt`, `ods`, `odp`.
## Chunk 8: EPUB

### Task 8.1: src/docs/epub.rs

**Files:** `src/docs/epub.rs`, `src/docs/fixture/epub.rs`.

- [ ] **Step 1:** the writers of these formats (D19), replacing C4's stub; the tests below use
  them:

```rust
//! The EPUB writer (M4 plan, D19).
use super::{esc, zip};

/// An EPUB: chapters (title in the table of contents, XHTML body). `nav` writes an EPUB 3
/// navigation document; without it, an EPUB 2 NCX.
pub fn epub(chapters: &[(&str, &str)], nav: bool) -> Vec<u8> {
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut toc = String::new();
    for (i, (title, body)) in chapters.iter().enumerate() {
        let n = i + 1;
        manifest.push_str(&format!(
            r#"<item id="c{n}" href="text/c{n}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{n}"/>"#));
        let x = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{}</title><style>p {{ x: y }}</style></head><body>{body}</body></html>"#,
            esc(title)
        );
        parts.push((format!("OEBPS/text/c{n}.xhtml"), x.into_bytes()));
        if nav {
            toc.push_str(&format!(
                r#"<li><a href="text/c{n}.xhtml">{}</a></li>"#,
                esc(title)
            ));
        } else {
            toc.push_str(&format!(
                r#"<navPoint id="n{n}"><navLabel><text>{}</text></navLabel><content src="text/c{n}.xhtml"/></navPoint>"#,
                esc(title)
            ));
        }
    }
    let (toc_item, toc_attr, toc_part) = if nav {
        (
            r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
            "",
            (
                "OEBPS/nav.xhtml".to_string(),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>{toc}</ol></nav></body></html>"#
                ),
            ),
        )
    } else {
        (
            r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
            r#" toc="ncx""#,
            (
                "OEBPS/toc.ncx".to_string(),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>{toc}</navMap></ncx>"#
                ),
            ),
        )
    };
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><manifest>{toc_item}{manifest}</manifest><spine{toc_attr}>{spine}</spine></package>"#
    );
    let container = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let mut all = vec![
        ("mimetype".to_string(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".to_string(),
            container.as_bytes().to_vec(),
        ),
        ("OEBPS/content.opf".to_string(), opf.into_bytes()),
        (toc_part.0, toc_part.1.into_bytes()),
    ];
    all.extend(parts);
    let refs: Vec<(&str, Vec<u8>)> = all.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &["mimetype"])
}
```

- [ ] **Step 2:** the tests, below the stub:

```rust
#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::super::{Kind, extract};
    use crate::trail::trail_for;

    fn trail(doc: &[u8], needle: &str) -> Vec<String> {
        let e = extract(Kind::Epub, doc).unwrap();
        let at = memchr::memmem::find(&e.text, needle.as_bytes())
            .unwrap_or_else(|| panic!("{needle} not in {:?}", String::from_utf8_lossy(&e.text)));
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    const CH1: &str = "<h1>The Harbour</h1><p>A needle in the water.</p><h2>At night</h2><p>Second needle &amp; more.</p>";
    const CH2: &str = "<p>No heading, a needle.</p><script>needle()</script>";

    #[test]
    fn chapters_from_the_nav_and_headings_inside() {
        let d = fixture::epub(&[("1. Arrival", CH1), ("2. The Storm", CH2)], true);
        assert_eq!(trail(&d, "in the water"), ["1. Arrival", "The Harbour"]);
        assert_eq!(
            trail(&d, "Second"),
            ["1. Arrival", "The Harbour", "At night"]
        );
        assert_eq!(trail(&d, "No heading"), ["2. The Storm"]);
        let e = extract(Kind::Epub, &d).unwrap();
        // Scripts and the head are not text.
        assert!(memchr::memmem::find(&e.text, b"needle()").is_none());
        assert!(memchr::memmem::find(&e.text, b"x: y").is_none());
        assert!(memchr::memmem::find(&e.text, b"needle & more").is_some());
    }

    #[test]
    fn an_epub2_ncx_names_the_chapters_too() {
        let d = fixture::epub(&[("1. Arrival", CH1), ("2. The Storm", CH2)], false);
        assert_eq!(trail(&d, "No heading"), ["2. The Storm"]);
    }

    #[test]
    fn without_a_container_it_is_an_error() {
        let z = fixture::zip(
            &[("mimetype", b"application/epub+zip".to_vec())],
            &["mimetype"],
        );
        assert!(extract(Kind::Epub, &z).is_err());
    }
}
```

- [ ] **Step 3:** `cargo test --lib docs::epub`: 2 tests FAIL;
  `without_a_container_it_is_an_error` passes over the stub too, which also errs: RED.

- [ ] **Step 4:** the Extractor: `META-INF/container.xml`, the package document, its spine in
  reading order (`linear="no"` items left out), the table of contents for the chapters' names,
  and each XHTML document's text:

```rust
//! EPUB (spec §5.8): `3. The Storm › The harbour`. The spine's documents in order, each a
//! chapter named by its entry in the table of contents (the EPUB 3 `nav`, else the EPUB 2
//! NCX), or `chapter N` without one; inside, the XHTML's `h1`–`h6` are headings.
use super::xml::{Ev, Pull, attr, local};
use super::zipped::{Zip, resolve};
use super::{Builder, DocError};
use std::collections::HashMap;

pub fn extract(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let container = z
        .text("META-INF/container.xml")?
        .ok_or_else(|| DocError::broken("no META-INF/container.xml"))?;
    let opf_path = Pull::new(&container)
        .find_map(|ev| match ev {
            Ev::Start { name, attrs, .. } if local(name) == "rootfile" => {
                attr(attrs, "full-path").map(|s| s.into_owned())
            }
            _ => None,
        })
        .ok_or_else(|| DocError::broken("no rootfile in META-INF/container.xml"))?;
    let opf = z
        .text(&opf_path)?
        .ok_or_else(|| DocError::broken(format!("no package document {opf_path}")))?;
    // Manifest: id → (part, media type, properties); spine: ids in reading order.
    let mut manifest: HashMap<String, (String, String, String)> = HashMap::new();
    let mut spine = Vec::new();
    let mut ncx = None;
    for ev in Pull::new(&opf) {
        let Ev::Start { name, attrs, .. } = ev else {
            continue;
        };
        match local(name) {
            "item" => {
                if let (Some(id), Some(href)) = (attr(attrs, "id"), attr(attrs, "href")) {
                    let media = attr(attrs, "media-type").unwrap_or_default().into_owned();
                    let props = attr(attrs, "properties").unwrap_or_default().into_owned();
                    manifest.insert(id.into_owned(), (resolve(&opf_path, &href), media, props));
                }
            }
            "spine" => ncx = attr(attrs, "toc").map(|s| s.into_owned()),
            "itemref" => {
                if attr(attrs, "linear").as_deref() != Some("no")
                    && let Some(id) = attr(attrs, "idref")
                {
                    spine.push(id.into_owned());
                }
            }
            _ => {}
        }
    }
    let titles = titles(&mut z, &manifest, ncx.as_deref())?;
    let mut n = 0;
    for id in spine {
        let Some((part, media, _)) = manifest.get(&id) else {
            continue;
        };
        if !media.contains("html") {
            continue;
        }
        let Some(xhtml) = z.text(part)? else { continue };
        n += 1;
        let title = titles
            .get(part)
            .cloned()
            .unwrap_or_else(|| format!("chapter {n}"));
        b.chapter(1, &title);
        body(&xhtml, b);
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The first table-of-contents title of each part.
fn titles(
    z: &mut Zip,
    manifest: &HashMap<String, (String, String, String)>,
    ncx: Option<&str>,
) -> Result<HashMap<String, String>, DocError> {
    let mut out = HashMap::new();
    if let Some((nav, _, _)) = manifest
        .values()
        .find(|m| m.2.split_whitespace().any(|p| p == "nav"))
        && let Some(xml) = z.text(nav)?
    {
        // <nav epub:type="toc"> … <a href="c1.xhtml#s">Title</a>
        let (mut in_toc, mut depth, mut link): (bool, usize, Option<(String, String)>) =
            (false, 0, None);
        for ev in Pull::new(&xml) {
            match ev {
                Ev::Start {
                    name,
                    attrs,
                    empty: false,
                } if local(name) == "nav" => {
                    if in_toc {
                        depth += 1;
                    } else if attr(attrs, "type")
                        .is_some_and(|t| t.split_whitespace().any(|t| t == "toc"))
                    {
                        in_toc = true;
                        depth = 0;
                    }
                }
                Ev::End { name } if local(name) == "nav" && in_toc => {
                    if depth == 0 {
                        in_toc = false;
                    } else {
                        depth -= 1;
                    }
                }
                Ev::Start {
                    name,
                    attrs,
                    empty: false,
                } if in_toc && local(name) == "a" => {
                    link = attr(attrs, "href").map(|h| (resolve(nav, &h), String::new()));
                }
                Ev::Text(t) => {
                    if let Some(l) = link.as_mut() {
                        l.1.push_str(&t);
                    }
                }
                Ev::End { name } if local(name) == "a" => {
                    if let Some((part, title)) = link.take() {
                        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
                        if !title.is_empty() {
                            out.entry(part).or_insert(title);
                        }
                    }
                }
                _ => {}
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    let Some((ncx, _, _)) = ncx.and_then(|id| manifest.get(id)) else {
        return Ok(out);
    };
    let Some(xml) = z.text(ncx)? else {
        return Ok(out);
    };
    // <navPoint><navLabel><text>Title</text></navLabel><content src="c1.xhtml"/>
    let (mut label, mut in_text) = (Vec::<String>::new(), false);
    for ev in Pull::new(&xml) {
        match ev {
            Ev::Start {
                name, empty: false, ..
            } if local(name) == "navPoint" => label.push(String::new()),
            Ev::Start {
                name, empty: false, ..
            } if local(name) == "text" => in_text = true,
            Ev::End { name } if local(name) == "text" => in_text = false,
            Ev::Text(t) if in_text => {
                if let Some(l) = label.last_mut() {
                    l.push_str(&t);
                }
            }
            Ev::Start { name, attrs, .. } if local(name) == "content" => {
                if let (Some(src), Some(l)) = (attr(attrs, "src"), label.last()) {
                    let title = l.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !title.is_empty() {
                        out.entry(resolve(ncx, &src)).or_insert(title);
                    }
                }
            }
            Ev::End { name } if local(name) == "navPoint" => {
                label.pop();
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The text of an XHTML document's body: a line per block, headings by their level.
fn body(xhtml: &str, b: &mut Builder) {
    let mut line = String::new();
    let mut heading: Option<(u8, String)> = None;
    let mut skip = 0usize;
    let mut in_body = false;
    let flush = |line: &mut String, b: &mut Builder| {
        b.line(line);
        line.clear();
    };
    for ev in Pull::new(xhtml) {
        match ev {
            Ev::Start { name, empty, .. } => {
                let l = local(name);
                if l == "body" {
                    in_body = true;
                }
                if matches!(l, "script" | "style" | "head") && !empty {
                    skip += 1;
                    continue;
                }
                if let Some(level) = l
                    .strip_prefix('h')
                    .and_then(|d| d.parse::<u8>().ok())
                    .filter(|d| (1..=6).contains(d))
                    && !empty
                {
                    flush(&mut line, b);
                    heading = Some((level, String::new()));
                } else if l == "br" {
                    match heading.as_mut() {
                        Some(h) => h.1.push(' '),
                        None => flush(&mut line, b),
                    }
                } else if block(l) {
                    flush(&mut line, b);
                }
            }
            Ev::End { name } => {
                let l = local(name);
                if matches!(l, "script" | "style" | "head") {
                    skip = skip.saturating_sub(1);
                    continue;
                }
                if l.len() == 2 && l.starts_with('h') && heading.is_some() {
                    let (level, text) = heading.take().unwrap();
                    b.heading(level, &text);
                } else if block(l) {
                    flush(&mut line, b);
                }
            }
            Ev::Text(t) if skip == 0 && in_body => match heading.as_mut() {
                Some(h) => h.1.push_str(&t),
                None => line.push_str(&t),
            },
            _ => {}
        }
        if b.full() {
            return;
        }
    }
    flush(&mut line, b);
}

/// Elements whose start and end break the line.
fn block(l: &str) -> bool {
    matches!(
        l,
        "p" | "div"
            | "li"
            | "dt"
            | "dd"
            | "tr"
            | "td"
            | "th"
            | "blockquote"
            | "pre"
            | "section"
            | "article"
            | "aside"
            | "header"
            | "footer"
            | "figcaption"
            | "caption"
            | "table"
            | "ul"
            | "ol"
            | "dl"
            | "hr"
            | "figure"
            | "nav"
            | "body"
    )
}
```

- [ ] **Step 5:** GREEN. No EPUB was on the probe machine: a real book is to verify in chunk 8
  (`soffice --headless --convert-to epub report.odt` writes one).
## Chunk 9: PDF

### Task 9.1: src/docs/pdf.rs

**Files:** `src/docs/pdf.rs`, `src/docs/fixture/pdf.rs`.

- [ ] **Step 1:** the writers of these formats (D19), replacing C4's stub; the tests below use
  them:

```rust
//! The PDF writers (M4 plan, D19): lopdf's own writer, and its RC4 encryption.

/// A PDF with Helvetica text, one `Tj` per line 14 points apart, its streams compressed, and
/// `bookmarks` as (level, title, 1-based page).
pub fn pdf(pages: &[&[&str]], bookmarks: &[(u8, &str, usize)]) -> Vec<u8> {
    let mut doc = pdf_doc(pages, bookmarks);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// `pdf`, encrypted (RC4, 128 bits) with `user` as the password needed to open it ("" opens
/// it without one, as most encrypted PDFs).
pub fn pdf_encrypted(pages: &[&[&str]], user: &str) -> Vec<u8> {
    let mut doc = pdf_doc(pages, &[]);
    let version = lopdf::EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: user,
        key_length: 128,
        permissions: lopdf::Permissions::all(),
    };
    let state = lopdf::EncryptionState::try_from(version).unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn pdf_doc(pages: &[&[&str]], bookmarks: &[(u8, &str, usize)]) -> lopdf::Document {
    use lopdf::content::{Content, Operation};
    use lopdf::{Dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    // The file identifier, which encryption keys are derived from.
    let id = Object::String(
        b"searchfor-fixtur".to_vec(),
        lopdf::StringFormat::Hexadecimal,
    );
    doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    let pages_id = doc.new_object_id();
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let font_id = doc.add_object(font);
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));
    let mut res = Dictionary::new();
    res.set("Font", Object::Dictionary(fonts));
    let res_id = doc.add_object(res);
    let mut kids = Vec::new();
    for lines in pages {
        let mut ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new("Td", vec![72.into(), 760.into()]),
        ];
        for (i, l) in lines.iter().enumerate() {
            if i > 0 {
                ops.push(Operation::new("Td", vec![0.into(), (-14).into()]));
            }
            // WinAnsi: the Latin-1 letters of the line, as one byte each.
            let bytes: Vec<u8> = l
                .chars()
                .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
                .collect();
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(bytes, lopdf::StringFormat::Literal)],
            ));
        }
        ops.push(Operation::new("ET", vec![]));
        let content = Content { operations: ops };
        let mut stream = Stream::new(Dictionary::new(), content.encode().unwrap());
        let _ = stream.compress();
        let content_id = doc.add_object(stream);
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        kids.push(Object::Reference(doc.add_object(page)));
    }
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Count", Object::Integer(kids.len() as i64));
    pages_dict.set("Kids", Object::Array(kids.clone()));
    pages_dict.set("Resources", Object::Reference(res_id));
    pages_dict.set(
        "MediaBox",
        Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    );
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    if !bookmarks.is_empty() {
        let outlines_id = doc.new_object_id();
        // Items are written flat per level: each item's parent is the last item one level up.
        let ids: Vec<_> = bookmarks.iter().map(|_| doc.new_object_id()).collect();
        let mut dicts: Vec<Dictionary> = Vec::new();
        for (i, (level, title, page)) in bookmarks.iter().enumerate() {
            let mut d = Dictionary::new();
            d.set(
                "Title",
                Object::String(title.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            );
            let parent = (0..i)
                .rev()
                .find(|&j| bookmarks[j].0 < *level)
                .map_or(outlines_id, |j| ids[j]);
            d.set("Parent", Object::Reference(parent));
            d.set(
                "Dest",
                Object::Array(vec![kids[page - 1].clone(), Object::Name(b"Fit".to_vec())]),
            );
            dicts.push(d);
        }
        for i in 0..bookmarks.len() {
            let level = bookmarks[i].0;
            // The next sibling: the next item at this level before one at a higher level.
            if let Some(j) = (i + 1..bookmarks.len())
                .take_while(|&j| bookmarks[j].0 >= level)
                .find(|&j| bookmarks[j].0 == level)
            {
                dicts[i].set("Next", Object::Reference(ids[j]));
            }
            if let Some(j) = (i + 1..bookmarks.len())
                .next()
                .filter(|&j| bookmarks[j].0 == level + 1)
            {
                dicts[i].set("First", Object::Reference(ids[j]));
            }
        }
        for (id, d) in ids.iter().zip(dicts) {
            doc.objects.insert(*id, Object::Dictionary(d));
        }
        let mut outlines = Dictionary::new();
        outlines.set("Type", Object::Name(b"Outlines".to_vec()));
        outlines.set("First", Object::Reference(ids[0]));
        doc.objects
            .insert(outlines_id, Object::Dictionary(outlines));
        catalog.set("Outlines", Object::Reference(outlines_id));
    }
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc.compress();
    doc
}
```

- [ ] **Step 2:** the tests, below the stub:

```rust
#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::super::{DocError, Kind, extract};
    use crate::trail::trail_for;

    fn trail(doc: &[u8], needle: &str) -> Vec<String> {
        let e = extract(Kind::Pdf, doc).unwrap();
        let at = memchr::memmem::find(&e.text, needle.as_bytes())
            .unwrap_or_else(|| panic!("{needle} not in {:?}", String::from_utf8_lossy(&e.text)));
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    fn report() -> Vec<u8> {
        fixture::pdf(
            &[
                &["Report", "front matter needle"],
                &["1 Intro", "the intro needle"],
                &["2 Costs", "costs needle"],
                &["2.1 Taxes", "taxes needle", "año café"],
            ],
            &[(1, "1 Intro", 2), (1, "2 Costs", 3), (2, "2.1 Taxes", 4)],
        )
    }

    #[test]
    fn bookmarks_are_chapters_holding_their_pages() {
        let d = report();
        assert_eq!(trail(&d, "front"), ["page 1"]);
        assert_eq!(trail(&d, "intro needle"), ["1 Intro", "page 2"]);
        assert_eq!(trail(&d, "costs needle"), ["2 Costs", "page 3"]);
        assert_eq!(
            trail(&d, "taxes needle"),
            ["2 Costs", "2.1 Taxes", "page 4"]
        );
    }

    /// D14: several bookmarks on one page start at the lines that hold their titles, and the
    /// page is a Segment under each of them.
    #[test]
    fn bookmarks_inside_a_page_split_it() {
        let d = fixture::pdf(
            &[
                &[
                    "Title page",
                    "1 Intro",
                    "intro needle",
                    "2 Costs",
                    "costs needle",
                ],
                &["more costs needle"],
            ],
            &[(1, "1 Intro", 1), (1, "2 Costs", 1), (1, "3 Missing", 2)],
        );
        assert_eq!(trail(&d, "Title page"), ["page 1"]);
        assert_eq!(trail(&d, "intro needle"), ["1 Intro", "page 1"]);
        assert_eq!(trail(&d, "costs needle"), ["2 Costs", "page 1"]);
        // A title not on its page starts the chapter at the top of the page.
        assert_eq!(trail(&d, "more costs"), ["3 Missing", "page 2"]);
    }

    #[test]
    fn lines_and_winansi_letters_come_through() {
        let e = extract(Kind::Pdf, &report()).unwrap();
        let text = String::from_utf8(e.text).unwrap();
        assert!(
            text.starts_with("Report\nfront matter needle\n1 Intro\n"),
            "{text}"
        );
        assert!(text.contains("año café\n"), "{text}");
    }

    #[test]
    fn an_empty_user_password_opens_and_another_one_does_not() {
        let open = fixture::pdf_encrypted(&[&["secret needle"]], "");
        assert_eq!(trail(&open, "secret"), ["page 1"]);
        let locked = fixture::pdf_encrypted(&[&["secret needle"]], "pw");
        assert_eq!(
            extract(Kind::Pdf, &locked).unwrap_err(),
            DocError::Encrypted
        );
    }

    #[test]
    fn a_broken_pdf_is_an_error() {
        assert!(matches!(
            extract(Kind::Pdf, b"%PDF-1.5\ngarbage"),
            Err(DocError::Broken(_))
        ));
        // A cut file: lopdf may recover part of it or fail, never panic.
        let d = report();
        for cut in [d.len() / 3, d.len() / 2, d.len() - 10] {
            let _ = extract(Kind::Pdf, &d[..cut]);
        }
    }

    use lopdf::{Dictionary, Document, Object, Stream, StringFormat};

    /// A one-page PDF whose page draws `content` with the font `font` as `/F1` and the Form
    /// XObjects `forms` (name, content, compressed) as `/Fm0`, `/Fm1`…, each with the page's
    /// resources.
    fn pdf_with(font: Dictionary, content: &[u8], forms: &[(&[u8], bool)]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let id = Object::String(b"searchfor-adverse".to_vec(), StringFormat::Hexadecimal);
        doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
        let pages_id = doc.new_object_id();
        let res_id = doc.new_object_id();
        // A stream is an object of its own, referred to.
        let mut font = font;
        for (_, v) in font.iter_mut() {
            if let Object::Stream(st) = v {
                *v = Object::Reference(doc.add_object(st.clone()));
            }
        }
        let font_id = doc.add_object(font);
        let mut xo = Dictionary::new();
        for (i, (body, compressed)) in forms.iter().enumerate() {
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"XObject".to_vec()));
            d.set("Subtype", Object::Name(b"Form".to_vec()));
            d.set("Resources", Object::Reference(res_id));
            let mut st = Stream::new(d, body.to_vec());
            if *compressed {
                st.compress().unwrap();
            }
            xo.set(format!("Fm{i}"), Object::Reference(doc.add_object(st)));
        }
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font_id));
        let mut res = Dictionary::new();
        res.set("Font", Object::Dictionary(fonts));
        res.set("XObject", Object::Dictionary(xo));
        doc.objects.insert(res_id, Object::Dictionary(res));
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.to_vec()));
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        page.set("Resources", Object::Reference(res_id));
        let page_id = doc.add_object(page);
        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Count", Object::Integer(1));
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let mut cat = Dictionary::new();
        cat.set("Type", Object::Name(b"Catalog".to_vec()));
        cat.set("Pages", Object::Reference(pages_id));
        let cat_id = doc.add_object(cat);
        doc.trailer.set("Root", Object::Reference(cat_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// A ToUnicode CMap mapping each `(code, text)` (codes of `width` bytes).
    fn cmap(width: usize, map: &[(u32, &str)]) -> Vec<u8> {
        let hex = |c: u32| format!("{c:0w$X}", w = width * 2);
        let mut s = format!(
            "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n/CMapName /T def /CMapType 2 def\n1 begincodespacerange <{}> <{}> endcodespacerange\n{} beginbfchar\n",
            hex(0),
            "FF".repeat(width),
            map.len()
        );
        for (c, t) in map {
            let u: String = t.encode_utf16().map(|u| format!("{u:04X}")).collect();
            s.push_str(&format!("<{}> <{u}>\n", hex(*c)));
        }
        s.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop end end\n");
        s.into_bytes()
    }

    fn simple_font(extra: impl FnOnce(&mut Dictionary)) -> Dictionary {
        let mut f = Dictionary::new();
        f.set("Type", Object::Name(b"Font".to_vec()));
        f.set("Subtype", Object::Name(b"Type1".to_vec()));
        f.set("BaseFont", Object::Name(b"ABCDEF+Subset".to_vec()));
        extra(&mut f);
        f
    }

    fn text_of(pdf: &[u8]) -> String {
        String::from_utf8(extract(Kind::Pdf, pdf).unwrap().text).unwrap()
    }

    /// A code the ToUnicode map does not hold reads through the font's `/Encoding` (LaTeX's
    /// `circlecopyrt` in a font whose map leaves it out was U+FFFD in the probe).
    #[test]
    fn a_code_the_map_leaves_out_reads_through_the_encoding() {
        let map = cmap(1, &[(0x6e, "n"), (0x65, "e")]);
        let font = simple_font(|f| {
            f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            );
        });
        let pdf = pdf_with(font, b"BT /F1 12 Tf 72 700 Td (needle) Tj ET", &[]);
        assert_eq!(text_of(&pdf), "needle\n");
    }

    /// Codex r1 #5: a simple font that declares both `/Encoding` and `/ToUnicode` is read
    /// through its ToUnicode map, which says what a subset font's codes mean; its ligatures
    /// expand.
    #[test]
    fn to_unicode_comes_before_the_encoding() {
        let map = cmap(
            1,
            &[
                (0x41, "n"),
                (0x42, "e"),
                (0x43, "d"),
                (0x44, "l"),
                (0x45, "\u{fb01}"),
            ],
        );
        let font = simple_font(|f| {
            f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            );
        });
        let pdf = pdf_with(
            font,
            b"BT /F1 12 Tf 72 700 Td (ABBCDB) Tj 0 -20 Td (EAB) Tj ET",
            &[],
        );
        assert_eq!(text_of(&pdf), "needle\nfine\n");
    }

    /// A one-page PDF drawing `content` with a Type0 font, Identity-H, whose ToUnicode map is
    /// `map`.
    fn type0(map: Vec<u8>, content: &[u8]) -> Vec<u8> {
        let mut cid = Dictionary::new();
        cid.set("Type", Object::Name(b"Font".to_vec()));
        cid.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
        cid.set("DW", Object::Integer(500));
        let mut font = Dictionary::new();
        font.set("Type", Object::Name(b"Font".to_vec()));
        font.set("Subtype", Object::Name(b"Type0".to_vec()));
        font.set("Encoding", Object::Name(b"Identity-H".to_vec()));
        font.set(
            "DescendantFonts",
            Object::Array(vec![Object::Dictionary(cid)]),
        );
        font.set(
            "ToUnicode",
            Object::Stream(Stream::new(Dictionary::new(), map)),
        );
        pdf_with(font, content, &[])
    }

    /// A Type0 font with Identity-H and a ToUnicode map, its words separated by `TJ` kerning.
    #[test]
    fn a_type0_font_reads_its_two_byte_codes() {
        let map = cmap(
            2,
            &[(1, "n"), (2, "e"), (3, "d"), (4, "l"), (5, "h"), (6, "r")],
        );
        let mut cid = Dictionary::new();
        cid.set("Type", Object::Name(b"Font".to_vec()));
        cid.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
        cid.set("DW", Object::Integer(500));
        let mut font = Dictionary::new();
        font.set("Type", Object::Name(b"Font".to_vec()));
        font.set("Subtype", Object::Name(b"Type0".to_vec()));
        font.set("Encoding", Object::Name(b"Identity-H".to_vec()));
        font.set(
            "DescendantFonts",
            Object::Array(vec![Object::Dictionary(cid)]),
        );
        font.set(
            "ToUnicode",
            Object::Stream(Stream::new(Dictionary::new(), map)),
        );
        let pdf = pdf_with(
            font,
            b"BT /F1 12 Tf 72 700 Td [<000100020002000300040002> -400 <0005000200060002>] TJ ET",
            &[],
        );
        assert_eq!(text_of(&pdf), "needle here\n");
    }

    fn compressed_zeros(n: usize) -> Stream {
        let mut st = Stream::new(Dictionary::new(), vec![b' '; n]);
        st.compress().unwrap();
        st
    }

    /// Codex r1 #1: a ToUnicode map and a Form that decompress past 64 MiB make the document
    /// fail, with the limit, instead of being read whole.
    #[test]
    fn a_bomb_in_a_font_or_a_form_is_refused() {
        let font = simple_font(|f| f.set("ToUnicode", Object::Stream(compressed_zeros(65 << 20))));
        let pdf = pdf_with(font, b"BT /F1 12 Tf (A) Tj ET", &[]);
        let e = extract(Kind::Pdf, &pdf).unwrap_err();
        assert!(matches!(&e, DocError::TooLarge(..)), "{e}");
        let font = simple_font(|_| {});
        let bomb = vec![b' '; 65 << 20];
        let pdf = pdf_with(font, b"/Fm0 Do", &[(&bomb, true)]);
        let e = extract(Kind::Pdf, &pdf).unwrap_err();
        assert!(matches!(&e, DocError::TooLarge(..)), "{e}");
    }

    /// Fable r1 #3: the streams of all the pages, Forms and fonts of a document spend one
    /// budget.
    #[test]
    fn the_streams_of_a_document_share_a_budget() {
        let doc = Document::with_version("1.5");
        let r = super::Reader::new(&doc);
        r.inflated(super::MAX_INFLATED).unwrap();
        let e = r.inflated(1).unwrap_err();
        assert!(matches!(&e, DocError::TooLarge(_, 1024)), "{e}");
    }

    /// The load filter of the object streams keeps every object: a file saved with object
    /// streams reads as it did (the probe's first filter lost them all, and hwloc's manual
    /// gave no text).
    #[test]
    fn a_file_with_object_streams_reads_whole() {
        let pdf = pdf_with(simple_font(|_| {}), b"BT /F1 12 Tf (needle) Tj ET", &[]);
        let mut doc = Document::load_mem(&pdf).unwrap();
        let mut packed = Vec::new();
        let options = lopdf::SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .build();
        doc.save_with_options(&mut packed, options).unwrap();
        assert!(memchr::memmem::find(&packed, b"/ObjStm").is_some());
        assert_eq!(text_of(&packed), "needle\n");
    }

    /// Fable r1 #10: a Form whose stream does not decompress draws nothing, and the page goes on.
    #[test]
    fn a_form_that_does_not_decompress_draws_nothing() {
        let page = b"/Fm0 Do BT /F1 12 Tf (needle) Tj ET";
        let pdf = pdf_with(simple_font(|_| {}), page, &[(&[b' '; 4096], true)]);
        let mut doc = Document::load_mem(&pdf).unwrap();
        for o in doc.objects.values_mut() {
            if let Object::Stream(st) = o
                && st.dict.get(b"Filter").is_ok()
            {
                st.set_content(b"not flate".to_vec());
            }
        }
        let mut broken = Vec::new();
        doc.save_to(&mut broken).unwrap();
        assert_eq!(text_of(&broken), "needle\n");
    }

    /// Eight nested Forms, each drawing the next sixteen times: 16⁷ draws of the last one.
    fn fan_out(text: bool) -> Vec<u8> {
        let mut forms: Vec<Vec<u8>> = Vec::new();
        for i in 0..8 {
            let mut body = if text {
                b"BT /F1 12 Tf (needle ) Tj ET ".to_vec()
            } else {
                Vec::new()
            };
            if i < 7 {
                for _ in 0..16 {
                    body.extend(format!("/Fm{} Do ", i + 1).as_bytes());
                }
            }
            forms.push(body);
        }
        let refs: Vec<(&[u8], bool)> = forms.iter().map(|f| (f.as_slice(), false)).collect();
        pdf_with(simple_font(|_| {}), b"/Fm0 Do", &refs)
    }

    /// Codex r1 #2: the text limit holds while the page is drawn, and the work of the content
    /// streams has a budget, so a page of Forms drawing Forms ends quickly either way.
    #[test]
    fn a_page_of_forms_drawing_forms_stops_at_its_budgets() {
        let started = std::time::Instant::now();
        let e = super::super::extract_with(Kind::Pdf, &fan_out(true), 1 << 20).unwrap();
        assert!(e.cut);
        assert!(e.text.len() <= 1 << 20, "{}", e.text.len());
        let e = extract(Kind::Pdf, &fan_out(false)).unwrap_err();
        assert!(
            matches!(&e, DocError::Broken(m) if m.contains("operations")),
            "{e}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "{:?}",
            started.elapsed()
        );
    }

    /// The content reader (M4 plan r2): strings with escapes and nested parentheses, hex
    /// strings, arrays, names, dictionaries, comments and inline images.
    #[test]
    fn the_content_reader_reads_one_operation_at_a_time() {
        use super::{Operand, Ops};
        let data = b"q 1 0 0 1 72 700 cm BT /F1 12 Tf (a\\)b\\050(n)) Tj <6E 65 6> Tj\n\
            [(x) -250 (y)] TJ ET % a comment\n/P << /MCID [1 2] >> BDC EMC\n\
            BI /W 1 /H 1 ID \0EI\xff EI Q";
        let mut ops = Ops::new(data);
        let mut o = Vec::new();
        let mut seen = Vec::new();
        while let Some(op) = ops.next(&mut o).unwrap() {
            seen.push(String::from_utf8_lossy(op).into_owned());
            match op {
                b"cm" => assert_eq!(o.len(), 6),
                b"Tj" => assert!(matches!(o[0], Operand::Literal(_) | Operand::Hex(_))),
                b"TJ" => assert!(matches!(&o[0], Operand::Array(a) if a.len() == 3)),
                b"BDC" => assert_eq!(o, [Operand::Name(b"P"), Operand::Other]),
                _ => {}
            }
            if op == b"Tj" {
                let t = o[0].string().unwrap();
                assert!(t == b"a)b((n)" || t == b"ne`", "{t:?}");
            }
        }
        assert_eq!(
            seen,
            [
                "q", "cm", "BT", "Tf", "Tj", "Tj", "TJ", "ET", "BDC", "EMC", "BI", "ID", "EI", "Q"
            ]
        );
    }

    /// Codex r2 #1: a range of a ToUnicode map stays a range, whatever its size: lopdf's
    /// reader listed each of its 2³² codes.
    #[test]
    fn a_map_range_is_read_as_a_range() {
        let mut map = cmap(2, &[]);
        let ranges =
            b"2 beginbfrange\n<0000> <FFFF> <0041>\n<00000000> <FFFFFFFF> <0061>\nendbfrange\n";
        let at = memchr::memmem::find(&map, b"endcmap").unwrap();
        map.splice(at..at, ranges.iter().copied());
        let started = std::time::Instant::now();
        let pdf = type0(map, b"BT /F1 12 Tf 72 700 Td <000000010005> Tj ET");
        assert_eq!(text_of(&pdf), "ABF\n");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    /// Codex r2 #2: a code that means 256 letters, repeated, is decoded only as far as the
    /// text has room, and what fits is kept.
    #[test]
    fn a_code_of_long_text_is_decoded_as_far_as_there_is_room() {
        let long = "n".repeat(256);
        let map = cmap(1, &[(0x41, long.as_str())]);
        let font = simple_font(|f| {
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            )
        });
        let mut content = b"BT /F1 12 Tf (".to_vec();
        content.extend(vec![b'A'; 64 << 10]);
        content.extend(b") Tj ET");
        let pdf = pdf_with(font, &content, &[]);
        let e = super::super::extract_with(Kind::Pdf, &pdf, 256 << 10).unwrap();
        assert!(e.cut);
        let n = e.text.len();
        assert!(n <= 256 << 10 && n > 128 << 10, "{n}");
        // Without the limit, a string writes at most `MAX_GROWTH` bytes for each of its bytes.
        let e = extract(Kind::Pdf, &pdf).unwrap();
        assert!(!e.cut);
        assert!(
            e.text.len() <= (64 << 10) * super::MAX_GROWTH + 65,
            "{}",
            e.text.len()
        );
    }

    /// Codex r2 #3: a Form of millions of operations spends its budget as they are read, and
    /// never becomes a list of them; `q` past `MAX_SAVES` saves nothing.
    #[test]
    fn a_form_of_millions_of_operations_is_refused_as_it_is_read() {
        let started = std::time::Instant::now();
        let body = b"q Q ".repeat(8 << 20);
        let pdf = pdf_with(simple_font(|_| {}), b"/Fm0 Do", &[(&body, true)]);
        let e = extract(Kind::Pdf, &pdf).unwrap_err();
        assert!(
            matches!(&e, DocError::Broken(m) if m.contains("operations")),
            "{e}"
        );
        let mut page = b"q ".repeat(1 << 20);
        page.extend(b"BT /F1 12 Tf (needle) Tj ET");
        assert_eq!(
            text_of(&pdf_with(simple_font(|_| {}), &page, &[])),
            "needle\n"
        );
        let mut tj = b"BT /F1 12 Tf [".to_vec();
        tj.extend(b"1 ".repeat(70_000));
        tj.extend(b"] TJ ET");
        let e = extract(Kind::Pdf, &pdf_with(simple_font(|_| {}), &tj, &[])).unwrap_err();
        assert!(
            matches!(&e, DocError::Broken(m) if m.contains("operands")),
            "{e}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn tj_kerning_wide_enough_is_a_space() {
        let data = b"BT /F1 10 Tf [(ne) -20 (edle) -300 (here)] TJ ET";
        let doc = lopdf::Document::with_version("1.5");
        let r = super::Reader::new(&doc);
        let res = super::Res::new(&r, Vec::new()).unwrap();
        let mut out = super::Layout::new(usize::MAX);
        super::run(&r, data, &res, super::Matrix::IDENTITY, &mut out, 0).unwrap();
        assert_eq!(out.out, "needle here");
    }
}
```

- [ ] **Step 3:** `cargo test --lib docs::pdf` does not compile: `Reader`, `Res`, `Layout`,
  `Matrix`, `run`, `Ops`, `Operand` and `MAX_INFLATED` do not exist: RED. The tests of review
  r2 were run against the round-1 code under `ulimit -v 6000000`:
  `a_map_range_is_read_as_a_range` and `a_form_of_millions_of_operations_is_refused_as_it_is_read`
  ran out of memory, and `a_code_of_long_text_is_decoded_as_far_as_there_is_room` kept no text
  (a string that did not fit was dropped whole). The four adversarial tests of review r1 were also run
  against the round-0 code, which had none of their fixes: `to_unicode_comes_before_the_encoding`
  gave `ABBCDB` (WinAnsi), `a_type0_font_reads_its_two_byte_codes` gave spaces (its fixture's
  map was a direct stream then, which lopdf does not read; it is an object now), the bombs
  were read whole, and the page of Forms drawing Forms ran for over 60 s.

- [ ] **Step 4:** the Extractor (D13, D14, D17, D29). A `Reader` holds what the pages of a
  document share: its fonts and its Forms, each read once (before the font cache one 296-page
  PDF took 0.54 s, after it 0.41 s; a Form is decompressed and given its resources once), and
  its budgets: the bytes all its streams decompress to (a content stream pages share counts
  once), and the work of its content streams. `Ops` reads a content stream one operation at a
  time (strings with escapes and nested parentheses, hex strings, arrays, dictionaries,
  comments, inline images), each operation spending one unit as it is read; `ToUnicode` reads a
  map's codespace ranges, `bfchar` and `bfrange` entries, a range kept as a range, and a code
  the map leaves out reads through the font's `/Encoding` when it has one that decides. lopdf
  is never given a map. A load filter counts each object stream's tokens before lopdf parses it.
  Widths come from `/Widths` (simple fonts, scaled by `FontMatrix` for Type 3) or `/W` and
  `/DW` (CID fonts). Every stream is read with `MAX_STREAM`, and lopdf's `MemoryLimitExceeded`
  is `TooLarge`, any other decompression error an empty page or Form:

```rust
//! PDF (spec §5.8): `3 Results › 3.2 Costs › page 42`. lopdf reads the file, its fonts and its
//! content streams; the text is laid out here, from the positions the content streams give
//! each string (M4 plan, D13): a string lower or higher than the last one by half its size starts
//! a line, and one further right than the last one ended by 0.15 of its size gets a space before
//! it, as does a TJ kerning of more than 0.1 em. The bookmarks are chapters, each starting at the
//! line of the page it points to that starts with its title, or at the top of that page (D14).
use super::{Builder, DocError};
use lopdf::{DecompressError, Dictionary, Document, Encoding, Object, ObjectId, Stream};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// The most bytes one stream may decompress to (64 MiB): a page, Form, font or object stream
/// past it makes the document fail, as a decompression bomb (D17).
const MAX_STREAM: usize = 64 << 20;
/// The most bytes all the page, Form and font streams of a document may decompress to, together
/// (1 GiB): many pages sharing one large stream are a bomb too (D17).
const MAX_INFLATED: usize = 1 << 30;
/// The most work the content streams of a document may do, plus `PAGE_WORK` for each of its
/// pages: each operation a page or a Form runs spends one, and each Form drawn `FORM_DRAW`
/// more. A small Form drawn by Forms drawn by Forms, or a page of millions of operations, is a
/// bomb of work (D17; M4 plan r2, Fable #1 and #2: 10 000 pages each drawing five Forms of 200
/// operations fit).
const MAX_WORK: usize = 8_000_000;
const PAGE_WORK: usize = 20_000;
/// The most bytes and tokens the object streams of a document may hold, together: lopdf
/// parses them whole when it loads the file, about 128 bytes of memory for each token (D17;
/// M4 plan r2, Fable #1).
const MAX_OBJSTM_BYTES: usize = 64 << 20;
const MAX_OBJSTM_TOKENS: usize = 2_000_000;
/// The most bytes one stream of an encrypted PDF may decompress to while lopdf loads it: lopdf
/// parses an encrypted file's object streams where `objstm` cannot count them.
const MAX_ENCRYPTED_LOAD: usize = 1 << 20;
/// The work of drawing a Form, before its operations run.
const FORM_DRAW: usize = 16;
/// Form XObjects nested deeper than this are not read.
const MAX_FORM_DEPTH: usize = 8;
/// At most this many bookmarks are read.
const MAX_BOOKMARKS: usize = 100_000;

pub fn extract(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let encrypted = memchr::memmem::find(bytes, b"/Encrypt").is_some();
    let options = lopdf::LoadOptions {
        max_decompressed_size: Some(if encrypted {
            MAX_ENCRYPTED_LOAD
        } else {
            MAX_STREAM
        }),
        filter: Some(objstm),
        ..Default::default()
    };
    OBJSTM.with(|c| c.set(ObjStmBudget::default()));
    let loaded = Document::load_mem_with_options(bytes, options);
    if let Some(e) = OBJSTM.with(|c| c.get()).refused {
        return Err(e.error());
    }
    let doc = loaded.map_err(load_error)?;
    if doc.is_encrypted() && doc.encryption_state.is_none() {
        return Err(DocError::Encrypted);
    }
    let pages = doc.get_pages();
    let numbers: HashMap<ObjectId, u32> = pages.iter().map(|(n, id)| (*id, *n)).collect();
    let mut marks = bookmarks(&doc, &numbers);
    marks.sort_by_key(|m| m.2);
    let mut marks = marks.into_iter().peekable();
    let r = Reader::new(&doc);
    for (n, id) in &pages {
        let mut text = Layout::new(b.remaining());
        let res = Res::of_page(&r, *id)?;
        match page_content(&doc, *id) {
            Ok(content) => {
                if r.contents.borrow_mut().insert(doc.get_page_contents(*id)) {
                    r.inflated(content.len())?;
                }
                run(&r, &content, &res, Matrix::IDENTITY, &mut text, 0)?;
            }
            // A stream past the limit fails the document; any other error leaves the page
            // empty (lopdf reads a stream that does not decompress as it is, which draws
            // nothing), and the next page is read.
            Err(e) => {
                if let Ok(e) = too_large(e, "a page's content") {
                    return Err(e);
                }
            }
        }
        let lines: Vec<&str> = text.out.lines().collect();
        // Each bookmark of this page starts at the first line, from the previous one's, that
        // starts with its title; else at the top of the page (D14).
        let mut starts = Vec::new();
        let mut from = 0;
        while let Some((level, title, _)) = marks.next_if(|m| m.2 <= *n) {
            let want = squeeze(&title);
            let at = (from..lines.len())
                .find(|&i| !want.is_empty() && squeeze(lines[i]).starts_with(&want))
                .unwrap_or(from);
            from = at;
            starts.push((at, level, title));
        }
        let name = format!("page {n}");
        let mut starts = starts.into_iter().peekable();
        let mut page = None;
        for (i, line) in lines.iter().enumerate() {
            // A chapter that starts inside the page splits it: the page is a Segment under
            // each chapter it holds part of.
            while let Some((_, level, title)) = starts.next_if(|s| s.0 <= i) {
                if let Some(p) = page.take() {
                    b.close(p);
                }
                b.chapter(level, &title);
            }
            if page.is_none() {
                page = Some(b.open("page", &name));
            }
            b.line(line);
        }
        for (_, level, title) in starts {
            if let Some(p) = page.take() {
                b.close(p);
            }
            b.chapter(level, &title);
        }
        if let Some(p) = page {
            b.close(p);
        }
        if text.full {
            b.mark_cut();
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// A page's content: its one stream decompressed, or its streams joined by lopdf, which
/// copies them (a page of one 63 MiB stream took 135 MiB that way; M4 plan r2).
fn page_content(doc: &Document, id: ObjectId) -> Result<Vec<u8>, lopdf::Error> {
    let ids = doc.get_page_contents(id);
    let [one] = ids.as_slice() else {
        return doc.get_page_content_with_limit(id, MAX_STREAM);
    };
    let Ok(s) = doc.get_object(*one).and_then(Object::as_stream) else {
        return Ok(Vec::new());
    };
    match s.decompressed_content_with_limit(MAX_STREAM) {
        Err(e @ lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. })) => Err(e),
        // As lopdf: a stream that does not decompress is read as it is.
        Err(_) if s.content.len() <= MAX_STREAM => Ok(s.content.clone()),
        Err(e) => Err(e),
        Ok(data) => Ok(data),
    }
}

/// What the object streams of the document being loaded on this thread have spent, and
/// whether one was refused (lopdf's load filter is a plain function).
#[derive(Clone, Copy, Default)]
struct ObjStmBudget {
    bytes: usize,
    tokens: usize,
    refused: Option<Refused>,
}

#[derive(Clone, Copy)]
enum Refused {
    Bytes,
    Tokens,
}

impl Refused {
    fn error(self) -> DocError {
        match self {
            Refused::Bytes => {
                DocError::TooLarge("its object streams".into(), MAX_OBJSTM_BYTES >> 20)
            }
            Refused::Tokens => DocError::broken(format!(
                "its object streams hold more than {MAX_OBJSTM_TOKENS} tokens"
            )),
        }
    }
}

thread_local! {
    static OBJSTM: Cell<ObjStmBudget> = Cell::new(ObjStmBudget::default());
}

/// lopdf's load filter: an object stream is decompressed and its tokens counted before lopdf
/// parses it, and past the document's `MAX_OBJSTM_BYTES` or `MAX_OBJSTM_TOKENS` it is left out
/// and the document refused.
fn objstm(id: ObjectId, o: &mut Object) -> Option<(ObjectId, Object)> {
    // lopdf keeps a file's object as the filter leaves it, and an object stream's object as
    // the filter returns it: both, then.
    let Object::Stream(s) = o else {
        return Some((id, o.clone()));
    };
    if !s.dict.has_type(b"ObjStm") {
        return Some((id, o.clone()));
    }
    let mut b = OBJSTM.with(|c| c.get());
    if b.refused.is_some() {
        return None;
    }
    match s.decompressed_content_with_limit(MAX_OBJSTM_BYTES) {
        Ok(data) => {
            b.bytes += data.len();
            b.tokens += tokens(&data);
        }
        Err(lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. })) => {
            b.bytes = usize::MAX;
        }
        // lopdf fails to read it too.
        Err(_) => {}
    }
    if b.bytes > MAX_OBJSTM_BYTES {
        b.refused = Some(Refused::Bytes);
    } else if b.tokens > MAX_OBJSTM_TOKENS {
        b.refused = Some(Refused::Tokens);
    }
    OBJSTM.with(|c| c.set(b));
    b.refused.is_none().then(|| (id, o.clone()))
}

/// How many tokens `data` holds, about: each run of regular bytes, and each delimiter.
fn tokens(data: &[u8]) -> usize {
    let mut n = 0;
    let mut in_word = false;
    for &c in data {
        if white(c) {
            in_word = false;
        } else if delimiter(c) {
            n += 1;
            in_word = false;
        } else if !in_word {
            n += 1;
            in_word = true;
        }
    }
    n
}

/// The words of `s` joined by single spaces.
fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `e` as the error of a stream of `what` over its 64 MiB, when it is that.
fn too_large(e: lopdf::Error, what: &str) -> Result<DocError, lopdf::Error> {
    match e {
        lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. }) => {
            Ok(DocError::TooLarge(what.into(), MAX_STREAM >> 20))
        }
        e => Err(e),
    }
}

fn load_error(e: lopdf::Error) -> DocError {
    match too_large(e, "a stream of the file") {
        Ok(e) => e,
        Err(lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword) => DocError::Encrypted,
        Err(lopdf::Error::Decompress(e)) => DocError::broken(e),
        Err(e) => DocError::broken(format!("not a PDF lopdf can read ({e})")),
    }
}

/// The bookmarks as (level, title, page number), in outline order.
fn bookmarks(doc: &Document, numbers: &HashMap<ObjectId, u32>) -> Vec<(u8, String, u32)> {
    let mut out = Vec::new();
    let Ok(catalog) = doc.catalog() else {
        return out;
    };
    let Some(first) = catalog
        .get_deref(b"Outlines", doc)
        .and_then(Object::as_dict)
        .ok()
        .and_then(|o| o.get(b"First").ok())
        .and_then(|f| f.as_reference().ok())
    else {
        return out;
    };
    let mut seen = HashSet::new();
    // (node, level)
    let mut stack = vec![(first, 1u8)];
    while let Some((id, level)) = stack.pop() {
        if out.len() >= MAX_BOOKMARKS || !seen.insert(id) {
            continue;
        }
        let Ok(item) = doc.get_dictionary(id) else {
            continue;
        };
        // Siblings after this one, then its children first (depth first, in order).
        if let Ok(next) = item.get(b"Next").and_then(Object::as_reference) {
            stack.push((next, level));
        }
        if let Ok(child) = item.get(b"First").and_then(Object::as_reference) {
            stack.push((child, level.saturating_add(1)));
        }
        let title = item
            .get_deref(b"Title", doc)
            .ok()
            .and_then(|t| lopdf::decode_text_string(t).ok())
            .unwrap_or_default();
        if let Some(page) = destination(doc, item).and_then(|p| numbers.get(&p)) {
            out.push((level, title, *page));
        }
    }
    out
}

/// The page a bookmark points to: its `/Dest`, or the `/D` of a GoTo action, an explicit array
/// or a named destination.
fn destination(doc: &Document, item: &Dictionary) -> Option<ObjectId> {
    let dest = match item.get_deref(b"Dest", doc) {
        Ok(d) => d,
        Err(_) => item
            .get_deref(b"A", doc)
            .ok()?
            .as_dict()
            .ok()?
            .get_deref(b"D", doc)
            .ok()?,
    };
    page_of(doc, dest, 0)
}

fn page_of(doc: &Document, dest: &Object, depth: usize) -> Option<ObjectId> {
    if depth > 4 {
        return None;
    }
    match dest {
        Object::Array(a) => a.first()?.as_reference().ok(),
        Object::Dictionary(d) => page_of(doc, d.get_deref(b"D", doc).ok()?, depth + 1),
        Object::Name(n) | Object::String(n, _) => {
            let target = named(doc, n)?;
            page_of(doc, target, depth + 1)
        }
        Object::Reference(r) => page_of(doc, doc.get_object(*r).ok()?, depth + 1),
        _ => None,
    }
}

/// A named destination: in the catalog's `/Dests` dictionary, or its `/Names /Dests` tree.
fn named<'a>(doc: &'a Document, name: &[u8]) -> Option<&'a Object> {
    let catalog = doc.catalog().ok()?;
    if let Ok(dests) = catalog.get_deref(b"Dests", doc).and_then(Object::as_dict)
        && let Ok(d) = dests.get_deref(name, doc)
    {
        return Some(d);
    }
    let tree = catalog
        .get_deref(b"Names", doc)
        .ok()?
        .as_dict()
        .ok()?
        .get_deref(b"Dests", doc)
        .ok()?
        .as_dict()
        .ok()?;
    let mut nodes = vec![(tree, 0)];
    let mut visits = 0;
    while let Some((node, depth)) = nodes.pop() {
        visits += 1;
        if visits > 10_000 || depth > 32 {
            return None;
        }
        if let Ok(names) = node.get_deref(b"Names", doc).and_then(Object::as_array) {
            for pair in names.chunks(2) {
                if let [k, v] = pair
                    && k.as_str().ok() == Some(name)
                {
                    return doc.dereference(v).ok().map(|(_, o)| o);
                }
            }
        }
        if let Ok(kids) = node.get_deref(b"Kids", doc).and_then(Object::as_array) {
            for k in kids {
                if let Ok((_, Object::Dictionary(d))) = doc.dereference(k) {
                    nodes.push((d, depth + 1));
                }
            }
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix([f64; 6]);

impl Matrix {
    const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self × other`, in PDF's row-vector convention: apply `self`, then `other`.
    fn then(self, o: Matrix) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = o.0;
        Matrix([
            a * a2 + b * c2,
            a * b2 + b * d2,
            c * a2 + d * c2,
            c * b2 + d * d2,
            e * a2 + f * c2 + e2,
            e * b2 + f * d2 + f2,
        ])
    }

    fn translate(x: f64, y: f64) -> Matrix {
        Matrix([1.0, 0.0, 0.0, 1.0, x, y])
    }

    fn of(ops: &[Operand]) -> Option<Matrix> {
        let v: Vec<f64> = ops.iter().filter_map(Operand::num).collect();
        (v.len() == 6).then(|| Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]))
    }

    fn from(ops: &[Object]) -> Option<Matrix> {
        let v: Vec<f64> = ops.iter().filter_map(num).collect();
        (v.len() == 6).then(|| Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]))
    }
}

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// What a document's pages share as they are read: its fonts and Forms, read once each, and
/// the budgets all its pages spend (D17).
struct Reader<'a> {
    doc: &'a Document,
    fonts: RefCell<HashMap<ObjectId, Rc<Font<'a>>>>,
    /// Each Form, read the first time it is drawn.
    forms: RefCell<HashMap<ObjectId, Rc<Form<'a>>>>,
    inflated: Cell<usize>,
    work: Cell<usize>,
    /// `MAX_WORK` and the `PAGE_WORK` of the document's pages.
    limit: usize,
    /// The content streams of the pages read so far: one a page shares with another is
    /// counted once in `MAX_INFLATED` (M4 plan r2, Fable #3).
    contents: RefCell<HashSet<Vec<ObjectId>>>,
}

impl<'a> Reader<'a> {
    fn new(doc: &'a Document) -> Self {
        Reader {
            doc,
            limit: MAX_WORK.saturating_add(PAGE_WORK.saturating_mul(doc.get_pages().len())),
            contents: RefCell::default(),
            fonts: RefCell::default(),
            forms: RefCell::default(),
            inflated: Cell::new(0),
            work: Cell::new(0),
        }
    }

    /// Spends `n` decompressed bytes of the document's `MAX_INFLATED`.
    fn inflated(&self, n: usize) -> Result<(), DocError> {
        let total = self.inflated.get().saturating_add(n);
        self.inflated.set(total);
        if total > MAX_INFLATED {
            return Err(DocError::TooLarge(
                "the whole document, decompressed,".into(),
                MAX_INFLATED >> 20,
            ));
        }
        Ok(())
    }

    /// Spends `n` units of the work of the document's content streams.
    fn work(&self, n: usize) -> Result<(), DocError> {
        let total = self.work.get().saturating_add(n);
        self.work.set(total);
        if total > self.limit {
            return Err(DocError::broken(format!(
                "its content streams run more than {} operations",
                self.limit
            )));
        }
        Ok(())
    }

    /// The font `d`, object `id`.
    fn font(&self, id: Option<ObjectId>, d: &'a Dictionary) -> Result<Rc<Font<'a>>, DocError> {
        let Some(id) = id else {
            return Ok(Rc::new(font(self, d)?));
        };
        if let Some(f) = self.fonts.borrow().get(&id) {
            return Ok(f.clone());
        }
        let f = Rc::new(font(self, d)?);
        self.fonts.borrow_mut().insert(id, f.clone());
        Ok(f)
    }

    /// The Form `s`, object `id`: its content stream decompressed (counted in the document's
    /// `MAX_INFLATED`, which bounds what the cache holds), or none when it does not decompress.
    fn form(&self, id: Option<ObjectId>, s: &'a Stream) -> Result<Rc<Form<'a>>, DocError> {
        if let Some(f) = id.and_then(|id| self.forms.borrow().get(&id).cloned()) {
            return Ok(f);
        }
        let data = match s.decompressed_content_with_limit(MAX_STREAM) {
            Ok(data) => {
                self.inflated(data.len())?;
                data
            }
            Err(e) => match too_large(e, "a Form") {
                Ok(e) => return Err(e),
                Err(_) => Vec::new(),
            },
        };
        let matrix = s
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(|m| m.as_array().ok())
            .and_then(|a| Matrix::from(a))
            .unwrap_or(Matrix::IDENTITY);
        // A Form without resources of its own uses its parent's.
        let res = match s
            .dict
            .get_deref(b"Resources", self.doc)
            .and_then(Object::as_dict)
        {
            Ok(own) => Some(Res::new(self, vec![own])?),
            Err(_) => None,
        };
        let f = Rc::new(Form { data, matrix, res });
        if let Some(id) = id {
            self.forms.borrow_mut().insert(id, f.clone());
        }
        Ok(f)
    }
}

/// A Form XObject: its content stream, its matrix, and its own resources if it has them.
struct Form<'a> {
    data: Vec<u8>,
    matrix: Matrix,
    res: Option<Res<'a>>,
}

/// A font of a page or form: how its strings decode and how wide each code is.
struct Font<'a> {
    decode: Decode<'a>,
    /// Bytes per code: 2 for a composite (Type0) font, 1 otherwise.
    bytes: usize,
    /// Code → width in thousandths of the size; `default` for the others.
    widths: HashMap<u32, f64>,
    default: f64,
}

/// The resources a content stream runs with: its resource dictionaries (a page's own and the
/// ones it inherits, or a form's), and their fonts.
struct Res<'a> {
    dicts: Vec<&'a Dictionary>,
    fonts: BTreeMap<Vec<u8>, Rc<Font<'a>>>,
}

impl<'a> Res<'a> {
    fn of_page(r: &Reader<'a>, page: ObjectId) -> Result<Res<'a>, DocError> {
        let doc = r.doc;
        let mut dicts = Vec::new();
        if let Ok((own, inherited)) = doc.get_page_resources(page) {
            dicts.extend(own);
            dicts.extend(
                inherited
                    .iter()
                    .filter_map(|id| doc.get_dictionary(*id).ok()),
            );
        }
        Res::new(r, dicts)
    }

    fn new(r: &Reader<'a>, dicts: Vec<&'a Dictionary>) -> Result<Res<'a>, DocError> {
        let doc = r.doc;
        let mut fonts = BTreeMap::new();
        for d in &dicts {
            let Ok(f) = d.get_deref(b"Font", doc).and_then(Object::as_dict) else {
                continue;
            };
            for (k, v) in f.iter() {
                if fonts.contains_key(k) {
                    continue;
                }
                let Ok((id, Object::Dictionary(d))) = doc.dereference(v) else {
                    continue;
                };
                fonts.insert(k.clone(), r.font(id, d)?);
            }
        }
        Ok(Res { dicts, fonts })
    }

    /// The Form XObject `name`, and its object.
    fn form(&self, doc: &'a Document, name: &[u8]) -> Option<(Option<ObjectId>, &'a Stream)> {
        for r in &self.dicts {
            let Ok(xo) = r.get_deref(b"XObject", doc).and_then(Object::as_dict) else {
                continue;
            };
            if let Ok((id, Object::Stream(s))) = xo.get(name).and_then(|o| doc.dereference(o)) {
                let is_form =
                    s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form");
                return is_form.then_some((id, s));
            }
        }
        None
    }
}

/// How a font's strings decode.
enum Decode<'a> {
    /// Through its ToUnicode map, read by our code, and for a code the map does not hold
    /// through the font's `/Encoding` when it has one that decides.
    Map(ToUnicode, Option<Encoding<'a>>),
    /// Through the encoding lopdf reads from `/Encoding`.
    Encoding(Encoding<'a>),
    /// As UTF-8.
    Raw,
}

/// How the strings of the font `d` decode: through its ToUnicode map when it has one that
/// reads, which says what each code means even in a subset font whose `/Encoding` does not
/// (D13); else through its `/Encoding`. lopdf is never given the map: it would read it again,
/// and its reader lists every code of every range (M4 plan r2, Codex #1). A stream of either
/// past `MAX_STREAM` fails the document.
fn decode<'a>(r: &Reader<'a>, d: &'a Dictionary) -> Result<Decode<'a>, DocError> {
    let doc = r.doc;
    if let Ok(map) = d.get_deref(b"ToUnicode", doc).and_then(Object::as_stream) {
        match map.get_plain_content_with_limit(MAX_STREAM) {
            Ok(plain) => {
                r.inflated(plain.len())?;
                let m = ToUnicode::parse(&plain)?;
                if !m.chars.is_empty() || !m.ranges.is_empty() {
                    let fallback = match decides(d, doc) {
                        true => d.get_font_encoding_with_limit(doc, MAX_STREAM).ok(),
                        false => None,
                    };
                    return Ok(Decode::Map(m, fallback));
                }
            }
            Err(e) => {
                if let Ok(e) = too_large(e, "a font's ToUnicode map") {
                    return Err(e);
                }
            }
        }
        if !decides(d, doc) {
            return Ok(Decode::Raw);
        }
    }
    match d.get_font_encoding_with_limit(doc, MAX_STREAM) {
        Ok(e) => Ok(Decode::Encoding(e)),
        Err(e) => match too_large(e, "a font's ToUnicode map") {
            Ok(e) => Err(e),
            Err(_) => Ok(Decode::Raw),
        },
    }
}

/// Whether the font `d` has an `/Encoding` lopdf reads without its ToUnicode map: one that is
/// not Identity (lopdf reads the map without one, or with an Identity one).
fn decides(d: &Dictionary, doc: &Document) -> bool {
    let identity = matches!(
        d.get_deref(b"Encoding", doc).and_then(Object::as_name),
        Ok(b"Identity-H" | b"Identity-V")
    );
    !identity && d.get(b"Encoding").is_ok()
}

impl Font<'_> {
    /// The text of the string `s`, at most `room` bytes of it (and one code more); true when
    /// it stopped there. Nothing is decoded past the room (M4 plan r2, Codex #2).
    fn text(&self, s: &[u8], room: usize) -> (String, bool) {
        let mut out = String::new();
        match &self.decode {
            Decode::Map(m, fallback) => {
                for (at, len) in Codes::new(s, Some(self)) {
                    if out.len() > room {
                        return (out, true);
                    }
                    let piece = &s[at..at + len];
                    if !m.write(len, code(piece), &mut out) {
                        match fallback
                            .as_ref()
                            .and_then(|e| Document::decode_text(e, piece).ok())
                        {
                            Some(t) => out.push_str(&t),
                            None => out.push('\u{fffd}'),
                        }
                    }
                }
            }
            Decode::Encoding(e) => {
                for piece in s.chunks(4096 * self.bytes) {
                    if out.len() > room {
                        return (out, true);
                    }
                    out.push_str(&Document::decode_text(e, piece).unwrap_or_default());
                }
            }
            Decode::Raw => return lossy(s, room),
        }
        let cut = out.len() > room;
        (out, cut)
    }
}

fn font<'a>(r: &Reader<'a>, d: &'a Dictionary) -> Result<Font<'a>, DocError> {
    let doc = r.doc;
    let decode = decode(r, d)?;
    let composite = d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type0");
    let mut widths = HashMap::new();
    let mut default = 500.0;
    if composite {
        let cid = d
            .get_deref(b"DescendantFonts", doc)
            .and_then(Object::as_array)
            .ok()
            .and_then(|a| a.first())
            .and_then(|f| doc.dereference(f).ok())
            .and_then(|(_, f)| f.as_dict().ok());
        if let Some(cid) = cid {
            default = cid.get(b"DW").ok().and_then(num).unwrap_or(1000.0);
            if let Ok(w) = cid.get_deref(b"W", doc).and_then(Object::as_array) {
                let mut i = 0;
                while i + 1 < w.len() && widths.len() < 100_000 {
                    let Some(first) = num(&w[i]) else { break };
                    let first = first as u32;
                    match doc.dereference(&w[i + 1]).map(|(_, o)| o) {
                        Ok(Object::Array(list)) => {
                            for (k, v) in list.iter().enumerate() {
                                if let Some(v) = num(v) {
                                    widths.insert(first + k as u32, v);
                                }
                            }
                            i += 2;
                        }
                        _ => {
                            let (Some(last), Some(v)) =
                                (num(&w[i + 1]), w.get(i + 2).and_then(num))
                            else {
                                break;
                            };
                            for c in first..=(last as u32).min(first + 65_535) {
                                widths.insert(c, v);
                            }
                            i += 3;
                        }
                    }
                }
            }
        }
    } else {
        let scale = if d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type3") {
            d.get_deref(b"FontMatrix", doc)
                .and_then(Object::as_array)
                .ok()
                .and_then(|m| m.first().and_then(num))
                .map_or(1.0, |s| s * 1000.0)
        } else {
            1.0
        };
        let first = d.get(b"FirstChar").ok().and_then(num).unwrap_or(0.0) as u32;
        if let Ok(w) = d.get_deref(b"Widths", doc).and_then(Object::as_array) {
            for (k, v) in w.iter().enumerate().take(256) {
                if let Some(v) = doc.dereference(v).ok().and_then(|(_, o)| num(o)) {
                    widths.insert(first + k as u32, v * scale);
                }
            }
        }
        if let Some(missing) = d
            .get_deref(b"FontDescriptor", doc)
            .and_then(Object::as_dict)
            .ok()
            .and_then(|fd| fd.get(b"MissingWidth").ok())
            .and_then(num)
            .filter(|w| *w > 0.0)
        {
            default = missing * scale;
        }
    }
    Ok(Font {
        decode,
        bytes: if composite { 2 } else { 1 },
        widths,
        default,
    })
}

/// The text of a page as it is laid out: where the last string ended, to decide whether the
/// next one starts a line or needs a space.
struct Layout {
    out: String,
    /// The last string: where it ended (x, y, in device space), its size, and whether its
    /// font gave the widths of its glyphs.
    last: Option<Last>,
    /// The bytes of text the document has room for still: the page stops being drawn when
    /// its text reaches them (D17).
    room: usize,
    full: bool,
}

#[derive(Clone, Copy)]
struct Last {
    x: f64,
    y: f64,
    size: f64,
    widths: bool,
}

impl Layout {
    fn new(room: usize) -> Self {
        Layout {
            out: String::new(),
            last: None,
            room,
            full: false,
        }
    }

    /// `space` is a TJ kerning wide enough to be a space before `s`. Without the widths of the
    /// last string's glyphs, where it ended is a guess, so only that kerning makes a space.
    fn push(&mut self, s: &str, x: f64, y: f64, last: Last, space: bool) {
        if s.is_empty() || self.full {
            return;
        }
        // A ligature's letters take no more bytes than it does, and a string writes at most
        // one break or space before it. A string that does not fit writes what fits.
        let mut s = s;
        if self.out.len() + s.len() + 1 > self.room {
            self.full = true;
            let mut fits = self.room.saturating_sub(self.out.len() + 1).min(s.len());
            while !s.is_char_boundary(fits) {
                fits -= 1;
            }
            s = &s[..fits];
            if s.is_empty() {
                return;
            }
        }
        if let Some(l) = self.last {
            let tall = last.size.max(l.size);
            let gap = l.widths && (x - l.x > 0.15 * tall || x < l.x - tall);
            if (y - l.y).abs() > 0.5 * tall {
                self.out.push('\n');
            } else if (space || gap) && !self.out.ends_with([' ', '\n']) && !s.starts_with(' ') {
                self.out.push(' ');
            }
        }
        // The ligatures of Unicode's Alphabetic Presentation Forms, as their letters.
        for c in s.chars() {
            match c {
                '\u{fb00}' => self.out.push_str("ff"),
                '\u{fb01}' => self.out.push_str("fi"),
                '\u{fb02}' => self.out.push_str("fl"),
                '\u{fb03}' => self.out.push_str("ffi"),
                '\u{fb04}' => self.out.push_str("ffl"),
                '\u{fb05}' | '\u{fb06}' => self.out.push_str("st"),
                c => self.out.push(c),
            }
        }
        self.last = Some(last);
    }
}

#[derive(Clone, Copy)]
struct State {
    ctm: Matrix,
    font: Option<usize>,
    size: f64,
    tc: f64,
    tw: f64,
    th: f64,
    tl: f64,
    rise: f64,
}

/// The most bytes of text a string may write for each of its bytes (D17).
const MAX_GROWTH: usize = 8;

/// The most `q` a content stream may have open: past it a `q` saves nothing, and its `Q`
/// restores nothing (D17).
const MAX_SAVES: usize = 256;

/// Runs a content stream, writing the text it shows into `out`, until the text is full. Its
/// operations are read one at a time (`Ops`), so a stream never becomes a list of them, and
/// each one spends the document's work as it is read (D17).
fn run<'a>(
    r: &Reader<'a>,
    data: &[u8],
    res: &Res<'a>,
    ctm: Matrix,
    out: &mut Layout,
    depth: usize,
) -> Result<(), DocError> {
    let doc = r.doc;
    let keys: Vec<&Vec<u8>> = res.fonts.keys().collect();
    let mut gs = State {
        ctm,
        font: None,
        size: 0.0,
        tc: 0.0,
        tw: 0.0,
        th: 1.0,
        tl: 0.0,
        rise: 0.0,
    };
    let mut stack = Vec::new();
    // The `q` past `MAX_SAVES`, which their `Q` match first.
    let mut lost = 0usize;
    let (mut tm, mut tlm) = (Matrix::IDENTITY, Matrix::IDENTITY);
    // A TJ kerning wide enough to be a space, waiting for the next string.
    let mut kern_space = false;
    let mut ops = Ops::new(data);
    let mut o = Vec::new();
    while let Some(op) = ops.next(&mut o)? {
        if out.full {
            break;
        }
        r.work(1)?;
        let n = |i: usize| o.get(i).and_then(Operand::num).unwrap_or(0.0);
        match op {
            b"q" if stack.len() < MAX_SAVES => stack.push(gs),
            b"q" => lost += 1,
            b"Q" if lost > 0 => lost -= 1,
            b"Q" => gs = stack.pop().unwrap_or(gs),
            b"cm" => {
                if let Some(m) = Matrix::of(&o) {
                    gs.ctm = m.then(gs.ctm);
                }
            }
            b"BT" => {
                tm = Matrix::IDENTITY;
                tlm = tm;
            }
            b"Tf" => {
                gs.font = match o.first() {
                    Some(Operand::Name(f)) => keys.iter().position(|k| k.as_slice() == *f),
                    _ => None,
                };
                gs.size = n(1);
            }
            b"Tc" => gs.tc = n(0),
            b"Tw" => gs.tw = n(0),
            b"Tz" => gs.th = n(0) / 100.0,
            b"TL" => gs.tl = n(0),
            b"Ts" => gs.rise = n(0),
            b"Td" => {
                tlm = Matrix::translate(n(0), n(1)).then(tlm);
                tm = tlm;
            }
            b"TD" => {
                gs.tl = -n(1);
                tlm = Matrix::translate(n(0), n(1)).then(tlm);
                tm = tlm;
            }
            b"Tm" => {
                if let Some(m) = Matrix::of(&o) {
                    tm = m;
                    tlm = m;
                }
            }
            b"T*" => {
                tlm = Matrix::translate(0.0, -gs.tl).then(tlm);
                tm = tlm;
            }
            b"Tj" | b"'" | b"\"" | b"TJ" => {
                if op == b"'" || op == b"\"" {
                    if op == b"\"" {
                        gs.tw = n(0);
                        gs.tc = n(1);
                    }
                    tlm = Matrix::translate(0.0, -gs.tl).then(tlm);
                    tm = tlm;
                }
                let items: &[Operand] = match (op, o.last()) {
                    (b"TJ", Some(Operand::Array(a))) => a,
                    (_, Some(s)) => std::slice::from_ref(s),
                    _ => continue,
                };
                let font = gs.font.map(|i| res.fonts[keys[i]].as_ref());
                for item in items {
                    if let Some(s) = item.string() {
                        show(&s, font, &gs, &mut tm, out, kern_space);
                        kern_space = false;
                    } else {
                        let k = item.num().unwrap_or(0.0);
                        tm = Matrix::translate(-k / 1000.0 * gs.size * gs.th, 0.0).then(tm);
                        if k < -100.0 {
                            kern_space = true;
                        }
                    }
                }
            }
            b"Do" if depth < MAX_FORM_DEPTH => {
                let Some(Operand::Name(name)) = o.first() else {
                    continue;
                };
                let Some((id, stream)) = res.form(doc, name) else {
                    continue;
                };
                let form = r.form(id, stream)?;
                r.work(FORM_DRAW)?;
                let inner = form.res.as_ref().unwrap_or(res);
                let m = form.matrix.then(gs.ctm);
                run(r, &form.data, inner, m, out, depth + 1)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Writes one string: decoded, placed where the text matrix puts it, and the matrix advanced
/// by its glyphs' widths. The text it decodes stops at what is left of the document's room.
fn show(s: &[u8], font: Option<&Font>, gs: &State, tm: &mut Matrix, out: &mut Layout, space: bool) {
    if out.full {
        return;
    }
    let trm = Matrix([gs.size * gs.th, 0.0, 0.0, gs.size, 0.0, gs.rise])
        .then(*tm)
        .then(gs.ctm);
    let size = (trm.0[2] * trm.0[2] + trm.0[3] * trm.0[3]).sqrt();
    let (x, y) = (trm.0[4], trm.0[5]);
    // A string writes at most `MAX_GROWTH` bytes of text for each of its bytes: a ToUnicode
    // map may give a code 256 letters, which made a 5 KB file 512 MiB of text (M4 plan r2).
    let room = out.room.saturating_sub(out.out.len());
    let most = s.len().saturating_mul(MAX_GROWTH).saturating_add(64);
    let (text, cut) = match font {
        Some(f) => f.text(s, room.min(most)),
        None => lossy(s, room.min(most)),
    };
    let cut = cut && room <= most;
    let text = if text.len() > most {
        let mut end = most;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        &text[..]
    };
    let (widths, default) = font.map_or((None, 500.0), |f| (Some(&f.widths), f.default));
    let known = widths.is_some_and(|w| !w.is_empty());
    let mut advance = 0.0;
    for (at, len) in Codes::new(s, font) {
        let c = code(&s[at..at + len]);
        let w = widths.and_then(|w| w.get(&c)).copied().unwrap_or(default);
        let word = if len == 1 && c == 32 { gs.tw } else { 0.0 };
        advance += (w / 1000.0 * gs.size + gs.tc + word) * gs.th;
    }
    *tm = Matrix::translate(advance, 0.0).then(*tm);
    let end = Matrix::translate(0.0, gs.rise).then(*tm).then(gs.ctm).0[4];
    let last = Last {
        x: end,
        y,
        size: size.abs().max(0.1),
        widths: known,
    };
    out.push(text, x, y, last, space);
    if cut {
        out.full = true;
    }
}

/// A code's bytes as a number, big-endian.
fn code(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |a, b| a << 8 | *b as u32)
}

/// `s` as UTF-8, invalid bytes replaced, at most `room` bytes of it; true when it stopped
/// there.
fn lossy(s: &[u8], room: usize) -> (String, bool) {
    let mut out = String::new();
    for piece in s.chunks(4096) {
        if out.len() >= room {
            return (out, true);
        }
        out.push_str(&String::from_utf8_lossy(piece));
    }
    let cut = out.len() > room;
    (out, cut)
}

/// The codes of a string in a font, as (start, length): by its ToUnicode map's codespace
/// ranges when it has them, else by its code size.
struct Codes<'s, 'f> {
    s: &'s [u8],
    at: usize,
    bytes: usize,
    map: Option<&'f ToUnicode>,
}

impl<'s, 'f> Codes<'s, 'f> {
    fn new(s: &'s [u8], font: Option<&'f Font>) -> Self {
        let map = font.and_then(|f| match &f.decode {
            Decode::Map(m, _) => Some(m),
            _ => None,
        });
        Codes {
            s,
            at: 0,
            bytes: font.map_or(1, |f| f.bytes),
            map,
        }
    }
}

impl Iterator for Codes<'_, '_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        let rest = &self.s[self.at..];
        if rest.is_empty() {
            return None;
        }
        let len = self
            .map
            .and_then(|m| m.code_len(rest))
            .unwrap_or(self.bytes)
            .min(rest.len());
        let at = self.at;
        self.at += len;
        Some((at, len))
    }
}

// --- Content streams ---

/// The most operands one operation may have, the items of its arrays and dictionaries counted
/// (D17): a `TJ` of a line has tens.
const MAX_OPERANDS: usize = 65_536;
/// Arrays and dictionaries nested deeper than this end the stream.
const MAX_NESTING: usize = 32;

/// An operand of a content stream (or of a CMap), borrowing the stream's bytes.
#[derive(Debug, Clone, PartialEq)]
enum Operand<'s> {
    Num(f64),
    /// A literal string, between its parentheses, escapes not yet read.
    Literal(&'s [u8]),
    /// A hexadecimal string's digits, between `<` and `>`.
    Hex(&'s [u8]),
    Name(&'s [u8]),
    Array(Vec<Operand<'s>>),
    /// A dictionary, `true`, `false` or `null`: read past, never used.
    Other,
}

impl Operand<'_> {
    fn num(&self) -> Option<f64> {
        match self {
            Operand::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// A string's bytes, its escapes or hexadecimal digits read.
    fn string(&self) -> Option<Vec<u8>> {
        match self {
            Operand::Literal(b) => Some(literal(b)),
            Operand::Hex(h) => Some(hex(h)),
            _ => None,
        }
    }
}

fn literal(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        i += 1;
        if c != b'\\' {
            out.push(c);
            continue;
        }
        let Some(&e) = b.get(i) else { break };
        i += 1;
        match e {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'0'..=b'7' => {
                let mut v = (e - b'0') as u32;
                for _ in 0..2 {
                    match b.get(i) {
                        Some(d @ b'0'..=b'7') => {
                            v = v * 8 + (d - b'0') as u32;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                out.push(v as u8);
            }
            // A backslash at the end of a line continues the string on the next.
            b'\r' => {
                if b.get(i) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\n' => {}
            e => out.push(e),
        }
    }
    out
}

fn hex(h: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = h
        .iter()
        .filter_map(|c| (*c as char).to_digit(16).map(|d| d as u8))
        .collect();
    digits
        .chunks(2)
        .map(|p| p[0] << 4 | p.get(1).copied().unwrap_or(0))
        .collect()
}

fn white(c: u8) -> bool {
    matches!(c, 0 | b'\t' | b'\n' | 12 | b'\r' | b' ')
}

fn delimiter(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// The operations of a content stream, one at a time: each operator, after its operands. What
/// it cannot read is skipped a byte at a time, as a reader does.
struct Ops<'s> {
    data: &'s [u8],
    at: usize,
    /// Operands and items read for the current operation.
    items: usize,
}

impl<'s> Ops<'s> {
    fn new(data: &'s [u8]) -> Self {
        Ops {
            data,
            at: 0,
            items: 0,
        }
    }

    /// The next operator, its operands in `operands`; None at the end.
    fn next(&mut self, operands: &mut Vec<Operand<'s>>) -> Result<Option<&'s [u8]>, DocError> {
        operands.clear();
        self.items = 0;
        loop {
            self.skip();
            let Some(&c) = self.data.get(self.at) else {
                return Ok(None);
            };
            if c == b')' || c == b'>' || c == b']' || c == b'}' || c == b'{' {
                self.at += 1;
                continue;
            }
            if !delimiter(c) {
                let word = self.word();
                match Self::number(word) {
                    Some(n) => self.push(operands, Operand::Num(n))?,
                    None if matches!(word, b"true" | b"false" | b"null") => {
                        self.push(operands, Operand::Other)?
                    }
                    None => {
                        if word == b"ID" {
                            self.skip_image();
                        }
                        return Ok(Some(word));
                    }
                }
                continue;
            }
            let o = self.object(0)?;
            self.push(operands, o)?;
        }
    }

    fn push(&mut self, operands: &mut Vec<Operand<'s>>, o: Operand<'s>) -> Result<(), DocError> {
        self.count()?;
        operands.push(o);
        Ok(())
    }

    fn count(&mut self) -> Result<(), DocError> {
        self.items += 1;
        if self.items > MAX_OPERANDS {
            return Err(DocError::broken(format!(
                "an operation has more than {MAX_OPERANDS} operands"
            )));
        }
        Ok(())
    }

    /// White space and comments.
    fn skip(&mut self) {
        while let Some(&c) = self.data.get(self.at) {
            if white(c) {
                self.at += 1;
            } else if c == b'%' {
                while self
                    .data
                    .get(self.at)
                    .is_some_and(|c| *c != b'\n' && *c != b'\r')
                {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    /// The bytes up to the next white space or delimiter.
    fn word(&mut self) -> &'s [u8] {
        let start = self.at;
        while self
            .data
            .get(self.at)
            .is_some_and(|c| !white(*c) && !delimiter(*c))
        {
            self.at += 1;
        }
        &self.data[start..self.at]
    }

    fn number(w: &[u8]) -> Option<f64> {
        let first = *w.first()?;
        if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
            return None;
        }
        std::str::from_utf8(w).ok()?.parse().ok()
    }

    /// An object that starts with a delimiter (or, inside an array, anything).
    fn object(&mut self, depth: usize) -> Result<Operand<'s>, DocError> {
        let data = self.data;
        let c = data[self.at];
        match c {
            b'(' => {
                self.at += 1;
                let start = self.at;
                let mut open = 1usize;
                while let Some(&c) = data.get(self.at) {
                    self.at += 1;
                    match c {
                        b'\\' => self.at += 1,
                        b'(' => open += 1,
                        b')' => {
                            open -= 1;
                            if open == 0 {
                                return Ok(Operand::Literal(&data[start..self.at - 1]));
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Operand::Literal(&data[start.min(data.len())..]))
            }
            b'<' if data.get(self.at + 1) == Some(&b'<') => {
                self.at += 2;
                self.nested(depth, b">>")?;
                Ok(Operand::Other)
            }
            b'<' => {
                self.at += 1;
                let start = self.at;
                while data.get(self.at).is_some_and(|c| *c != b'>') {
                    self.at += 1;
                }
                let end = self.at.min(data.len());
                self.at += 1;
                Ok(Operand::Hex(&data[start..end]))
            }
            b'[' => {
                self.at += 1;
                Ok(Operand::Array(self.nested(depth, b"]")?))
            }
            b'/' => {
                self.at += 1;
                Ok(Operand::Name(self.word()))
            }
            _ if !delimiter(c) => {
                let w = self.word();
                Ok(Self::number(w).map_or(Operand::Other, Operand::Num))
            }
            _ => {
                self.at += 1;
                Ok(Operand::Other)
            }
        }
    }

    /// The items of an array or a dictionary, up to `end`.
    fn nested(&mut self, depth: usize, end: &[u8]) -> Result<Vec<Operand<'s>>, DocError> {
        if depth >= MAX_NESTING {
            self.at = self.data.len();
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.at >= self.data.len() {
                return Ok(items);
            }
            if self.data[self.at..].starts_with(end) {
                self.at += end.len();
                return Ok(items);
            }
            if end == b"]" && self.data[self.at] == b'>' {
                self.at += 1;
                continue;
            }
            self.count()?;
            items.push(self.object(depth + 1)?);
        }
    }

    /// After `ID`, the bytes of an inline image, up to its `EI`.
    fn skip_image(&mut self) {
        let d = self.data;
        let mut i = self.at + 1;
        while i + 2 <= d.len() {
            if &d[i..i + 2] == b"EI"
                && white(d[i - 1])
                && d.get(i + 2).is_none_or(|c| white(*c) || delimiter(*c))
            {
                self.at = i;
                return;
            }
            i += 1;
        }
        self.at = d.len();
    }
}

// --- ToUnicode maps ---

/// The most entries a ToUnicode map may hold: its single codes, its ranges and the strings of
/// its range arrays (D17). A range counts as one, whatever its size: ranges are kept as
/// ranges, never one entry per code.
const MAX_MAP: usize = 200_000;
/// The most UTF-16 units a code may map to (the CMap format's 512 bytes).
const MAX_DEST: usize = 256;

/// A font's ToUnicode CMap, read by our code (D13): its codespace ranges tell how many bytes
/// each code takes, and its `bfchar` and `bfrange` entries what text each code means.
#[derive(Debug, Default)]
struct ToUnicode {
    /// (bytes, low, high).
    spaces: Vec<(usize, u32, u32)>,
    chars: HashMap<(usize, u32), String>,
    /// (bytes, low, high, what the codes mean), sorted.
    ranges: Vec<(usize, u32, u32, Dest)>,
}

#[derive(Debug)]
enum Dest {
    /// The text of the range's first code; each next code adds one to its last unit.
    From(Vec<u16>),
    /// The text of each code of the range.
    Each(Vec<String>),
}

impl ToUnicode {
    fn parse(data: &[u8]) -> Result<ToUnicode, DocError> {
        let mut m = ToUnicode::default();
        let mut entries = 0usize;
        let mut ops = Ops::new(data);
        let mut o = Vec::new();
        let code_of = |o: &Operand| -> Option<(usize, u32)> {
            let b = o.string()?;
            (1..=4).contains(&b.len()).then(|| (b.len(), code(&b)))
        };
        let units = |o: &Operand| -> Option<Vec<u16>> {
            let b = o.string()?;
            (b.len() <= 2 * MAX_DEST).then(|| {
                b.chunks(2)
                    .map(|p| (p[0] as u16) << 8 | p.get(1).copied().unwrap_or(0) as u16)
                    .collect()
            })
        };
        while let Some(op) = ops.next(&mut o)? {
            match op {
                b"endcodespacerange" => {
                    for p in o.chunks(2) {
                        if let [lo, hi] = p
                            && let (Some((n, lo)), Some((_, hi))) = (code_of(lo), code_of(hi))
                        {
                            m.spaces.push((n, lo, hi));
                        }
                    }
                }
                b"endbfchar" => {
                    for p in o.chunks(2) {
                        if let [src, dst] = p
                            && let (Some(src), Some(dst)) = (code_of(src), units(dst))
                        {
                            entries += 1;
                            m.chars.insert(src, String::from_utf16_lossy(&dst));
                        }
                    }
                }
                b"endbfrange" => {
                    for p in o.chunks(3) {
                        let [lo, hi, dst] = p else { continue };
                        let (Some((n, lo)), Some((_, hi))) = (code_of(lo), code_of(hi)) else {
                            continue;
                        };
                        if hi < lo {
                            continue;
                        }
                        entries += 1;
                        let dest = match dst {
                            Operand::Array(list) => {
                                entries += list.len();
                                Dest::Each(
                                    list.iter()
                                        .filter_map(units)
                                        .map(|u| String::from_utf16_lossy(&u))
                                        .collect(),
                                )
                            }
                            d => match units(d) {
                                Some(u) if !u.is_empty() => Dest::From(u),
                                _ => continue,
                            },
                        };
                        m.ranges.push((n, lo, hi, dest));
                    }
                }
                _ => {}
            }
            if entries > MAX_MAP {
                return Err(DocError::broken(format!(
                    "a font's ToUnicode map has more than {MAX_MAP} entries"
                )));
            }
        }
        m.ranges.sort_by_key(|r| (r.0, r.1));
        Ok(m)
    }

    /// How many bytes the code at the start of `s` takes, by the codespace ranges.
    fn code_len(&self, s: &[u8]) -> Option<usize> {
        self.spaces
            .iter()
            .filter(|(n, lo, hi)| s.len() >= *n && (*lo..=*hi).contains(&code(&s[..*n])))
            .map(|(n, ..)| *n)
            .min()
    }

    /// What the code `c` of `n` bytes means, written into `out`; false when the map does not
    /// say.
    fn write(&self, n: usize, c: u32, out: &mut String) -> bool {
        if let Some(t) = self.chars.get(&(n, c)) {
            out.push_str(t);
            return true;
        }
        let i = self.ranges.partition_point(|r| (r.0, r.1) <= (n, c));
        if let Some((rn, lo, hi, dest)) = i.checked_sub(1).map(|i| &self.ranges[i])
            && *rn == n
            && c <= *hi
        {
            let k = (c - lo) as usize;
            match dest {
                Dest::From(u) => {
                    let mut u = u.clone();
                    if let Some(last) = u.last_mut() {
                        *last = last.wrapping_add(k as u16);
                    }
                    out.push_str(&String::from_utf16_lossy(&u));
                }
                Dest::Each(list) => match list.get(k) {
                    Some(t) => out.push_str(t),
                    None => out.push('\u{fffd}'),
                },
            }
            return true;
        }
        false
    }
}
```

- [ ] **Step 5:** GREEN: 18 tests, 10 s in debug. Then the quality check of D13 over real PDFs against `pdftotext`
  (poppler): extract both, and compare the multiset of words of 3 letters or more (recall and
  precision) and of adjacent word pairs (recall), with
  `/tmp/subtask/m4-plan/cmp.py` of the probe. The probe's six PDFs give the numbers of D13
  with the reader of D29 too, and the same text but for one line of `bdb_usenix.pdf` (`2`
  and `48`, a superscript, now on one line as `248`). The first probe of D29 lost every object
  of an object stream (lopdf keeps a stream's objects as the load filter returns them), which
  `hwloc-a4.pdf` showed as no text: `a_file_with_object_streams_reads_whole` keeps it. The probe's PDFs came from the system's
  documentation (Apple pstopdf, Apache FOP, pdfTeX, Ghostscript) and from LibreOffice; an
  encrypted one with an empty and with a real user password was made with `qpdf --encrypt`.
## Chunk 10: SQLite

### Task 10.1: src/docs/sqlite.rs

**Files:** `src/docs/sqlite.rs`.

- [ ] **Step 1:** the tests, below the stub (they read C4's fixtures):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_are_sqlites() {
        assert_eq!(varint(&[0x05], 0), Some((5, 1)));
        assert_eq!(varint(&[0x81, 0x00], 0), Some((128, 2)));
        assert_eq!(varint(&[0xff; 9], 0), Some((u64::MAX, 9)));
        assert_eq!(varint(&[0x81], 0), None);
    }

    #[test]
    fn create_table_gives_the_column_names() {
        let c = columns(
            "CREATE TABLE \"users\" (id INTEGER PRIMARY KEY, \"full name\" TEXT NOT NULL, [e,mail] TEXT, `n` INT DEFAULT (1+2), s REAL, CONSTRAINT u UNIQUE(email), CHECK (n > 0))",
        );
        assert_eq!(c.names, ["id", "full name", "e,mail", "n", "s"]);
        assert_eq!(c.real, [false, false, false, false, true]);
        assert_eq!((c.ipk, c.without_rowid), (Some(0), false));
        let c = columns("CREATE TABLE kv(k TEXT PRIMARY KEY, v) WITHOUT ROWID");
        assert_eq!(c.names, ["k", "v"]);
        assert_eq!((c.ipk, c.without_rowid), (None, true));
        // A `[` inside a bracketed name does not close it (found in the probe's review).
        assert_eq!(columns("CREATE TABLE t([a[b] TEXT, c)").names, ["a[b", "c"]);
        let c = columns("CREATE TABLE t(a INTEGER, b, PRIMARY KEY(a))");
        assert_eq!(c.names, ["a", "b"]);
        assert_eq!(c.ipk, Some(0));
    }

    #[test]
    fn values_print_as_sqlite_shows_them() {
        assert_eq!(Value::Float(100.0).shown(), "100.0");
        assert_eq!(Value::Float(1.5).shown(), "1.5");
        assert_eq!(Value::Int(-3).shown(), "-3");
        assert_eq!(Value::Null.shown(), "");
        assert_eq!(Value::Blob(12).shown(), "[blob 12 bytes]");
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/docs/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn extracted(name: &str) -> super::super::Extracted {
        super::super::extract(super::super::Kind::Sqlite, &fixture(name)).unwrap()
    }

    fn trail(e: &super::super::Extracted, needle: &str) -> Vec<String> {
        let at = memchr::memmem::find(&e.text, needle.as_bytes()).expect(needle);
        crate::trail::trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn tables_rows_and_columns() {
        let e = extracted("shop.sqlite");
        assert_eq!(trail(&e, "ana.needle"), ["users", "row 12", "email"]);
        // The INTEGER PRIMARY KEY is the rowid, which the row stores as NULL.
        let text = String::from_utf8_lossy(&e.text);
        assert!(
            text.contains("12\tUser 12\tana.needle@example.com\t18.0\n"),
            "{text}"
        );
        assert!(text.contains("1\tshort note\t[blob 8 bytes]\n"));
        // A value longer than its page continues on overflow pages.
        assert_eq!(trail(&e, "needle at the end"), ["notes", "row 2", "body"]);
        // WITHOUT ROWID: rows in key order, numbered by position.
        assert_eq!(trail(&e, "value of gamma"), ["kv", "row 3", "v"]);
        assert_eq!(trail(&e, "value of alpha"), ["kv", "row 1", "v"]);
        // Indexes and views are not tables.
        assert!(!text.contains("CREATE"));
    }

    /// Codex r1 #4 and #12: a WITHOUT ROWID table stores its primary key first; each value is
    /// named by its declared column and written in the declared order, with its affinity; a NULL
    /// keeps its empty field.
    #[test]
    fn values_follow_their_declared_columns() {
        let e = extracted("shop.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        assert_eq!(trail(&e, "needle payload"), ["late", "row 1", "payload"]);
        assert!(text.contains("\nneedle payload\tk1\n"), "{text}");
        assert!(text.contains("\na1\t3.0\tc1\n"), "{text}");
        assert!(text.contains("\n\tneedle gap\n"), "{text}");
        assert_eq!(trail(&e, "needle gap"), ["gaps", "row 1", "y"]);
        let c = columns("CREATE TABLE t(a, b, c, PRIMARY KEY (c DESC, a)) WITHOUT ROWID");
        assert_eq!(c.stored, [2, 0, 1]);
        assert_eq!(
            columns("CREATE TABLE t(a, b PRIMARY KEY) WITHOUT ROWID").stored,
            [1, 0]
        );
        assert_eq!(columns("CREATE TABLE t(a, b)").stored, [0, 1]);
    }

    /// Codex r2 #4: the schema is read by its tokens, so spaces, line breaks and comments
    /// between keywords, quoted names holding commas or keywords, and parentheses inside
    /// strings change nothing.
    #[test]
    fn the_schema_is_read_by_its_tokens() {
        for sql in [
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY KEY) WITHOUT\nROWID",
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY KEY) WITHOUT  ROWID",
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY KEY) without /* c */ rowid",
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY  KEY) WITHOUT -- c\n ROWID",
        ] {
            let c = columns(sql);
            assert_eq!(
                (c.without_rowid, c.stored.as_slice()),
                (true, &[1, 0][..]),
                "{sql}"
            );
        }
        let c = columns(
            "CREATE TABLE t(payload TEXT, \"k,key\" REAL, PRIMARY KEY(\"k,key\")) WITHOUT ROWID",
        );
        assert_eq!(c.names, ["payload", "k,key"]);
        assert_eq!((c.stored, c.real), (vec![1, 0], vec![false, true]));
        let c = columns("CREATE TABLE t(a /* , x */ TEXT, -- b,\n b INT, \"PRIMARY KEY\" TEXT)");
        assert_eq!(c.names, ["a", "b", "PRIMARY KEY"]);
        assert_eq!((c.ipk, c.stored), (None, vec![0, 1, 2]));
        let c = columns("CREATE TABLE t(a VARCHAR(10, 2) DEFAULT ')', b INTEGER PRIMARY KEY)");
        assert_eq!(
            (c.names, c.ipk),
            (vec!["a".to_string(), "b".to_string()], Some(1))
        );
        let e = extracted("shop.sqlite");
        assert_eq!(trail(&e, "needle odd"), ["odd", "row 1", "payload"]);
        assert!(String::from_utf8_lossy(&e.text).contains("\nneedle odd\t2.0\n"));
    }

    #[test]
    fn interior_pages_keep_every_row_in_order() {
        let e = extracted("many.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 4000);
        assert_eq!(rows[0], "1\tword1");
        assert_eq!(rows[1999], "2000\tword2000");
        assert_eq!(rows[2000], "1\tword1");
        assert_eq!(rows[3999], "2000\tword2000");
        assert_eq!(trail(&e, "word1999\n"), ["t", "row 1999", "word"]);
    }

    #[test]
    fn utf16_databases_are_transcoded() {
        let e = extracted("utf16.sqlite");
        assert_eq!(e.text, "año needle\n".as_bytes());
        assert_eq!(trail(&e, "needle"), ["t", "row 1", "name"]);
    }

    #[test]
    fn what_is_not_a_database_is_an_error() {
        assert!(Db::open(b"SQLite format 3\0short").is_err());
        assert!(Db::open(&[0u8; 200]).is_err());
    }
}
```

- [ ] **Step 2:** `cargo test --lib docs::sqlite` does not compile: `Db`, `Value`, `varint`
  and `columns` do not exist: RED. `the_schema_is_read_by_its_tokens` failed over the round-1
  code, which looked for the text `WITHOUT ROWID` and split the key's list at its commas
  (review r2, Codex #4).

- [ ] **Step 3:** the reader (D15, D16), from SQLite's file format document: the 100-byte
  header, b-tree pages (`0x0d` table leaf, `0x05` table interior, `0x0a` index leaf, `0x02`
  index interior), cells with their payload and overflow pages, records with their serial types.
  The `CREATE TABLE` is read by its tokens (`tokens`: words, quoted names and strings, other
  characters; white space and `--` and `/* */` comments between them). An interior index cell
  holds a row between its children, so a `WITHOUT ROWID` table is walked in key order; its
  records hold the PRIMARY KEY columns first, in the key's order
  (`Columns::stored`, from a column's `PRIMARY KEY` or the table's `PRIMARY KEY (…)`), and are
  put back in declared order (`values_follow_their_declared_columns`; review r1, Codex #4,
  Fable #2). A REAL column's integral values are stored as integers and shown as `3.0`:

```rust
//! SQLite (spec §5.8): `users › row 12 › email`. A reader of the file format of our own (M4
//! plan, D15): the schema table, then every table's b-tree, in page order of its keys. Each row
//! is one line of tab-separated values, named by its rowid; each value a `column` Segment named
//! by the column's name in the table's `CREATE TABLE`. Indexes, views and triggers are not read;
//! neither is a `-wal` file next to the database, so changes not yet checkpointed into the file
//! are not found (D16).
use super::{Builder, DocError};

/// The b-trees of one database are at most this deep (SQLite's own limit is 20).
const MAX_DEPTH: usize = 40;

struct Db<'a> {
    bytes: &'a [u8],
    page_size: usize,
    /// The usable bytes of each page: the page size less the reserved bytes at its end.
    usable: usize,
    pages: usize,
    /// 1 UTF-8, 2 UTF-16le, 3 UTF-16be.
    encoding: u32,
}

/// A value of a record.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Blob(usize),
}

impl Value {
    fn shown(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{f:.1}"),
            Value::Float(f) => f.to_string(),
            Value::Text(t) => t.clone(),
            Value::Blob(n) => format!("[blob {n} bytes]"),
        }
    }
}

fn be(b: &[u8], at: usize, n: usize) -> Option<u64> {
    let s = b.get(at..at + n)?;
    Some(s.iter().fold(0u64, |a, &x| a << 8 | x as u64))
}

/// SQLite's varint: up to 9 bytes, big-endian, 7 bits per byte but the ninth's 8.
fn varint(b: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for i in 0..9 {
        let x = *b.get(at + i)?;
        if i == 8 {
            return Some((v << 8 | x as u64, 9));
        }
        v = v << 7 | (x & 0x7f) as u64;
        if x & 0x80 == 0 {
            return Some((v, i + 1));
        }
    }
    None
}

impl<'a> Db<'a> {
    fn open(bytes: &'a [u8]) -> Result<Db<'a>, DocError> {
        if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
            return Err(DocError::broken("not an SQLite 3 database"));
        }
        let raw = be(bytes, 16, 2).unwrap() as usize;
        let page_size = if raw == 1 { 65536 } else { raw };
        if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
            return Err(DocError::broken(format!("page size {raw} is not valid")));
        }
        let reserved = bytes[20] as usize;
        let usable = page_size.saturating_sub(reserved);
        if usable < 480 {
            return Err(DocError::broken(
                "the reserved space leaves pages too small",
            ));
        }
        let encoding = be(bytes, 56, 4).unwrap() as u32;
        Ok(Db {
            bytes,
            page_size,
            usable,
            pages: bytes.len() / page_size,
            encoding: if (1..=3).contains(&encoding) {
                encoding
            } else {
                1
            },
        })
    }

    fn page(&self, n: usize) -> Option<&'a [u8]> {
        if n == 0 || n > self.pages {
            return None;
        }
        self.bytes.get((n - 1) * self.page_size..n * self.page_size)
    }

    /// The payload of a cell: its local bytes and, past them, its overflow pages.
    fn payload(&self, page: &[u8], at: usize, size: usize, index: bool) -> Option<Vec<u8>> {
        let u = self.usable;
        let max_local = if index {
            (u - 12) * 64 / 255 - 23
        } else {
            u - 35
        };
        if size <= max_local {
            return page.get(at..at + size).map(<[u8]>::to_vec);
        }
        let min_local = (u - 12) * 32 / 255 - 23;
        let k = min_local + (size - min_local) % (u - 4);
        let local = if k <= max_local { k } else { min_local };
        let mut out = page.get(at..at + local)?.to_vec();
        let mut next = be(page, at + local, 4)? as usize;
        let mut hops = 0;
        while out.len() < size {
            hops += 1;
            if hops > self.pages {
                return None;
            }
            let p = self.page(next)?;
            next = be(p, 0, 4)? as usize;
            let take = (size - out.len()).min(u - 4);
            out.extend(p.get(4..4 + take)?);
        }
        Some(out)
    }

    fn text(&self, b: &[u8]) -> String {
        match self.encoding {
            2 => encoding_rs::UTF_16LE
                .decode_without_bom_handling(b)
                .0
                .into_owned(),
            3 => encoding_rs::UTF_16BE
                .decode_without_bom_handling(b)
                .0
                .into_owned(),
            _ => String::from_utf8_lossy(b).into_owned(),
        }
    }

    fn record(&self, p: &[u8]) -> Option<Vec<Value>> {
        let (hlen, mut at) = varint(p, 0)?;
        let hlen = hlen as usize;
        let mut body = hlen;
        let mut out = Vec::new();
        while at < hlen {
            let (t, n) = varint(p, at)?;
            at += n;
            let (v, len) = match t {
                0 => (Value::Null, 0),
                1..=6 => {
                    let len = [0, 1, 2, 3, 4, 6, 8][t as usize];
                    let raw = be(p, body, len)?;
                    let shift = 64 - 8 * len as u32;
                    (Value::Int(((raw << shift) as i64) >> shift), len)
                }
                7 => (Value::Float(f64::from_bits(be(p, body, 8)?)), 8),
                8 => (Value::Int(0), 0),
                9 => (Value::Int(1), 0),
                10 | 11 => return None,
                t if t % 2 == 0 => (
                    Value::Blob(((t - 12) / 2) as usize),
                    ((t - 12) / 2) as usize,
                ),
                t => {
                    let len = ((t - 13) / 2) as usize;
                    (Value::Text(self.text(p.get(body..body + len)?)), len)
                }
            };
            if body + len > p.len() {
                return None;
            }
            body += len;
            out.push(v);
        }
        Some(out)
    }

    /// Calls `row` with (rowid, values) for every row of the table b-tree rooted at `root`, in
    /// rowid order; for a `WITHOUT ROWID` table (an index b-tree), with the 1-based position
    /// as its id. Stops when `row` returns false. Err: a page that is not what its parent says.
    fn walk(
        &self,
        root: usize,
        row: &mut dyn FnMut(i64, Vec<Value>) -> bool,
    ) -> Result<(), DocError> {
        // A page to read, or the row an interior index cell holds between its children.
        enum Next {
            Page(usize, usize),
            Cell(usize, usize),
        }
        let mut stack = vec![Next::Page(root, 0)];
        let mut seen = std::collections::HashSet::new();
        let mut ordinal = 0i64;
        // Interior pages push their children right to left, so rows come out in key order.
        while let Some(next) = stack.pop() {
            let (n, depth) = match next {
                Next::Page(n, d) => (n, d),
                Next::Cell(n, c) => {
                    ordinal += 1;
                    let page = self.page(n).unwrap_or_default();
                    if !self.index_cell(page, c, ordinal, row) {
                        return Ok(());
                    }
                    continue;
                }
            };
            if depth > MAX_DEPTH || !seen.insert(n) {
                return Err(DocError::broken("a b-tree that loops"));
            }
            let page = self
                .page(n)
                .ok_or_else(|| DocError::broken(format!("page {n} is past the end")))?;
            let h = if n == 1 { 100 } else { 0 };
            let kind = *page
                .get(h)
                .ok_or_else(|| DocError::broken("an empty page"))?;
            let cells = be(page, h + 3, 2).unwrap_or(0) as usize;
            let interior = matches!(kind, 0x02 | 0x05);
            let ptrs = h + if interior { 12 } else { 8 };
            let cell = |i: usize| be(page, ptrs + 2 * i, 2).map(|c| c as usize);
            match kind {
                0x05 | 0x02 => {
                    let right = be(page, h + 8, 4).unwrap_or(0) as usize;
                    stack.push(Next::Page(right, depth + 1));
                    for i in (0..cells).rev() {
                        let c = cell(i).ok_or_else(|| DocError::broken("a cell past its page"))?;
                        // An interior index cell holds a row of its own, after its left child.
                        if kind == 0x02 {
                            stack.push(Next::Cell(n, c + 4));
                        }
                        let child = be(page, c, 4).unwrap_or(0) as usize;
                        stack.push(Next::Page(child, depth + 1));
                    }
                }
                0x0d => {
                    for i in 0..cells {
                        let c = cell(i).ok_or_else(|| DocError::broken("a cell past its page"))?;
                        let Some((size, a)) = varint(page, c) else {
                            continue;
                        };
                        let Some((rowid, b)) = varint(page, c + a) else {
                            continue;
                        };
                        let Some(p) = self.payload(page, c + a + b, size as usize, false) else {
                            continue;
                        };
                        if let Some(v) = self.record(&p)
                            && !row(rowid as i64, v)
                        {
                            return Ok(());
                        }
                    }
                }
                0x0a => {
                    for i in 0..cells {
                        let c = cell(i).ok_or_else(|| DocError::broken("a cell past its page"))?;
                        ordinal += 1;
                        if !self.index_cell(page, c, ordinal, row) {
                            return Ok(());
                        }
                    }
                }
                _ => return Err(DocError::broken(format!("page {n} is not a b-tree page"))),
            }
        }
        Ok(())
    }

    fn index_cell(
        &self,
        page: &[u8],
        c: usize,
        id: i64,
        row: &mut dyn FnMut(i64, Vec<Value>) -> bool,
    ) -> bool {
        let Some((size, a)) = varint(page, c) else {
            return true;
        };
        let Some(p) = self.payload(page, c + a, size as usize, true) else {
            return true;
        };
        match self.record(&p) {
            Some(v) => row(id, v),
            None => true,
        }
    }
}

/// What a `CREATE TABLE` says of its columns.
#[derive(Debug, Default, PartialEq)]
struct Columns {
    names: Vec<String>,
    /// Columns of REAL affinity, whose integral values SQLite stores as integers and shows
    /// as floats (`3.0`).
    real: Vec<bool>,
    /// The `INTEGER PRIMARY KEY`, which a row stores as NULL: its value is the rowid.
    ipk: Option<usize>,
    without_rowid: bool,
    /// The declared column of each value as a record stores it: in order, except in a
    /// `WITHOUT ROWID` table, which stores its primary key's columns first, in the key's order,
    /// then the others (Codex r1 #4).
    stored: Vec<usize>,
}

/// A token of SQL (M4 plan r2, Codex #4): a word, a quoted name (`"…"`, `` `…` ``, `[…]`) or
/// string (`'…'`), or one other character. White space and comments separate tokens.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// A bare word: a keyword or a name, as written.
    Word(String),
    /// A quoted name or a string, unquoted: never a keyword.
    Quoted(String),
    Punct(char),
}

impl Tok {
    /// Whether this is the keyword `kw` (any case).
    fn is(&self, kw: &str) -> bool {
        matches!(self, Tok::Word(w) if w.eq_ignore_ascii_case(kw))
    }

    fn name(&self) -> Option<String> {
        match self {
            Tok::Word(w) | Tok::Quoted(w) => Some(w.clone()),
            Tok::Punct(_) => None,
        }
    }
}

fn tokens(sql: &str) -> Vec<Tok> {
    let c: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '-' && c.get(i + 1) == Some(&'-') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if matches!(ch, '"' | '`' | '\'' | '[') {
            let close = if ch == '[' { ']' } else { ch };
            let mut t = String::new();
            i += 1;
            while i < c.len() {
                if c[i] == close {
                    // A doubled quote is the quote itself; `]` always closes.
                    if close != ']' && c.get(i + 1) == Some(&close) {
                        t.push(close);
                        i += 2;
                        continue;
                    }
                    break;
                }
                t.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Quoted(t));
        } else if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '.' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || matches!(c[i], '_' | '$' | '.')) {
                i += 1;
            }
            out.push(Tok::Word(c[start..i].iter().collect()));
        } else {
            out.push(Tok::Punct(ch));
            i += 1;
        }
    }
    out
}

/// `toks` split at the commas outside parentheses.
fn split_commas(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => depth = depth.saturating_sub(1),
            Tok::Punct(',') if depth == 0 => {
                parts.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&toks[start..]);
    parts
}

/// The index of the token after `PRIMARY KEY` in `toks`, if it holds one.
fn primary_key(toks: &[Tok]) -> Option<usize> {
    toks.windows(2)
        .position(|w| w[0].is("PRIMARY") && w[1].is("KEY"))
        .map(|i| i + 2)
}

fn columns(sql: &str) -> Columns {
    let toks = tokens(sql);
    let Some(open) = toks.iter().position(|t| *t == Tok::Punct('(')) else {
        return Columns::default();
    };
    let mut depth = 0usize;
    let mut close = toks.len();
    for (i, t) in toks.iter().enumerate().skip(open) {
        match t {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    close = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let options = &toks[(close + 1).min(toks.len())..];
    let without_rowid = options
        .windows(2)
        .any(|w| w[0].is("WITHOUT") && w[1].is("ROWID"));
    let mut names = Vec::new();
    let mut types = Vec::new();
    let mut ipk = None;
    // The primary key's columns, by name, from a column's `PRIMARY KEY` or the table's.
    let mut pk: Vec<String> = Vec::new();
    for part in split_commas(&toks[open + 1..close.max(open + 1)]) {
        let Some(first) = part.first() else { continue };
        if ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"]
            .iter()
            .any(|k| first.is(k))
        {
            if let Some(at) = primary_key(part)
                && part.get(at) == Some(&Tok::Punct('('))
            {
                let end = part
                    .iter()
                    .rposition(|t| *t == Tok::Punct(')'))
                    .unwrap_or(part.len());
                pk = split_commas(&part[at + 1..end.max(at + 1)])
                    .iter()
                    .filter_map(|c| c.first().and_then(Tok::name))
                    .collect();
            }
            continue;
        }
        let Some(name) = first.name() else { continue };
        // The type: the words after the name, up to a constraint or a parenthesis.
        let ty: Vec<String> = part[1..]
            .iter()
            .take_while(|t| {
                matches!(t, Tok::Word(_))
                    && ![
                        "CONSTRAINT",
                        "PRIMARY",
                        "NOT",
                        "NULL",
                        "UNIQUE",
                        "CHECK",
                        "DEFAULT",
                        "COLLATE",
                        "REFERENCES",
                        "GENERATED",
                        "AS",
                    ]
                    .iter()
                    .any(|k| t.is(k))
            })
            .filter_map(Tok::name)
            .map(|w| w.to_ascii_uppercase())
            .collect();
        let ty = ty.join(" ");
        if let Some(at) = primary_key(part) {
            pk = vec![name.clone()];
            if ty == "INTEGER" && !part.get(at).is_some_and(|t| t.is("DESC")) {
                ipk = Some(names.len());
            }
        }
        names.push(name);
        types.push(ty);
    }
    let key: Vec<usize> = pk
        .iter()
        .filter_map(|k| names.iter().position(|n| n.eq_ignore_ascii_case(k)))
        .collect();
    if ipk.is_none()
        && let [only] = key.as_slice()
        && types[*only] == "INTEGER"
    {
        // A table constraint `PRIMARY KEY(id)` over an INTEGER column makes it the rowid too.
        ipk = Some(*only);
    }
    let stored = if without_rowid {
        let mut s = key.clone();
        s.extend((0..names.len()).filter(|i| !key.contains(i)));
        s
    } else {
        (0..names.len()).collect()
    };
    let real = types
        .iter()
        .map(|t| !t.contains("INT") && ["REAL", "FLOA", "DOUB"].iter().any(|r| t.contains(r)))
        .collect();
    Columns {
        names,
        real,
        ipk: if without_rowid { None } else { ipk },
        without_rowid,
        stored,
    }
}

pub fn extract(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let db = Db::open(bytes)?;
    // sqlite_schema: type, name, tbl_name, rootpage, sql.
    let mut tables = Vec::new();
    db.walk(1, &mut |_, v| {
        if let [
            Value::Text(ty),
            Value::Text(name),
            _,
            Value::Int(root),
            sql,
            ..,
        ] = v.as_slice()
            && ty == "table"
            && !name.starts_with("sqlite_")
            && *root > 0
        {
            let sql = match sql {
                Value::Text(s) => s.clone(),
                _ => String::new(),
            };
            tables.push((name.clone(), *root as usize, sql));
        }
        true
    })?;
    for (name, root, sql) in tables {
        let cols = columns(&sql);
        let t = b.open("table", &name);
        let r = db.walk(root, &mut |rowid, stored| {
            b.start_row(Some(&format!("row {rowid}")));
            // The values in their declared order: a record may hold fewer (columns added
            // since), which are NULL, or more, which keep their stored place.
            let n = cols.names.len().max(stored.len());
            let mut values = vec![Value::Null; n];
            for (k, v) in stored.into_iter().enumerate() {
                let at = cols.stored.get(k).copied().unwrap_or(k);
                values[at] = v;
            }
            for (i, v) in values.iter().enumerate() {
                let col = cols
                    .names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("column {}", i + 1));
                let shown = match v {
                    Value::Null if Some(i) == cols.ipk => rowid.to_string(),
                    Value::Int(n) if cols.real.get(i) == Some(&true) => {
                        Value::Float(*n as f64).shown()
                    }
                    v => v.shown(),
                };
                b.cell("column", &col, &shown);
            }
            b.end_row();
            !b.full()
        });
        b.close(t);
        r?;
        if b.full() {
            break;
        }
    }
    Ok(())
}
```

- [ ] **Step 4:** GREEN. Then compare row counts with Python's `sqlite3` over real databases,
  without printing their content: the probe matched every table (27) of five browser databases
  up to 86 MB (`examples/sqlcount.rs` of the probe, not part of the plan).
## Chunk 11: the pipeline

### Task 11.1: the end-to-end tests

**Files:** `tests/cli_m4.rs`.

- [ ] **Step 1:** the tests (spec §11 #2, #6, #7, #23–#25). `fixture()` writes M4's documents
  and object files into the fixture copy under `m4/`, with C2's and C4's writers:

```rust
//! End-to-end tests of M4 (plan, Chunk 11): object files, documents with --docs, ripgrep's
//! --pre and -z. The documents and object files are written into the fixture copy by the
//! writers of `searchfor::docs::fixture` and `searchfor::processors::object::fixture`; the
//! SQLite databases are tests/fixtures/docs/*.sqlite (scripts/gen-sqlite-fixtures.py).
mod common;

use common::{Fixture, code, run, stderr, stdout};
use searchfor::docs::fixture as doc;
use searchfor::processors::object::fixture as obj;

/// The fixture copy plus M4's generated files under `m4/`.
fn fixture() -> Fixture {
    let fx = Fixture::new();
    let d = fx.path().join("m4");
    std::fs::create_dir_all(d.join("bin")).unwrap();
    let w = |p: &str, b: &[u8]| std::fs::write(d.join(p), b).unwrap();
    let rodata = b"\0\0\0\0hello\0version=1.2.3-needle\0another needle here\0";
    let spec = obj::Spec::new()
        .section(".text", b"\x55\x48\x89\xe5\xc3\0\0\0")
        .section(".rodata", rodata)
        .sized("main.greeting", 1, 4, 6)
        .sized("main.version", 1, 10, 21);
    w("bin/app", &obj::elf(&spec).file);
    let mut stripped = spec.clone();
    stripped.symbols.clear();
    w("bin/app-stripped", &obj::elf(&stripped).file);
    let pe = obj::Spec::new()
        .section(".text", b"\xc3\0\0\0")
        .section(".rdata", b"\0\0version=1.2.3-needle\0")
        .symbol("version_string", 1, 2);
    w("bin/app.exe", &obj::pe(&pe).file);
    let mo = obj::Spec::new()
        .section("__TEXT,__text", b"\xc3\0\0\0")
        .section("__TEXT,__cstring", b"hi\0version=1.2.3-needle\0")
        .symbol("_version", 1, 3);
    let x86 = obj::macho(&mo).file;
    let arm = obj::macho(&mo.clone().machine(0x0100_000c)).file;
    w(
        "bin/app-mac",
        &obj::fat(&[(0x0100_0007, x86), (0x0100_000c, arm)]).0,
    );
    let report = [
        doc::h(1, "1 Intro"),
        doc::p("The intro needle."),
        doc::h(2, "1.1 Scope"),
        doc::p("Scope needle."),
        doc::h(1, "2 Costs"),
        doc::table(&[&["Item", "Price"], &["Pens", "1.50 needle"]]),
    ];
    w("report.docx", &doc::docx(&report));
    w("report.odt", &doc::odt(&report));
    let sheet: &[&[&str]] = &[&["Item", "Price"], &["Pens", "1.5"], &["needle pins", "2"]];
    w("budget.xlsx", &doc::xlsx(&[("Budget", sheet)]));
    w("budget.ods", &doc::ods(&[("Budget", sheet)]));
    let slides: &[(&[&str], Option<&str>)] = &[
        (
            &["Results", "a needle margin"],
            Some("tell the needle joke"),
        ),
        (&["Next", "find the needle"], None),
    ];
    w("deck.pptx", &doc::pptx(slides));
    w("deck.odp", &doc::odp(slides));
    w(
        "book.epub",
        &doc::epub(
            &[
                (
                    "1. Arrival",
                    "<h1>The Harbour</h1><p>A needle in the water.</p>",
                ),
                ("2. The Storm", "<p>The storm needle.</p>"),
            ],
            true,
        ),
    );
    w(
        "report.pdf",
        &doc::pdf(
            &[
                &["Report"],
                &["1 Intro", "intro needle"],
                &["2 Costs", "costs needle"],
            ],
            &[(1, "1 Intro", 2), (1, "2 Costs", 3)],
        ),
    );
    w(
        "locked.pdf",
        &doc::pdf_encrypted(&[&["locked needle"]], "pw"),
    );
    w("broken.docx", b"PK\x03\x04 not really a zip, needle");
    w(
        "notes.zip",
        &doc::zip(&[("a.txt", b"zipped needle".to_vec())], &[]),
    );
    fx
}

fn ok(fx: &Fixture, args: &[&str]) -> String {
    let o = run(&mut fx.sfor(args));
    assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
    assert_eq!(stderr(&o), "", "{args:?}");
    stdout(&o)
}

#[test]
fn m4_01_every_format_in_three_layouts() {
    // Spec §11 #2 for M4's Formats.
    let fx = fixture();
    let cases: &[(&str, &[&str])] = &[
        ("elf", &["needle", "m4/bin/app"]),
        ("pe", &["needle", "m4/bin/app.exe"]),
        ("macho", &["needle", "m4/bin/app-mac"]),
        ("pdf", &["--docs", "needle", "m4/report.pdf"]),
        ("docx", &["--docs", "needle", "m4/report.docx"]),
        ("odt", &["--docs", "needle", "m4/report.odt"]),
        ("xlsx", &["--docs", "needle", "m4/budget.xlsx"]),
        ("ods", &["--docs", "needle", "m4/budget.ods"]),
        ("pptx", &["--docs", "needle", "m4/deck.pptx"]),
        ("odp", &["--docs", "needle", "m4/deck.odp"]),
        ("epub", &["--docs", "needle", "m4/book.epub"]),
        ("sqlite", &["--docs", "needle", "docs/shop.sqlite"]),
    ];
    for (name, args) in cases {
        for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
            let mut a = args.to_vec();
            a.push(flag);
            let mut out = ok(&fx, &a);
            if layout == "json" {
                common::assert_valid_json_schema(&out);
                assert!(
                    out.contains(&format!("\"format\":\"{name}\"")),
                    "{name}: {out}"
                );
                out = common::mask_json_times(&out);
            }
            insta::assert_snapshot!(format!("m4_01_{name}_{layout}"), out);
        }
    }
}

#[test]
fn m4_02_object_rows_with_and_without_symbols() {
    // Spec §11 #6.
    let fx = fixture();
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--sort",
                "path",
                "needle",
                "m4/bin/app",
                "m4/bin/app-stripped"
            ]
        ),
        "m4/bin/app:0x68: ELF x86-64 › .rodata › main.version+0xe │ version=1.2.3-needle\n\
         m4/bin/app:0x77: ELF x86-64 › .rodata │ another needle here\n\
         m4/bin/app-stripped:0x68: ELF x86-64 › .rodata │ version=1.2.3-needle\n\
         m4/bin/app-stripped:0x77: ELF x86-64 › .rodata │ another needle here\n"
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--sort",
                "path",
                "needle",
                "m4/bin/app.exe",
                "m4/bin/app-mac"
            ]
        ),
        "m4/bin/app.exe:0x410: PE x86-64 › .rdata › version_string+0xe │ version=1.2.3-needle\n\
         m4/bin/app-mac:0x1141: Mach-O x86-64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle\n\
         m4/bin/app-mac:0x2141: Mach-O arm64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle\n"
    );
}

#[test]
fn m4_03_walked_objects_are_searched_as_rg_searches_them() {
    let fx = fixture();
    // Walked without --binary, rg stops at the first NUL: nothing to describe.
    let o = run(&mut fx.sfor(["--flat", "version=", "m4/bin"]));
    assert_eq!(code(&o), 1, "{}", stdout(&o));
    // With --binary every object file with a Match gets its rows.
    let out = ok(
        &fx,
        &["--flat", "--binary", "--sort", "path", "version=", "m4/bin"],
    );
    assert_eq!(out.lines().count(), 5, "{out}");
    assert!(out.lines().all(|l| l.contains(":0x")), "{out}");
    // -v, -c and --no-trail print what rg prints.
    if common::reference_rg() {
        for args in [
            &["--no-trail", "needle", "m4/bin/app"][..],
            &["-c", "needle", "m4/bin/app"],
            &["--flat", "-v", "needle", "m4/bin/app"],
            &[
                "--no-trail",
                "--binary",
                "--sort",
                "path",
                "needle",
                "m4/bin",
            ],
        ] {
            let rg_args: Vec<&str> = args
                .iter()
                .copied()
                .filter(|a| *a != "--flat" && *a != "--no-trail")
                .collect();
            let s = run(&mut fx.sfor(args));
            let r = run(&mut fx.rg(&rg_args));
            assert_eq!(stdout(&s), stdout(&r), "{args:?}");
            assert_eq!(code(&s), code(&r), "{args:?}");
        }
    }
}

#[test]
fn m4_04_object_rows_count_for_m_and_json() {
    let fx = fixture();
    assert_eq!(
        ok(&fx, &["--flat", "-m1", "needle", "m4/bin/app"])
            .lines()
            .count(),
        1
    );
    let json = ok(&fx, &["--json", "needle", "m4/bin/app"]);
    let end: serde_json::Value = json
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["type"] == "end")
        .unwrap();
    let size = std::fs::metadata(fx.file("m4/bin/app")).unwrap().len();
    assert_eq!(end["data"]["stats"]["bytes_searched"], size);
    assert_eq!(end["data"]["stats"]["matched_lines"], 2);
    assert_eq!(end["data"]["binary_offset"], serde_json::Value::Null);
    // --within selects rows by their Trail.
    assert_eq!(
        ok(
            &fx,
            &["--flat", "--within", "main.version", "needle", "m4/bin/app"]
        )
        .lines()
        .count(),
        1
    );
}

#[test]
fn m4_05_pdf_chapters_only_with_docs_and_zips_never() {
    // Spec §11 #7.
    let fx = fixture();
    assert_eq!(
        ok(&fx, &["--docs", "--flat", "needle", "m4/report.pdf"]),
        "3: 1 Intro › page 2 │ intro needle\n5: 2 Costs › page 3 │ costs needle\n"
    );
    if common::reference_rg() {
        // Without --docs the PDF is searched as rg searches it (its streams are compressed).
        let s = run(&mut fx.sfor(["--flat", "needle", "m4/report.pdf"]));
        let r = run(&mut fx.rg(["needle", "m4/report.pdf"]));
        assert_eq!((stdout(&s), code(&s)), (stdout(&r), code(&r)));
    }
    // A zip is never opened (8390), --docs or not.
    let o = run(&mut fx.sfor(["--docs", "--flat", "zipped", "m4/notes.zip"]));
    assert_eq!(code(&o), 1, "{}", stdout(&o));
}

#[test]
fn m4_06_document_trails() {
    let fx = fixture();
    let flat = |f: &str| ok(&fx, &["--docs", "--flat", "needle", f]);
    assert_eq!(
        flat("m4/report.docx"),
        "2: 1 Intro │ The intro needle.\n\
         4: 1 Intro › 1.1 Scope │ Scope needle.\n\
         7: 2 Costs › table 1 › row 2 › Price │ Pens\t1.50 needle\n"
    );
    assert_eq!(flat("m4/report.odt"), flat("m4/report.docx"));
    assert_eq!(flat("m4/budget.xlsx"), "3: Budget › A3 │ needle pins\t2\n");
    assert_eq!(flat("m4/budget.ods"), flat("m4/budget.xlsx"));
    assert_eq!(
        flat("m4/deck.pptx"),
        "2: slide 1 │ a needle margin\n3: slide 1 › notes │ tell the needle joke\n5: slide 2 │ find the needle\n"
    );
    assert_eq!(flat("m4/deck.odp"), flat("m4/deck.pptx"));
    assert_eq!(
        flat("m4/book.epub"),
        "2: 1. Arrival › The Harbour │ A needle in the water.\n3: 2. The Storm │ The storm needle.\n"
    );
    assert_eq!(
        ok(&fx, &["--docs", "--flat", "needle", "docs/shop.sqlite"]),
        "12: users › row 12 › email │ 12\tUser 12\tana.needle@example.com\t18.0\n\
         42: notes › row 2 › body │ …[+1365] g long long long long long long long long long long long long long long long long long long long long long long long long long long long needle at the end\t\n\
         45: kv › row 3 › v │ gamma\tvalue of gamma needle\n\
         46: late › row 1 › payload │ needle payload\tk1\n\
         48: gaps › row 1 › y │ \tneedle gap\n\
         49: odd › row 1 › payload │ needle odd\t2.0\n"
    );
}

#[test]
fn m4_07_documents_that_cannot_be_read_warn_and_are_searched_as_rg() {
    let fx = fixture();
    let o = run(&mut fx.sfor([
        "--docs",
        "--flat",
        "--sort",
        "path",
        "needle",
        "m4/locked.pdf",
        "m4/broken.docx",
    ]));
    assert_eq!(
        stderr(&o),
        "sfor: m4/locked.pdf: pdf: encrypted, and the empty password does not open it\n\
         sfor: m4/broken.docx: docx: not a zip file the reader can open (invalid Zip archive: Could not find EOCD)\n"
    );
    // broken.docx is text with a needle: printed as rg prints it, exit 0.
    assert_eq!(code(&o), 0);
    assert!(
        stdout(&o).contains("m4/broken.docx:1: PK"),
        "{}",
        stdout(&o)
    );
    let quiet = run(&mut fx.sfor([
        "--docs",
        "--no-messages",
        "--flat",
        "needle",
        "m4/locked.pdf",
    ]));
    assert_eq!(stderr(&quiet), "");
}

#[test]
fn m4_08_pre_and_z_as_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    let d = fx.path().join("m4/pre");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("a.txt"), "alpha needle\n").unwrap();
    std::fs::write(d.join("b.md"), "# B\ngamma needle\n").unwrap();
    let script = |name: &str, body: &str| {
        let p = d.join(name);
        std::fs::write(&p, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p.to_string_lossy().into_owned()
    };
    let up = script("up.sh", "#!/bin/sh\ntr a-z A-Z < \"$1\"\n");
    let fail = script("fail.sh", "#!/bin/sh\necho oops >&2\nexit 3\n");
    let mut gz = std::process::Command::new("gzip");
    gz.arg("-k").arg(d.join("a.txt"));
    let have_gzip = gz.status().is_ok_and(|s| s.success());
    let mut cases: Vec<Vec<String>> = vec![
        vec![
            "--pre".into(),
            up.clone(),
            "--sort".into(),
            "path".into(),
            "NEEDLE".into(),
            "m4/pre/a.txt".into(),
            "m4/pre/b.md".into(),
        ],
        vec![
            "--pre".into(),
            up.clone(),
            "--pre-glob".into(),
            "*.md".into(),
            "-i".into(),
            "needle".into(),
            "m4/pre".into(),
            "--sort".into(),
            "path".into(),
        ],
        vec![
            "--pre".into(),
            fail.clone(),
            "needle".into(),
            "m4/pre/a.txt".into(),
        ],
        vec![
            "--pre".into(),
            "/nonexistent/pre".into(),
            "needle".into(),
            "m4/pre/a.txt".into(),
        ],
        vec![
            "--json".into(),
            "--pre".into(),
            up.clone(),
            "NEEDLE".into(),
            "m4/pre/a.txt".into(),
        ],
    ];
    if have_gzip {
        cases.push(vec!["-z".into(), "needle".into(), "m4/pre/a.txt.gz".into()]);
        cases.push(vec![
            "--pre".into(),
            up.clone(),
            "-z".into(),
            "needle".into(),
            "m4/pre/a.txt.gz".into(),
        ]);
        cases.push(vec![
            "-z".into(),
            "--pre".into(),
            up.clone(),
            "NEEDLE".into(),
            "m4/pre/a.txt.gz".into(),
        ]);
    }
    for args in cases {
        let s = run(&mut fx.sfor(args.iter().map(String::as_str).chain(["--no-trail"])));
        let r = run(&mut fx.rg(&args));
        let (so, ro) = if args.contains(&"--json".to_string()) {
            (
                common::mask_json_times(&stdout(&s)),
                common::mask_json_times(&stdout(&r)),
            )
        } else {
            (stdout(&s), stdout(&r))
        };
        assert_eq!(so, ro, "{args:?}");
        assert_eq!(code(&s), code(&r), "{args:?}");
        assert_eq!(stderr(&s).replace("sfor: ", "rg: "), stderr(&r), "{args:?}");
    }
    // In the Trail layouts the output of --pre gets the Trails of the file's Format.
    assert_eq!(
        ok(&fx, &["--flat", "--pre", &up, "NEEDLE", "m4/pre/b.md"]),
        "2: B │ GAMMA NEEDLE\n"
    );
}

#[test]
fn m4_09_docs_in_the_ripgrep_modes_and_with_z() {
    let fx = fixture();
    assert_eq!(
        ok(&fx, &["--docs", "--no-trail", "needle", "m4/report.docx"]),
        "The intro needle.\nScope needle.\nPens\t1.50 needle\n"
    );
    assert_eq!(
        ok(&fx, &["--docs", "-c", "needle", "m4/report.docx"]),
        "3\n"
    );
    let l = ok(
        &fx,
        &[
            "--docs",
            "-l",
            "--sort",
            "path",
            "needle",
            "m4/report.docx",
            "m4/budget.ods",
            "m4/bin",
        ],
    );
    assert!(
        l.contains("m4/report.docx\n") && l.contains("m4/budget.ods\n"),
        "{l}"
    );
    // A document -z decompresses is searched as text, never extracted (D21).
    let gz = std::process::Command::new("gzip")
        .arg("-k")
        .arg(fx.file("m4/report.docx"))
        .status();
    if gz.is_ok_and(|s| s.success()) {
        let o = run(&mut fx.sfor([
            "--docs",
            "-z",
            "--flat",
            "intro needle",
            "m4/report.docx.gz",
        ]));
        assert_eq!(code(&o), 1, "{}", stdout(&o));
    }
}

#[test]
fn m4_10_structural_flags_on_documents_and_objects() {
    let fx = fixture();
    assert_eq!(
        ok(
            &fx,
            &[
                "--docs",
                "--flat",
                "--in-headings",
                "Scope",
                "m4/report.docx"
            ]
        ),
        "3: 1 Intro │ 1.1 Scope\n"
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--docs",
                "--flat",
                "--within",
                "Costs",
                "needle",
                "m4/report.docx",
                "m4/report.pdf"
            ]
        ),
        "m4/report.docx:7: 2 Costs › table 1 › row 2 › Price │ Pens\t1.50 needle\n\
         m4/report.pdf:5: 2 Costs › page 3 │ costs needle\n"
    );
    // --expand: a table cell's Section is its row; an object row is printed as a row (D8).
    assert_eq!(
        ok(
            &fx,
            &["--docs", "--flat", "--expand", "1.50", "m4/report.docx"]
        ),
        "7-7: 2 Costs › table 1 › row 2 › Price\nPens\t1.50 needle\n\n"
    );
    // At its first Match, past the start of its string, as without --expand (M4 plan r1,
    // Fable #4).
    assert_eq!(
        ok(&fx, &["--flat", "--expand", "needle here", "m4/bin/app"]),
        "0x77: ELF x86-64 › .rodata │ another needle here\n"
    );
}

#[test]
fn m4_11_without_docs_documents_are_ripgreps() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    for args in [
        &["--sort", "path", "needle", "m4"][..],
        &["-a", "needle", "m4/report.docx"],
    ] {
        let s = run(&mut fx.sfor(args.iter().copied().chain(["--no-trail"])));
        let r = run(&mut fx.rg(args));
        assert_eq!(stdout(&s), stdout(&r), "{args:?}");
        assert_eq!(code(&s), code(&r), "{args:?}");
        let f = run(&mut fx.sfor(args.iter().copied().chain(["--flat"])));
        assert_eq!(stderr(&f), "", "{args:?}");
        assert!(!stdout(&f).contains(" › page "), "{args:?}");
    }
}

/// Runs `c` with `input` on its stdin, failing the test past 20 s instead of hanging.
fn run_within(c: &mut std::process::Command, input: &[u8]) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let started = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > std::time::Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("{c:?} still runs after 20 s");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    writer.join().unwrap();
    child.wait_with_output().unwrap()
}

/// Codex r1 #3: with --docs, a path that is not a regular file (stdin by name, a descriptor, a
/// FIFO) is never a document: its bytes are read once, by the search, as rg reads them.
#[cfg(unix)]
#[test]
fn m4_12_docs_reads_streams_once() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    let fifo = fx.path().join("m4/pipe.pdf");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    for path in ["/dev/stdin", "/dev/fd/0", "m4/pipe.pdf"] {
        // The FIFO is named like a document, and its writer writes once.
        let feed = || {
            let fifo = fifo.clone();
            (path == "m4/pipe.pdf")
                .then(|| std::thread::spawn(move || std::fs::write(fifo, b"a fifo needle\n")))
        };
        let writer = feed();
        let s = run_within(
            &mut fx.sfor(["--docs", "--no-trail", "needle", path]),
            b"a needle\n",
        );
        if let Some(w) = writer {
            w.join().unwrap().unwrap();
        }
        assert_eq!(stderr(&s), "", "{path}");
        let writer = feed();
        let r = run_within(&mut fx.rg(["needle", path]), b"a needle\n");
        if let Some(w) = writer {
            w.join().unwrap().unwrap();
        }
        assert_eq!(code(&r), 0, "{path}");
        assert_eq!(code(&s), code(&r), "{path}");
        assert_eq!(stdout(&s), stdout(&r), "{path}");
    }
}

/// Codex r1 #7 and #8: --within keeps the Matches of an object row that are within, not the
/// whole row, and -m after it counts rows, which share their line.
#[test]
fn m4_13_within_on_object_rows() {
    let fx = fixture();
    let rodata = b"\0first needle|second needle\0third needle\0fourth needle\0";
    let spec = obj::Spec::new()
        .section(".text", b"\xc3\0\0\0")
        .section(".rodata", rodata)
        .sized("one", 1, 1, 13)
        .sized("two", 1, 14, 13);
    let elf = obj::elf(&spec).file;
    std::fs::write(fx.path().join("m4/bin/two"), &elf).unwrap();
    let at = |s: &[u8]| format!("{:#x}", memchr::memmem::find(&elf, s).unwrap());
    assert_eq!(
        ok(&fx, &["--flat", "--within", "two", "needle", "m4/bin/two"]),
        format!(
            "{}: ELF x86-64 › .rodata › two+0x7 │ first needle|second needle\n",
            at(b"needle\0third")
        )
    );
    let json = ok(&fx, &["--json", "--within", "two", "needle", "m4/bin/two"]);
    let m: Vec<serde_json::Value> = json
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .filter(|v: &serde_json::Value| v["type"] == "match")
        .collect();
    assert_eq!(m.len(), 1, "{json}");
    let subs = m[0]["data"]["submatches"].as_array().unwrap();
    assert_eq!(subs.len(), 1, "{json}");
    assert_eq!(subs[0]["start"], 20, "{json}");
    // Three rows on one line: -m1 keeps the first.
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--within",
                ".rodata",
                "-m1",
                "needle",
                "m4/bin/two"
            ]
        ),
        format!(
            "{}: ELF x86-64 › .rodata › one+0x6 │ first needle|second needle\n",
            at(b"needle|")
        )
    );
}

/// Codex r1 #11: what a transform outputs gets no object rows and no object Processor, even
/// when its bytes are an object file or --format names one: it prints as Plain (spec §5.9).
#[test]
fn m4_14_transformed_objects_print_as_plain() {
    let fx = fixture();
    let gz = std::process::Command::new("gzip")
        .args(["-k", "m4/bin/app"])
        .current_dir(fx.path())
        .status()
        .unwrap();
    assert!(gz.success());
    for args in [
        &["--pre", "cat", "needle", "m4/bin/app"][..],
        &["--pre", "cat", "-a", "needle", "m4/bin/app"],
        &["--pre", "cat", "--format", "elf", "needle", "m4/bin/app"],
        &["-z", "needle", "m4/bin/app.gz"],
        &["-z", "-a", "--format", "elf", "needle", "m4/bin/app.gz"],
    ] {
        for layout in ["--flat", "--tree", "--json"] {
            let o = run(&mut fx.sfor(args.iter().copied().chain([layout])));
            assert_eq!(code(&o), 0, "{args:?} {layout}: {}", stderr(&o));
            let out = stdout(&o);
            if layout == "--json" {
                for v in out
                    .lines()
                    .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                {
                    if v["type"] == "match" {
                        assert_eq!(v["data"]["trail"], serde_json::json!([]), "{args:?}: {out}");
                        assert_eq!(
                            v["data"]["format"],
                            serde_json::Value::Null,
                            "{args:?}: {out}"
                        );
                    }
                }
            } else {
                assert!(
                    !out.contains("ELF x86-64") && !out.contains(" › ") && !out.contains("0x"),
                    "{args:?} {layout}: {out}"
                );
            }
        }
        if common::reference_rg() && !args.contains(&"--format") {
            let s = run(&mut fx.sfor(args.iter().copied().chain(["--no-trail"])));
            let r = run(&mut fx.rg(args));
            assert_eq!(stdout(&s), stdout(&r), "{args:?}");
        }
    }
}

/// Codex r2 #5: one string with two Matches 5 000 bytes apart is one row in every layout, also
/// with -m1.
#[test]
fn m4_15_a_long_string_is_one_row() {
    let fx = fixture();
    let mut rodata = b"\0needle".to_vec();
    rodata.extend(vec![b'a'; 5000]);
    rodata.extend(b"needle\0");
    let spec = obj::Spec::new()
        .section(".text", b"\xc3\0\0\0")
        .section(".rodata", &rodata);
    std::fs::write(fx.path().join("m4/bin/long"), obj::elf(&spec).file).unwrap();
    for m in [&[][..], &["-m1"]] {
        let args = |layout: &'static str| {
            let mut a = vec![layout];
            a.extend_from_slice(m);
            a.extend(["needle", "m4/bin/long"]);
            a
        };
        // One row, at one offset; a long row prints a Fragment around each Match (spec §6.3).
        let offsets = |out: &str| {
            let mut o: Vec<String> = out
                .lines()
                .filter_map(|l| l.trim_start().split_once(':').map(|p| p.0.to_string()))
                .filter(|p| p.starts_with("0x"))
                .collect();
            o.dedup();
            o
        };
        let flat = ok(&fx, &args("--flat"));
        assert_eq!(offsets(&flat), ["0x51"], "{m:?}: {flat}");
        assert_eq!(flat.matches("needle").count(), 2, "{m:?}: {flat}");
        let tree = ok(&fx, &args("--tree"));
        assert_eq!(offsets(&tree), ["0x51"], "{m:?}: {tree}");
        assert_eq!(tree.matches("needle").count(), 2, "{m:?}: {tree}");
        let json = ok(&fx, &args("--json"));
        let rows: Vec<serde_json::Value> = json
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .filter(|v| v["type"] == "match")
            .collect();
        assert_eq!(rows.len(), 1, "{m:?}: {json}");
        assert_eq!(
            rows[0]["data"]["submatches"].as_array().unwrap().len(),
            2,
            "{m:?}"
        );
    }
}
```

- [ ] **Step 2:** `cargo test --test cli_m4`: RED. Before this chunk an object file prints
  `binary file matches`, `--docs` finds nothing in a document (`--docs` is parsed but unused),
  and `--pre` and `-z` are parsed but unused, so `m4_08` differs from rg (to verify in chunk
  11: the probe wrote the tests after the pipeline, and corrected several of their first
  expectations against its output, each checked by eye: hex offsets, line numbers, a Fragment's
  count, and the order of several files, which `--sort path` now fixes). The tests of review
  r1 were seen RED against the round-0 pipeline: `m4_12` (`/dev/stdin` with `--docs` exited 1,
  its bytes read by the detection), `m4_13` (`--within two` kept the row at `one+0x6`;
  `--within .rodata -m1` printed three rows of one line), `m4_14` (`--pre cat` over an ELF
  printed `0x68: ELF x86-64 › .rodata › main.version+0xe`, and with the object Processor alone
  disabled `--format elf` still named the Format `elf`). `m4_10` searches `needle here`, whose
  Match is past the start of its string, so its offset `0x77` tells the first Match from the
  string (review r1, Fable #4: the round-0 code already printed `0x77` there). `m4_15` (one
  string, two Matches 5 000 bytes apart: one row in the three layouts, with and without `-m1`)
  covers what the unit test of Task 2.3 showed RED against round 1's rows (review r2, Codex
  #5).

### Task 11.2: `retain_rows`

**Files:** `src/filter.rs`.

- [ ] **Step 1:** the filter of object rows (D8): each row keeps its kept Matches and their
  Trails, a row with none is dropped, and `max` stops at `-m` rows:

```diff
diff --git a/src/filter.rs b/src/filter.rs
--- a/src/filter.rs
+++ b/src/filter.rs
@@ -24,6 +24,39 @@ pub fn within(trail: &Trail, wanted: &[String]) -> bool {
 /// a Match with none kept becomes a Context line when it lies within `before`/`after` lines of
 /// a kept Match, and is dropped otherwise. Context lines stay only near a kept Match.
 /// `stats.binary_cut` is moved so it still counts the items found before the binary data.
+/// `retain` for an object file's rows (M4 plan, D8): a row keeps only its kept Matches and
+/// their Trails, and a row with none kept is dropped (code review of the M4 plan r1, Codex #7).
+/// Rows are strings, not lines, so none becomes a context line. With `max`, the first `max`
+/// rows that stay: rows share their lines, so -m counts rows (Codex #8).
+pub fn retain_rows(d: &mut Described, keep: &[Vec<bool>], max: Option<u64>) {
+    let items = std::mem::take(&mut d.file.items);
+    let trails = std::mem::take(&mut d.trails);
+    let max = max.map_or(usize::MAX, |m| usize::try_from(m).unwrap_or(usize::MAX));
+    for ((item, ts), ks) in items.into_iter().zip(trails).zip(keep) {
+        if d.file.items.len() == max {
+            break;
+        }
+        let Item::Match(mut m) = item else { continue };
+        if !ks.iter().any(|&b| b) {
+            continue;
+        }
+        debug_assert_eq!(ks.len(), m.submatches.len());
+        m.submatches = m
+            .submatches
+            .into_iter()
+            .zip(ks)
+            .filter_map(|(s, &k)| k.then_some(s))
+            .collect();
+        let ts = ts
+            .into_iter()
+            .zip(ks)
+            .filter_map(|(t, &k)| k.then_some(t))
+            .collect();
+        d.file.items.push(Item::Match(m));
+        d.trails.push(ts);
+    }
+}
+
 pub fn retain(d: &mut Described, keep: &[Vec<bool>], before: usize, after: usize) {
     debug_assert_eq!(keep.len(), d.trails.len());
     // Lines of the kept Matches, in file order (items are sorted by line).
@@ -236,6 +269,7 @@ mod tests {
             format: Some("markdown".into()),
             trails,
             sections: vec![],
+            offsets: false,
         }
     }
 
```

### Task 11.3: the pipeline (src/app.rs)

**Files:** `src/app.rs`.

- [ ] **Step 1:** the changes, by function (the two `offsets: false` lines are C1's):
  - `main`: the Registry and the Detector are built before `Ctx` when the mode is `Trail` or
    `--docs` is given, the Detector `with_docs(args.docs)`; `Ctx` gets `pre` (`Preprocess`) and
    `docs` (the Detector, with `--docs`).
  - `Target` gets `through` and `doc`; `Ctx::target` sets `through` (`Preprocess::through`, else
    for a regular file `Ctx::document`, which asks `Detector::may_be_document` first) and makes
    the Target not `regular` when it is set.
  - `Ctx::walk` passes every Target through `Ctx::prepare` (design note 4); `Ctx::open_input`
    opens what is searched (design note 3), and replaces `File::open` in `search_raw`,
    `stream_file`, `search_first`, `search_unnamed` and `read_probed`; every in-memory branch
    requires `through.is_none()`.
  - `trail_file`: its fast path only without `through`; `plain_file` returns false for a file
    with a Match whose first bytes are an object magic (`head_of`, design note 2).
  - `describe`: a document's Trails from its Outline (`outline_trails`); else, unless a
    transform gave the bytes, `Ctx::object`; else as before, a transform's output without an
    `object` or `docs` Format (D21). `--within` and `--expand` move to `Ctx::filter`, which uses
    `retain_rows` (with `-m`) and no Sections for object rows.
  - `names_of`: a document's names from its Outline.
  - `disk_of`: None with `through`.

```diff
diff --git a/src/app.rs b/src/app.rs
--- a/src/app.rs
+++ b/src/app.rs
@@ -1,6 +1,7 @@
 //! The pipeline (plan, Task 9.1): command line, actions, walk, search, Processor, filters and
 //! printers, with ripgrep's exit codes (design notes 9 to 11).
 use crate::cli::{self, Args, CliError, ColorWhen};
+use crate::docs;
 use crate::filter;
 use crate::formats::{self, Registry, detector::Detector};
 use crate::model::{Described, FileMatches, FileStats, Input, Item, Line, Name, Submatch, Trail};
@@ -11,6 +12,7 @@ use crate::output::{
     stream::{StreamFile, StreamSink},
     summary,
 };
+use crate::preprocess::{Preprocess, Through};
 use crate::processors::{self, Processor};
 use crate::search::{self, CollectSink, MaxCount};
 use crate::source::Source;
@@ -295,10 +297,20 @@ fn main(args: &Args) -> anyhow::Result<i32> {
     } else {
         args.fragment_width
     };
+    // The Format Definitions: for the Trails, and with --docs for every mode, which must tell a
+    // document by its name before the search (M4 plan, design note 3).
+    let registry = (matches!(mode, Mode::Trail) || args.docs)
+        .then(|| load_registry(&msgs, formats::user_formats_dir().as_deref()));
+    let detector = match &registry {
+        Some(r) => Some(Detector::new(r, &args.formats)?.with_docs(args.docs)),
+        None => None,
+    };
     let ctx = Ctx {
         args,
         matcher,
         msgs,
+        pre: Preprocess::new(args, &std::env::current_dir()?)?,
+        docs: detector.as_ref().filter(|_| args.docs),
         in_memory: search::rg_in_memory(&paths, implicit_stdin),
         paths,
         strip_dot: implicit_cwd,
@@ -350,9 +362,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
             }
         }
         Mode::Trail => {
-            let user_dir = formats::user_formats_dir();
-            let registry = load_registry(&ctx.msgs, user_dir.as_deref());
-            let detector = Detector::new(&registry, &args.formats)?;
+            let detector = detector.as_ref().expect("built for Mode::Trail");
             let ro = RenderOptions {
                 layout,
                 line_numbers: !args.no_line_number,
@@ -371,7 +381,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
                     buf: ctx.out.buffer(),
                     binary_quit: false,
                 },
-                |st, t| ctx.trail_file(st, &t, &detector, &ro),
+                |st, t| ctx.trail_file(st, &t, detector, &ro),
             )?;
             if layout == Layout::Json && summary_mode.is_none() {
                 ctx.print_summary();
@@ -425,7 +435,13 @@ struct Target {
     path: PathBuf,
     fs: Option<PathBuf>,
     explicit: bool,
+    /// False also for a file read through `--pre`, `-z` or an Extractor: what is searched is
+    /// not the file's bytes, and can be read only once, as a pipe (M4 plan, design note 3).
     regular: bool,
+    /// How the file is read when not as it is.
+    through: Option<Through>,
+    /// A document's text and Outline, extracted in `Ctx::prepare` (M4 plan, design note 4).
+    doc: Option<std::sync::Arc<docs::Extracted>>,
 }
 
 /// What a Trail-mode thread keeps between files: a searcher for files and stdin (BOM sniffing
@@ -503,6 +519,10 @@ struct Ctx<'a> {
     /// The run's Processors (design note 3), dropped with `Ctx`, which closes every helper's
     /// stdin (spec §4.4, Lifetime).
     processors: processors::Processors,
+    /// `--pre` and `-z`.
+    pre: Preprocess,
+    /// With --docs, the Detector that tells documents by their name or magic bytes.
+    docs: Option<&'a Detector<'a>>,
 }
 
 impl Ctx<'_> {
@@ -525,7 +545,7 @@ impl Ctx<'_> {
                 match r {
                     Ok(d) => {
                         if let Some(t) = self.target(&d) {
-                            visit(&mut state, t);
+                            visit(&mut state, self.prepare(t));
                         }
                     }
                     Err(e) => self.msgs.error(e),
@@ -542,7 +562,7 @@ impl Ctx<'_> {
                     match r {
                         Ok(d) => {
                             if let Some(t) = self.target(&d) {
-                                visit(&mut state, t);
+                                visit(&mut state, self.prepare(t));
                             }
                         }
                         Err(e) => self.msgs.error(e),
@@ -567,6 +587,8 @@ impl Ctx<'_> {
                 fs: None,
                 explicit: true,
                 regular: false,
+                through: None,
+                doc: None,
             });
         }
         let is_dir = d.file_type().is_some_and(|t| t.is_dir());
@@ -583,14 +605,108 @@ impl Ctx<'_> {
         // The walker only yields regular files, and its file type costs no system call; a path
         // named on the command line may be a link to anything, so it alone is looked up.
         let regular = d.file_type().is_some_and(|t| t.is_file()) || (explicit && rewindable(p));
+        // Only a regular file can be a document: its first bytes are read before the search
+        // and the file is opened again, which a FIFO, a descriptor or stdin by name would not
+        // survive (code review of the M4 plan r1, Codex #3).
+        let through = self
+            .pre
+            .through(p)
+            .or_else(|| regular.then(|| self.document(p)).flatten());
         Some(Target {
             path: shown.to_path_buf(),
             fs: Some(p.to_path_buf()),
             explicit,
-            regular,
+            regular: regular && through.is_none(),
+            through,
+            doc: None,
         })
     }
 
+    /// With --docs, `Through::Docs` for a file whose Format is a document Format: by its name,
+    /// or, for a name without an extension, by its first bytes (read only then).
+    fn document(&self, p: &Path) -> Option<Through> {
+        let det = self.docs.filter(|d| d.may_be_document(p))?;
+        let def = match det.detect_by_path(p) {
+            Some(d) => d?,
+            None => {
+                let mut head = Vec::with_capacity(crate::source::READ_HEAD);
+                File::open(p)
+                    .and_then(|f| {
+                        f.take(crate::source::READ_HEAD as u64)
+                            .read_to_end(&mut head)
+                    })
+                    .ok()?;
+                det.detect(p, &head)?
+            }
+        };
+        if def.processor != "docs" {
+            return None;
+        }
+        let extractor = def.params.get("extractor")?.as_str()?;
+        docs::Kind::from_name(extractor)?;
+        Some(Through::Docs(def.name.clone(), extractor.to_string()))
+    }
+
+    /// Extracts a document's text (`Through::Docs`). A file whose bytes are not of its
+    /// document Format is searched as it is, silently; one the Extractor cannot read is too,
+    /// with one warning, `sfor: <path>: <format>: <reason>` (spec §5.8, M4 plan D10).
+    fn prepare(&self, mut t: Target) -> Target {
+        let Some(Through::Docs(name, extractor)) = &t.through else {
+            return t;
+        };
+        let name = name.clone();
+        let kind = docs::Kind::from_name(extractor).expect("checked by `document`");
+        let fs = t.fs.clone().expect("only files are documents");
+        let as_is = |mut t: Target| {
+            t.through = None;
+            t.regular = rewindable(&fs);
+            t
+        };
+        let src = match Source::open(&fs, t.explicit) {
+            Ok(s) => s,
+            // The search opens it again and reports the error as ripgrep would.
+            Err(_) => return as_is(t),
+        };
+        if !kind.looks_like(src.as_read()) {
+            return as_is(t);
+        }
+        match docs::extract(kind, src.as_read()) {
+            Ok(e) => {
+                if e.cut {
+                    self.msgs.warn(format_args!(
+                        "sfor: {}: {name}: the text is cut at {} MiB",
+                        t.path.display(),
+                        docs::MAX_TEXT >> 20
+                    ));
+                }
+                t.doc = Some(std::sync::Arc::new(e));
+                t
+            }
+            Err(e) => {
+                self.msgs
+                    .warn(format_args!("sfor: {}: {name}: {e}", t.path.display()));
+                as_is(t)
+            }
+        }
+    }
+
+    /// What is searched for `t`: stdin, a document's text, the output of `--pre` or of a
+    /// decompressor, or the file.
+    fn open_input<'t>(&'t self, t: &'t Target) -> std::io::Result<Box<dyn Read + 't>> {
+        let Some(p) = &t.fs else {
+            return Ok(Box::new(std::io::stdin().lock()));
+        };
+        if let Some(doc) = &t.doc {
+            return Ok(Box::new(std::io::Cursor::new(doc.text.as_slice())));
+        }
+        match &t.through {
+            Some(how @ (Through::Pre | Through::Zip)) => {
+                self.pre.open(how, p).map(|r| r as Box<dyn Read>)
+            }
+            _ => Ok(Box::new(File::open(p)?)),
+        }
+    }
+
     /// Searches a file on disk or stdin, streaming (passthrough and the quiet path). A file
     /// searched in memory is mapped.
     fn search_raw<S: Sink<Error = std::io::Error>>(
@@ -602,9 +718,8 @@ impl Ctx<'_> {
         searcher.set_binary_detection(search::binary_detection(self.args, t.explicit));
         let m = &self.matcher;
         match &t.fs {
-            None => searcher.search_reader(m, std::io::stdin().lock(), sink),
-            Some(p) if self.in_memory => searcher.search_path(m, p, sink),
-            Some(p) => searcher.search_reader(m, File::open(p)?, sink),
+            Some(p) if self.in_memory && t.through.is_none() => searcher.search_path(m, p, sink),
+            _ => searcher.search_reader(m, self.open_input(t)?, sink),
         }
     }
 
@@ -777,9 +892,13 @@ impl Ctx<'_> {
     fn trail_file(&self, st: &mut TrailState, t: &Target, detector: &Detector, ro: &RenderOptions) {
         // Without structural flags, rows are printed as the searcher reports them: Plain files,
         // and files on disk whose Processor builds an Outline (code review r1, Fable #1).
-        if !self.args.structural() && self.summary.is_none() && !self.args.quiet {
+        if !self.args.structural()
+            && self.summary.is_none()
+            && !self.args.quiet
+            && t.through.is_none()
+        {
             match detector.detect_by_path(&t.path) {
-                Some(None) => return self.plain_file(st, t, ro),
+                Some(None) if self.plain_file(st, t, ro, detector) => return,
                 Some(Some(def)) if t.regular => {
                     if let Some(p) = processors::by_name(&def.processor)
                         && builds_outline(p)
@@ -893,17 +1012,30 @@ impl Ctx<'_> {
     /// A Plain file without structural flags, printed row by row as the searcher reports
     /// them, without a model of the file (code review r1, Fable #1). In memory, as ripgrep
     /// (explicit files, at most 10), through the Source, which says whether a BOM makes the
-    /// search a reader one; else straight from the file or stdin.
-    fn plain_file(&self, st: &mut TrailState, t: &Target, ro: &RenderOptions) {
+    /// search a reader one; else straight from the file or stdin. Returns false, printing
+    /// nothing, for a file with a Match whose first bytes are an object Format's magic: its
+    /// name said Plain, but it may be an object file (spec §4.1, M4 plan D5).
+    fn plain_file(
+        &self,
+        st: &mut TrailState,
+        t: &Target,
+        ro: &RenderOptions,
+        detector: &Detector,
+    ) -> bool {
+        let object = |head: &[u8]| !self.args.invert_match && detector.object(head).is_some();
         match &t.fs {
-            Some(p) if self.in_memory => match Source::open(p, t.explicit) {
+            Some(p) if self.in_memory && t.through.is_none() => match Source::open(p, t.explicit) {
+                Ok(src) if object(src.head()) => return false,
                 Ok(src) => self.stream_file(st, t, ro, Streamed::Source(&src), None, None),
                 Err(e) => self.msgs.error(format_args!("{}: {e}", t.path.display())),
             },
             // A file without Matches costs what it costs ripgrep: no line is counted (grill 8385).
             Some(p) if t.regular && self.probe(st, t, p) != Some(true) => {}
+            // Its first bytes are read only once it has a Match.
+            Some(p) if t.regular && object(&head_of(p)) => return false,
             _ => self.stream_file(st, t, ro, Streamed::Raw, None, None),
         }
+        true
     }
 
     /// Whether the file on disk at `path` has a Match, searched up to its first one without
@@ -1043,8 +1175,7 @@ impl Ctx<'_> {
             StreamSink::new(&mut w, &self.matcher, self.args.invert_match, ro, &jo, file);
         let m = &self.matcher;
         let r = match (how, &t.fs) {
-            (Streamed::Raw, None) => searcher.search_reader(m, std::io::stdin().lock(), &mut sink),
-            (Streamed::Raw, Some(p)) => match File::open(p) {
+            (Streamed::Raw, _) => match self.open_input(t) {
                 Ok(f) => searcher.search_reader(m, f, &mut sink),
                 Err(e) => Err(e),
             },
@@ -1104,7 +1235,7 @@ impl Ctx<'_> {
                 Err(())
             }
         };
-        if let (Some(p), true) = (&t.fs, self.in_memory) {
+        if let (Some(p), true) = (&t.fs, self.in_memory && t.through.is_none()) {
             let src = open(p)?;
             // A BOM: a reader search that decodes it, as ripgrep's (`Source::as_read`).
             let in_memory = !src.had_bom;
@@ -1148,16 +1279,13 @@ impl Ctx<'_> {
             Some(p) if t.regular => {
                 File::open(p).and_then(|f| st.unnumbered.search_reader(&self.matcher, f, &mut sink))
             }
-            fs => {
-                let inner: Box<dyn Read> = match fs {
-                    None => Box::new(std::io::stdin().lock()),
-                    Some(p) => match std::fs::File::open(p) {
-                        Ok(f) => Box::new(f),
-                        Err(e) => {
-                            report(&e);
-                            return Err(());
-                        }
-                    },
+            _ => {
+                let inner = match self.open_input(t) {
+                    Ok(r) => r,
+                    Err(e) => {
+                        report(&e);
+                        return Err(());
+                    }
                 };
                 let tee = kept.insert(Tee {
                     inner,
@@ -1251,8 +1379,33 @@ impl Ctx<'_> {
         let found = if self.args.in_headings {
             self.names_of(st, t, detector)?
         } else {
-            self.search_first(st, t, detector)?.map(|(src, fm)| {
-                let def = detector.detect(&src.path, src.head());
+            let Some((src, fm)) = self.search_first(st, t, detector)? else {
+                return Ok(None);
+            };
+            if let Some(doc) = &t.doc {
+                // A document: the Trails of its Extractor's Outline (M4 plan, design note 4).
+                let Some(Through::Docs(name, _)) = &t.through else {
+                    unreachable!("a document is read through its Extractor")
+                };
+                let d = Described {
+                    trails: outline_trails(&doc.outline, &fm),
+                    file: fm,
+                    format: Some(name.clone()),
+                    sections: Vec::new(),
+                    offsets: false,
+                };
+                return self.filter(st, t, src, d);
+            }
+            // What a transform outputs is no object file nor document, whatever its bytes or
+            // --format say: Plain (spec §5.9; code review of the M4 plan r1, Codex #11).
+            let transformed = matches!(t.through, Some(Through::Pre | Through::Zip));
+            if !transformed && let Some(d) = self.object(st, detector, &src, &fm)? {
+                return self.filter(st, t, src, d);
+            }
+            Some({
+                let def = detector.detect(&src.path, src.head()).filter(|d| {
+                    !(transformed && matches!(d.processor.as_str(), "object" | "docs"))
+                });
                 // A binary file's coordinates follow NUL conversion: no Processor (step 7). A
                 // Format without a Processor, or whose Processor fails, is Plain (D33).
                 let described = def
@@ -1281,13 +1434,25 @@ impl Ctx<'_> {
                     format,
                     trails,
                     sections: Vec::new(),
+                    offsets: false,
                 };
                 (src, d)
             })
         };
-        let Some((src, mut d)) = found else {
+        let Some((src, d)) = found else {
             return Ok(None);
         };
+        self.filter(st, t, src, d)
+    }
+
+    /// `--within` and `--expand` over a described file.
+    fn filter(
+        &self,
+        st: &mut TrailState,
+        t: &Target,
+        src: Source,
+        mut d: Described,
+    ) -> Result<Option<(Source, Described)>, ()> {
         let searched_to = d.file.items.last().map_or(0, |i| item_line(i).line_number);
         if let Some(w) = &self.args.within {
             let wanted = filter::parse_within(w);
@@ -1296,9 +1461,15 @@ impl Ctx<'_> {
                 .iter()
                 .map(|ts| ts.iter().map(|t| filter::within(t, &wanted)).collect())
                 .collect();
-            filter::retain(&mut d, &keep, self.args.before(), self.args.after());
+            // An object file's rows have no context: a row the filter drops is gone, and -m
+            // counts the rows it keeps (D8).
+            if d.offsets {
+                filter::retain_rows(&mut d, &keep, self.args.max_count);
+            } else {
+                filter::retain(&mut d, &keep, self.args.before(), self.args.after());
+            }
             // -m counts the lines the filter kept (code review r1, Fable #3).
-            if let Some(m) = self.args.max_count {
+            if let Some(m) = self.args.max_count.filter(|_| !d.offsets) {
                 filter::limit(&mut d, m, self.args.before(), self.max_count_after());
                 if d.file.stats.binary_offset.is_some() && !self.detected_by(&d, t, searched_to) {
                     d.file.stats.binary_offset = None;
@@ -1307,12 +1478,103 @@ impl Ctx<'_> {
                 }
             }
         }
-        if self.args.expand && !self.args.in_headings {
+        // An object file's Sections are byte ranges of a binary: --expand prints its rows (M4
+        // plan, D8).
+        if self.args.expand && !self.args.in_headings && !d.offsets {
             d.sections = filter::sections(&d);
         }
         Ok(Some((src, d)))
     }
 
+    /// An object file with Matches (spec §5.7): its rows, the printable strings that hold the
+    /// Matches of a search of the whole file, each with the Trail of each Match. None: not an
+    /// object file goblin reads, or `-v`, whose lines are the non-matching bytes. The search
+    /// that found the file's first Match is rg's; this one reads on past the binary data, so it
+    /// costs only a file with a Match (8385).
+    fn object(
+        &self,
+        st: &mut TrailState,
+        detector: &Detector,
+        src: &Source,
+        fm: &FileMatches,
+    ) -> Result<Option<Described>, ()> {
+        if self.args.invert_match {
+            return Ok(None);
+        }
+        let def = match detector.detect(&src.path, src.head()) {
+            Some(d) if d.processor == "object" => d,
+            Some(_) => return Ok(None),
+            None => match detector.object(src.head()) {
+                Some(d) => d,
+                None => return Ok(None),
+            },
+        };
+        let Some(map) = processors::object::Map::parse(src.bytes()) else {
+            return Ok(None);
+        };
+        let started = Instant::now();
+        st.source.set_binary_detection(BinaryDetection::none());
+        let mut sink = CollectSink::new(&self.matcher, false, false).in_memory();
+        if let Err(e) = st
+            .source
+            .search_slice(&self.matcher, src.bytes(), &mut sink)
+        {
+            self.msgs.error(format_args!("{}: {e}", fm.path.display()));
+            return Err(());
+        }
+        let hits: Vec<std::ops::Range<usize>> = sink
+            .items
+            .iter()
+            .filter_map(|i| match i {
+                Item::Match(l) => Some(l),
+                Item::Context(_) => None,
+            })
+            .flat_map(|l| {
+                let base = l.absolute_offset as usize;
+                l.submatches
+                    .iter()
+                    .map(move |s| base + s.start..base + s.end)
+            })
+            .collect();
+        let lines = LineIndex::new(src.bytes());
+        let rows = processors::object::rows::rows(
+            src.bytes(),
+            &hits,
+            search::max_count(self.args),
+            &lines,
+        );
+        let trails = rows
+            .iter()
+            .map(|r| {
+                r.submatches
+                    .iter()
+                    .map(|s| map.trail(r.absolute_offset as usize + s.start, &lines))
+                    .collect()
+            })
+            .collect();
+        let stats = FileStats {
+            bytes_searched: src.bytes().len() as u64,
+            matched_lines: rows.len() as u64,
+            matches: rows.iter().map(|r| r.submatches.len() as u64).sum(),
+            binary_offset: None,
+            binary_cut: None,
+            elapsed: started.elapsed(),
+        };
+        st.binary_quit = false;
+        Ok(Some(Described {
+            file: FileMatches {
+                path: fm.path.clone(),
+                explicit: fm.explicit,
+                items: rows.into_iter().map(Item::Match).collect(),
+                stats,
+            },
+            format: Some(def.name.clone()),
+            trails,
+            sections: Vec::new(),
+            offsets: true,
+        }))
+    }
+
     /// --in-headings: the Source and its names, and the Matches in them. A Plain file has no
     /// names: searched, nothing found (code review r1, Fable #5), and not read when its path
     /// says so.
@@ -1390,30 +1652,39 @@ impl Ctx<'_> {
                 replaced = true;
             }
         }
-        let def = detector.detect(&src.path, src.head());
-        // A Format without a Processor is Plain: no names (D33).
-        let Some((p, def)) = def.and_then(|d| Some((self.processors.for_definition(d)?, d))) else {
-            self.count_unnamed(st, binary.as_ref(), &src);
-            return Ok(None);
-        };
-        let disk = if replaced { None } else { disk_of(t, &src) };
-        let input = Input {
-            bytes: src.bytes(),
-            path: &src.path,
-            format: &def.name,
-            disk: disk.as_deref(),
-        };
-        let names = match p.names(&input, &def.params) {
-            Ok(n) => n,
-            Err(e) => {
-                // A failing Processor skips the file with one warning; an unavailable one
-                // (a disabled helper) says nothing more (spec §4.4, Failures).
-                if e.downcast_ref::<crate::helper::Unavailable>().is_none() {
-                    self.msgs
-                        .warn(format_args!("sfor: {}: {e:#}", src.path.display()));
+        // A document's names are its Outline's: its headings (M4 plan, D11).
+        let (names, format) = if let (Some(doc), Some(Through::Docs(name, _))) =
+            (&t.doc, &t.through)
+        {
+            (crate::trail::names_of(&doc.outline), name.clone())
+        } else {
+            let def = detector.detect(&src.path, src.head());
+            // A Format without a Processor is Plain: no names (D33).
+            let Some((p, def)) = def.and_then(|d| Some((self.processors.for_definition(d)?, d)))
+            else {
+                self.count_unnamed(st, binary.as_ref(), &src);
+                return Ok(None);
+            };
+            let disk = if replaced { None } else { disk_of(t, &src) };
+            let input = Input {
+                bytes: src.bytes(),
+                path: &src.path,
+                format: &def.name,
+                disk: disk.as_deref(),
+            };
+            let names = match p.names(&input, &def.params) {
+                Ok(n) => n,
+                Err(e) => {
+                    // A failing Processor skips the file with one warning; an unavailable one
+                    // (a disabled helper) says nothing more (spec §4.4, Failures).
+                    if e.downcast_ref::<crate::helper::Unavailable>().is_none() {
+                        self.msgs
+                            .warn(format_args!("sfor: {}: {e:#}", src.path.display()));
+                    }
+                    return Err(());
                 }
-                return Err(());
-            }
+            };
+            (names, def.name.clone())
         };
         let mut found = self.heading_matches(&src, &names);
         // What rg detects: `-m` and its after context may stop its search first.
@@ -1464,7 +1735,8 @@ impl Ctx<'_> {
         Ok(found.map(|(file, trails, sections)| {
             let d = Described {
                 file,
-                format: Some(def.name.clone()),
+                format: Some(format),
+                offsets: false,
                 trails,
                 // With --expand, the Section of each named Segment (code review r1, Fable #4).
                 sections: if self.args.expand {
@@ -1483,15 +1755,12 @@ impl Ctx<'_> {
         let report = |e: &dyn std::fmt::Display| {
             self.msgs.error(format_args!("{}: {e}", t.path.display()));
         };
-        let reader: Box<dyn Read> = match &t.fs {
-            None => Box::new(std::io::stdin().lock()),
-            Some(p) => match File::open(p) {
-                Ok(f) => Box::new(f),
-                Err(e) => {
-                    report(&e);
-                    return Err(());
-                }
-            },
+        let reader = match self.open_input(t) {
+            Ok(r) => r,
+            Err(e) => {
+                report(&e);
+                return Err(());
+            }
         };
         let detection = search::binary_detection(self.args, t.explicit);
         let quit = detection.quit_byte().is_some();
@@ -1604,7 +1873,7 @@ impl Ctx<'_> {
         if (!quit && detection.convert_byte().is_none()) || !src.bytes().contains(&0) {
             return Ok(None);
         }
-        let in_memory = t.fs.is_some() && self.in_memory && !src.had_bom;
+        let in_memory = t.fs.is_some() && self.in_memory && t.through.is_none() && !src.had_bom;
         if in_memory {
             let ahead = &src.bytes()[..src.bytes().len().min(64 * 1024)];
             return Ok(Some(BinaryProbe {
@@ -1645,15 +1914,12 @@ impl Ctx<'_> {
         let report = |e: &dyn std::fmt::Display| {
             self.msgs.error(format_args!("{}: {e}", t.path.display()));
         };
-        let inner: Box<dyn Read> = match &t.fs {
-            None => Box::new(std::io::stdin().lock()),
-            Some(p) => match File::open(p) {
-                Ok(f) => Box::new(f),
-                Err(e) => {
-                    report(&e);
-                    return Err(());
-                }
-            },
+        let inner = match self.open_input(t) {
+            Ok(r) => r,
+            Err(e) => {
+                report(&e);
+                return Err(());
+            }
         };
         let mut tee = Tee {
             inner,
@@ -1945,6 +2211,10 @@ fn builds_outline(p: &dyn Processor) -> bool {
 /// resolve to a regular file. Anything else reaches the helper as a temporary copy (spec §4.4).
 /// Called only for files with Matches (D8, review r4, Codex #2).
 fn disk_of(t: &Target, src: &Source) -> Option<PathBuf> {
+    // Bytes from `--pre`, `-z` or an Extractor are not the file's (spec §4.4, Paths).
+    if t.through.is_some() {
+        return None;
+    }
     let real = std::fs::canonicalize(t.fs.as_deref()?).ok()?;
     if real.starts_with("/dev") || real.starts_with("/proc") {
         return None;
@@ -1953,12 +2223,28 @@ fn disk_of(t: &Target, src: &Source) -> Option<PathBuf> {
     (m.is_file() && !src.had_bom && m.len() == src.bytes().len() as u64).then_some(real)
 }
 
+/// `trails[i][k]` of the Matches of `fm` from an Outline, as `OutlineProcessor` gives them.
+fn outline_trails(o: &Outline, fm: &FileMatches) -> Vec<Vec<Trail>> {
+    let mut c = crate::trail::TrailCursor::new(o);
+    fm.matches()
+        .map(|m| m.anchors().into_iter().map(|a| c.trail(a)).collect())
+        .collect()
+}
+
 fn empty_trails(fm: &FileMatches) -> Vec<Vec<Trail>> {
     fm.matches()
         .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
         .collect()
 }
 
+/// The first 16 bytes of the file at `p`, enough for every object magic; empty on an error,
+/// which the search reports.
+fn head_of(p: &Path) -> Vec<u8> {
+    let mut head = Vec::with_capacity(16);
+    let _ = File::open(p).and_then(|f| f.take(16).read_to_end(&mut head));
+    head
+}
+
 /// A regular file, which can be opened again and read the same: not `/dev/stdin` or a FIFO.
 fn rewindable(p: &Path) -> bool {
     std::fs::metadata(p).is_ok_and(|m| m.is_file())
```

- [ ] **Step 2:** `cargo test --test cli_m4` with `INSTA_UPDATE=always`, then read every new
  `tests/snapshots/cli_m4__*.snap` in full. The one-line and tree snapshots of the probe are
  those of `m4_02`, `m4_06` and the spec's examples; the JSON ones carry `"format"` and
  validate against the schema. Then `SFOR_REQUIRE_REFERENCE=1 cargo test --test cli_m4`:
  15 tests pass.
- [ ] **Step 3:** the whole suite: every M1–M3 test passes unchanged (the fixture copy now holds
  `tests/fixtures/docs/*.sqlite`, which rg and sfor without `--docs` search alike).

## Chunk 12: the fuzz

### Task 12.1: the M4 targets of tests/fuzz.rs

**Files:** `tests/fuzz.rs`, and `src/docs/fixture/mod.rs` only if `rezip` is missing (C4 writes it).

- [ ] **Step 1:** the three tests (D28): mutated documents through `docs::extract`, run in a
  child process (this test binary running only that test) that `supervised` watches, killing
  and reaping it when a case prints nothing for 2 s and going on from the next case in a new
  child; mutated object files through `object::Map` and `object::rows`; and
  `the_supervisor_kills_a_case_that_never_ends`, whose child spins and allocates at case 1 and
  reports a panic at case 3: both are reported, and the run ends in under 10 s (RED before
  `supervised` exists: it does not compile; review r1, Codex #13, and r2, Codex #7):

```diff
diff --git a/tests/fuzz.rs b/tests/fuzz.rs
--- a/tests/fuzz.rs
+++ b/tests/fuzz.rs
@@ -688,3 +688,379 @@ fn generated_files_do_not_panic_sfor() {
     }
     eprintln!("fuzz: {files} files through sfor");
 }
+
+// M4 (plan, Task 12.1): the Extractors and the `object` Processor over mutated files. Each seed
+// is a file the fixture writers make; a mutation flips, deletes, repeats or inserts bytes,
+// cuts the file, or, inside a zip, does the same to the XML of one part and zips it again.
+// Nothing may panic, take more than 2 s, or give an Outline outside its text.
+
+use searchfor::docs::{self, Kind, fixture as doc};
+use searchfor::processors::object::{self, fixture as obj};
+use std::time::Duration;
+
+/// Bytes the XML mutations insert.
+#[rustfmt::skip]
+const XML: &[&str] = &[
+    "<", ">", "</", "/>", "<w:p>", "</w:p>", "<w:t>", "</w:t>", "<w:tbl>", "<w:tr>", "<w:tc>",
+    "</w:tc>", "<a:p>", "<text:p>", "<text:h text:outline-level=\"9\">", "<table:table-cell table:number-columns-repeated=\"99999999\"/>",
+    "<row r=\"999999999\">", "<c r=\"XFD1048576\" t=\"s\"><v>99999</v></c>", "&amp;", "&#x0;", "&#xFFFFFF;", "&bogus;",
+    "<![CDATA[", "]]>", "<!--", "-->", "\"", "'", "=", "needle", "ñandú", "\0", "\u{feff}",
+    "<h1>", "</h1>", "<br/>", "<script>", "<nav epub:type=\"toc\">", "<a href=\"../../../x\">",
+];
+
+fn mutate(rng: &mut Rng, mut b: Vec<u8>, pool: &[&str]) -> Vec<u8> {
+    for _ in 0..1 + rng.below(4) {
+        let at = rng.below(b.len() + 1);
+        match rng.below(6) {
+            0 if !b.is_empty() => {
+                let i = at.min(b.len() - 1);
+                b[i] ^= 1 << rng.below(8);
+            }
+            1 => {
+                let end = (at + rng.below(64)).min(b.len());
+                b.drain(at..end);
+            }
+            2 => {
+                let end = (at + rng.below(256)).min(b.len());
+                let copy = b[at..end].to_vec();
+                b.splice(at..at, copy);
+            }
+            3 => b.truncate(at),
+            _ => {
+                let s = rng.pick(pool).as_bytes().to_vec();
+                b.splice(at..at, s);
+            }
+        }
+    }
+    b
+}
+
+/// The seeds of each document kind.
+fn document_seeds() -> Vec<(Kind, Vec<u8>)> {
+    let blocks = [
+        doc::h(1, "1 Intro"),
+        doc::p("needle é"),
+        doc::h(2, "1.1 ñandú"),
+        doc::table(&[&["A", "B"], &["needle", "日本"], &["x"]]),
+        doc::p("after"),
+    ];
+    let sheet: &[&[&str]] = &[&["Item", "Price"], &["needle", "1.5"], &["", "", "x"]];
+    let slides: &[(&[&str], Option<&str>)] =
+        &[(&["T", "needle"], Some("notes needle")), (&["2"], None)];
+    let sqlite = |n: &str| std::fs::read(common::source_dir().join("docs").join(n)).unwrap();
+    vec![
+        (Kind::Docx, doc::docx(&blocks)),
+        (Kind::Odt, doc::odt(&blocks)),
+        (Kind::Xlsx, doc::xlsx(&[("S", sheet), ("T", sheet)])),
+        (Kind::Ods, doc::ods(&[("S", sheet)])),
+        (Kind::Pptx, doc::pptx(slides)),
+        (Kind::Odp, doc::odp(slides)),
+        (
+            Kind::Epub,
+            doc::epub(&[("1", "<h1>H</h1><p>needle</p>"), ("2", "<p>x</p>")], true),
+        ),
+        (Kind::Epub, doc::epub(&[("1", "<p>needle</p>")], false)),
+        (
+            Kind::Pdf,
+            doc::pdf(
+                &[&["1 Intro", "needle"], &["2 Costs", "é needle"]],
+                &[(1, "1 Intro", 1), (2, "2 Costs", 2)],
+            ),
+        ),
+        (Kind::Sqlite, sqlite("shop.sqlite")),
+        (Kind::Sqlite, sqlite("many.sqlite")),
+    ]
+}
+
+/// The checks of one extraction: no panic, ended within 2 s whatever it returns, and an
+/// Outline inside its text. A case that never ends is the supervisor's (`supervised`).
+fn check_doc(kind: Kind, bytes: &[u8]) -> Result<(), String> {
+    let started = std::time::Instant::now();
+    let got = catch_unwind(AssertUnwindSafe(|| docs::extract(kind, bytes)));
+    let took = started.elapsed();
+    if took > DEADLINE {
+        return Err(format!("took {took:?}"));
+    }
+    let e = match got {
+        Err(e) => {
+            let msg = e
+                .downcast_ref::<String>()
+                .cloned()
+                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
+                .unwrap_or_default();
+            return Err(format!("panic: {msg}"));
+        }
+        Ok(Err(_)) => return Ok(()),
+        Ok(Ok(e)) => e,
+    };
+    if std::str::from_utf8(&e.text).is_err() || e.text.contains(&0) {
+        return Err("text is not UTF-8 without NUL".into());
+    }
+    for s in &e.outline.segments {
+        if s.range.start > s.range.end || s.range.end > e.text.len() {
+            return Err(format!("Segment {:?} over {} bytes", s.range, e.text.len()));
+        }
+    }
+    check_names(&searchfor::trail::names_of(&e.outline), &e.text)
+}
+
+const DEADLINE: Duration = Duration::from_secs(2);
+/// In a child process of `supervised`: the first case it runs.
+const CHILD_FROM: &str = "SFOR_FUZZ_CHILD_FROM";
+
+/// Runs the cases of the test `test` in a child process, this test binary running only that
+/// test with `CHILD_FROM` set, and watches it (code review of the M4 plan r2, Codex #7). The
+/// child prints `fuzzcase run N` before case N, `fuzzcase end N` after it, followed by its
+/// failure if it failed, and `fuzzcase done` at the end. A case that prints nothing for
+/// `limit` (plus a second for the child's start) has its child killed and reaped, and the
+/// cases after it run in a new child; one whose child dies (an abort, out of memory) fails
+/// too. Returns the failures.
+fn supervised(test: &str, limit: Duration) -> Vec<String> {
+    use std::io::BufRead;
+    let mut failures = Vec::new();
+    let mut from = 0usize;
+    'children: loop {
+        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
+            .args(["--exact", test, "--nocapture", "--test-threads", "1"])
+            .env(CHILD_FROM, from.to_string())
+            .stdout(std::process::Stdio::piped())
+            .stderr(std::process::Stdio::null())
+            .spawn()
+            .unwrap();
+        let out = std::io::BufReader::new(child.stdout.take().unwrap());
+        let (tx, rx) = std::sync::mpsc::channel();
+        std::thread::spawn(move || {
+            for line in out.lines().map_while(Result::ok) {
+                if tx.send(line).is_err() {
+                    break;
+                }
+            }
+        });
+        let mut running: Option<usize> = None;
+        let mut wait = limit + Duration::from_secs(1);
+        loop {
+            match rx.recv_timeout(wait) {
+                Ok(line) => {
+                    wait = limit;
+                    let Some(msg) = line.strip_prefix("fuzzcase ") else {
+                        continue;
+                    };
+                    if msg == "done" {
+                        let _ = child.wait();
+                        return failures;
+                    }
+                    let (what, rest) = msg.split_once(' ').unwrap_or((msg, ""));
+                    let (n, failure) = rest.split_once(' ').unwrap_or((rest, ""));
+                    let n: usize = n.parse().unwrap();
+                    match what {
+                        "run" => running = Some(n),
+                        _ => {
+                            running = None;
+                            if !failure.is_empty() {
+                                failures.push(failure.replace("\\n", "\n"));
+                            }
+                        }
+                    }
+                }
+                Err(e) => {
+                    let timed_out = e == std::sync::mpsc::RecvTimeoutError::Timeout;
+                    if timed_out {
+                        child.kill().unwrap();
+                    }
+                    // Reaped: the child is gone, and its memory with it.
+                    let status = child.wait().unwrap();
+                    let Some(n) = running else {
+                        failures.push(format!("the child stopped between cases ({status})"));
+                        return failures;
+                    };
+                    failures.push(if timed_out {
+                        format!("case {n}: still running after {limit:?}, killed")
+                    } else {
+                        format!("case {n}: the child died ({status})")
+                    });
+                    from = n + 1;
+                    continue 'children;
+                }
+            }
+        }
+    }
+}
+
+/// In a child of `supervised`: runs the cases from `CHILD_FROM` on, reporting each.
+fn run_cases(cases: impl Iterator<Item = (Kind, Vec<u8>)>) {
+    let from: usize = std::env::var(CHILD_FROM).unwrap().parse().unwrap();
+    std::panic::set_hook(Box::new(|_| {}));
+    for (i, (kind, bytes)) in cases.enumerate() {
+        if i < from {
+            continue;
+        }
+        println!("fuzzcase run {i}");
+        let failure = check_doc(kind, &bytes)
+            .err()
+            .map(|e| {
+                format!("{kind:?}: {e}\n  input: \"{}\"", bytes.escape_ascii()).replace('\n', "\\n")
+            })
+            .unwrap_or_default();
+        println!("fuzzcase end {i} {failure}");
+    }
+    println!("fuzzcase done");
+}
+
+/// The mutated documents, in order: the same for every child.
+fn mutated_documents() -> impl Iterator<Item = (Kind, Vec<u8>)> {
+    let mut rng = Rng(seed() ^ 2);
+    let n = count() / 4;
+    document_seeds()
+        .into_iter()
+        .flat_map(move |(kind, seed_bytes)| {
+            let mut out = Vec::with_capacity(n);
+            for _ in 0..n {
+                let zipped = seed_bytes.starts_with(b"PK\x03\x04");
+                let bytes = if zipped && rng.below(2) == 0 {
+                    // The XML of one part, changed and zipped again.
+                    let mut target = None;
+                    doc::rezip(&seed_bytes, |name, data| {
+                        if target.is_none() && name.ends_with("ml") && rng.below(3) == 0 {
+                            target = Some(name.to_string());
+                            return mutate(&mut rng, data, XML);
+                        }
+                        data
+                    })
+                } else {
+                    mutate(&mut rng, seed_bytes.clone(), XML)
+                };
+                out.push((kind, bytes));
+            }
+            out
+        })
+}
+
+#[test]
+fn mutated_documents_do_not_panic_the_extractors() {
+    if std::env::var_os(CHILD_FROM).is_some() {
+        return run_cases(mutated_documents());
+    }
+    let tried = mutated_documents().count();
+    let failures = supervised("mutated_documents_do_not_panic_the_extractors", DEADLINE);
+    eprintln!("fuzz: {tried} documents, {} failures", failures.len());
+    assert!(
+        failures.is_empty(),
+        "{} of {tried} documents failed; the first ones:\n{}",
+        failures.len(),
+        failures[..failures.len().min(3)].join("\n")
+    );
+}
+
+/// The supervisor's own test: in its child, case 1 never ends, spinning and allocating, and
+/// case 3 panics; the supervisor kills and reaps the child at case 1, goes on from case 2 in a
+/// new one, and reports both.
+#[test]
+fn the_supervisor_kills_a_case_that_never_ends() {
+    const TEST: &str = "the_supervisor_kills_a_case_that_never_ends";
+    if let Ok(from) = std::env::var(CHILD_FROM) {
+        let from: usize = from.parse().unwrap();
+        for i in from..5 {
+            println!("fuzzcase run {i}");
+            if i == 1 {
+                let mut hog: Vec<Vec<u8>> = Vec::new();
+                loop {
+                    hog.push(vec![1; 1 << 20]);
+                    if hog.len() > 64 {
+                        hog.clear();
+                    }
+                }
+            }
+            let failure = if i == 3 { "Pdf: panic: boom" } else { "" };
+            println!("fuzzcase end {i} {failure}");
+        }
+        println!("fuzzcase done");
+        return;
+    }
+    let started = std::time::Instant::now();
+    let failures = supervised(TEST, Duration::from_millis(300));
+    assert_eq!(
+        failures,
+        [
+            "case 1: still running after 300ms, killed",
+            "Pdf: panic: boom"
+        ]
+    );
+    assert!(
+        started.elapsed() < Duration::from_secs(10),
+        "{:?}",
+        started.elapsed()
+    );
+}
+
+#[test]
+fn mutated_objects_do_not_panic_the_object_processor() {
+    let mut rng = Rng(seed() ^ 3);
+    let spec = obj::Spec::new()
+        .section(".text", b"\x55\xc3\0\0")
+        .section(".rodata", b"\0hello\0version=needle\0")
+        .sized("v", 1, 7, 15)
+        .symbol("_ZN4core3fmt5write17h0123456789abcdefE", 0, 0);
+    let mo = spec.clone();
+    let seeds = vec![
+        obj::elf(&spec).file,
+        obj::pe(&spec).file,
+        obj::macho(&mo).file,
+        obj::fat(&[
+            (0x0100_0007, obj::macho(&mo).file),
+            (
+                0x0100_000c,
+                obj::macho(&mo.clone().machine(0x0100_000c)).file,
+            ),
+        ])
+        .0,
+    ];
+    let hook = std::panic::take_hook();
+    std::panic::set_hook(Box::new(|_| {}));
+    let mut failures = Vec::new();
+    let mut tried = 0;
+    for s in &seeds {
+        for _ in 0..count() / 4 {
+            tried += 1;
+            let bytes = mutate(&mut rng, s.clone(), COMMON);
+            let anchors: Vec<usize> = (0..4).map(|_| rng.below(bytes.len() + 1)).collect();
+            let got = catch_unwind(AssertUnwindSafe(|| {
+                let lines = searchfor::trail::LineIndex::new(&bytes);
+                if let Some(m) = object::Map::parse(&bytes) {
+                    for &a in &anchors {
+                        let _ = m.trail(a, &lines);
+                    }
+                }
+                let hits: Vec<_> = anchors
+                    .iter()
+                    .map(|&a| a..(a + 3).min(bytes.len()))
+                    .collect();
+                let mut sorted = hits.clone();
+                sorted.sort_by_key(|h| h.start);
+                object::rows::rows(&bytes, &sorted, None, &lines)
+            }));
+            match got {
+                Err(_) => failures.push(format!("panic\n  input: \"{}\"", bytes.escape_ascii())),
+                Ok(rows) => {
+                    for r in rows {
+                        let end = r.absolute_offset as usize + r.len;
+                        if end > bytes.len() || r.submatches.iter().any(|s| s.end > r.len) {
+                            failures.push(format!(
+                                "row {}..{end} over {} bytes",
+                                r.absolute_offset,
+                                bytes.len()
+                            ));
+                        }
+                    }
+                }
+            }
+        }
+    }
+    std::panic::set_hook(hook);
+    eprintln!("fuzz: {tried} object files, {} failures", failures.len());
+    assert!(
+        failures.is_empty(),
+        "{} of {tried} failed:\n{}",
+        failures.len(),
+        failures[..failures.len().min(3)].join("\n")
+    );
+}
```

- [ ] **Step 2:** `cargo test --test fuzz mutated` (default: 250 inputs per document seed, 250 per
  object seed), then `SFOR_FUZZ_N=20000 cargo test --release --test fuzz mutated` once (55 000
  documents and 20 000 object files in the probe, 69 s). Before the two fixes of D28 the
  probe's run reported `Docx: verbatim piece 0..8 of "1\0 Intro" over "1  Intro"` and
  `Docx: panic: byte index 12 is not a char boundary; it is inside 'é' (bytes 11..13) of
  "&eedle é"`; a failure found later is fixed in its module, with the input as a unit test.

## Chunk 13: perf ceilings, benchmark and binary size

### Task 13.1: the ceilings of tests/perf.rs

**Files:** `tests/perf.rs`.

- [ ] **Step 1:** the ten tests (spec §11 #26, D17, D23): among them the PDF bomb (a ToUnicode
  map of 300 MiB), the zip of six 200 MiB sheets (review r1, Fable #1 and #3), a page of 63 MiB
  of `h`, an object stream of 63 MiB of `1 `, a ToUnicode range of 2³² codes and a 4 MiB string
  of a code that means 256 letters (review r2, Fable #1, Codex #1–#3). Run with `SFOR_BIN` set
  to the round-1 binary under `ulimit -v 8000000`, the page aborted out of memory and the
  object stream took 7.8 s and 3 850 MiB; the range and the long codes passed there, for a
  reason the probe did not look into (lopdf's reader may not have taken those maps); their RED
  is the unit tests of Task 9.1:

```diff
diff --git a/tests/perf.rs b/tests/perf.rs
--- a/tests/perf.rs
+++ b/tests/perf.rs
@@ -1239,3 +1239,521 @@ fn the_rows_of_one_wide_line_cost_the_line_once() {
         );
     }
 }
+
+// M4 (plan, Task 13.1): object files and documents.
+
+use searchfor::docs::fixture as doc;
+use searchfor::processors::object::fixture as obj;
+
+/// An ELF whose `.rodata` is `SIZE` bytes of NUL-separated strings, with `needle` in the first,
+/// the middle and the last one, and `symbols` symbols spread over it.
+fn big_elf(fx: &Fixture, symbols: usize) -> &'static str {
+    let mut ro = Vec::with_capacity(SIZE + 64);
+    let mut n = 0usize;
+    let mut middle = false;
+    while ro.len() < SIZE {
+        let word: &[u8] = if n == 0 || (!middle && ro.len() >= SIZE / 2) {
+            middle = n > 0;
+            b"version needle!\0"
+        } else {
+            b"some string of hay\0"
+        };
+        ro.extend_from_slice(word);
+        n += 1;
+    }
+    ro.extend_from_slice(b"last needle!\0");
+    let mut spec = obj::Spec::new()
+        .section(".text", b"\xc3\0\0\0")
+        .section(".rodata", &ro);
+    let step = (ro.len() / symbols.max(1)).max(1);
+    for i in 0..symbols {
+        spec = spec.symbol(&format!("sym_{i}"), 1, (i * step) as u64);
+    }
+    std::fs::write(fx.file("big.elf"), obj::elf(&spec).file).unwrap();
+    "big.elf"
+}
+
+fn object_ceiling(symbols: usize) {
+    let fx = Fixture::new();
+    let file = big_elf(&fx, symbols);
+    let args = ["--flat", "needle", file];
+    let (out, err) = sfor_output(&fx, &args);
+    assert_eq!(err, "");
+    assert!(out.lines().count() >= 3, "{out:.400}");
+    assert!(
+        out.lines()
+            .all(|l| l.starts_with("0x") && l.contains("ELF x86-64 › .rodata")),
+        "{out:.400}"
+    );
+    let Some(rg) = rg_bin() else { return };
+    let Some((t, rss)) = best_of_three_drained(&sfor_bin(), &fx, &args, 0) else {
+        return;
+    };
+    // rg -a searches the whole file, as sfor does for an object file with a Match.
+    let (base, base_rss) = best_of_three_drained(&rg, &fx, &["-a", "needle", file], 0).unwrap();
+    eprintln!(
+        "50 MB ELF, {symbols} symbols: sfor {t:?}, RSS {} MiB; rg -a {base:?}, RSS {} MiB",
+        rss / 1_048_576,
+        base_rss / 1_048_576
+    );
+    assert!(
+        t <= base + Duration::from_secs(1),
+        "{t:?} is more than rg -a's {base:?} + 1 s"
+    );
+    assert!(
+        rss <= base_rss + 64 * 1_048_576,
+        "RSS {rss} is more than rg -a's {base_rss} + 64 MiB"
+    );
+}
+
+#[test]
+#[ignore]
+fn object_rows_of_a_big_elf_stay_under_rg_a() {
+    let _alone = alone();
+    object_ceiling(0);
+}
+
+#[test]
+#[ignore]
+fn an_elf_with_two_hundred_thousand_symbols_stays_under_rg_a() {
+    let _alone = alone();
+    object_ceiling(200_000);
+}
+
+/// A document of each kind with `needle` in its first, middle and last unit; the Extractor's
+/// text of it is written beside it as `NAME.txt`, so rg has the same text to search.
+fn big_document(fx: &Fixture, kind: &str, units: usize) -> (String, usize) {
+    let text = |i: usize| {
+        let n = if i == 0 || i == units / 2 || i == units - 1 {
+            "needle"
+        } else {
+            "hay"
+        };
+        format!("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do {n} {i}.")
+    };
+    let bytes = match kind {
+        "docx" | "odt" => {
+            let mut blocks = Vec::new();
+            for i in 0..units {
+                if i % 1000 == 0 {
+                    blocks.push(doc::h(1, &format!("Chapter {i}")));
+                }
+                blocks.push(doc::p(&text(i)));
+            }
+            if kind == "docx" {
+                doc::docx(&blocks)
+            } else {
+                doc::odt(&blocks)
+            }
+        }
+        "xlsx" | "ods" => {
+            let rows: Vec<[String; 3]> = (0..units)
+                .map(|i| [format!("item {i}"), format!("{}", i * 3), text(i)])
+                .collect();
+            let refs: Vec<Vec<&str>> = rows
+                .iter()
+                .map(|r| r.iter().map(String::as_str).collect())
+                .collect();
+            let rows: Vec<&[&str]> = refs.iter().map(Vec::as_slice).collect();
+            if kind == "xlsx" {
+                doc::xlsx(&[("Sheet", &rows)])
+            } else {
+                doc::ods(&[("Sheet", &rows)])
+            }
+        }
+        "pptx" => {
+            let slides: Vec<[String; 2]> = (0..units)
+                .map(|i| [format!("Slide {i}"), text(i)])
+                .collect();
+            let refs: Vec<Vec<&str>> = slides
+                .iter()
+                .map(|s| s.iter().map(String::as_str).collect())
+                .collect();
+            let slides: Vec<(&[&str], Option<&str>)> =
+                refs.iter().map(|s| (s.as_slice(), None)).collect();
+            doc::pptx(&slides)
+        }
+        "epub" => {
+            let chapters: Vec<(String, String)> = (0..units / 1000)
+                .map(|c| {
+                    (
+                        format!("Chapter {c}"),
+                        (c * 1000..(c + 1) * 1000)
+                            .map(|i| format!("<p>{}</p>", text(i)))
+                            .collect(),
+                    )
+                })
+                .collect();
+            let refs: Vec<(&str, &str)> = chapters
+                .iter()
+                .map(|(a, b)| (a.as_str(), b.as_str()))
+                .collect();
+            doc::epub(&refs, true)
+        }
+        "pdf" => {
+            let lines: Vec<String> = (0..units).map(text).collect();
+            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
+            let pages: Vec<&[&str]> = refs.chunks(50).collect();
+            doc::pdf(&pages, &[(1, "Start", 1)])
+        }
+        k => panic!("{k}"),
+    };
+    let name = format!("big.{kind}");
+    std::fs::write(fx.file(&name), &bytes).unwrap();
+    let kind = searchfor::docs::Kind::from_name(kind).unwrap();
+    let e = searchfor::docs::extract(kind, &bytes).unwrap();
+    std::fs::write(fx.file(&format!("{name}.txt")), &e.text).unwrap();
+    (name, e.text.len())
+}
+
+/// D23: an Extractor reads its document at `rate` MB/s of text or faster, on top of rg's search
+/// of that text and 1 s; its memory stays under rg's plus six times the text, plus 400 bytes
+/// per cell Segment (`cells`: a spreadsheet's), plus 64 MiB.
+fn document_ceiling(kind: &str, units: usize, rate: f64, cells: usize) {
+    let fx = Fixture::new();
+    let (name, text) = big_document(&fx, kind, units);
+    let args = ["--docs", "--flat", "needle", name.as_str()];
+    let (out, err) = sfor_output(&fx, &args);
+    assert_eq!(err, "", "{kind}");
+    assert_eq!(out.lines().count(), 3, "{kind}: {out:.400}");
+    let Some(rg) = rg_bin() else { return };
+    let Some((t, rss)) = best_of_three_drained(&sfor_bin(), &fx, &args, 0) else {
+        return;
+    };
+    let txt = format!("{name}.txt");
+    let (base, base_rss) = best_of_three_drained(&rg, &fx, &["needle", txt.as_str()], 0).unwrap();
+    let budget =
+        base + Duration::from_secs_f64(text as f64 / (rate * 1e6)) + Duration::from_secs(1);
+    eprintln!(
+        "{kind}: {} MB of text: sfor {t:?} (budget {budget:?}), RSS {} MiB; rg on the text {base:?}, RSS {} MiB",
+        text / 1_000_000,
+        rss / 1_048_576,
+        base_rss / 1_048_576
+    );
+    assert!(t <= budget, "{kind}: {t:?} is over {budget:?}");
+    let bound = base_rss + 6 * text as u64 + 400 * cells as u64 + 64 * 1_048_576;
+    assert!(rss <= bound, "{kind}: RSS {rss} is over {bound}");
+}
+
+#[test]
+#[ignore]
+fn documents_extract_within_their_ceilings() {
+    let _alone = alone();
+    // The rates are about a third of what the M4 probe measured (M4 plan, D23).
+    for (kind, units, rate, cells) in [
+        ("docx", 200_000, 12.0, 0),
+        ("odt", 200_000, 10.0, 0),
+        ("epub", 200_000, 20.0, 0),
+        ("pdf", 100_000, 7.0, 0),
+        ("xlsx", 200_000, 2.0, 600_000),
+        ("ods", 200_000, 1.0, 600_000),
+        ("pptx", 20_000, 1.3, 0),
+    ] {
+        document_ceiling(kind, units, rate, cells);
+    }
+}
+
+/// D17: a document whose part decompresses past 256 MiB is refused with a warning, quickly and
+/// within 256 MiB plus rg's memory and 64 MiB, and is then searched as rg searches it.
+#[test]
+#[ignore]
+fn a_zip_bomb_is_refused_within_its_limit() {
+    let _alone = alone();
+    let fx = Fixture::new();
+    let mut xml = String::from("<w:document><w:body><w:p><w:r><w:t>");
+    xml.push_str(&" ".repeat(300 << 20));
+    xml.push_str("needle</w:t></w:r></w:p></w:body></w:document>");
+    let bomb = doc::zip(&[("word/document.xml", xml.into_bytes())], &[]);
+    std::fs::write(fx.file("bomb.docx"), &bomb).unwrap();
+    let Some(rg) = rg_bin() else { return };
+    let args = ["--docs", "--flat", "needle", "bomb.docx"];
+    let (t, rss, _) = measure_to(&sfor_bin(), &fx, &args, None, Stdio::null(), 1).unwrap();
+    let (_, base_rss) = measure(&rg, &fx, &["needle", "bomb.docx"], 1).unwrap();
+    eprintln!(
+        "zip bomb of {} KiB: {t:?}, RSS {} MiB",
+        bomb.len() >> 10,
+        rss / 1_048_576
+    );
+    assert!(t <= Duration::from_secs(5), "{t:?}");
+    assert!(rss <= base_rss + (256 + 64) * 1_048_576, "RSS {rss}");
+    let o = common::run(
+        Command::new(sfor_bin())
+            .args(args)
+            .current_dir(fx.path())
+            .env_remove("RIPGREP_CONFIG_PATH")
+            .stdin(Stdio::null()),
+    );
+    assert!(
+        common::stderr(&o).contains("bomb.docx: docx: word/document.xml is over the 256 MiB limit"),
+        "{}",
+        common::stderr(&o)
+    );
+}
+
+/// A one-page PDF whose only font, without `/Encoding`, has a ToUnicode map that decompresses
+/// to `inflated` bytes of spaces.
+fn pdf_font_bomb(inflated: usize) -> Vec<u8> {
+    let content = b"BT /F1 12 Tf 72 760 Td (needle here) Tj ET".to_vec();
+    one_page_pdf(content, false, Some(vec![b' '; inflated]))
+}
+
+/// A one-page PDF drawing `content` (compressed when `compress`) with a Type 1 font, without
+/// `/Encoding`, whose ToUnicode map (compressed) is `map`.
+fn one_page_pdf(content: Vec<u8>, compress: bool, map: Option<Vec<u8>>) -> Vec<u8> {
+    use lopdf::{Dictionary, Document, Object, Stream};
+    let mut doc = Document::with_version("1.5");
+    let mut font = Dictionary::new();
+    font.set("Type", Object::Name(b"Font".to_vec()));
+    font.set("Subtype", Object::Name(b"Type1".to_vec()));
+    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
+    if let Some(map) = map {
+        let mut map = Stream::new(Dictionary::new(), map);
+        map.compress().unwrap();
+        font.set("ToUnicode", Object::Reference(doc.add_object(map)));
+    }
+    let font_id = doc.add_object(font);
+    let mut content = Stream::new(Dictionary::new(), content);
+    if compress {
+        content.compress().unwrap();
+    }
+    let content_id = doc.add_object(content);
+    let pages_id = doc.new_object_id();
+    let mut fonts = Dictionary::new();
+    fonts.set("F1", Object::Reference(font_id));
+    let mut res = Dictionary::new();
+    res.set("Font", Object::Dictionary(fonts));
+    let mut page = Dictionary::new();
+    page.set("Type", Object::Name(b"Page".to_vec()));
+    page.set("Parent", Object::Reference(pages_id));
+    page.set("Contents", Object::Reference(content_id));
+    page.set("Resources", Object::Dictionary(res));
+    let page_id = doc.add_object(page);
+    let mut pages = Dictionary::new();
+    pages.set("Type", Object::Name(b"Pages".to_vec()));
+    pages.set("Count", Object::Integer(1));
+    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
+    let media = vec![0.into(), 0.into(), 612.into(), 792.into()];
+    pages.set("MediaBox", Object::Array(media));
+    doc.objects.insert(pages_id, Object::Dictionary(pages));
+    let mut cat = Dictionary::new();
+    cat.set("Type", Object::Name(b"Catalog".to_vec()));
+    cat.set("Pages", Object::Reference(pages_id));
+    let cat_id = doc.add_object(cat);
+    doc.trailer.set("Root", Object::Reference(cat_id));
+    let mut out = Vec::new();
+    doc.save_to(&mut out).unwrap();
+    out
+}
+
+/// A PDF 1.5 whose one object stream decompresses to `inflated` bytes of `1 `, an array of
+/// small integers, referred to by a cross-reference stream (M4 plan r2, Fable #1: lopdf parses
+/// object streams whole while it loads the file).
+fn pdf_objstm_bomb(inflated: usize) -> Vec<u8> {
+    let content = b"BT /F1 12 Tf 72 700 Td (needle) Tj ET";
+    let mut big = b"7 0\n[".to_vec();
+    big.extend(b"1 ".repeat(inflated / 2));
+    big.push(b']');
+    let mut packed = lopdf::Stream::new(lopdf::Dictionary::new(), big);
+    packed.compress().unwrap();
+    let stream =
+        |dict: String, data: &[u8]| [dict.as_bytes(), b"\nstream\n", data, b"\nendstream"].concat();
+    let page =
+        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>";
+    let objs: Vec<Vec<u8>> = vec![
+        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
+        b"<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 612 792] >>".to_vec(),
+        page.as_bytes().to_vec(),
+        stream(format!("<< /Length {} >>", content.len()), content),
+        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
+        stream(
+            format!(
+                "<< /Type /ObjStm /N 1 /First 4 /Length {} /Filter /FlateDecode >>",
+                packed.content.len()
+            ),
+            &packed.content,
+        ),
+    ];
+    let mut body = b"%PDF-1.5\n".to_vec();
+    let mut offsets = Vec::new();
+    for (i, o) in objs.iter().enumerate() {
+        offsets.push(body.len());
+        body.extend(format!("{} 0 obj\n", i + 1).as_bytes());
+        body.extend(o);
+        body.extend(b"\nendobj\n");
+    }
+    let xref_at = body.len();
+    let row =
+        |t: u8, a: usize, b: u16| [&[t][..], &(a as u32).to_be_bytes(), &b.to_be_bytes()].concat();
+    let mut xref = row(0, 0, 65535);
+    for o in &offsets {
+        xref.extend(row(1, *o, 0));
+    }
+    // Object 7 is the first of object stream 6; object 8 is the cross-reference stream.
+    xref.extend(row(2, 6, 0));
+    xref.extend(row(1, xref_at, 0));
+    let dict = format!(
+        "8 0 obj\n<< /Type /XRef /Size 9 /W [1 4 2] /Root 1 0 R /Length {} >>",
+        xref.len()
+    );
+    body.extend(stream(dict, &xref));
+    body.extend(format!("\nendobj\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());
+    body
+}
+
+/// Runs `sfor --docs --flat needle <name>` on the bomb `bytes`: it ends within 5 s and
+/// `extra_mib` plus rg's memory and 64 MiB, warning `warning`, and exits with `code`, as rg
+/// searching the file as it is.
+fn refuses_the_bomb(name: &str, bytes: &[u8], extra_mib: u64, warning: &str, code: i32) {
+    let fx = Fixture::new();
+    std::fs::write(fx.file(name), bytes).unwrap();
+    let Some(rg) = rg_bin() else { return };
+    let args = ["--docs", "--flat", "needle", name];
+    let (t, rss, _) = measure_to(&sfor_bin(), &fx, &args, None, Stdio::null(), code).unwrap();
+    let (_, base_rss) = measure(&rg, &fx, &["needle", name], code).unwrap();
+    eprintln!(
+        "{name} of {} KiB: {t:?}, RSS {} MiB",
+        bytes.len() >> 10,
+        rss / 1_048_576
+    );
+    assert!(t <= Duration::from_secs(5), "{t:?}");
+    assert!(rss <= base_rss + (extra_mib + 64) * 1_048_576, "RSS {rss}");
+    let o = common::run(
+        Command::new(sfor_bin())
+            .args(args)
+            .current_dir(fx.path())
+            .env_remove("RIPGREP_CONFIG_PATH")
+            .stdin(Stdio::null()),
+    );
+    assert!(
+        common::stderr(&o).contains(warning),
+        "{}",
+        common::stderr(&o)
+    );
+    if warning.is_empty() {
+        assert_eq!(common::stderr(&o), "");
+    }
+}
+
+/// D17 (Fable r1 #1): a PDF whose font's ToUnicode map decompresses past 64 MiB is refused with
+/// a warning, quickly and within 64 MiB plus rg's memory and 64 MiB, and is then searched as
+/// rg searches it.
+#[test]
+#[ignore]
+fn a_pdf_bomb_is_refused_within_its_limit() {
+    let _alone = alone();
+    refuses_the_bomb(
+        "bomb.pdf",
+        &pdf_font_bomb(300 << 20),
+        64,
+        "bomb.pdf: pdf: a font's ToUnicode map is over the 64 MiB limit",
+        // The page's content stream is not compressed: rg finds the needle in it.
+        0,
+    );
+}
+
+/// D17 (Fable r1 #3): a workbook of sheets under 256 MiB each and over 1 GiB together is
+/// refused with a warning, quickly and within 256 MiB plus rg's memory and 64 MiB, and is then
+/// searched as rg searches it.
+#[test]
+#[ignore]
+fn a_zip_bomb_of_many_parts_is_refused_within_its_limit() {
+    let _alone = alone();
+    let names = ["S1", "S2", "S3", "S4", "S5", "S6"];
+    let row: &[&str] = &["needle"];
+    let sheets: Vec<(&str, &[&[&str]])> = names
+        .iter()
+        .map(|n| (*n, std::slice::from_ref(&row)))
+        .collect();
+    let pad = " ".repeat(200 << 20);
+    let bomb = doc::rezip(&doc::xlsx(&sheets), |name, part| {
+        if !name.starts_with("xl/worksheets/") {
+            return part;
+        }
+        String::from_utf8(part)
+            .unwrap()
+            .replace("</sheetData>", &format!("{pad}</sheetData>"))
+            .into_bytes()
+    });
+    refuses_the_bomb(
+        "bomb.xlsx",
+        &bomb,
+        256,
+        "bomb.xlsx: xlsx: the whole document, decompressed, is over the 1024 MiB limit",
+        1,
+    );
+}
+
+/// D17 (M4 plan r2, Fable #1, Codex #3): a page of 63 MiB of `h`, 33 million operations, is
+/// refused by the work budget as it is read, without becoming a list of operations: lopdf's
+/// took 6 GB for one.
+#[test]
+#[ignore]
+fn a_pdf_page_of_millions_of_operations_is_refused_within_its_limit() {
+    let _alone = alone();
+    refuses_the_bomb(
+        "ops.pdf",
+        &one_page_pdf(b"h\n".repeat(63 << 19), true, None),
+        64,
+        "ops.pdf: pdf: its content streams run more than 8020000 operations",
+        1,
+    );
+}
+
+/// D17 (M4 plan r2, Fable #1): an object stream of 63 MiB of small integers is refused before
+/// lopdf parses it: parsed, 32 MiB took 2 GB.
+#[test]
+#[ignore]
+fn a_pdf_object_stream_bomb_is_refused_within_its_limit() {
+    let _alone = alone();
+    refuses_the_bomb(
+        "objstm.pdf",
+        &pdf_objstm_bomb(63 << 20),
+        64,
+        "objstm.pdf: pdf: its object streams hold more than 2000000 tokens",
+        // The page's content stream is not compressed: rg finds the needle in it.
+        0,
+    );
+}
+
+/// D17 (M4 plan r2, Codex #1): a ToUnicode range of 2³² codes is read as one range: lopdf's
+/// reader listed every code.
+#[test]
+#[ignore]
+fn a_pdf_map_range_of_every_code_is_read_within_its_limit() {
+    let _alone = alone();
+    let map = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
+        1 begincodespacerange <00> <FF> endcodespacerange\n2 beginbfrange\n\
+        <00> <FF> <0000>\n<00000000> <FFFFFFFF> <0041>\nendbfrange\nendcmap end end\n";
+    let content = b"BT /F1 12 Tf 72 700 Td (needle) Tj ET".to_vec();
+    refuses_the_bomb(
+        "range.pdf",
+        &one_page_pdf(content, true, Some(map.to_vec())),
+        64,
+        "",
+        0,
+    );
+}
+
+/// D17 (M4 plan r2, Codex #2): a ToUnicode map that gives a code 256 letters, and a string of
+/// 4 MiB of it, is decoded at most `MAX_GROWTH` bytes for each byte of the string: it made 512 MiB
+/// of text, 9 s and 1.7 GB, from a 5 KB file.
+#[test]
+#[ignore]
+fn a_pdf_map_of_long_text_is_read_within_its_limit() {
+    let _alone = alone();
+    let mut map = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
+        1 begincodespacerange <00> <FF> endcodespacerange\n1 beginbfchar\n<41> <"
+        .to_vec();
+    map.extend(b"006E".repeat(256));
+    map.extend(b">\nendbfchar\nendcmap end end\n");
+    let mut content = b"BT /F1 12 Tf 72 700 Td (".to_vec();
+    content.extend(vec![b'A'; 4 << 20]);
+    content.extend(b") Tj ET");
+    refuses_the_bomb(
+        "long.pdf",
+        &one_page_pdf(content, true, Some(map)),
+        64,
+        "",
+        1,
+    );
+}
```

- [ ] **Step 2:** `SFOR_REQUIRE_REFERENCE=1 cargo test --release --test perf -- --ignored
  --nocapture --test-threads 1 object_ elf_ documents_ zip_bomb pdf_`; paste the measured lines into
  the chunk report. A RED run of each ceiling against a build that breaks what it bounds is
  to verify in chunk 13: the probe measured only the GREEN side (for instance, the zip bomb test
  against a `zipped::Zip::bytes` without its `take(MAX_PART + 1)`, which must exceed the RSS
  bound).

### Task 13.2: the benchmark

**Files:** `scripts/gen-m4-corpus.py`, `scripts/bench-m4.sh`.

- [ ] **Step 1:** the corpus generator (Python's standard library) and the benchmark:

```python
#!/usr/bin/env python3
"""The corpus of scripts/bench-m4.sh (M4 plan, D23), written with Python's standard library
only: documents of each --docs Format, object files copied from the system, and 20 000 small
text files. Every 40th document and object file holds SPARSE. `texts/` holds the text of each
document as the Extractors write it (a line per paragraph or row), for rg to search.
Usage: scripts/gen-m4-corpus.py DIR SPARSE"""
import os, shutil, sqlite3, sys, zipfile, zlib

out, sparse = sys.argv[1], sys.argv[2]
N = 120  # documents of each kind
PARAS = 200
for d in ["docs", "texts", "bin", "text"]:
    os.makedirs(os.path.join(out, d), exist_ok=True)

def para(i, k, word):
    w = word if k == PARAS // 2 else "hay"
    return f"Lorem ipsum dolor sit amet {i}, consectetur adipiscing elit, sed do {w} {k}."

def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

def zipped(path, parts):
    with zipfile.ZipFile(path, "w") as z:
        for name, data in parts:
            method = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
            z.writestr(zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0)), data, compress_type=method)

CT = '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>'
def rels(items):
    body = "".join(f'<Relationship Id="{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{t}" Target="{g}"/>' for i, t, g in items)
    return f'<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>'

def docx(path, lines):
    body = "".join(f"<w:p><w:r><w:t>{esc(l)}</w:t></w:r></w:p>" for l in lines)
    doc = f'<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>'
    zipped(path, [("[Content_Types].xml", CT), ("_rels/.rels", rels([("rId1", "officeDocument", "word/document.xml")])), ("word/document.xml", doc)])

def xlsx(path, lines):
    rows = "".join(f'<row r="{r+1}"><c r="A{r+1}" t="inlineStr"><is><t>{esc(l)}</t></is></c></row>' for r, l in enumerate(lines))
    sheet = f'<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>'
    book = '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>'
    zipped(path, [("[Content_Types].xml", CT), ("_rels/.rels", rels([("rId1", "officeDocument", "xl/workbook.xml")])), ("xl/workbook.xml", book), ("xl/_rels/workbook.xml.rels", rels([("rId1", "worksheet", "worksheets/sheet1.xml")])), ("xl/worksheets/sheet1.xml", sheet)])

def pptx(path, lines):
    ns = 'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
    parts, ids, rl = [], "", []
    for s in range(0, len(lines), 20):
        n = s // 20 + 1
        ps = "".join(f"<a:p><a:r><a:t>{esc(l)}</a:t></a:r></a:p>" for l in lines[s:s + 20])
        parts.append((f"ppt/slides/slide{n}.xml", f'<?xml version="1.0"?><p:sld {ns}><p:cSld><p:spTree><p:sp><p:txBody>{ps}</p:txBody></p:sp></p:spTree></p:cSld></p:sld>'))
        ids += f'<p:sldId id="{255 + n}" r:id="rId{n}"/>'
        rl.append((f"rId{n}", "slide", f"slides/slide{n}.xml"))
    pres = f'<?xml version="1.0"?><p:presentation {ns}><p:sldIdLst>{ids}</p:sldIdLst></p:presentation>'
    zipped(path, [("[Content_Types].xml", CT), ("_rels/.rels", rels([("rId1", "officeDocument", "ppt/presentation.xml")])), ("ppt/presentation.xml", pres), ("ppt/_rels/presentation.xml.rels", rels(rl))] + parts)

ODF = 'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"'
def odf(path, mime, body):
    zipped(path, [("mimetype", mime), ("content.xml", f'<?xml version="1.0"?><office:document-content {ODF}><office:body>{body}</office:body></office:document-content>')])

def odt(path, lines):
    odf(path, "application/vnd.oasis.opendocument.text", "<office:text>" + "".join(f"<text:p>{esc(l)}</text:p>" for l in lines) + "</office:text>")

def ods(path, lines):
    rows = "".join(f'<table:table-row><table:table-cell office:value-type="string"><text:p>{esc(l)}</text:p></table:table-cell></table:table-row>' for l in lines)
    odf(path, "application/vnd.oasis.opendocument.spreadsheet", f'<office:spreadsheet><table:table table:name="S">{rows}</table:table></office:spreadsheet>')

def odp(path, lines):
    pages = "".join('<draw:page><draw:frame><draw:text-box>' + "".join(f"<text:p>{esc(l)}</text:p>" for l in lines[s:s + 20]) + '</draw:text-box></draw:frame></draw:page>' for s in range(0, len(lines), 20))
    odf(path, "application/vnd.oasis.opendocument.presentation", f"<office:presentation>{pages}</office:presentation>")

def epub(path, lines):
    body = "".join(f"<p>{esc(l)}</p>" for l in lines)
    x = f'<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>'
    opf = '<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>'
    cont = '<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>'
    zipped(path, [("mimetype", "application/epub+zip"), ("META-INF/container.xml", cont), ("OEBPS/content.opf", opf), ("OEBPS/c.xhtml", x)])

def pdf(path, lines):
    objs = ["<< /Type /Catalog /Pages 2 0 R >>", None, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"]
    kids = []
    for s in range(0, len(lines), 50):
        ops = "BT /F1 10 Tf 72 760 Td " + " ".join(f"({l}) Tj 0 -14 Td" for l in lines[s:s + 50]) + " ET"
        data = zlib.compress(ops.encode("latin-1"))
        objs.append(f"<< /Length {len(data)} /Filter /FlateDecode >>stream\n".encode() + data + b"\nendstream")
        objs.append(f"<< /Type /Page /Parent 2 0 R /Contents {len(objs)} 0 R >>")
        kids.append(len(objs))
    objs[1] = f"<< /Type /Pages /Kids [{' '.join(f'{k} 0 R' for k in kids)}] /Count {len(kids)} /Resources << /Font << /F1 3 0 R >> >> /MediaBox [0 0 612 792] >>"
    b, offs = bytearray(b"%PDF-1.5\n"), []
    for i, o in enumerate(objs, 1):
        offs.append(len(b))
        b += f"{i} 0 obj\n".encode() + (o if isinstance(o, bytes) else o.encode()) + b"\nendobj\n"
    x = len(b)
    b += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode() + "".join(f"{o:010d} 00000 n \n" for o in offs).encode()
    b += f"trailer << /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n".encode()
    open(path, "wb").write(b)

def sqlite(path, lines):
    if os.path.exists(path):
        os.remove(path)
    db = sqlite3.connect(path)
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, body TEXT)")
    db.executemany("INSERT INTO t (body) VALUES (?)", [(l,) for l in lines])
    db.commit()
    db.close()

kinds = [("docx", docx), ("xlsx", xlsx), ("pptx", pptx), ("odt", odt), ("ods", ods), ("odp", odp), ("epub", epub), ("pdf", pdf), ("sqlite", sqlite)]
for ext, write in kinds:
    for i in range(N):
        word = sparse if i % 40 == 0 else "hay"
        lines = [para(i, k, word) for k in range(PARAS)]
        write(os.path.join(out, "docs", f"d{i:03d}.{ext}"), lines)
        with open(os.path.join(out, "texts", f"d{i:03d}.{ext}.txt"), "w") as f:
            f.write("".join(l + "\n" for l in lines))

# Object files: the system's ELF executables under 4 MB, every 40th with SPARSE appended
# after its sections (the file stays a valid ELF).
n = 0
for d in ["/usr/bin", "/usr/sbin", "/usr/lib/x86_64-linux-gnu"]:
    if not os.path.isdir(d):
        continue
    for name in sorted(os.listdir(d)):
        p = os.path.join(d, name)
        try:
            if not os.path.isfile(p) or os.path.islink(p) or os.path.getsize(p) > 4 << 20:
                continue
            with open(p, "rb") as f:
                if f.read(4) != b"\x7fELF":
                    continue
        except OSError:
            continue
        dst = os.path.join(out, "bin", f"o{n:04d}")
        shutil.copyfile(p, dst)
        if n % 40 == 0:
            with open(dst, "ab") as f:
                f.write(b"\0" + sparse.encode() + b" marker\0")
        n += 1
        if n >= 300:
            break
    if n >= 300:
        break

for i in range(20000):
    with open(os.path.join(out, "text", f"t{i:04d}.txt"), "w") as f:
        f.write("".join(f"plain line {k} of file {i}\n" for k in range(40)))
```

```bash
#!/usr/bin/env bash
# M4 acceptance (spec §11 #13, M4 plan D23), on a corpus that scripts/gen-m4-corpus.py writes:
# 1 080 documents of the nine --docs Formats, 300 object files copied from the system, and
# 20 000 text files (enough files that sfor's start, about a millisecond more than rg's, does
# not decide a ratio: M3 plan, «Verified»), against ripgrep 14.1.1:
#   - without --docs and --binary, a pattern no file matches, a sparse one and --no-trail:
#     sfor ≤ 1.1 × rg (a document or a walked object costs what it costs rg: 8385);
#   - --docs over the text files and the object files, a pattern no file matches: sfor ≤ 1.1 ×
#     rg (no document, no cost; the object files have no extension, so --docs reads their first
#     bytes: M4 plan r1, Fable #5);
#   - --binary with the sparse pattern over the object files: sfor − rg ≤ the largest matching
#     object ÷ 100 MB/s + the matching objects ÷ (100 MB/s × threads) + 0.05 s;
#   - --docs with the sparse pattern over the documents: sfor ≤ rg over their texts (texts/)
#     + the texts' bytes ÷ (2 MB/s × threads) + 0.1 s, and sfor finds the lines rg finds.
# Usage: scripts/bench-m4.sh [CORPUS_DIR]  (generated there when missing; a temporary one by
# default). Needs python3, hyperfine, jq, awk and GNU stat. SFOR_BIN replaces the sfor under
# test; SFOR_RG, else /usr/bin/rg, is the reference and must be ripgrep 14.1.1.
set -euo pipefail
export LC_ALL=C
cargo build --release --quiet
sfor="${SFOR_BIN:-./target/release/sfor}"
export SFOR_BIN_PATH="$sfor"
rg="${SFOR_RG:-/usr/bin/rg}"
if ! "$rg" --version 2>/dev/null | head -1 | grep -qx 'ripgrep 14.1.1'; then
  echo "FAIL: the reference $rg is not ripgrep 14.1.1"; exit 1
fi
export SFOR_RG_PATH="$rg"
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SFOR_FORMATS_DIR="$iso/formats" SFOR_GRAMMARS_DIR="$iso/grammars" SFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH
threads=$(nproc); [ "$threads" -gt 12 ] && threads=12
dir="${1:-$iso/corpus}"
# Split in two, so no file holds them, this script included (M3 code review r1).
sparse='zqm4''sparse'; none='zzqq-sfor-m4-''absent'
fail=0
[ -d "$dir" ] || python3 "$(dirname "$0")/gen-m4-corpus.py" "$dir" "$sparse"
export SFOR_BENCH_DIR="$dir" SFOR_BENCH_PATTERN="$sparse" SFOR_BENCH_NONE="$none"

median() {
  # 40 runs: a ratio of two 45 ms medians of 15 runs moved by ±3 % between runs (M4 plan r1).
  hyperfine --ignore-failure --warmup 5 --runs 40 --export-json "$iso/$1.json" "$2" "$3" >/dev/null
  if ! jq -e '[.results[].exit_codes[]] | all(. == 0 or . == 1)' "$iso/$1.json" >/dev/null; then
    echo "  FAIL $1: a timed run did not exit 0 or 1"; fail=1
  fi
  RG_S=$(jq '.results[0].median' "$iso/$1.json")
  SFOR_S=$(jq '.results[1].median' "$iso/$1.json")
  awk -v a="$SFOR_S" -v b="$RG_S" -v n="$1" 'BEGIN { printf "%-14s rg %.3f s  sfor %.3f s  ratio %.2f\n", n, b, a, a / b }'
}
within() { awk -v a="$SFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }; }

# lines NAME FILE_SFOR FILE_RG: sfor printed at least the lines rg printed.
lines() {
  local a b; a=$(wc -l <"$2"); b=$(wc -l <"$3")
  [ "$a" -ge "$b" ] || { echo "  FAIL $1: sfor printed $a lines, rg $b"; fail=1; }
}

docs=$(find "$dir/docs" -type f | wc -l); objs=$(find "$dir/bin" -type f | wc -l)
echo "== $dir: $docs documents, $objs object files, threads $threads"
[ "$objs" -gt 0 ] || { echo "FAIL: no object file was copied"; exit 1; }

# 1. Without --docs and --binary: rg's cost.
for q in "$none" "$sparse"; do
  rc=0; "$sfor" --color never --no-trail -e "$q" -- "$dir" >"$iso/s.out" 2>"$iso/s.err" || rc=$?
  rrc=0; "$rg" --color never -e "$q" -- "$dir" >"$iso/r.out" 2>"$iso/r.err" || rrc=$?
  [ "$rc" = "$rrc" ] && cmp -s <(sort "$iso/s.out") <(sort "$iso/r.out") && [ ! -s "$iso/s.err" ] \
    || { echo "  FAIL --no-trail $q: not rg's output"; fail=1; }
done
median none '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"'; within
median sparse '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'; within
median no-trail '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --no-trail -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'; within

# 2. --docs where there is no document: text files, and object files without an extension.
median docs-none '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR/text" "$SFOR_BENCH_DIR/bin"' \
  '"$SFOR_BIN_PATH" --color never --flat --docs -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR/text" "$SFOR_BENCH_DIR/bin"'; within

# 3. --binary over the object files.
"$sfor" --color never --flat --binary -e "$sparse" -- "$dir/bin" >"$iso/s.out" 2>"$iso/s.err" || true
"$rg" --color never --binary -l -e "$sparse" -- "$dir/bin" >"$iso/r.out" || true
[ ! -s "$iso/s.err" ] || { echo "  FAIL --binary: stderr: $(head -3 "$iso/s.err")"; fail=1; }
lines binary "$iso/s.out" "$iso/r.out"
sizes=$(tr '\n' '\0' <"$iso/r.out" | xargs -0 -r stat -c %s)
bytes=$(echo "$sizes" | awk '{ s += $1 } END { print s + 0 }')
largest=$(echo "$sizes" | awk '$1 > m { m = $1 } END { print m + 0 }')
median binary '"$SFOR_RG_PATH" --color never --binary -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR/bin"' \
  '"$SFOR_BIN_PATH" --color never --flat --binary -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR/bin"'
awk -v a="$SFOR_S" -v b="$RG_S" -v by="$bytes" -v big="$largest" -v t="$threads" \
  'BEGIN { budget = big / 100e6 + by / (100e6 * t) + 0.05; printf "  extra %.3f s, budget %.3f s\n", a - b, budget; exit !(a - b <= budget) }' \
  || { echo "  FAIL: over the budget"; fail=1; }

# 4. --docs over the documents, against rg over their texts.
"$sfor" --color never --no-trail --docs -e "$sparse" -- "$dir/docs" >"$iso/s.out" 2>"$iso/s.err" || true
"$rg" --color never -e "$sparse" -- "$dir/texts" >"$iso/r.out" || true
[ ! -s "$iso/s.err" ] || { echo "  FAIL --docs: stderr: $(head -3 "$iso/s.err")"; fail=1; }
lines docs "$iso/s.out" "$iso/r.out"
text=$(find "$dir/texts" -type f -printf '%s\n' | awk '{ s += $1 } END { print s + 0 }')
median docs '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR/texts"' \
  '"$SFOR_BIN_PATH" --color never --flat --docs -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR/docs"'
awk -v a="$SFOR_S" -v b="$RG_S" -v by="$text" -v t="$threads" \
  'BEGIN { budget = by / (2e6 * t) + 0.1; printf "  extra %.3f s, budget %.3f s (%d MB of text)\n", a - b, budget, by / 1e6; exit !(a - b <= budget) }' \
  || { echo "  FAIL: over the budget"; fail=1; }
exit "$fail"
```

- [ ] **Step 2:** `scripts/bench-m4.sh` exits 0 (its `docs-none` case runs over the text files
  and the object files, which have no extension and so are detected by their first bytes:
  review r1, Fable #5); `SFOR_RG=/nonexistent scripts/bench-m4.sh`
  exits 1. Paste its lines into the report. Then `PATH=/usr/bin:$PATH scripts/bench.sh`,
  `scripts/bench-code.sh` and `scripts/bench-m3.sh` still exit 0 (to verify in chunk 13: the
  probe ran only `bench-m4.sh`).

### Task 13.3: the binary

- [ ] **Step 1:** `cargo build --release && ls -l target/release/sfor`: at most 23 000 000
  bytes (spec §11 #22); report the size, and the stripped size (`strip -o /tmp/sfor.s
  target/release/sfor`). The probe: 19 064 256 bytes, 17 657 616 stripped (16 598 640 and 15 381 072 at `191dbaf`).

## Chunk 14: documentation

### Task 14.1: README, README.es and the skill

**Files:** `README.md`, `README.es.md`, `skills/searchfor/SKILL.md`.

- [ ] **Step 1:** `README.md`:
  - «Status»: v0.4, adding «object files and, with `--docs`, documents».
  - «Formats»: rows for ELF, PE and Mach-O («files whose first bytes say so, any name: image ›
    section › symbol, at the hex offset») and for the nine document Formats, with their
    extensions, marked «with `--docs`»; then two subsections with the real output of the merged
    binary over C11's fixture (`m4/bin/app` in both layouts, `m4/report.docx`, `m4/budget.xlsx`,
    `m4/deck.pptx`, `m4/report.pdf`, `docs/shop.sqlite` with `--flat`), and the rules of spec
    §5.7 (where rows appear: explicit files, `--binary`, `-a`) and §5.8 (without `--docs` as rg;
    what cannot be read warns once).
  - «searchfor's own flags»: the `--docs` row.
  - «ripgrep compatibility»: «v0.4 implements 56 of them. The other 90 …», and that `--pre`,
    `--pre-glob` and `-z` work as in ripgrep.
  - «Roadmap»: M4 done, with its date and the content of this plan's milestone table.
- [ ] **Step 2:** `README.es.md`: the same changes in Spanish (8375: the docs also in Spanish,
  English first).
- [ ] **Step 3:** the skill: when to use `--docs` («find the slide, sheet or page where a phrase
  is») and `--binary` with object files («which section and symbol hold a string»), with one
  command each, and that without `--docs` documents are rg's.

### Task 14.2: the JSON schema and the spec

**Files:** `docs/json-schema.json`, `docs/specs/2026-10-16-searchfor-design.md`.

- [ ] **Step 1:** the kinds of M4 in the description of `kind`:

```diff
diff --git a/docs/json-schema.json b/docs/json-schema.json
--- a/docs/json-schema.json
+++ b/docs/json-schema.json
@@ -51,7 +51,7 @@
       "additionalProperties": false,
       "properties": {
         "kind": {
-          "description": "A free string, never an enumeration. Markdown: document, heading, code, item, column. YAML, JSON and TOML (from v0.2): key, element, and document for the documents of a YAML stream. Code (from v0.2): the `kinds` of the Format Definition, such as class, method, function, namespace, struct, impl, or the tree-sitter node kind when `kinds` has none. HTML and XML (from v0.3): tag. CSV and TSV (from v0.3): row, column. Subtitles (from v0.3): cue, with a signature. Helper Processors and later Formats add kinds.",
+          "description": "A free string, never an enumeration. Markdown: document, heading, code, item, column. YAML, JSON and TOML (from v0.2): key, element, and document for the documents of a YAML stream. Code (from v0.2): the `kinds` of the Format Definition, such as class, method, function, namespace, struct, impl, or the tree-sitter node kind when `kinds` has none. HTML and XML (from v0.3): tag. CSV and TSV (from v0.3): row, column. Subtitles (from v0.3): cue, with a signature. Object files (from v0.4): object, section, symbol. Documents with --docs (from v0.4): heading, chapter, page, table, row, column, sheet, cell, slide, notes. Helper Processors and later Formats add kinds.",
           "type": "string"
         },
         "name": { "type": "string" },
```

- [ ] **Step 2:** reconcile the spec with the merged code: every example output of §5.7 and
  §5.8 is pasted from the merged binary; a difference is fixed in the spec when the spec only
  described, and reported when the spec decided.

