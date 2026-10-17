# searchfor M3 (v0.3: HTML, XML, CSV, TSV and subtitles) Implementation Plan

> **For agentic workers:** each chunk below is implemented by one subagent in its own git
> worktree, following `test-driven-development`: every task writes its tests first, runs them
> and sees them fail (RED), implements, and runs them green (GREEN). Steps use checkbox
> (`- [ ]`) syntax. Read `CONTEXT.md` (vocabulary), `docs/specs/2026-10-16-searchfor-design.md`
> (the spec; §4.3, §5.4, §5.5, §5.6, §6.5 and §11 #13 and #18–#21 are M3's) and this plan's
> «Design notes» before starting. The spec wins over this plan if they disagree, and the
> disagreement is reported. Everything is written in English: code, comments, docs, commit
> messages. Commit messages carry no attribution lines.

**Goal:** `sfor` v0.3, which adds Trails for HTML and XML (element paths,
`section#pricing › table › tr[3] › td[2]`), CSV and TSV (`row 128 › Plan`) and subtitles
(`cue 12 00:01:02,500`), and stops a Segment name with a decoded line break or escape sequence
from breaking a printed row in every Format.

**Architecture:** the M2 pipeline stays as it is: search first, then the Detector, then the
Processor of the file's Format, only for files with Matches. Three stateless built-in Processors
join `markdown` and `keypath` in `processors::by_name`: `markup` (HTML and XML, a tag scanner of
our own feeding keypath's `Collector`), `csv` (CSV and TSV over `csv-core`, walking records only
up to the last Match) and `subtitles` (SubRip and WebVTT, a line scanner). None of them uses a
tree-sitter Grammar (D1). The printers escape control characters in Segment names (D14).

**Tech stack:** Rust 1.90 (edition 2024), as M2. One new dependency: `csv-core` 0.1 (0.1.13 on
2026-10-17, no dependencies of its own besides `memchr`, which stays at ripgrep's 2.7.4). No
tree-sitter Grammar is added; tree-sitter stays at `=0.24.7` and every ripgrep 14.1.1 pin of
ADR 0001 stays where it is (verified, «Verified for this plan»).

**Reference ripgrep:** 14.1.1, as M1 and M2.

**Branch:** work branch `t-768-searchfor-m3`, which starts from `t-768-searchfor` at `f7cccfe`
(M1 and M2 closed and merged), worktree
`/home/sebas/robotin/apps/mdgrep/.workspace/t-768-mdgrep/wt/m3`. Each chunk branches
`t-768-searchfor-m3-c<N>` from the work branch and is merged back after its tanda, as `ciclo`
stage 7 describes (`using-git-worktrees`, `finishing-a-development-branch`). Line numbers quoted
below are those of `f7cccfe` and are only hints: locate code by the names given. The code of
this plan was built and tested in a probe copy of the repository (`/tmp/subtask/m3-plan/m3tree`);
the diffs below are against `f7cccfe`.

---

## Why M3, and what stays for M4–M6

M2 shipped code and configuration. M3 is the milestone of «documents written as text»: markup,
tables and subtitles, the last text Formats of grill #1 («HTML y XML: ruta de elementos», «CSV:
fila y nombre de la columna») and the subtitles that 8380 named among the most valuable Formats.
The milestone table of the M2 plan is updated with 8385, which moved the extra Formats of
Questionnaire 49 to M6:

| Milestone | Release | Content |
|---|---|---|
| M1 (done) | v0.1 | Source, Detector, Format Definitions, Processor contract, Markdown, core ripgrep flags, tree / one-line / JSON output, Fragments, `--within`, `--expand`, `--in-headings`, skill, crates.io |
| M2 (done) | v0.2 | `keypath` (YAML, JSON, TOML); `treesitter` with eight compiled-in Grammars; Helper Processors (`pool`, `concurrent`, `oneshot`); `processor = "external"`; `--install-grammar`, `--list-grammars` |
| **M3 (this plan)** | v0.3 | `markup` (HTML, XML), `csv` (CSV, TSV), `subtitles` (SubRip, WebVTT); control characters escaped in printed names; M3 benchmark, perf ceilings and fuzz targets |
| M4 | v0.4 | Object files (ELF, PE, Mach-O), `--docs` extractors (PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB, SQLite; 8393–8395), ripgrep's `--pre`, the binary Formats kept by 8390 |
| M5 | v1.0 | MCP server, the rest of ripgrep's flags, `dist` (GitHub Releases, Homebrew tap, npm), Windows and macOS in CI, Grammar Helper binaries in the index, signed releases, the External Processor guide |
| M6 | v1.x | The other Formats of Questionnaire 49 that 8385 accepted (AsciiDoc, reStructuredText, Org, LaTeX, notebooks, INI/.env/.properties/systemd units, nginx/Apache, Dockerfile, Makefile/justfile, HCL, ssh config, SQL, diffs, Vue and Svelte components, Blade, GraphQL, Protobuf, HAR, mail, calendars and contacts, BibTeX, chat exports, image/audio metadata, WebAssembly, Java `.class`) |

**What M3 leaves out, and why:**

- **The Formats of Questionnaire 49 other than subtitles** go to M6 (8385). That includes the
  subtitle formats nobody named (`.ass`/`.ssa`, `.sbv`, `.lrc`) and the single-file components
  of Vue and Svelte, which hold HTML.
- **ripgrep's `-E/--encoding`** is a flag of M5 («the rest of ripgrep's flags»). Until then a
  Latin-1 HTML, CSV or subtitle file is searched as ripgrep searches it without `-E`, and a name
  with bytes that are not UTF-8 shows U+FFFD (spec §5.4–§5.6).
- **Streaming the Trails of a file with millions of Matches.** A 50 MB file where every line
  matches costs sfor seconds and hundreds of MB, in every Format, because the pipeline keeps the
  Trails of a whole file before printing it (measured: JSON Lines 3.5 s and 900 MB, Markdown
  3.6 s and 420 MB, CSV 5.4 s and 1.5 GB, against 0.2–0.5 s with `--no-trail`). That is the
  pipeline's, not a Processor's, so M3 reports it (D16) and proposes it for M5's flag work, where
  the printers are touched anyway.
- **An id attribute for XML** (`android:id`, `name`), entity decoding in names, HTML templates
  and foreign content beyond `<x/>`: none was asked for, and each is a Definition parameter or a
  scanner rule that can come later without changing a Trail that M3 prints (D24).
- **M2's tree-sitter-yaml fallback idea** (M2 plan, D3) stays out: no Helm chart in M3's corpora
  needed it.

## Grill decisions this plan implements

| # | Decision | Where in M3 |
|---|---|---|
| 1 | HTML and XML: element path; CSV: row and column name | C2, C3 |
| 5 | Magic bytes decide a Format (here `<?xml`, for files without an extension) | C1 |
| 6 | Embedded TOML Definitions, user directory, `--export-formats` (the six new ones export too) | C1 |
| 8375 | Everything in English | all chunks |
| 8380 | Subtitles with timestamp, among the most valuable Formats, before the Windows/macOS step | C4 |
| 8381 | Coloured output: the new Segments print in the Trail colour of M1 | C5 |
| 8385 | Search as fast as ripgrep; extra cost only in files with Matches; a Match with no structure prints as ripgrep prints it; Questionnaire 49's other Formats in M6 | C1 (scope), C2–C4 (D17, D22), C7 benchmark |
| 8390 | Decorate the Formats ripgrep already finds things in | C2–C4 (all are text) |
| 8453 + 8454 (ADR 0005) | v1 Grammars compiled in; M3 decides whether HTML and XML use one | D1: they do not |

Decisions 2–4, 7–21 and the others of 8355–8459 were implemented by M1 or M2 or belong to M4–M6.

## Decisions delegated to this plan

Sebastián left these open; each one is taken here, with its reason, and the spec carries the
ones it describes marked «M3 plan».

| Id | Decision | Reason |
|---|---|---|
| D1 | HTML and XML use a tag scanner of our own (`markup`), CSV and TSV `csv-core`, subtitles a line scanner; no tree-sitter Grammar | Measured on 20 MB generated files: tree-sitter-html 0.23.2 parses at 5.0 MB/s plus 1.5 s of query, tree-sitter-xml 0.7.0 at 3.9 MB/s plus 2.6 s; the M3 scanner walks the same files with Trails in about 0.15 s and 0.11 s, ripgrep searches them in 13 ms. Both Grammars are ABI 14 and resolve with the pins, so the choice is speed (8385), not compatibility. quick-xml 0.42.0 scanned XML at 192 MB/s, but HTML needs our own tokenizer anyway (void elements, raw text, unquoted attributes), and one scanner serves both |
| D2 | One `markup` Processor for HTML and XML, chosen by `syntax` in `[params]`, as `keypath` | The tokenizer, the walk and the naming are shared; the syntaxes differ in a few rules |
| D3 | `csv-core` 0.1, not the `csv` crate and not a scanner of our own | `read_field` reports how many input bytes each call consumed, so every field's byte range in the Source is known; the `csv` crate's records give positions in its decoded buffer only. csv-core is BurntSushi's, has no dependencies besides memchr, and read 140–160 MB/s in the probe |
| D4 | CSV `delimiter = "auto"`: the most frequent of `,` `;` tab `|` in the first line outside quotes, `,` on a tie; TSV `delimiter = "\t"`, `quote = ""` | Spreadsheets in many locales (es-AR among them) export `;`; IANA's TSV has no quoting, and a stray `"` in a TSV must not swallow the rest of the file |
| D5 | `row N` counts records after the header, 1-based, blank lines excluded (as csv-core reads them); `column K`, 1-based, for an empty header cell, an extra field or `headers = false` | `row 1` is the first data row, as M4's SQLite `row 12` will be; the line number printed beside it gives the file position |
| D6 | A cue is `cue ID START`: ID is the identifier line (SRT counter, WebVTT identifier, 40 characters), else the cue's 1-based position; START as written; `signature` `cue ID START --> END` for the tree; the cue's label is empty | The user asked for the cue number and the timestamp; as written keeps it greppable in the file; an empty label puts a Match in the timing line inside its cue, which has no parent to fall back to |
| D7 | `html` and `body` are hidden Segments, not searchable by name; `head` is shown | They hold the whole page and would open every HTML Trail with the same two names; a name that never shows in a Trail should not be found by `--in-headings` either (review r1, Fable #6) |
| D8 | An element without an id gets `[n]`, 1-based as XPath, when its parent has two or more children with its tag; empty elements count; the index goes after the (cut) name | Spec §5.4's `li[3]`; XPath and CSS `:nth-of-type` count from 1, so `li[3]` reads as both; an id is unique and needs no index |
| D9 | An element's label is its whole start tag: a Match in its name or an attribute gets the parent's Trail | The rule of headings and keys (spec §5.1, §5.2); one rule for every Format |
| D10 | A CSV column's Section is its row (CONTEXT.md updated) | In Markdown a column's Section is its table; in CSV the table is the file, and `--expand` would print a 1 GB file |
| D11 | HTML: void elements, raw text elements, the end tags HTML lets authors omit (`li`, `dt`/`dd`, `tr`, `td`/`th`, table sections, `option`, `optgroup` — which also ends an open `option` —, `p` before a block, heading after heading), each with the stop list of spec §5.4, and an end tag closing what is inside it; unquoted attribute values run to a space or `>`; nothing is inserted | The Trail describes the markup as written; an inserted `tbody` would name an element that is not in the file |
| D12 | XML: the first construct that is not well formed ends the structure, silently; elements open at the end of the file end there | YAML's and JSON's rule (M2 plan, D3): a warning per broken file would bury the output |
| D13 | Void and empty elements are not Segments | They hold nothing but their tag, which is their label (D9), so they could never be in a Trail |
| D14 | Tree and one-line output print the control characters of names and signatures as escapes (`\n`, `\u{1b}`); `--json`, `--within` and `--in-headings` keep the decoded name. The fix is in the printers (`text::visible`), not in `Name::text` | M2's leftover: `"a\nb"` broke the row in JSON, TOML, YAML and JavaScript. `Name::text` feeds the `--in-headings` search, which must see the decoded name; only the terminal rendering needs one line, and escaping also stops a key from writing escape sequences to the terminal |
| D15 | Benchmark criterion (`scripts/bench-m3.sh`): no-match and `--no-trail` ≤ 1.1 × rg; sparse extra time ≤ largest matching file ÷ 50 MB/s + matching bytes ÷ (50 MB/s × threads) + 0.05 s | One file is walked by one thread, so the largest file bounds the wall time; 50 MB/s is below the slowest measured walk (a tag-dense 50 MB HTML file, 66 MB/s under a load average of 5); 0.05 s covers process start and Detector work on 200 small files |
| D16 | Dense Matches (every row matching) are reported by the benchmark, not bounded | The cost is the pipeline's, in every Format (see «What M3 leaves out») |
| D17 | No `max_bytes` for `markup`, `csv` and `subtitles` | They walk a file once at 60 MB/s or more and keep memory proportional to depth × Matches; `treesitter`'s limit exists because it parses at 3 MB/s |
| D18 | A Match in the CSV header row has an empty Trail; `--in-headings` searches header cells; a cell that csv-core read leniently (stray quotes) names its column but is not searched | The header is the columns' names (spec §5.2's rule for keys); the pieces of a lenient reading cannot be mapped to the Source exactly, and the fuzz found such cells |
| D19 | When a CSV file ends inside a quoted field, the structure ends at that record | Otherwise every Match after an unclosed quote would claim to be in one giant field of that record |
| D20 | Extensions: html `html`, `htm`, `xhtml`; xml `xml`, `xsd`, `xsl`, `xslt`, `svg`, `rss`, `atom`, `plist`, `wsdl`, `xlf`, `xliff`, `kml`, `gpx`, `csproj`, `vbproj`, `fsproj`, `nuspec`, `resx`, `xaml`, and the magic `<?xml`; csv `csv`; tsv `tsv`; srt `srt`; vtt `vtt` | `.xhtml` is read by the tolerant HTML rules, which give the same Trails on well-formed files and lose nothing at a syntax error; the XML list is the common XML-syntax files; `.tab` is left out because MapInfo uses it for another format |
| D21 | `name_chars = 40` in the html, xml, csv and tsv Definitions; 40 characters for cue identifiers | Markdown's `item_name_chars` is 40; a Tailwind class list or a misdetected header line must not fill the row |
| D22 | `markup` builds a Segment only when its element closes and holds an anchor (`Collector::keeps` and `closed`, as M2's JSON walker); children wait in their parent until their siblings are counted | Elements without Matches cost no allocation; the M2 `Collector` API is used unchanged |
| D23 | No `--no-headers` flag: the Definition's `headers` parameter | searchfor's own flags apply to every Format; a CSV without a header row is a user Definition picked with `--format` (spec §5.5) |
| D24 | Attribute values are used verbatim in names (no entity decoding) | Entities in `id` and `class` are rare, and verbatim pieces map to the Source byte for byte |
| D25 | Past 256 open elements no more Segments open | JSON's limit (M2 plan, D34); the walk is iterative, so any depth is safe |
| D26 | `tests/fixtures/table/` and `tests/fixtures/subs/` carry a `.gitattributes` with `* -text` | Their fixtures hold CRLF line ends on purpose; `core.autocrlf=input` would rewrite them on commit (as M1's `misc/`) |
| D27 | `Outline::build` computes the lines of its Segments by counting line breaks up to the bytes asked for (`trail::fill_lines`), instead of a `LineIndex` of the whole file | A `LineIndex` holds one `usize` per line: one Match in 50 MB of line breaks took 467 MiB (review r1, Codex #2). Every Processor that builds an Outline gets the same lines; markup, keypath and treesitter stop paying a line index, while Markdown and the Helper Processors keep their own (M1, M2; review r2, Fable #4) |
| D28 | Each open element counts its children's tags in a `Vec`, and past 8 distinct tags also in a `HashMap` from the tag (lowercased in HTML) to its slot | A linear search per child was quadratic: 200 000 distinct sibling tags took 82 s (review r1, Codex #1, Fable #1). Memory: one entry per distinct child tag of each open element |
| D29 | WebVTT blocks: in a file that starts with `WEBVTT`, the header and the `NOTE`, `STYLE` and `REGION` blocks run to their blank line and hold no cue; a timing line is never an identifier | A `NOTE` line before a timing line became `cue NOTE …` and shifted the ordinals; two timing lines in a row made one cue named by the first (review r1, Codex #3, Fable #4). The `WEBVTT` header decides, so the two Definitions keep no parameters |
| D30 | XML tags are checked against XML's syntax: Names with XML 1.0's NameStartChar and NameChar over Unicode, XML's white space (no form feed), `S attr S? = S? "value"`, no `<` in values, no attribute twice, `</name S?>`; a doctype ends at its first `>` outside literals, comments and processing instructions | `<a=b>` and `<a x=bare>` were taken as elements, `<!ENTITY e "]> <fake>">` invented a `fake` (review r1, Codex #4, #5), and `<©>`, `<a\fb='x'>` and `<a x='1' x='2'>` still were (review r2, Codex #3); spec §5.4 ends the structure at the first construct that is not well formed. Duplicates are compared one by one up to 16 attributes and in a set past them |
| D31 | `text::push_lossy` and `text::cut_name`, written in C1 and used by C2–C4: a name's text is built with one piece per valid UTF-8 run and one per replaced sequence, and a cut keeps the pieces of the visible prefix; only `…` maps to nothing | Each Processor dropped the piece that crossed the cut, so a 50-letter header, `id` or cue identifier was not found by `--in-headings` (review r1, Codex #6), and a Latin-1 `é` before the letters made the whole run one decoded piece, dropped the same way (review r2, Codex #2); one module, because three chunks of one tanda need it |
| D32 | The end of a CSV file is inside a quote only when csv-core's states say so: quoted from the first byte, `""` stays quoted, a closing quote followed by anything else turns the rest into text | Counting quotes lost `1,"needle"junk"` at the end of a file, which csv-core reads closed (review r1, Codex #7, Fable #2) |
| D34 | The markup walk never searches the open elements per element: each open element keeps its `html::Reach` (where each implied-end rule would stop, computed from its parent's when it opens), HTML tags get an id with a count of open elements per id, and `close_implied` trims the white space before a byte once | 40 000 nested `div`s took 4.8 s (each start tag searched the stack for a `p`), 20 000 end tags with nothing open 2.9 s, and 20 000 elements ended after 200 000 spaces 5.7 s (review r2, Codex #1). An end tag's search now only covers the elements it closes, and the limit of 256 Segments (D25) stays |
| D33 | The reference is `SFOR_RG`, else `/usr/bin/rg`; it must be ripgrep 14.1.1. Under `SFOR_REQUIRE_REFERENCE=1` a comparison that cannot run (no rg 14.1.1, no `/usr/bin/time`) fails instead of skipping; acceptance runs set it, and `scripts/bench-m3.sh` always refuses another rg | The `rg` on `PATH` was 15.2.0 in a review session, and a skipped comparison passed the test (review r1, Codex #8) |

## Verified for this plan (2026-10-17)

Outputs in `/tmp/subtask/m3-plan/outputs/`; probes in `/tmp/subtask/m3-plan/realtree` (the
repository at `f7cccfe` plus the candidate dependencies) and, for this plan's code, the frozen
copy `/tmp/subtask/m3-plan/m3tree-43e9b76`: the repository at `f7cccfe` plus the code of this
version of the plan, named after the commit that wrote that code into the plan. The working copy
`/tmp/subtask/m3-plan/m3tree` keeps changing with the reviews; the frozen one does not (review
r2, Fable #2).

- `rustc 1.90.0`, `cargo 1.90.0`, `rg 14.1.1`, 20 cores.
- `realtree`: adding `tree-sitter-html =0.23.2`, `tree-sitter-xml =0.7.0`, `quick-xml 0.42` and
  `csv 1.4` to the whole Cargo.toml of `f7cccfe` resolves and builds; the lock only gains
  `csv`, `csv-core 0.1.13`, `quick-xml 0.42.0`, `ryu`, `tree-sitter-html 0.23.2` and
  `tree-sitter-xml 0.7.0`; `regex` 1.10.6, `regex-automata` 0.4.7, `regex-syntax` 0.8.4 and
  `memchr` 2.7.4 stay. Both Grammars report ABI 14 under tree-sitter 0.24.7.
- Parser speed (`realtree/src/bin/m3probe.rs`, release, one thread), on generated files
  (`scripts/gen.py`: 20 MB HTML, 20 MB XML, 50 MB CSV, 10 MB SRT) and real ones: tree-sitter-html
  5.0 MB/s parse + 1.47 s query (20 MB), 32.7 MB/s on `/usr/share/code/LICENSES.chromium.html`
  (15.1 MB, few tags); tree-sitter-xml 3.9 MB/s + 2.6 s (20 MB), 2.7 MB/s on
  `/usr/share/vulkan/registry/vk.xml`; quick-xml 192 and 234 MB/s; a prototype tag scanner 342
  and 437 MB/s; csv-core 140 MB/s (50 MB) and 157 MB/s on `/usr/share/ieee-data/oui.csv`;
  ripgrep 13 ms on each 20 MB file, 26 ms on the 50 MB CSV.
- csv-core 0.1.13 semantics (`realtree/src/bin/csvsem.rs`): blank lines are skipped and their
  bytes are consumed with the next record's first field; a CRLF record's field consumes the `\r`
  and leaves the `\n` to the next read; a quote inside an unquoted field is text; an unclosed
  quote runs to the end of the file; a last field without a line break comes from the call with
  empty input.
- The frozen copy (this plan's code, after review r2): `cargo build --workspace`, `cargo clippy
  --workspace --all-targets -- -D warnings` (no findings) and `SFOR_REQUIRE_REFERENCE=1 cargo
  test --workspace` pass entirely: 393 unit tests passed and 2 ignored, `cli` 41, `cli_m2` 16 (with the one known
  change, `m2_07`, Task 1.3), `cli_m3` 14, `fuzz` 2, `helpers` 22, `rg_compat` 37, and the
  helpers' own. The fuzz passes with 17 000 inputs (default) and 340 000 (`SFOR_FUZZ_N=20000 SFOR_FUZZ_SEED=42`); before review r1
  it found two mapping bugs in header and cue names, fixed in the code below. `Cargo.lock`
  gains only `csv-core 0.1.13`. Without a lock (`cargo generate-lockfile`, what `cargo install
  searchfor` does) the same versions resolve, every ripgrep pin included.
- Review r1 regressions, each measured RED with the release binary built before its fix and
  GREEN after it: 40 000 distinct sibling tags 3.26 s → 0.02 s (Codex #1; Fable measured 82 s for
  200 000); one Match in 50 MB of line breaks 467 MiB → 58 MiB of RSS, rg 56 MiB (Codex #2);
  `--in-headings AAAA` over a 50-letter header, `id` and cue identifier, and `ññññ` over a
  multi-byte header, exit 1 → 0 (Codex #6). The unit tests of Codex #3, #4, #5, #7, #9 and Fable
  #2, #4, #5 FAILED against the first version of their file and PASS now.
- Review r2 regressions, RED with the release binary of review r1 and GREEN now: 40 000 nested
  `div`s 3.96 s → 0.03 s, 20 000 end tags with nothing open 2.88 s → 0.02 s, 20 000 elements
  ended after 200 000 spaces 5.65 s → 0.02 s (rg about 0.009 s; Codex #1); `--in-headings AAAA`
  over a header, `id` and cue identifier with a Latin-1 `é` before the letters, exit 1 → 0
  (Codex #2); the XML cases of Codex #3 in the unit tests. The RSS guard of `fill_lines`, run
  with the walk of this plan and the `Outline::build` of `f7cccfe`, FAILS on its RSS assertion
  (457 MiB against rg's 56 + 32) and PASSES with `fill_lines` (Codex #4). The walk's bookkeeping
  costs time on tag-dense HTML: the generated 20 MB HTML takes 0.15 s against 0.12 s for the
  review-r1 walk, measured side by side under a load average of 5.
- The perf ceilings of Task 7.1 and the guards of Task 2.5 in release, with
  `SFOR_REQUIRE_REFERENCE=1` and `/usr/bin/rg` 14.1.1: 50 MB HTML 0.76 s (rg 0.022 s), XML 0.37 s, CSV 0.30 s, TSV 0.30 s, SRT 0.16 s, VTT 0.14 s, RSS 57 MiB against rg's 56; 40 000 nested `div`s 0.03 s, 20 000 end tags with nothing open 0.02 s, 20 000 implied ends after 200 000 spaces 0.02 s (rg 0.008 s); 200 000 distinct siblings and 50 MB of line breaks pass. The HTML ceiling keeps 0.26 s of margin under that load; `scripts/bench-m3.sh` (24 006 files) gave none 1.02 × rg, `--no-trail` 1.01 ×, sparse extra 0.21 s against 0.58 s, dense 14.5 × and 660 MiB.
- `scripts/bench-m3.sh` on its generated corpus (24 006 files, 246 of them matching the sparse pattern): none 1.04 × rg, `--no-trail` 1.02 ×, sparse extra 0.14 s against a budget of 0.58 s, dense 14.4 × rg with 660 MiB of RSS against rg's 61 (reported only); `SFOR_RG=/nonexistent` and `SFOR_BIN=scripts/fake-sfor-exit-2.sh` make it exit 1.
  A 5 004-file corpus once gave 1.10 × for «none» before review r1: the process start of sfor
  (5.8 ms against rg's 4.5 ms) weighs too much on a 22 ms run, hence the larger corpus.
- Dense, 50 MB, every line matching (release, `/usr/bin/time`): JSON Lines 3.51 s and 876 MiB,
  Markdown 3.62 s and 409 MiB, CSV 5.36 s and 1.48 GiB; `--no-trail` 0.16–0.46 s (D16). With
  `.` as the pattern (every character a Match) the bench's CSV files took 24 s and 6.3 GiB, which
  is why its dense pattern is `^.`, one Match per line.
- The release binary of `m3tree` weighs 16.5 MB (spec §7 gives 16.4 MB for v0.2).
- The M2 leftover reproduces at `f7cccfe`: `{"a\nb": "needle"}`, `"a\nb" = "needle"`,
  `"a\nb": needle` and `{"a\nb": () => "needle"}` print the row split in two (`1: a` / `b │ …`)
  in one-line output, and the tree name on two lines, except JavaScript's tree, which shows the
  signature as written.

**To verify:** M1's `scripts/bench.sh` and M2's `scripts/bench-code.sh` still exit 0 — to verify
in chunk 7. Every other statement above was measured.

## M3 acceptance criteria

M3 is done when all of these hold; each names the test or script that measures it.

1. `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and
   `SFOR_REQUIRE_REFERENCE=1 cargo test --workspace` pass, every M1 and M2 test included (`m2_07`
   updated, Task 1.3).
2. Spec §11 #2 for the six new Formats: a fixture and tree, one-line and JSON snapshots each,
   JSON valid against `docs/json-schema.json` (C6, `m3_01`).
3. Spec §11 #18: the HTML and XML Trails of `m3_02` and `m3_03`, line for line, and the unit
   tests of C2 (`html_element_paths`, `xml_keeps_the_trails_before_a_syntax_error`).
4. Spec §11 #19: the CSV and TSV Trails of `m3_04` (`row 2 › Plan`, the `;` file, the TSV) and
   the unit tests of C3.
5. Spec §11 #20: the cues of `m3_05` (SRT with BOM and CRLF, WebVTT with header, note and
   identifiers) and `m3_12` (UTF-16).
6. Spec §11 #21: `m3_08` for JSON, TOML, YAML and JavaScript, and ESC escaped.
7. Spec §11 #13 on M3's Formats: `scripts/bench-m3.sh` exits 0 (it checks that
   `/usr/bin/rg` is 14.1.1 and refuses any other), and `PATH=/usr/bin:$PATH scripts/bench.sh`
   and `PATH=/usr/bin:$PATH scripts/bench-code.sh` still exit 0 after `PATH=/usr/bin:$PATH rg
   --version` printed `ripgrep 14.1.1` (C7).
8. The eleven M3 tests of Tasks 2.5 and 7.1 pass in release with the reference required:
   `SFOR_REQUIRE_REFERENCE=1 cargo test --release --test perf -- --ignored` — 50 MB of each of
   the six M3 Formats with three Matches within rg's time + 1 s and rg's RSS + 32 MiB; 200 000
   distinct sibling tags, 40 000 nested `div`s, 20 000 end tags with nothing open and 20 000
   elements ended after 200 000 spaces, each within rg's time + 1 s; and one Match in 50 MB of
   line breaks within rg's RSS + 32 MiB. A run without `SFOR_REQUIRE_REFERENCE=1`, or one that printed
   `skipping:`, does not count.
9. The fuzz of Task 6.2 passes with its default 1 000 inputs per Format (17 Formats) and with
   `SFOR_FUZZ_N=20000` once before the merge.
10. `--no-trail` prints ripgrep's lines over the three fixture directories (`m3_10`), run as
    `SFOR_REQUIRE_REFERENCE=1 cargo test --test cli_m3` so that it cannot skip.
11. `Cargo.lock` gains `csv-core` and nothing else; `cargo tree -i regex-automata` and
    `cargo tree -i regex-syntax` show only 0.4.7 and 0.8.4 (Task 1.1).
12. `target/release/sfor` is at most 20 MB (measured in C7 and reported; 16.5 MB in the probe).

## Design notes the chunks share

1. **The Processor contract stays.** `Processor::{trails, names}` keep their signatures; the
   three new Processors implement them directly (no `outline_of`), like `keypath`: Trails for
   the anchors asked, in their order (they sort a copy and write back). They are stateless, so
   `processors::by_name` serves them and `Processors` needs no change.
2. **Search first, describe later**, as M1 and M2: they run only on files with Matches. A file
   they cannot describe gets empty Trails from `Ok`, never an `Err`: the only `Err` is a
   Definition with bad `[params]` (`syntax`, `delimiter`, `quote`), which the pipeline turns into
   one warning per file (`app.rs`, `trails`).
3. **markup** (C2). `scan.rs` finds tags with `memchr` and skips comments, CDATA, processing
   instructions and doctypes; `html.rs` classifies a tag name once (`html::info`) and decides which
   open elements a start tag ends; `mod.rs` walks the tags with a stack of `Frame`s (each counts
   its children's tags, with a map past 8 distinct ones, D28, and keeps its `Reach` and tag id,
   D34) and hands
   keypath's `Collector` (`crate::processors::keypath::collect`, unchanged) only the elements that
   hold an anchor, built when they close (D22). A child without an id waits in its parent's
   `pending` list until the parent closes, when the sibling counts are final and `[n]` can be
   added (D8).
4. **csv** (C3). `Records` drives `csv_core::Reader::read_field` over the Source and keeps each
   field's raw byte range (`Field { raw, end }`); `Lines` counts line numbers incrementally, so
   nothing scans past the record of the last anchor. Header names are decoded twice — by
   csv-core and by `header_name`, which also produces the pieces — and a cell where the two
   disagree is not searchable (D18).
5. **subtitles** (C4). `cues` walks the lines once and calls back per cue, stopping when the
   caller has no anchor left; `timing` recognises a timing line without a regex.
6. **The printers** (C5). `text::visible` escapes control characters; `flat::fit_trail` measures
   and cuts the escaped names, and `tree::label` escapes names and signatures. The tree labels of
   the new kinds: `tag` and `row` show their name, `column` keeps Markdown's `column «Name»`,
   `cue` shows its signature.
7. **New fixtures and older tests.** C2, C3 and C4 add `tests/fixtures/{markup,table,subs}/`,
   which M1's end-to-end and compatibility tests copy whole. Those tests compare with rg under
   `--no-trail` or check with `contains`; in the probe all of them passed with the new fixtures.
   An M1 or M2 test that changes for another reason is reported, not edited, and fixed in the
   tanda's integration.
8. **Snapshots** as M1 and M2: insta, `INSTA_UPDATE=always cargo test`, every new `.snap` read in
   full and checked by eye before committing; the chunk report pastes one snapshot per Format.
9. After each tanda: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D
   warnings`, `cargo test --workspace`; `cargo fmt` before every commit.

## File structure

```
Cargo.toml, Cargo.lock                       csv-core (C1 only)
formats/{html,xml,csv,tsv,srt,vtt}.toml      the six Definitions (C1)
src/formats/builtin.rs                       their embedding (C1)
src/formats/mod.rs                           their test (C1)
src/processors/mod.rs                        module lines, by_name (C1)
src/processors/markup/{mod,scan,html}.rs     markup: walk, tag scanner, HTML rules (stubs C1, C2)
src/processors/csv.rs                        csv (stub C1, C3)
src/processors/subtitles.rs                  subtitles (stub C1, C4)
src/text.rs                                  cut_name() (C1), visible() (C5)
src/trail.rs                                 fill_lines(): Outline lines without a line index (C2)
tests/common/mod.rs, tests/rg_compat.rs      the reference rg, SFOR_REQUIRE_REFERENCE (C1)
src/output/{flat,tree}.rs                    escaped names, labels of tag/row/cue (C5)
tests/cli_m2.rs                              m2_07's count of exported Definitions (C1)
tests/fixtures/markup/                       page.html, feed.xml, broken.xml (C2)
tests/fixtures/table/                        plans.csv, semi.csv, people.tsv, .gitattributes (C3)
tests/fixtures/subs/                         movie.srt, talk.vtt, .gitattributes (C4)
tests/cli_m3.rs, tests/snapshots/cli_m3__*   end-to-end tests of the new Formats (C6)
tests/fuzz.rs                                the M3 targets (C6)
tests/perf.rs                                the markup guards (C2), the M3 ceilings (C7)
scripts/bench-m3.sh                          the M3 benchmark (C7)
README.md, README.es.md, skills/searchfor/SKILL.md, docs/json-schema.json, the spec   (C8)
```

## Chunk graph

| Chunk | Content | Depends on | Files and folders (exclusive within its tanda) | Tanda |
|---|---|---|---|---|
| C1 | csv-core, the six Definitions, stubs of the three Processors, `by_name`, `m2_07`, `text::cut_name`, the reference rg of the tests | — | `Cargo.toml`, `Cargo.lock`, `formats/{html,xml,csv,tsv,srt,vtt}.toml`, `src/formats/builtin.rs`, `src/formats/mod.rs` (tests), `src/processors/mod.rs`, stubs `src/processors/markup/{mod,scan,html}.rs`, `src/processors/{csv,subtitles}.rs`, `src/text.rs` (`cut_name`), `tests/cli_m2.rs`, `tests/common/mod.rs`, `tests/rg_compat.rs`, `tests/perf.rs` (three hunks) | 1 |
| C2 | markup: scanner, HTML rules, walk; Outline lines without a line index; the walk's perf guards | C1 | `src/processors/markup/**`, `src/trail.rs`, `tests/fixtures/markup/**`, `tests/perf.rs` (the markup guards) | 2 |
| C3 | csv | C1 | `src/processors/csv.rs`, `tests/fixtures/table/**` | 2 |
| C4 | subtitles | C1 | `src/processors/subtitles.rs`, `tests/fixtures/subs/**` | 2 |
| C5 | Escaped names in the printers, tree labels of the new kinds | C1 | `src/text.rs` (`visible`), `src/output/flat.rs`, `src/output/tree.rs` | 2 |
| C6 | End-to-end tests and snapshots; fuzz targets | C2–C5 | `tests/cli_m3.rs`, `tests/snapshots/cli_m3__*.snap`, `tests/fuzz.rs` | 3 |
| C7 | Perf ceilings of the six Formats, benchmark; M1's and M2's benchmarks rerun | C2–C5 | `tests/perf.rs` (the Format ceilings), `scripts/bench-m3.sh` | 3 |
| C8 | README, README.es, skill, JSON schema, spec reconciliation | C2–C5 | `README.md`, `README.es.md`, `skills/searchfor/SKILL.md`, `docs/json-schema.json`, `docs/specs/2026-10-16-searchfor-design.md` | 3 |

Eight chunks in three tandas, as before review r1; C1 grew two tasks (1.4, 1.5) and C2 one
(2.5), and no file is shared within a tanda: C1 and C5 both edit `src/text.rs`, and C1, C2 and
C7 `tests/perf.rs`, each in its own tanda (review r2, Codex #4 moved the markup guards to C2, so
their RED runs against C2's own code). C1 writes every `mod` line, the dependency, the
Definitions, `cut_name` and a stub for every new file, so the chunks of tanda 2 only fill their
own files and never share one;
after tanda 2 the pipeline describes every new Format, so the three chunks of tanda 3 test,
measure and document the merged result in parallel. C8 pastes real output: it runs the merged
binary of tanda 2, not a snapshot of C6.

---

## Chunk 1: csv-core, the Definitions, the stubs and the registry

### Task 1.1: the dependency

**Files:** `Cargo.toml`, `Cargo.lock`. This is the only task of M3 that edits them.

- [ ] **Step 1:** add `csv-core` after `tempfile` in `[dependencies]`, keeping every pinned line:

```diff
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -67,6 +67,8 @@
 ureq = "3"
 # Temporary files for Helper Processors (spec §4.4, Paths).
 tempfile = "3"
+# CSV and TSV fields with their byte offsets (spec §5.5; M3 plan, D3).
+csv-core = "0.1"
 
 # Ctrl-C with Helper Processors running (spec §4.4, Lifetime; M2 plan, D42).
 [target.'cfg(unix)'.dependencies]
```

- [ ] **Step 2:** `cargo build`. Use `cargo build`, never `cargo update` or
  `cargo generate-lockfile`, so the locked versions stay. Check: `git diff Cargo.lock` adds only
  the `csv-core` package (0.1.13 on 2026-10-17) and its line in `searchfor`'s dependencies;
  `cargo tree -i regex-automata` and `cargo tree -i regex-syntax` show only 0.4.7 and 0.8.4;
  `cargo tree -i memchr` shows only 2.7.4. Record the versions in the report.

### Task 1.2: the failing tests

**Files:** `src/formats/mod.rs` (tests), `src/processors/mod.rs` (tests).

- [ ] **Step 1:** in the `tests` module of `src/formats/mod.rs`, before
  `user_file_overrides_builtin_and_adds_new`, and in the `tests` module of
  `src/processors/mod.rs`, before `by_name_knows_the_stateless_built_ins_only`:

```rust
    // src/formats/mod.rs
    #[test]
    fn the_m3_definitions_load_and_detect() {
        let (r, warnings) = Registry::load(None, &known);
        assert!(warnings.is_empty(), "{warnings:?}");
        let det = detector::Detector::new(&r, &[]).unwrap();
        for (path, head, format) in [
            ("a/page.HTML", &b""[..], "html"),
            ("p.xhtml", b"", "html"),
            ("feed.xml", b"", "xml"),
            ("icon.svg", b"", "xml"),
            ("feed", b"<?xml version=\"1.0\"?>\n<r/>", "xml"),
            ("t.csv", b"", "csv"),
            ("t.tsv", b"", "tsv"),
            ("m.srt", b"", "srt"),
            ("m.vtt", b"", "vtt"),
        ] {
            let got = det
                .detect(std::path::Path::new(path), head)
                .map(|d| d.name.as_str());
            assert_eq!(got, Some(format), "{path}");
        }
    }

    // src/processors/mod.rs
    #[test]
    fn by_name_knows_the_m3_built_ins() {
        for p in ["markup", "csv", "subtitles"] {
            assert!(by_name(p).is_some(), "{p}");
            assert!(Processors::known(p), "{p}");
        }
    }
```

- [ ] **Step 2:** `cargo test --lib -- the_m3_definitions by_name_knows_the_m3`: both FAIL (no
  `html` Definition; `by_name("markup")` is None).

### Task 1.3: Definitions, stubs, registry

**Files:** `formats/{html,xml,csv,tsv,srt,vtt}.toml`, `src/formats/builtin.rs`,
`src/processors/mod.rs`, `src/processors/markup/{mod,scan,html}.rs`,
`src/processors/{csv,subtitles}.rs`, `tests/cli_m2.rs`.

- [ ] **Step 1:** the six Definitions (spec §5.4–§5.6, D4, D20, D21):

```toml
# formats/html.toml — HTML element paths (spec §5.4): `section#pricing › table › tr[3] › td[2]`.
name       = "html"
extensions = ["html", "htm", "xhtml"]
processor  = "markup"

[params]
syntax = "html"
# Characters of an element's name (`tag#id.class…`) before it is cut with `…`.
name_chars = 40
```

```toml
# formats/xml.toml — XML element paths (spec §5.4): `catalog › book[2] › title`.
name       = "xml"
extensions = ["xml", "xsd", "xsl", "xslt", "svg", "rss", "atom", "plist", "wsdl", "xlf", "xliff", "kml", "gpx", "csproj", "vbproj", "fsproj", "nuspec", "resx", "xaml"]
# `<?xml`, read only for files without an extension (spec §4.1).
magic      = [{ hex = "3c3f786d6c" }]
processor  = "markup"

[params]
syntax = "xml"
name_chars = 40
```

```toml
# formats/csv.toml — CSV rows and columns (spec §5.5): `row 128 › Plan`.
name       = "csv"
extensions = ["csv"]
processor  = "csv"

[params]
# "auto": the most frequent of `,` `;` tab `|` in the first line, outside quotes.
delimiter = "auto"
quote     = "\""
# The first row names the columns; false names them `column 1`, `column 2`…
headers   = true
name_chars = 40
```

```toml
# formats/tsv.toml — TSV rows and columns (spec §5.5). Quotes are text, as in IANA's TSV.
name       = "tsv"
extensions = ["tsv"]
processor  = "csv"

[params]
delimiter = "\t"
quote     = ""
headers   = true
name_chars = 40
```

```toml
# formats/srt.toml — SubRip subtitles (spec §5.6): `cue 12 00:01:02,500`.
name       = "srt"
extensions = ["srt"]
processor  = "subtitles"
```

```toml
# formats/vtt.toml — WebVTT subtitles (spec §5.6): `cue 12 00:01:02.500`.
name       = "vtt"
extensions = ["vtt"]
processor  = "subtitles"
```

- [ ] **Step 2:** embed them, after `yaml.toml`:

```diff
--- a/src/formats/builtin.rs
+++ b/src/formats/builtin.rs
@@ -19,4 +19,10 @@
     ("json.toml", include_str!("../../formats/json.toml")),
     ("toml.toml", include_str!("../../formats/toml.toml")),
     ("yaml.toml", include_str!("../../formats/yaml.toml")),
+    ("html.toml", include_str!("../../formats/html.toml")),
+    ("xml.toml", include_str!("../../formats/xml.toml")),
+    ("csv.toml", include_str!("../../formats/csv.toml")),
+    ("tsv.toml", include_str!("../../formats/tsv.toml")),
+    ("srt.toml", include_str!("../../formats/srt.toml")),
+    ("vtt.toml", include_str!("../../formats/vtt.toml")),
 ];
```

- [ ] **Step 3:** the stubs. `src/processors/markup/scan.rs` and `src/processors/markup/html.rs`
  hold one line each, `//! Filled by C2 (M3 plan).`. `src/processors/markup/mod.rs`:

```rust
//! markup: HTML and XML element paths (spec §5.4). Filled by C2 (M3 plan).
pub mod html;
pub mod scan;

use super::Processor;
use crate::model::{Input, Name, Trail};

pub struct Markup;

impl Processor for Markup {
    fn trails(&self, _: &Input, _: &toml::Table, _: &[usize]) -> anyhow::Result<Vec<Trail>> {
        Err(crate::helper::Unavailable.into())
    }

    fn names(&self, _: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Err(crate::helper::Unavailable.into())
    }
}
```

  `src/processors/csv.rs` and `src/processors/subtitles.rs` are the same file without the two
  `pub mod` lines, with `pub struct Csv;` (`//! csv: rows and columns of CSV and TSV files
  (spec §5.5). Filled by C3 (M3 plan).`) and `pub struct Subtitles;` (`//! subtitles: the cues
  of SubRip and WebVTT files (spec §5.6). Filled by C4 (M3 plan).`). `Unavailable` makes a file
  Plain without a warning (M2 plan, D33), so between tanda 1 and tanda 2 the new Formats print as
  ripgrep prints them.

- [ ] **Step 4:** the module lines and `by_name` in `src/processors/mod.rs` (the test is Task
  1.2's):

```diff
--- a/src/processors/mod.rs
+++ b/src/processors/mod.rs
@@ -1,8 +1,11 @@
 //! Processors turn Match anchors into Trails (design note 6).
+pub mod csv;
 pub mod external;
 pub mod frontmatter;
 pub mod keypath;
 pub mod markdown;
+pub mod markup;
+pub mod subtitles;
 pub mod treesitter;
 
 use crate::model::{Input, Name, Trail};
@@ -68,6 +71,9 @@
     match name {
         "markdown" => Some(&markdown::Markdown),
         "keypath" => Some(&keypath::Keypath),
+        "markup" => Some(&markup::Markup),
+        "csv" => Some(&csv::Csv),
+        "subtitles" => Some(&subtitles::Subtitles),
         _ => None,
     }
 }
@@ -355,6 +361,14 @@
     }
 
     #[test]
+    fn by_name_knows_the_m3_built_ins() {
+        for p in ["markup", "csv", "subtitles"] {
+            assert!(by_name(p).is_some(), "{p}");
+            assert!(Processors::known(p), "{p}");
+        }
+    }
+
+    #[test]
     fn by_name_knows_the_stateless_built_ins_only() {
         assert!(by_name("markdown").is_some());
         assert!(by_name("keypath").is_some());
```

- [ ] **Step 5:** `m2_07` counts the exported Definitions, which are now 18:

```diff
--- a/tests/cli_m2.rs
+++ b/tests/cli_m2.rs
@@ -181,7 +181,8 @@
     );
     assert_eq!(
         std::fs::read_dir(&target).unwrap().count(),
-        1 + FORMATS.len()
+        // Markdown, M2's Formats, and M3's html, xml, csv, tsv, srt and vtt.
+        1 + FORMATS.len() + 6
     );
 }
 
```

- [ ] **Step 6:** `cargo test --workspace`: Task 1.2's tests PASS and so does everything else.
  `cargo clippy --workspace --all-targets -- -D warnings` is clean. `sfor --list-formats` lists
  the six new Formats as built-in.

- [ ] **Step 7:** commit: `git add -- Cargo.toml Cargo.lock formats/ src/formats/builtin.rs
  src/formats/mod.rs src/processors/ tests/cli_m2.rs` and
  `git commit -m "Add the M3 Format Definitions, csv-core and Processor stubs"`.

### Task 1.4: `text::push_lossy` and `text::cut_name`, shared by the three Processors (D31)

C2, C3 and C4 build and cut names at `name_chars` (D21) in parallel, so both functions live in
C1. `push_lossy` gives each valid UTF-8 run and each replaced sequence its own piece, and a cut
keeps the pieces of what stays visible: before review r1 each Processor dropped the piece that
crossed the cut, and `--in-headings AAAA` found nothing in a header, an `id` or a cue identifier
of 50 `A`s (Codex #6); before review r2 a Latin-1 `é` before the letters still lost them
(Codex #2).

- [ ] **Step 1:** the test, first in `src/text.rs`'s `tests` module:

```rust
    #[test]
    fn cut_names_keep_the_pieces_of_what_stays_visible() {
        // A verbatim piece across the cut keeps its prefix, multi-byte included.
        let mut t = "ab".to_string() + &"ñ".repeat(10);
        let mut p: Pieces = vec![(0..2, 100..102), (2..22, 200..220)];
        cut_name(&mut t, &mut p, 5);
        assert_eq!(t, "abññ…");
        assert_eq!(p, vec![(0..2, 100..102), (2..6, 200..204)]);
        // Latin-1: `push_lossy` gives the replaced byte its own piece, so the run after it is
        // verbatim and keeps its visible prefix (M3 review r2, Codex #2).
        let src = [b"\xe9".as_slice(), &[b'A'; 40]].concat();
        let (mut t, mut p) = (String::new(), Pieces::new());
        push_lossy(&mut t, &mut p, &src, 0..src.len());
        assert_eq!(p, vec![(0..3, 0..1), (3..43, 1..41)]);
        cut_name(&mut t, &mut p, 10);
        assert_eq!(t, format!("\u{FFFD}{}…", "A".repeat(8)));
        assert_eq!(p, vec![(0..3, 0..1), (3..11, 1..9)]);
        // A decoded piece of several characters across the cut maps its visible part to its
        // whole Source range, unless the lengths would match, which would read as verbatim.
        let mut t = "\u{FFFD}\u{FFFD}ab".to_string();
        let mut p: Pieces = vec![(0..8, 0..4)];
        cut_name(&mut t, &mut p, 2);
        assert_eq!((t.as_str(), p), ("\u{FFFD}…", vec![(0..3, 0..4)]));
        // Short names are untouched.
        let mut t = "short".to_string();
        let mut p: Pieces = vec![(0..5, 0..5)];
        cut_name(&mut t, &mut p, 40);
        assert_eq!((t.as_str(), p), ("short", vec![(0..5, 0..5)]));
    }
```

- [ ] **Step 2:** `cargo test --lib text::`: FAILS to compile (`push_lossy`, `cut_name`, `Pieces`).

- [ ] **Step 3:** above the `tests` module:

```rust
/// A name's pieces, in the format of `model::Name::pieces`: (range in the text, range in the
/// Source).
pub type Pieces = Vec<(std::ops::Range<usize>, std::ops::Range<usize>)>;

/// Appends the text of `src[r]`, invalid UTF-8 as U+FFFD, with one piece per valid run
/// (verbatim) and one per invalid sequence (its U+FFFD over its bytes): a cut then keeps every
/// visible run, a Latin-1 `é` before 40 `A`s included (M3 review r2, Codex #2).
pub fn push_lossy(text: &mut String, pieces: &mut Pieces, src: &[u8], r: std::ops::Range<usize>) {
    let mut at = r.start;
    for chunk in src[r].utf8_chunks() {
        for (part, replaced) in [(chunk.valid().as_bytes(), false), (chunk.invalid(), true)] {
            if part.is_empty() {
                continue;
            }
            let t = text.len();
            if replaced {
                text.push('\u{FFFD}');
            } else {
                text.push_str(chunk.valid());
            }
            pieces.push((t..text.len(), at..at + part.len()));
            at += part.len();
        }
    }
}

/// Cuts `text` to `max` characters, the last one `…`, when it is longer (M3 plan, D21). The
/// pieces keep mapping what stays visible (M3 review r1, Codex #6): a piece past the cut goes;
/// a verbatim piece across it keeps its bytes before the cut; a decoded piece across it maps
/// its visible part to its whole Source range, unless that would make the two lengths equal,
/// which would read as verbatim. Only the `…` maps to nothing.
pub fn cut_name(text: &mut String, pieces: &mut Pieces, max: usize) {
    if max == 0 || text.chars().count() <= max {
        return;
    }
    let cut = text
        .char_indices()
        .nth(max - 1)
        .map_or(text.len(), |(i, _)| i);
    text.truncate(cut);
    text.push('…');
    pieces.retain_mut(|(t, s)| {
        if t.end <= cut {
            return true;
        }
        if t.start >= cut {
            return false;
        }
        let verbatim = t.len() == s.len();
        t.end = cut;
        if verbatim {
            s.end = s.start + t.len();
            true
        } else {
            t.len() != s.len()
        }
    });
}
```

- [ ] **Step 4:** `cargo test --lib text::`: PASS. Commit: `git add -- src/text.rs` and
  `git commit -m "Cut a name keeping the pieces of its visible part"`.

### Task 1.5: the reference ripgrep in the tests (D33)

`rg` on `PATH` may be another version (15.2.0 in a review session, while `/usr/bin/rg` was
14.1.1), and `reference_rg()` turned a missing reference into a silent skip, so a perf ceiling
or a compatibility test could pass without comparing anything (Codex #8). C1 owns the shared
test helpers, so the tanda-3 chunks only use them.

- [ ] **Step 1:** RED: `SFOR_RG=/nonexistent SFOR_REQUIRE_REFERENCE=1 cargo test --test rg_compat`
  still PASSES at this point (nothing reads those variables): that is the gap.

- [ ] **Step 2:** `rg_path`, `missing_reference` and `reference_rg` in `tests/common/mod.rs`;
  `Fixture::rg`, `rg_compat.rs`'s `reference` and `perf.rs`'s `rg_bin` and `/usr/bin/time` check
  use them:

```diff
--- a/tests/common/mod.rs
+++ b/tests/common/mod.rs
@@ -68,13 +68,14 @@
         c
     }
 
-    /// rg 14.1.1 in the fixture copy, stdin from /dev/null, `--color never` unless set.
+    /// The reference rg (`rg_path`) in the fixture copy, stdin from /dev/null, `--color never`
+    /// unless set.
     pub fn rg<I, S>(&self, args: I) -> Command
     where
         I: IntoIterator<Item = S>,
         S: AsRef<OsStr>,
     {
-        let mut c = Command::new("rg");
+        let mut c = Command::new(rg_path());
         c.current_dir(self.path())
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(Stdio::null());
@@ -152,17 +153,38 @@
     exe
 }
 
-/// True when the reference rg 14.1.1 is installed; prints a note otherwise.
+/// The reference ripgrep: `SFOR_RG` when set, else `/usr/bin/rg` when it exists (another rg,
+/// such as 15.x, may come first on `PATH`), else `rg` on `PATH` (M3 review r1, Codex #8).
+pub fn rg_path() -> PathBuf {
+    if let Some(p) = std::env::var_os("SFOR_RG") {
+        return p.into();
+    }
+    let usr = Path::new("/usr/bin/rg");
+    if usr.is_file() {
+        return usr.to_path_buf();
+    }
+    PathBuf::from("rg")
+}
+
+/// A comparison or measurement that cannot run: a note, or a failure when
+/// `SFOR_REQUIRE_REFERENCE=1`, which acceptance runs set so that a skipped comparison never
+/// counts as passed (M3 review r1, Codex #8). Returns false.
+pub fn missing_reference(what: &str) -> bool {
+    if std::env::var_os("SFOR_REQUIRE_REFERENCE").is_some_and(|v| v == "1") {
+        panic!("{what} is required (SFOR_REQUIRE_REFERENCE=1) and missing");
+    }
+    eprintln!("skipping: {what} not found");
+    false
+}
+
+/// True when `rg_path()` is ripgrep 14.1.1; otherwise `missing_reference`.
 pub fn reference_rg() -> bool {
-    let ok = Command::new("rg")
+    let ok = Command::new(rg_path())
         .arg("--version")
         .env_remove("RIPGREP_CONFIG_PATH")
         .output()
         .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).starts_with("ripgrep 14.1.1"));
-    if !ok {
-        eprintln!("skipping: reference rg 14.1.1 not found");
-    }
-    ok
+    ok || missing_reference(&format!("reference rg 14.1.1 at {}", rg_path().display()))
 }
 
 pub fn run(c: &mut Command) -> Output {
```

```diff
--- a/tests/rg_compat.rs
+++ b/tests/rg_compat.rs
@@ -432,7 +432,7 @@
 
 /// rg 14.1.1 run with `args` and `RIPGREP_CONFIG_PATH` unset, as the reference.
 fn reference(args: &[&str]) -> Output {
-    run(Command::new("rg")
+    run(Command::new(common::rg_path())
         .args(args)
         .env_remove("RIPGREP_CONFIG_PATH")
         .stdin(Stdio::null()))
```

  and in `tests/perf.rs` the three places that skipped (`measure_to`, `rg_bin`,
  `json_without_trails_of_a_big_file_is_as_fast_as_ripgrep`):

```diff
--- a/tests/perf.rs
+++ b/tests/perf.rs
@@ -97,6 +97,7 @@
 ) -> Option<(Duration, u64)> {
     let time = Path::new("/usr/bin/time");
     if !time.exists() {
+        common::missing_reference("/usr/bin/time");
         return None;
     }
     let mut c = Command::new(time);
@@ -361,10 +362,10 @@
 #[ignore]
 fn json_without_trails_of_a_big_file_is_as_fast_as_ripgrep() {
     let _alone = alone();
-    let rg = Path::new("/usr/bin/rg");
-    if !common::reference_rg() || !rg.exists() {
+    let Some(rg) = rg_bin() else {
         return;
-    }
+    };
+    let rg = rg.as_path();
     let fx = Fixture::new();
     let line = "line of text here\n";
     let file = repeated(&fx, "ten.txt", line, 10 * 1024 * 1024 / line.len());
@@ -456,14 +457,10 @@
     }
 }
 
-/// The reference rg 14.1.1 on `PATH`, or None (with a note) when it is missing.
+/// The reference rg 14.1.1 (`common::rg_path`), or None (with a note, or a failure under
+/// `SFOR_REQUIRE_REFERENCE=1`) when it is missing.
 fn rg_bin() -> Option<std::path::PathBuf> {
-    if !common::reference_rg() {
-        return None;
-    }
-    std::env::split_paths(&std::env::var_os("PATH")?)
-        .map(|d| d.join("rg"))
-        .find(|p| p.is_file())
+    common::reference_rg().then(common::rg_path)
 }
 
 /// The fastest of three runs of `bin args` with their output read, and the largest RSS seen.
```

- [ ] **Step 3:** `SFOR_RG=/nonexistent SFOR_REQUIRE_REFERENCE=1 cargo test --test rg_compat`:
  FAILS with `reference rg 14.1.1 at /nonexistent is required (SFOR_REQUIRE_REFERENCE=1) and
  missing`. `SFOR_REQUIRE_REFERENCE=1 cargo test --workspace`: PASSES with `/usr/bin/rg` 14.1.1.
  Commit: `git add -- tests/common/mod.rs tests/rg_compat.rs tests/perf.rs` and
  `git commit -m "Compare against /usr/bin/rg 14.1.1 and fail acceptance runs that skip it"`.

## Chunk 2: markup — HTML and XML

Read spec §5.4 and design note 3 first. The three files are written in this order: the scanner
(tags only), the HTML rules, the walk. Every unit test below was run in the probe.

### Task 2.1: the tag scanner (src/processors/markup/scan.rs)

- [ ] **Step 1:** replace the stub with its tests only (the module line `//!` of Step 3 first):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn tags(src: &str, xml: bool) -> Vec<String> {
        let mut sc = Scanner::new(src.as_bytes(), xml);
        let mut out = Vec::new();
        loop {
            match sc.step() {
                Step::Tag(Tag::Start {
                    name, self_closing, ..
                }) => out.push(format!(
                    "<{}{}",
                    &src[name],
                    if self_closing { "/" } else { "" }
                )),
                Step::Tag(Tag::End { name, .. }) => out.push(format!("</{}", &src[name])),
                Step::Eof => return out,
                Step::Error(at) => {
                    out.push(format!("error at {at}"));
                    return out;
                }
            }
        }
    }

    #[test]
    fn html_skips_comments_doctypes_and_text_angles() {
        assert_eq!(
            tags("<!DOCTYPE html><!-- <b> --><p>a < b<br/></p>", false),
            ["<p", "<br/", "</p"]
        );
        // A quote opens a value only after `=`: `it's` does not hide the `>`.
        assert_eq!(tags("<a title=it's>x</a><i>", false), ["<a", "</a", "<i"]);
        assert_eq!(tags(r#"<a href="x>y">z</a>"#, false), ["<a", "</a"]);
        // A `/` inside an unquoted value is not a self-closing slash (M3 review r1, Fable #5).
        assert_eq!(
            tags("<a href=/x/>z</a><a href=/>z</a>", false),
            ["<a", "</a", "<a", "</a"]
        );
        assert_eq!(tags("<br/><x a=1 />", false), ["<br/", "<x/"]);
        // The file ends inside a comment or a tag: what came before stays.
        assert_eq!(tags("<p>x<!-- open", false), ["<p"]);
        assert_eq!(tags("<p>x<a href='", false), ["<p"]);
    }

    #[test]
    fn xml_skips_cdata_pis_and_doctype_subsets() {
        assert_eq!(
            tags(
                "<?xml version='1.0'?><!DOCTYPE r [<!ENTITY e '>'>]><r><![CDATA[<x>]]><x:y/></r>",
                true
            ),
            ["<r", "<x:y/", "</r"]
        );
    }

    #[test]
    fn xml_reports_where_it_stops_being_well_formed() {
        assert_eq!(tags("<r>a < b</r>", true), ["<r", "error at 5"]);
        assert_eq!(tags("<r><!-- open", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r><!ELEMENT x></r>", true), ["<r", "error at 3"]);
    }

    #[test]
    fn a_doctype_ends_outside_its_literals_and_comments() {
        assert_eq!(
            tags(
                r#"<!DOCTYPE r [<!ENTITY e "]> <fake>"><!-- ]> --><?p ]>?>]><r>x</r>"#,
                true
            ),
            ["<r", "</r"]
        );
        assert_eq!(tags("<!DOCTYPE r SYSTEM 'a>b.dtd'><r/>", true), ["<r/"]);
    }

    #[test]
    fn xml_tags_must_be_well_formed() {
        assert_eq!(tags("<r><a=b>x</a=b></r>", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r><a x=bare>x</a></r>", true), ["<r", "error at 3"]);
        assert_eq!(
            tags("<r><a x='1'y='2'>x</a></r>", true),
            ["<r", "error at 3"]
        );
        assert_eq!(tags("<r><a x=\"<\">x</a></r>", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r></r junk>", true), ["<r", "error at 3"]);
        assert_eq!(
            tags("<r a = 'x' b=\"y\" ><s.t-1 /></r >", true),
            ["<r", "<s.t-1/", "</r"]
        );
        // XML's names and spaces, and no attribute twice (M3 review r2, Codex #3).
        assert_eq!(tags("<r><©>x</©></r>", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r><a\x0cb='x'>x</a></r>", true), ["<r", "error at 3"]);
        assert_eq!(
            tags("<r><a x='1' x='2'>x</a></r>", true),
            ["<r", "error at 3"]
        );
        assert_eq!(
            tags("<r><ñandú·x ü='1'>x</ñandú·x></r>", true),
            ["<r", "<ñandú·x", "</ñandú·x", "</r"]
        );
        assert_eq!(tags("<r><·a/></r>", true), ["<r", "error at 3"]);
        // HTML stays tolerant.
        assert_eq!(tags("<p a=bare b>x</p junk>", false), ["<p", "</p"]);
    }

    #[test]
    fn raw_text_ends_at_its_own_end_tag_only() {
        let src = "<script>if (a<b) x = '</div>';</SCRIPT ><p>";
        let mut sc = Scanner::new(src.as_bytes(), false);
        assert!(matches!(sc.step(), Step::Tag(Tag::Start { .. })));
        sc.skip_raw_text(b"script");
        assert_eq!(sc.pos(), src.find("</SCRIPT").unwrap());
        assert!(matches!(sc.step(), Step::Tag(Tag::End { .. })));
        assert!(matches!(sc.step(), Step::Tag(Tag::Start { .. })));
    }

    #[test]
    fn attributes_by_name() {
        let src = br#"<div data-x="id=1" ID='main' class = "a  b" hidden>"#;
        let tag = 0..src.len();
        let get = |a: &[u8], xml| {
            attribute(src, &tag, 4, a, xml)
                .map(|r| std::str::from_utf8(&src[r]).unwrap().to_string())
        };
        assert_eq!(get(b"id", false).as_deref(), Some("main"));
        assert_eq!(get(b"id", true), None); // XML compares names with case
        assert_eq!(get(b"class", false).as_deref(), Some("a  b"));
        assert_eq!(get(b"hidden", false), None);
        assert_eq!(attribute(b"<p id=x>", &(0..8), 2, b"id", false), Some(6..7));
    }
}
```

- [ ] **Step 2:** `cargo test --lib markup::scan`: FAILS to compile (`Scanner`, `Step`, `Tag`,
  `attribute` do not exist). That is the RED.

- [ ] **Step 3:** the scanner, above the tests:

```rust
//! The tags of HTML and XML markup, in file order (spec §5.4): just what Trails need. Text,
//! comments, CDATA sections, processing instructions and doctypes are skipped.
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    /// `<name …>` or `<name …/>`: `tag` covers it from `<` to `>`.
    Start {
        name: Range<usize>,
        tag: Range<usize>,
        self_closing: bool,
    },
    /// `</name …>`.
    End {
        name: Range<usize>,
        tag: Range<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Tag(Tag),
    /// No more tags.
    Eof,
    /// XML only: the markup is not well formed from this byte on (spec §5.4).
    Error(usize),
}

pub struct Scanner<'a> {
    src: &'a [u8],
    pos: usize,
    xml: bool,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// The end of the XML Name that starts at `i`: a NameStartChar, then NameChars, as XML 1.0
/// (fifth edition) defines them over Unicode; Err when there is none or the bytes are not
/// UTF-8 (M3 review r2, Codex #3: `<©>` is no tag).
fn xml_name_end(s: &[u8], mut i: usize) -> Result<usize, ()> {
    let start = i;
    while i < s.len() {
        // ASCII without decoding: the common case.
        if s[i] < 0x80 {
            let b = s[i];
            let ok = b.is_ascii_alphabetic()
                || b == b'_'
                || b == b':'
                || (i > start && (b.is_ascii_digit() || b == b'-' || b == b'.'));
            if !ok {
                break;
            }
            i += 1;
            continue;
        }
        let len = match s[i] {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => return Err(()),
        };
        let c = std::str::from_utf8(s.get(i..i + len).ok_or(())?)
            .map_err(|_| ())?
            .chars()
            .next()
            .unwrap();
        let ok = if i == start {
            xml_name_start(c)
        } else {
            xml_name_char(c)
        };
        if !ok {
            break;
        }
        i += len;
    }
    if i == start { Err(()) } else { Ok(i) }
}

fn xml_name_start(c: char) -> bool {
    matches!(c, ':' | 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

fn xml_name_char(c: char) -> bool {
    xml_name_start(c)
        || matches!(
            c,
            '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}'
        )
}

/// Where a tag name may start: an ASCII letter in HTML; in XML also `_`, `:` and any non-ASCII
/// byte (names are UTF-8).
fn name_start(b: u8, xml: bool) -> bool {
    b.is_ascii_alphabetic() || (xml && (b == b'_' || b == b':' || b >= 0x80))
}

impl<'a> Scanner<'a> {
    pub fn new(src: &'a [u8], xml: bool) -> Self {
        Self { src, pos: 0, xml }
    }

    /// The byte after `pat`, searched from `from`; None when the file ends first.
    fn after(&self, from: usize, pat: &[u8]) -> Option<usize> {
        memchr::memmem::find(&self.src[from.min(self.src.len())..], pat)
            .map(|i| from + i + pat.len())
    }

    /// HTML: the end of a tag whose name ends at `from` — the byte after its `>` — and whether
    /// a `/` right before that `>` makes it self-closing. Attribute values are skipped: a
    /// quote opens a value only right after `=` and optional spaces, and an unquoted value runs
    /// to the next space or `>`, so the `/`s of `href=/docs/` are part of it (M3 review r1,
    /// Fable #5).
    fn tag_end(&self, from: usize) -> Option<(usize, bool)> {
        let s = self.src;
        let mut i = from;
        let mut after_eq = false;
        let mut slash = false;
        while i < s.len() {
            let b = s[i];
            if after_eq && (b == b'"' || b == b'\'') {
                i = memchr::memchr(b, &s[i + 1..]).map(|k| i + 1 + k)?;
                after_eq = false;
                slash = false;
            } else if after_eq && !is_space(b) && b != b'>' {
                while i < s.len() && !is_space(s[i]) && s[i] != b'>' {
                    i += 1;
                }
                after_eq = false;
                slash = false;
                continue;
            } else if b == b'>' {
                return Some((i + 1, slash));
            } else if b == b'=' {
                after_eq = true;
            } else if !is_space(b) {
                slash = b == b'/';
            }
            i += 1;
        }
        None
    }

    /// The byte after the `>` that ends a doctype whose keyword ends at `from`: outside quoted
    /// literals and, in the internal subset, outside comments and processing instructions, so a
    /// `]>` inside an entity value ends nothing (M3 review r1, Codex #4).
    fn doctype_end(&self, from: usize) -> Option<usize> {
        let s = self.src;
        let mut i = from;
        let mut subset = false;
        while i < s.len() {
            match s[i] {
                q @ (b'"' | b'\'') => {
                    i = memchr::memchr(q, &s[i + 1..]).map(|k| i + 1 + k)?;
                }
                b'<' if subset && s[i..].starts_with(b"<!--") => {
                    i = self.after(i + 4, b"-->")? - 1;
                }
                b'<' if subset && s[i..].starts_with(b"<?") => {
                    i = self.after(i + 2, b"?>")? - 1;
                }
                b'[' if !subset => subset = true,
                b']' if subset => subset = false,
                b'>' if !subset => return Some(i + 1),
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// XML: the byte after a well-formed tag whose name starts at `ns` (`<name (S attr S? =
    /// S? "value")* S? /?>`, or `</name S?>`); Err when it is not one.
    fn xml_tag_end(&self, ns: usize, close: bool) -> Result<usize, ()> {
        let s = self.src;
        // XML's white space: no form feed (M3 review r2, Codex #3).
        let spaces = |mut i: usize| {
            while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            i
        };
        let mut i = xml_name_end(s, ns)?;
        if close {
            i = spaces(i);
            return if s.get(i) == Some(&b'>') {
                Ok(i + 1)
            } else {
                Err(())
            };
        }
        // The attribute names seen, for duplicates; past 16 of them, also a set.
        let mut seen: Vec<std::ops::Range<usize>> = Vec::new();
        let mut set: std::collections::HashSet<&[u8]> = std::collections::HashSet::new();
        loop {
            let j = spaces(i);
            match s.get(j) {
                Some(b'>') => return Ok(j + 1),
                Some(b'/') if s.get(j + 1) == Some(&b'>') => return Ok(j + 2),
                Some(_) if j > i => {}
                _ => return Err(()),
            }
            let name_end = xml_name_end(s, j)?;
            let attr = &s[j..name_end];
            let dup = if seen.len() < 16 {
                seen.iter().any(|r| &s[r.clone()] == attr)
            } else {
                if set.is_empty() {
                    set.extend(seen.iter().map(|r| &s[r.clone()]));
                }
                !set.insert(attr)
            };
            if dup {
                return Err(());
            }
            seen.push(j..name_end);
            i = spaces(name_end);
            if s.get(i) != Some(&b'=') {
                return Err(());
            }
            i = spaces(i + 1);
            let q = *s.get(i).filter(|&&b| b == b'"' || b == b'\'').ok_or(())?;
            let v = memchr::memchr(q, &s[i + 1..]).ok_or(())? + i + 1;
            if s[i + 1..v].contains(&b'<') {
                return Err(());
            }
            i = v + 1;
        }
    }

    /// The end of the name that starts at `from`.
    fn name_end(&self, from: usize) -> usize {
        let s = self.src;
        let mut i = from;
        while i < s.len() && !is_space(s[i]) && s[i] != b'>' && s[i] != b'/' {
            i += 1;
        }
        i
    }

    /// Jumps to the `</name` that ends a raw text element (HTML's `script`, `style`, …),
    /// compared without case, or to the end of the file.
    pub fn skip_raw_text(&mut self, name: &[u8]) {
        let s = self.src;
        let mut i = self.pos;
        while let Some(k) = memchr::memchr(b'<', &s[i.min(s.len())..]) {
            let at = i + k;
            let rest = &s[at..];
            if rest.len() >= 2 + name.len()
                && rest[1] == b'/'
                && rest[2..2 + name.len()].eq_ignore_ascii_case(name)
                && rest
                    .get(2 + name.len())
                    .is_none_or(|&b| is_space(b) || b == b'>' || b == b'/')
            {
                self.pos = at;
                return;
            }
            i = at + 1;
        }
        self.pos = s.len();
    }

    /// The next tag, or the end, or (XML) the first byte that is not well formed.
    pub fn step(&mut self) -> Step {
        let s = self.src;
        loop {
            let Some(k) = memchr::memchr(b'<', &s[self.pos.min(s.len())..]) else {
                self.pos = s.len();
                return Step::Eof;
            };
            let lt = self.pos + k;
            let rest = &s[lt..];
            // In HTML a construct the file ends inside is dropped, as the HTML parser drops an
            // unfinished tag; in XML it is an error.
            let unfinished = |me: &mut Self| {
                me.pos = s.len();
                if me.xml { Step::Error(lt) } else { Step::Eof }
            };
            if rest.starts_with(b"<!--") {
                match self.after(lt + 4, b"-->") {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<![CDATA[") && self.xml {
                match self.after(lt + 9, b"]]>") {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<?") && self.xml {
                match self.after(lt + 2, b"?>") {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<!") && self.xml {
                // A doctype, with its internal subset in brackets.
                if !rest[2..].starts_with(b"DOCTYPE") {
                    self.pos = s.len();
                    return Step::Error(lt);
                }
                match self.doctype_end(lt + 9) {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
                // HTML: a doctype or a bogus comment, to the next `>`.
                match memchr::memchr(b'>', rest) {
                    Some(g) => self.pos = lt + g + 1,
                    None => return unfinished(self),
                }
                continue;
            }
            let close = rest.get(1) == Some(&b'/');
            let ns = lt + 1 + close as usize;
            if !s.get(ns).is_some_and(|&b| name_start(b, self.xml)) {
                if self.xml {
                    self.pos = s.len();
                    return Step::Error(lt);
                }
                if close {
                    // `</` and no name: a bogus comment in HTML.
                    match memchr::memchr(b'>', rest) {
                        Some(g) => self.pos = lt + g + 1,
                        None => return unfinished(self),
                    }
                } else {
                    // `<` as text.
                    self.pos = lt + 1;
                }
                continue;
            }
            let ne = self.name_end(ns);
            let (end, self_closing) = if self.xml {
                // XML: a tag must be well formed, attributes included (M3 review r1, Codex #5).
                match self.xml_tag_end(ns, close) {
                    // Values are quoted, so a `/` before the `>` can only be `/>`.
                    Ok(e) => (e, s[e - 2] == b'/'),
                    Err(()) => {
                        self.pos = s.len();
                        return Step::Error(lt);
                    }
                }
            } else {
                match self.tag_end(ne) {
                    Some(t) => t,
                    None => return unfinished(self),
                }
            };
            self.pos = end;
            let name = ns..ne;
            let tag = lt..end;
            if close {
                return Step::Tag(Tag::End { name, tag });
            }
            return Step::Tag(Tag::Start {
                name,
                tag,
                self_closing,
            });
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }
}

/// The value of attribute `attr` (compared without case in HTML) in the start tag `tag`,
/// whose name ends at `name_end`: its byte range, quotes excluded. None when absent or empty.
pub fn attribute(
    src: &[u8],
    tag: &Range<usize>,
    name_end: usize,
    attr: &[u8],
    xml: bool,
) -> Option<Range<usize>> {
    let s = &src[..tag.end];
    let mut i = name_end;
    while i < s.len() {
        while i < s.len() && (is_space(s[i]) || s[i] == b'/') {
            i += 1;
        }
        if i >= s.len() || s[i] == b'>' {
            return None;
        }
        let a = i;
        while i < s.len() && !is_space(s[i]) && !matches!(s[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let key = &s[a..i];
        while i < s.len() && is_space(s[i]) {
            i += 1;
        }
        let mut value = None;
        if i < s.len() && s[i] == b'=' {
            i += 1;
            while i < s.len() && is_space(s[i]) {
                i += 1;
            }
            if i < s.len() && (s[i] == b'"' || s[i] == b'\'') {
                let q = s[i];
                let v0 = i + 1;
                let v1 = memchr::memchr(q, &s[v0..]).map_or(s.len(), |k| v0 + k);
                value = Some(v0..v1);
                i = v1 + 1;
            } else {
                let v0 = i;
                while i < s.len() && !is_space(s[i]) && s[i] != b'>' {
                    i += 1;
                }
                value = Some(v0..i);
            }
        }
        let same = if xml {
            key == attr
        } else {
            key.eq_ignore_ascii_case(attr)
        };
        if same {
            return value.filter(|v| !v.is_empty());
        }
    }
    None
}
```

- [ ] **Step 4:** `cargo test --lib markup::scan`: 7 tests PASS. `html_skips_comments_…` holds
  review r1's Fable #5 (`<a href=/x/>` is no self-closing tag). Two tests come from review r1: `a_doctype_ends_outside_its_literals_and_comments` (Codex #4: `<!DOCTYPE r [<!ENTITY e
  "]> <fake>">]>` invented a `fake` element) and `xml_tags_must_be_well_formed` (Codex #5:
  `<a=b>` and `<a x=bare>` were taken as elements); both FAILED against the first version of
  this scanner (D30).

### Task 2.2: the HTML rules (src/processors/markup/html.rs)

- [ ] **Step 1:** the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn closed(open: &[&str], name: &str) -> usize {
        let open: Vec<Info> = open.iter().map(|s| info(s.as_bytes())).collect();
        let mut r = NO_REACH;
        for (at, i) in open.iter().enumerate() {
            r = reach(&r, i, at);
        }
        closed_by_start(open.len(), open.last(), &r, &info(name.as_bytes()))
    }

    #[test]
    fn list_items_rows_and_cells_end_their_siblings() {
        assert_eq!(closed(&["ul", "li"], "li"), 1);
        assert_eq!(closed(&["ul", "li", "b"], "LI"), 2);
        // A nested list stops the search: its first item ends nothing outside it.
        assert_eq!(closed(&["ul", "li", "ol"], "li"), 0);
        assert_eq!(closed(&["table", "tr", "td"], "td"), 1);
        assert_eq!(closed(&["table", "tr", "td"], "tr"), 2);
        assert_eq!(closed(&["table", "tbody", "tr", "td"], "tbody"), 3);
        assert_eq!(closed(&["dl", "dt"], "dd"), 1);
        assert_eq!(closed(&["select", "option"], "option"), 1);
        assert_eq!(closed(&["select", "option"], "optgroup"), 1);
        assert_eq!(closed(&["select", "optgroup", "option"], "optgroup"), 2);
    }

    #[test]
    fn block_starts_end_a_paragraph_within_scope() {
        assert_eq!(closed(&["body", "p"], "div"), 1);
        assert_eq!(closed(&["body", "p", "b"], "p"), 2);
        assert_eq!(closed(&["p", "table", "tr", "td"], "div"), 0);
        assert_eq!(closed(&["body", "p"], "span"), 0);
        assert_eq!(closed(&["body", "h2"], "h3"), 1);
    }

    #[test]
    fn classification_ignores_case_and_long_names() {
        assert!(info(b"BR").void && info(b"Script").raw_text && info(b"BODY").hidden);
        assert_eq!(info(b"a-very-long-custom-element").known, Known::Other);
    }
}
```

- [ ] **Step 2:** `cargo test --lib markup::html`: FAILS to compile.

- [ ] **Step 3:** the rules (D11, D13, D7):

```rust
//! The HTML rules the markup walk applies (spec §5.4): void elements, raw text elements, and
//! the end tags HTML lets an author leave out. A subset of the HTML parsing algorithm: it closes
//! elements, and never inserts one (no implied `tbody`, `head` or `body`). Each tag name is
//! classified once, when its start tag is read (`Info`), so the rules cost a few comparisons.

/// The tags the rules name, as their own value; every other tag is `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    Li,
    Dt,
    Dd,
    Tr,
    Td,
    Th,
    Thead,
    Tbody,
    Tfoot,
    Option,
    Optgroup,
    P,
    Ul,
    Ol,
    Menu,
    Dl,
    Table,
    Template,
    Select,
    Datalist,
    Heading,
    Other,
}

/// What the rules need to know about a tag name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub known: Known,
    /// Its start tag closes an open `p` (HTML's «close a p element» list).
    pub closes_p: bool,
    /// The search for a `p` to close stops at it.
    pub scope: bool,
    /// It never has content.
    pub void: bool,
    /// Its content is text up to its own end tag.
    pub raw_text: bool,
    /// A hidden Segment, not searchable by name: it holds the whole page (M3 plan, D7).
    pub hidden: bool,
    /// Bit `k`: it is what rule `k` ends, or what stops rule `k`'s search (`RULES`, `P_RULE`).
    targets: u8,
    stops: u8,
}

/// Classifies a tag name, without case.
pub fn info(name: &[u8]) -> Info {
    let mut buf = [0u8; 12];
    let mut i = Info {
        known: Known::Other,
        closes_p: false,
        scope: false,
        void: false,
        raw_text: false,
        hidden: false,
        targets: 0,
        stops: 0,
    };
    if name.len() > buf.len() {
        return i;
    }
    let lower = &mut buf[..name.len()];
    lower.copy_from_slice(name);
    lower.make_ascii_lowercase();
    i.known = match &*lower {
        b"li" => Known::Li,
        b"dt" => Known::Dt,
        b"dd" => Known::Dd,
        b"tr" => Known::Tr,
        b"td" => Known::Td,
        b"th" => Known::Th,
        b"thead" => Known::Thead,
        b"tbody" => Known::Tbody,
        b"tfoot" => Known::Tfoot,
        b"option" => Known::Option,
        b"optgroup" => Known::Optgroup,
        b"p" => Known::P,
        b"ul" => Known::Ul,
        b"ol" => Known::Ol,
        b"menu" => Known::Menu,
        b"dl" => Known::Dl,
        b"table" => Known::Table,
        b"template" => Known::Template,
        b"select" => Known::Select,
        b"datalist" => Known::Datalist,
        b"h1" | b"h2" | b"h3" | b"h4" | b"h5" | b"h6" => Known::Heading,
        _ => Known::Other,
    };
    i.closes_p = matches!(
        &*lower,
        b"address"
            | b"article"
            | b"aside"
            | b"blockquote"
            | b"details"
            | b"dialog"
            | b"dd"
            | b"div"
            | b"dl"
            | b"dt"
            | b"fieldset"
            | b"figcaption"
            | b"figure"
            | b"footer"
            | b"form"
            | b"h1"
            | b"h2"
            | b"h3"
            | b"h4"
            | b"h5"
            | b"h6"
            | b"header"
            | b"hgroup"
            | b"hr"
            | b"li"
            | b"main"
            | b"menu"
            | b"nav"
            | b"ol"
            | b"p"
            | b"pre"
            | b"section"
            | b"table"
            | b"ul"
    );
    i.scope = matches!(
        &*lower,
        b"table" | b"td" | b"th" | b"caption" | b"template" | b"button" | b"html" | b"object"
    );
    i.void = matches!(
        &*lower,
        b"area"
            | b"base"
            | b"br"
            | b"col"
            | b"embed"
            | b"hr"
            | b"img"
            | b"input"
            | b"link"
            | b"meta"
            | b"param"
            | b"source"
            | b"track"
            | b"wbr"
    );
    i.raw_text = matches!(
        &*lower,
        b"script"
            | b"style"
            | b"textarea"
            | b"title"
            | b"xmp"
            | b"iframe"
            | b"noembed"
            | b"noframes"
            | b"plaintext"
    );
    i.hidden = matches!(&*lower, b"html" | b"body");
    let (targets, stops) = masks()[i.known as usize];
    i.targets = targets;
    i.stops = stops;
    i.stops |= (i.scope as u8) << P_RULE;
    i
}

/// The implied-end rules that search the open elements: what a start tag ends, and what stops
/// the search (spec §5.4). `P` is the «close a p element» rule, which `scope` elements stop.
const RULES: [(&[Known], &[Known]); 7] = {
    use Known::*;
    [
        (&[Li], &[Ul, Ol, Menu, Table, Td, Th, Template]),
        (&[Dt, Dd], &[Dl, Table, Td, Th, Template]),
        (&[Tr], &[Table, Template]),
        (&[Td, Th], &[Tr, Table, Template]),
        (&[Thead, Tbody, Tfoot], &[Table, Template]),
        (&[Option], &[Select, Datalist, Optgroup]),
        (&[Optgroup], &[Select]),
    ]
};
const P_RULE: usize = RULES.len();

/// For each rule, the 0-based position among the open elements of the innermost one that rule
/// would end, when no stop lies between it and the current element. Kept per open element and
/// computed when it opens (`reach`), so a start tag finds what it ends in constant time, however
/// deep the elements are (M3 review r2, Codex #1: 40 000 nested `div`s searched the whole stack
/// for a `p` each).
/// `NONE` where nothing is reachable.
pub type Reach = [u32; RULES.len() + 1];
pub const NONE: u32 = u32::MAX;
pub const NO_REACH: Reach = [NONE; RULES.len() + 1];

/// The `Reach` of an element at position `at` with tag `info`, opened inside `parent`'s.
pub fn reach(parent: &Reach, info: &Info, at: usize) -> Reach {
    let mut r = *parent;
    if info.targets | info.stops == 0 {
        return r;
    }
    for (k, slot) in r.iter_mut().enumerate() {
        if info.targets >> k & 1 == 1 {
            *slot = at as u32;
        } else if info.stops >> k & 1 == 1 {
            *slot = NONE;
        }
    }
    r
}

/// For each `Known`, the bits of the rules it is a target or a stop of (`RULES`), computed once.
fn masks() -> &'static [(u8, u8); Known::Other as usize + 1] {
    static MASKS: std::sync::OnceLock<[(u8, u8); Known::Other as usize + 1]> =
        std::sync::OnceLock::new();
    MASKS.get_or_init(|| {
        let mut m = [(0u8, 0u8); Known::Other as usize + 1];
        for (k, (targets, stops)) in RULES.iter().enumerate() {
            for t in *targets {
                m[*t as usize].0 |= 1 << k;
            }
            for t in *stops {
                m[*t as usize].1 |= 1 << k;
            }
        }
        m[Known::P as usize].0 |= 1 << P_RULE;
        m
    })
}

/// How many of the `open` open elements the start tag `start` closes, given the current
/// element `top` and its `Reach`: the innermost ones, up to and including the element it ends
/// implicitly. 0 when it ends none.
pub fn closed_by_start(open: usize, top: Option<&Info>, reach: &Reach, start: &Info) -> usize {
    use Known::*;
    let rule = match start.known {
        Li => Some(0),
        Dt | Dd => Some(1),
        Tr => Some(2),
        Td | Th => Some(3),
        Thead | Tbody | Tfoot => Some(4),
        Option => Some(5),
        Optgroup => Some(6),
        _ => None,
    };
    let ends = |k: usize| (reach[k] != NONE).then(|| open - reach[k] as usize);
    if let Some(n) = rule.and_then(ends) {
        return n;
    }
    if start.closes_p
        && let Some(n) = ends(P_RULE)
    {
        return n;
    }
    // An `optgroup` that closes no group still ends an option that is the current element
    // (M3 review r1, Codex #9).
    if start.known == Optgroup && top.is_some_and(|el| el.known == Option) {
        return 1;
    }
    // A heading start tag closes a heading that is the current element.
    if start.known == Heading && top.is_some_and(|el| el.known == Heading) {
        return 1;
    }
    0
}
```

- [ ] **Step 4:** `cargo test --lib markup::html`: 3 tests PASS (the `optgroup` asserts are
  review r1's Codex #9).

### Task 2.3: the fixtures

**Files:** `tests/fixtures/markup/{page.html,feed.xml,broken.xml}`. Each `needle` is listed with
its line, counted with `grep -n needle` in the probe.

- [ ] **Step 1:** `tests/fixtures/markup/page.html` (30 lines; `needle` on lines 4, 5, 12, 19, 23,
  26, 27):

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <title>Pricing needle</title>
  <script>if (a < b) { label = "</div> needle"; }</script>
</head>
<body>
  <nav class="top menu">
    <ul>
      <li><a href="/">Home</a>
      <li><a href="/docs">Docs</a>
      <li><a href="/needle">Needle</a>
    </ul>
  </nav>
  <main id="content">
    <section id="pricing" class="card wide">
      <h2>Plans</h2>
      <p>Pick one.<br>
      <p class="note">The needle plan is free.
      <table>
        <tr><th>Plan<th>Price
        <tr><td>Free<td>0
        <tr><td>Pro<td>needle 9
      </table>
    </section>
    <div><div><div>deep needle</div></div></div>
    <img src="needle.png" alt="">
  </main>
</body>
</html>
```

- [ ] **Step 2:** `tests/fixtures/markup/feed.xml` (18 lines; `needle` on lines 3, 6, 12, 14, 17):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE catalog [
  <!ENTITY pub "needle press">
]>
<catalog xmlns:dc="http://purl.org/dc/elements/1.1/">
  <!-- needle in a comment -->
  <book id="b1">
    <title>First</title>
    <dc:creator>Ana</dc:creator>
  </book>
  <book id="b2">
    <title>The needle</title>
    <dc:creator>Bea</dc:creator>
    <note><![CDATA[ a < b needle ]]></note>
    <empty/>
  </book>
  <magazine><title>needle monthly</title></magazine>
</catalog>
```

- [ ] **Step 3:** `tests/fixtures/markup/broken.xml` (5 lines; `needle` on lines 2, 3, 4; the
  `</a>` of line 3 does not close the current `b`, so the structure ends there):

```xml
<root>
  <ok>needle before</ok>
  <a><b>needle inside</a>
  <c>needle after</c>
</root>
```

- [ ] **Step 4:** check: `grep -n needle tests/fixtures/markup/*` prints exactly the lines above,
  and `wc -l` gives 30, 18 and 5.

### Task 2.4: the walk and the Processor (src/processors/markup/mod.rs)

- [ ] **Step 1:** keep the stub's `Processor` impl and add the tests at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn params(syntax: &str) -> toml::Table {
        toml::from_str(&format!("syntax = '{syntax}'")).unwrap()
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/markup")
                .join(name),
        )
        .unwrap()
    }

    /// (line, Trail names) of every `needle` in `src`.
    fn needles(src: &[u8], syntax: &str) -> Vec<(usize, String)> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: syntax,
            disk: None,
        };
        let t = Markup.trails(&input, &params(syntax), &anchors).unwrap();
        anchors
            .iter()
            .zip(t)
            .map(|(&a, t)| {
                let line = src[..a].iter().filter(|&&b| b == b'\n').count() + 1;
                (
                    line,
                    t.iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" › "),
                )
            })
            .collect()
    }

    fn pairs(v: &[(usize, &str)]) -> Vec<(usize, String)> {
        v.iter().map(|(l, s)| (*l, s.to_string())).collect()
    }

    #[test]
    fn html_element_paths() {
        assert_eq!(
            needles(&fixture("page.html"), "html"),
            pairs(&[
                (4, "head › title"),
                (5, "head › script"),
                (12, "nav.top.menu › ul › li[3]"),
                (19, "main#content › section#pricing.card.wide › p.note[2]"),
                (
                    23,
                    "main#content › section#pricing.card.wide › table › tr[3] › td[2]"
                ),
                (26, "main#content › div › div › div"),
                (27, "main#content"),
            ])
        );
    }

    #[test]
    fn html_sections_end_where_html_ends_them() {
        let src = fixture("page.html");
        let at = memchr::memmem::find(&src, b"needle plan").unwrap();
        let input = Input {
            bytes: &src,
            path: Path::new("x"),
            format: "html",
            disk: None,
        };
        let t = Markup.trails(&input, &params("html"), &[at]).unwrap();
        let lines: Vec<(&str, &str, u64, u64)> = t[0]
            .iter()
            .map(|s| (s.kind.as_str(), s.name.as_str(), s.line, s.end_line))
            .collect();
        assert_eq!(
            lines,
            [
                ("tag", "main#content", 15, 28),
                ("tag", "section#pricing.card.wide", 16, 25),
                // The `<table>` start tag ends the open `p` (HTML's rules), on line 19.
                ("tag", "p.note[2]", 19, 19),
            ]
        );
    }

    #[test]
    fn xml_element_paths() {
        assert_eq!(
            needles(&fixture("feed.xml"), "xml"),
            pairs(&[
                (3, ""),
                (6, "catalog"),
                (12, "catalog › book[2] › title"),
                (14, "catalog › book[2] › note"),
                (17, "catalog › magazine › title"),
            ])
        );
    }

    #[test]
    fn xml_keeps_the_trails_before_a_syntax_error() {
        assert_eq!(
            needles(&fixture("broken.xml"), "xml"),
            pairs(&[(2, "root › ok"), (3, "root › a › b"), (4, "")])
        );
    }

    #[test]
    fn xml_doctypes_and_malformed_tags_do_not_invent_elements() {
        // M3 review r1, Codex #4 and #5.
        assert_eq!(
            needles(
                br#"<!DOCTYPE r [<!ENTITY e "]> <fake>">]><r>needle</r>"#,
                "xml"
            ),
            pairs(&[(1, "r")])
        );
        assert_eq!(
            needles(
                b"<r><ok>needle</ok><a=b>needle</a=b><c x=bare>needle</c></r>",
                "xml"
            ),
            pairs(&[(1, "r › ok"), (1, ""), (1, "")])
        );
        // M3 review r2, Codex #3: a name XML does not allow, a form feed, a repeated attribute.
        for bad in [
            "<©>needle</©>",
            "<a\x0cb='x'>needle</a>",
            "<a x='1' x='2'>needle</a>",
        ] {
            let src = format!("<r><ok>needle</ok>{bad}</r>");
            assert_eq!(
                needles(src.as_bytes(), "xml"),
                pairs(&[(1, "r › ok"), (1, "")]),
                "{bad}"
            );
        }
    }

    #[test]
    fn an_optgroup_ends_the_open_option() {
        // M3 review r1, Codex #9.
        let src = b"<select><option>first<optgroup label=x><option>needle</optgroup><optgroup><option>needle</select>";
        assert_eq!(
            needles(src, "html"),
            pairs(&[
                (1, "select › optgroup[1] › option"),
                (1, "select › optgroup[2] › option")
            ])
        );
    }

    #[test]
    fn many_distinct_sibling_tags_keep_their_indexes() {
        // Past LINEAR distinct tags the index finds the slot (M3 review r1, Codex #1).
        let mut src = String::from("<r>");
        for i in 0..40 {
            src.push_str(&format!("<t{i}/><T{i}/>"));
        }
        src.push_str("<t7>needle</t7><x>needle</x></r>");
        assert_eq!(
            needles(src.as_bytes(), "xml"),
            pairs(&[(1, "r › t7[2]"), (1, "r › x")])
        );
        let html = src.replace("<r>", "<div>").replace("</r>", "</div>");
        // HTML compares tags without case: t7, T7 and the last t7 are three.
        assert_eq!(
            needles(html.as_bytes(), "html"),
            pairs(&[(1, "div › t7[3]"), (1, "div › x")])
        );
    }

    #[test]
    fn names_are_searchable_with_their_pieces() {
        let src = br#"<div id="main" class="a b"><p>x</p><p>y</p></div>"#;
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: "html",
            disk: None,
        };
        let n = Markup.names(&input, &params("html")).unwrap();
        let got: Vec<(&str, Pieces)> = n
            .iter()
            .map(|n| (n.text.as_str(), n.pieces.clone()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "div#main.a.b",
                    vec![
                        (0..3, 1..4),
                        (4..8, 9..13),
                        (9..10, 22..23),
                        (11..12, 24..25)
                    ]
                ),
                ("p[1]", vec![(0..1, 28..29)]),
                ("p[2]", vec![(0..1, 36..37)]),
            ]
        );
    }

    #[test]
    fn html_and_body_are_not_names() {
        let src = b"<html><body><p>x</p></body></html>";
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: "html",
            disk: None,
        };
        let n = Markup.names(&input, &params("html")).unwrap();
        let texts: Vec<&str> = n.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["p"]);
    }

    #[test]
    fn the_sibling_index_agrees_with_the_linear_count() {
        // Fable r1 #1: 20 distinct tags cross LINEAR, then `a` again is `a[2]`.
        let mut src = String::from("<r><a>needle</a>");
        for i in 0..20 {
            src.push_str(&format!("<t{i}/>"));
        }
        src.push_str("<a>needle</a></r>");
        assert_eq!(
            needles(src.as_bytes(), "xml"),
            pairs(&[(1, "r › a[1]"), (1, "r › a[2]")])
        );
    }

    #[test]
    fn long_names_are_cut_and_deep_nesting_stops_opening_segments() {
        let classes: String = (0..30).map(|i| format!(" c{i}")).collect();
        let src = format!("<div class=\"{classes}\">needle</div>");
        let got = needles(src.as_bytes(), "html");
        assert_eq!(got[0].1.chars().count(), 40);
        assert!(got[0].1.ends_with('…'), "{got:?}");
        let deep = format!("{}needle{}", "<x>".repeat(100_000), "</x>".repeat(100_000));
        let t = needles(deep.as_bytes(), "xml");
        assert_eq!(t[0].1.matches('›').count() + 1, MAX_DEPTH);
    }
}
```

- [ ] **Step 2:** `cargo test --lib processors::markup::tests`: FAILS (compile errors for
  `MAX_DEPTH`, and the stub's `Unavailable` once it compiles).

- [ ] **Step 3:** replace everything above the tests with the walk (design note 3, D2, D7–D9,
  D12, D13, D21, D22, D24, D25):

```rust
//! markup: HTML and XML element paths (spec §5.4, M3 plan design note 3). One walk over the
//! tags feeds keypath's Collector the elements that hold an anchor, so a big file with three
//! Matches keeps three short stacks.
pub mod html;
pub mod scan;

use super::Processor;
use crate::model::{Input, Name, Trail};
use crate::processors::keypath::collect::Collector;
use crate::trail::{Outline, OutlineSegment, TrailCursor, names_of};
use scan::{Scanner, Step, Tag};
use std::collections::HashMap;
use std::ops::Range;

use crate::text::Pieces;

pub struct Markup;

/// Past this many open elements no more Segments open (as JSON's 256, M2 plan D34).
pub const MAX_DEPTH: usize = 256;
pub const DEFAULT_NAME_CHARS: usize = 40;
/// Distinct child tags a parent compares one by one before it builds an index.
const LINEAR: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    Html,
    Xml,
}

struct Params {
    syntax: Syntax,
    name_chars: usize,
}

fn params(t: &toml::Table) -> anyhow::Result<Params> {
    let syntax = match t.get("syntax").and_then(|v| v.as_str()) {
        Some("html") => Syntax::Html,
        Some("xml") => Syntax::Xml,
        _ => anyhow::bail!("markup needs `syntax` = \"html\" or \"xml\" in [params]"),
    };
    let name_chars = t
        .get("name_chars")
        .and_then(|v| v.as_integer())
        .filter(|&n| n > 0)
        .map_or(DEFAULT_NAME_CHARS, |n| n as usize);
    Ok(Params { syntax, name_chars })
}

/// One open element of the walk.
struct Frame {
    /// The tag name in the Source.
    name: Range<usize>,
    /// The start tag, `<` to `>`.
    tag: Range<usize>,
    /// HTML: its tag classified once (`html::info`).
    info: html::Info,
    /// HTML: what the implied-end rules would end from here (`html::reach`).
    reach: html::Reach,
    /// HTML: the id of its tag in `Walk::ids`.
    id: u32,
    /// It may become a Segment (not the root, not past MAX_DEPTH).
    seg: bool,
    /// Its place among its parent's children of the same tag: (slot in the parent's
    /// `counts`, 1-based ordinal).
    slot: usize,
    ordinal: u32,
    /// Children seen so far, by tag name: (a name of that tag, count).
    counts: Vec<(Range<usize>, u32)>,
    /// Past `LINEAR` distinct tags, the slot in `counts` of each tag's key (`key`), so a parent
    /// with thousands of distinct child tags costs constant time per child (M3 review r1,
    /// Codex #1). Memory: one entry per distinct child tag of each open element.
    index: Option<Map<usize>>,
    /// Closed children kept as Segments and without an id, which get `[n]` when their tag
    /// repeats: they reach the Collector when this element closes and the counts are final.
    pending: Vec<(OutlineSegment, usize, u32)>,
}

impl Frame {
    fn root() -> Self {
        Frame {
            name: 0..0,
            tag: 0..0,
            info: html::info(b""),
            reach: html::NO_REACH,
            id: 0,
            seg: false,
            slot: 0,
            ordinal: 0,
            counts: Vec::new(),
            index: None,
            pending: Vec::new(),
        }
    }
}

/// A name's text and pieces from the Source bytes `r` (`text::push_lossy`).
fn push_piece(src: &[u8], r: Range<usize>, text: &mut String, pieces: &mut Pieces) {
    crate::text::push_lossy(text, pieces, src, r);
}

/// The walk builds a Segment only when its element closes and holds an anchor (`keeps`), so
/// the elements without Matches cost no allocation (M3 plan, design note 3).
struct Walk<'s, 'c, 'a> {
    src: &'s [u8],
    p: Params,
    c: &'c mut Collector<'a>,
    stack: Vec<Frame>,
    /// HTML: an id per distinct tag (`fill_key`) of the file, and how many elements of each are
    /// open: an end tag with none open is ignored at once instead of searching the stack (M3
    /// review r2, Codex #1). One allocation per distinct tag; a closing element finds its count
    /// by the id its Frame keeps.
    ids: Map<u32>,
    open: Vec<u32>,
    /// The key being looked up, reused.
    buf: Vec<u8>,
    /// The last `at` that `close_implied` trimmed back from, and where white space before it
    /// starts: many elements ended at one byte trim its white space once.
    trimmed: (usize, usize),
}

/// FNV-1a: the maps of the walk hash short tag names, where SipHash's cost per lookup showed in
/// the walk's time; nothing in them comes from an attacker who can choose collisions that
/// matter more than a slower walk.
#[derive(Default)]
struct Fnv(u64);

impl std::hash::Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        };
        for &b in bytes {
            h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = h;
    }
}

type Map<V> = HashMap<Box<[u8]>, V, std::hash::BuildHasherDefault<Fnv>>;

/// The key of a tag in `buf`: its bytes, lowercased in HTML.
fn fill_key(buf: &mut Vec<u8>, name: &[u8], syntax: Syntax) {
    buf.clear();
    buf.extend_from_slice(name);
    if syntax == Syntax::Html {
        buf.make_ascii_lowercase();
    }
}

impl Walk<'_, '_, '_> {
    fn same(&self, a: &Range<usize>, b: &Range<usize>) -> bool {
        let (a, b) = (&self.src[a.clone()], &self.src[b.clone()]);
        match self.p.syntax {
            Syntax::Html => a.eq_ignore_ascii_case(b),
            Syntax::Xml => a == b,
        }
    }

    /// `tag#id.class1.class2` (HTML) or the tag (XML), cut at `name_chars` characters, with
    /// its pieces (spec §5.4). Returns whether the element has an id.
    fn name_of(&self, f: &Frame) -> (String, Pieces, bool) {
        let src = self.src;
        let mut text = String::new();
        let mut pieces = Vec::new();
        push_piece(src, f.name.clone(), &mut text, &mut pieces);
        let mut has_id = false;
        if self.p.syntax == Syntax::Html {
            if let Some(id) = scan::attribute(src, &f.tag, f.name.end, b"id", false) {
                has_id = true;
                text.push('#');
                push_piece(src, id, &mut text, &mut pieces);
            }
            if let Some(cls) = scan::attribute(src, &f.tag, f.name.end, b"class", false) {
                let mut i = cls.start;
                while i < cls.end {
                    while i < cls.end && src[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let s = i;
                    while i < cls.end && !src[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if s < i {
                        text.push('.');
                        push_piece(src, s..i, &mut text, &mut pieces);
                    }
                }
            }
        }
        crate::text::cut_name(&mut text, &mut pieces, self.p.name_chars);
        (text, pieces, has_id)
    }

    /// Counts a child named `name` in the current element; returns its slot in `counts`.
    fn slot(&mut self, name: &Range<usize>) -> usize {
        let src = self.src;
        let syntax = self.p.syntax;
        let indexed = self.stack.last().unwrap().counts.len() >= LINEAR;
        if indexed {
            fill_key(&mut self.buf, &src[name.clone()], syntax);
        }
        let buf = &self.buf;
        let parent = self.stack.last_mut().unwrap();
        let found = match (indexed, &parent.index) {
            (true, Some(ix)) => ix.get(&buf[..]).copied(),
            _ => parent.counts.iter().position(|(n, _)| {
                let (a, b) = (&src[n.clone()], &src[name.clone()]);
                if syntax == Syntax::Html {
                    a.eq_ignore_ascii_case(b)
                } else {
                    a == b
                }
            }),
        };
        if let Some(k) = found {
            parent.counts[k].1 += 1;
            return k;
        }
        parent.counts.push((name.clone(), 1));
        let k = parent.counts.len() - 1;
        if indexed {
            let ix = parent.index.get_or_insert_with(|| {
                // Index the tags counted so far, then this one.
                let mut ix = Map::default();
                let mut key = Vec::new();
                for (j, (n, _)) in parent.counts[..k].iter().enumerate() {
                    fill_key(&mut key, &src[n.clone()], syntax);
                    ix.insert(key.clone().into_boxed_slice(), j);
                }
                ix
            });
            ix.insert(buf.clone().into_boxed_slice(), k);
        }
        k
    }

    /// HTML: the id of the tag `name`, given one when it is new.
    fn tag_id(&mut self, name: &Range<usize>) -> u32 {
        fill_key(&mut self.buf, &self.src[name.clone()], self.p.syntax);
        if let Some(&id) = self.ids.get(&self.buf[..]) {
            return id;
        }
        let id = self.open.len() as u32;
        self.ids.insert(self.buf.clone().into_boxed_slice(), id);
        self.open.push(0);
        id
    }

    /// Closes the innermost open element at `end` (exclusive).
    fn close(&mut self, end: usize) {
        let mut f = self.stack.pop().expect("the root is never closed here");
        if self.p.syntax == Syntax::Html {
            self.open[f.id as usize] -= 1;
        }
        self.flush(&mut f);
        if f.seg && self.c.keeps(f.tag.start, end) {
            let (text, pieces, has_id) = self.name_of(&f);
            let mut s = OutlineSegment::new("tag", text, f.tag.start..end, f.tag.clone());
            // `html` and `body` are hidden and not searchable by name (D7, M3 review r1,
            // Fable #6).
            s.name = (!f.info.hidden).then_some(pieces);
            s.name_anchor = f.name.start;
            s.hidden = f.info.hidden;
            if has_id {
                self.c.closed(s, end);
            } else {
                let parent = self.stack.last_mut().unwrap();
                parent.pending.push((s, f.slot, f.ordinal));
            }
        }
    }

    /// Hands `f`'s pending children to the Collector, with `[n]` on those whose tag repeats
    /// among their siblings (spec §5.4).
    fn flush(&mut self, f: &mut Frame) {
        for (mut s, slot, ordinal) in std::mem::take(&mut f.pending) {
            if f.counts[slot].1 >= 2 {
                s.segment.name.push_str(&format!("[{ordinal}]"));
            }
            let end = s.range.end;
            self.c.closed(s, end);
        }
    }

    /// Closes the innermost open element where something else ends it (a start tag HTML lets
    /// end it, an ancestor's end tag, the end of the file or an XML error at `at`): its Section
    /// ends after its last byte that is not white space, so it ends on its own last line.
    fn close_implied(&mut self, at: usize) {
        let floor = self.stack.last().map_or(0, |f| f.tag.end);
        if self.trimmed.0 != at {
            let mut t = at;
            while t > 0 && self.src[t - 1].is_ascii_whitespace() {
                t -= 1;
            }
            self.trimmed = (at, t);
        }
        // Trimming back from `at` and stopping at `floor` ends at the larger of the two.
        let end = if at <= floor {
            floor
        } else {
            self.trimmed.1.max(floor)
        };
        self.close(end);
    }

    fn start(
        &mut self,
        name: Range<usize>,
        tag: Range<usize>,
        self_closing: bool,
        sc: &mut Scanner,
    ) {
        let html = self.p.syntax == Syntax::Html;
        let info = html::info(if html { &self.src[name.clone()] } else { b"" });
        if html {
            let top = self.stack.last().unwrap();
            let open = self.stack.len() - 1;
            let current = (open > 0).then_some(&top.info);
            for _ in 0..html::closed_by_start(open, current, &top.reach, &info) {
                self.close_implied(tag.start);
            }
        }
        let slot = self.slot(&name);
        let ordinal = self.stack.last().unwrap().counts[slot].1;
        // An empty element (`<x/>`, a void element) holds nothing but its tag: no Segment, but
        // it counts among its siblings.
        if self_closing || info.void {
            return;
        }
        let raw = info.raw_text;
        let reach = html::reach(
            &self.stack.last().unwrap().reach,
            &info,
            self.stack.len() - 1,
        );
        let id = if html {
            let id = self.tag_id(&name);
            self.open[id as usize] += 1;
            id
        } else {
            0
        };
        self.stack.push(Frame {
            name: name.clone(),
            tag,
            info,
            reach,
            id,
            seg: self.stack.len() <= MAX_DEPTH,
            slot,
            ordinal,
            counts: Vec::new(),
            index: None,
            pending: Vec::new(),
        });
        if raw {
            sc.skip_raw_text(&self.src[name]);
        }
    }

    /// An end tag: XML closes the current element or stops (Err with the error's byte); HTML
    /// closes the innermost open element of that name and the ones inside it, or ignores it.
    /// Each search is linear only in the elements it then closes, so the walk stays linear.
    fn end(&mut self, name: Range<usize>, tag: Range<usize>) -> Result<(), usize> {
        let top = self.stack.len() - 1;
        if self.p.syntax == Syntax::Xml {
            if top > 0 && self.same(&self.stack[top].name, &name) {
                self.close(tag.end);
                return Ok(());
            }
            return Err(tag.start);
        }
        fill_key(&mut self.buf, &self.src[name.clone()], self.p.syntax);
        let Some(&id) = self.ids.get(&self.buf[..]) else {
            return Ok(());
        };
        if self.open[id as usize] == 0 {
            return Ok(());
        }
        let k = (1..self.stack.len())
            .rev()
            .find(|&k| self.stack[k].id == id)
            .expect("an element of this tag is open");
        while self.stack.len() > k + 1 {
            self.close_implied(tag.start);
        }
        self.close(tag.end);
        Ok(())
    }

    /// Walks the file; returns where the structure ends: its length, or the byte where XML
    /// stops being well formed (spec §5.4).
    fn run(&mut self) -> usize {
        let mut sc = Scanner::new(self.src, self.p.syntax == Syntax::Xml);
        let end = loop {
            match sc.step() {
                Step::Tag(Tag::Start {
                    name,
                    tag,
                    self_closing,
                }) => self.start(name, tag, self_closing, &mut sc),
                Step::Tag(Tag::End { name, tag }) => {
                    if let Err(at) = self.end(name, tag) {
                        break at;
                    }
                }
                Step::Eof => break self.src.len(),
                Step::Error(at) => break at,
            }
        };
        while self.stack.len() > 1 {
            self.close_implied(end);
        }
        let mut root = self.stack.pop().unwrap();
        self.flush(&mut root);
        end
    }
}

fn outline(src: &[u8], p: Params, mut c: Collector<'_>) -> Outline {
    let end = Walk {
        src,
        p,
        c: &mut c,
        stack: vec![Frame::root()],
        ids: Map::default(),
        open: Vec::new(),
        buf: Vec::new(),
        trimmed: (usize::MAX, 0),
    }
    .run();
    c.finish(end, src)
}

impl Processor for Markup {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let p = self::params(params)?;
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let sorted: Vec<usize> = order.iter().map(|&i| anchors[i]).collect();
        let o = outline(input.bytes, p, Collector::for_anchors(&sorted));
        let mut cursor = TrailCursor::new(&o);
        let mut out = vec![Vec::new(); anchors.len()];
        for (k, &i) in order.iter().enumerate() {
            out[i] = cursor.trail(sorted[k]);
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let p = self::params(params)?;
        Ok(names_of(&outline(input.bytes, p, Collector::for_names())))
    }
}
```

- [ ] **Step 4:** `cargo test --lib markup`: the 11 walk tests and the 10 of Tasks 2.1–2.2
  PASS. From review r1: `xml_doctypes_and_malformed_tags_do_not_invent_elements` (Codex #4, #5),
  `an_optgroup_ends_the_open_option` (Codex #9, it FAILED before `html.rs` closed the option),
  `many_distinct_sibling_tags_keep_their_indexes` and
  `the_sibling_index_agrees_with_the_linear_count` (Codex #1, Fable #1: 20 distinct tags cross
  `LINEAR`, then `a` again is `a[2]`; D28), and `html_and_body_are_not_names` (Fable #6, D7).
  Then `cargo test --workspace` and clippy.

- [ ] **Step 5:** by hand, with `cargo build`: `target/debug/sfor --flat needle
  tests/fixtures/markup/page.html` prints the seven rows of `m3_02` (Task 6.1), and `--tree`
  shows `li[3]` under `ul` under `nav.top.menu`.

- [ ] **Step 6:** commit: `git add -- src/processors/markup tests/fixtures/markup` and
  `git commit -m "Trail HTML and XML elements with a tag scanner of our own"`.

### Task 2.5: the walk's perf guards, and Outline lines without a line index (D27, D28, D34)

C2 owns these guards because their RED must run against C2's own code (review r2, Codex #4).
`Outline::build` built a `LineIndex` of the whole Source, one `usize` per line, whatever the
number of Segments kept: one Match in an XML file of 50 MB of line breaks took 467 MiB, against
rg's 56 (review r1, Codex #2). `fill_lines` counts line breaks only up to the bytes it is asked
for; Markdown, keypath and treesitter get the same lines.

- [ ] **Step 1:** append the five guards to `tests/perf.rs` (after the ceilings of M2; Task 1.5's
  three hunks are already in the file):

```diff
--- a/tests/perf.rs
+++ b/tests/perf.rs
@@ -705,3 +705,117 @@
         "{t:?} is more than rg's {base:?} + 1 s"
     );
 }
+
+/// M3 review r1, Codex #1 and Fable #1: a parent with 200 000 distinct child tags costs
+/// constant time per child (82 s before the sibling index, against rg's 0.01 s).
+#[test]
+#[ignore]
+fn markup_many_distinct_sibling_tags_stay_linear() {
+    let _alone = alone();
+    let fx = Fixture::new();
+    let mut v = String::from("<r>");
+    for i in 0..200_000 {
+        v.push_str(&format!("<row{i}/>"));
+    }
+    v.push_str("<x>needle</x></r>\n");
+    std::fs::write(fx.file("siblings.xml"), &v).unwrap();
+    let args = ["--flat", "needle", "siblings.xml"];
+    let (out, _) = sfor_output(&fx, &args);
+    assert!(out.starts_with("1: r › x │ "), "{out:.100}");
+    let Some(((t, _), (base, _))) = against_rg(&fx, &args) else {
+        return;
+    };
+    assert!(
+        t <= base + Duration::from_millis(500),
+        "{t:?} against rg's {base:?}"
+    );
+}
+
+/// M3 review r1, Codex #2: one Match in 50 MB of line breaks keeps no index of the lines
+/// (467 MiB of RSS before the fix, against rg's 56 MiB).
+#[test]
+#[ignore]
+fn markup_keeps_no_index_of_the_lines() {
+    let _alone = alone();
+    let fx = Fixture::new();
+    let mut v = String::with_capacity(SIZE + 64);
+    v.push_str("<r>\n");
+    v.push_str(&"\n".repeat(SIZE));
+    v.push_str("<x>needle</x></r>\n");
+    std::fs::write(fx.file("blank.xml"), &v).unwrap();
+    drop(v);
+    let args = ["--flat", "needle", "blank.xml"];
+    let (out, _) = sfor_output(&fx, &args);
+    assert!(out.ends_with(": r › x │ <x>needle</x></r>\n"), "{out:.100}");
+    let Some(((t, rss), (base, base_rss))) = against_rg(&fx, &args) else {
+        return;
+    };
+    assert!(
+        t <= base + Duration::from_secs(1),
+        "{t:?} against rg's {base:?}"
+    );
+    assert!(
+        rss <= base_rss + 32 * 1_048_576,
+        "RSS {rss} is more than rg's {base_rss} + 32 MiB"
+    );
+}
+
+/// One of M3 review r2's linear walks (Codex #1): `body` in `name`, one Match, sfor within rg's
+/// time + 1 s, and the Match's Trail with `segments` Segments (256 at most, D25).
+fn markup_linear(name: &str, body: String, segments: usize) {
+    let fx = Fixture::new();
+    std::fs::write(fx.file(name), &body).unwrap();
+    let (out, _) = sfor_output(&fx, &["--json", "needle", name]);
+    let m = out
+        .lines()
+        .find(|l| l.contains(r#""type":"match""#))
+        .unwrap();
+    assert_eq!(m.matches(r#""kind":"tag""#).count(), segments, "{name}");
+    let Some(((t, _), (base, _))) = against_rg(&fx, &["--flat", "needle", name]) else {
+        return;
+    };
+    eprintln!("{name}: sfor {t:?}, rg {base:?}");
+    assert!(
+        t <= base + Duration::from_secs(1),
+        "{name}: {t:?} against rg's {base:?}"
+    );
+}
+
+/// 40 000 nested `div`s: a start tag finds the `p` it ends without searching the stack (4.8 s
+/// before `html::Reach`).
+#[test]
+#[ignore]
+fn markup_deep_nesting_stays_linear() {
+    let _alone = alone();
+    let body = format!(
+        "{}needle{}",
+        "<div>".repeat(40_000),
+        "</div>".repeat(40_000)
+    );
+    markup_linear("deep.html", body, 256);
+}
+
+/// 20 000 end tags with no element of their tag open: each is ignored at once (2.9 s before
+/// the open counts).
+#[test]
+#[ignore]
+fn markup_end_tags_with_nothing_open_stay_linear() {
+    let _alone = alone();
+    let body = format!(
+        "{}{}needle{}",
+        "<x>".repeat(20_000),
+        "</q>".repeat(20_000),
+        "</x>".repeat(20_000)
+    );
+    markup_linear("unmatched.html", body, 256);
+}
+
+/// 20 000 elements ended together at the end of the file, after 200 000 spaces: the white
+/// space is trimmed once (6.6 s before).
+#[test]
+#[ignore]
+fn markup_implied_ends_at_one_byte_stay_linear() {
+    let _alone = alone();
+    let body = format!("{}needle{}", "<x>".repeat(20_000), " ".repeat(200_000));
+    markup_linear("trim.html", body, 256);
+}
```

- [ ] **Step 2:** RED of the line index: `cargo build --release` with `src/trail.rs` still as in
  `f7cccfe`, then `SFOR_REQUIRE_REFERENCE=1 cargo test --release --test perf -- --ignored
  --nocapture markup_keeps_no_index_of_the_lines`: FAILS on its RSS assertion, not on the Trail
  (probe: `RSS 479186944 is more than rg's 58785792 + 32 MiB`).

- [ ] **Step 3:** `fill_lines`:

```diff
--- a/src/trail.rs
+++ b/src/trail.rs
@@ -88,7 +88,6 @@
                 .cmp(&b.range.start)
                 .then(b.range.end.cmp(&a.range.end))
         });
-        let lines = LineIndex::new(content);
         let mut stack: Vec<usize> = Vec::new();
         for i in 0..segments.len() {
             let start = segments[i].range.start;
@@ -105,14 +104,9 @@
                 }
             }
             segments[i].parent = stack.last().copied();
-            let r = segments[i].range.clone();
-            let seg = &mut segments[i].segment;
-            if seg.line == 0 {
-                seg.line = lines.line_of(r.start);
-                seg.end_line = lines.line_of(r.end.saturating_sub(1).max(r.start));
-            }
             stack.push(i);
         }
+        fill_lines(&mut segments, content);
         Self { segments }
     }
 
@@ -135,6 +129,35 @@
     }
 }
 
+/// Fills the `line` and `end_line` left at 0 from the ranges, counting line breaks only up to
+/// the last byte asked: memory grows with the Segments, never with the lines of the file (M3
+/// review r1, Codex #2).
+fn fill_lines(segments: &mut [OutlineSegment], content: &[u8]) {
+    // (byte, Segment index, whether it is the end) for every line to compute.
+    let mut asks: Vec<(usize, usize, bool)> = Vec::new();
+    for (i, s) in segments.iter().enumerate() {
+        if s.segment.line == 0 {
+            let r = &s.range;
+            asks.push((r.start, i, false));
+            asks.push((r.end.saturating_sub(1).max(r.start), i, true));
+        }
+    }
+    asks.sort_unstable();
+    let (mut at, mut line) = (0usize, 1u64);
+    for (byte, i, end) in asks {
+        let to = byte.min(content.len());
+        if to > at {
+            line += memchr::memchr_iter(b'\n', &content[at..to]).count() as u64;
+            at = to;
+        }
+        if end {
+            segments[i].segment.end_line = line;
+        } else {
+            segments[i].segment.line = line;
+        }
+    }
+}
+
 /// Trails of byte offsets asked in non-decreasing order, from a stack of the Segments open at
 /// the last offset instead of a scan of the whole Outline per offset. An offset smaller than the
 /// last one starts over.
```

- [ ] **Step 4:** `cargo test --workspace`: PASS, no expectation changed. Then the five guards,
  `SFOR_REQUIRE_REFERENCE=1 cargo test --release --test perf -- --ignored --nocapture markup_`:
  PASS (the walk of Task 2.4 already holds D28 and D34). They cannot be seen RED against this
  plan's walk, which is linear from its first version; the probe measured each of them against
  the walk of review r1 instead: 200 000 distinct siblings 82 s (Fable), 40 000 nested `div`s
  3.96 s, 20 000 end tags with nothing open 2.88 s, 20 000 elements ended after 200 000 spaces
  5.65 s, against rg's 0.01 s.

- [ ] **Step 5:** commit: `git add -- src/trail.rs tests/perf.rs` and
  `git commit -m "Count Outline lines without a line index and guard the markup walk"`.

## Chunk 3: csv — CSV and TSV

Read spec §5.5 and design note 4 first.

### Task 3.1: the fixtures

**Files:** `tests/fixtures/table/{plans.csv,semi.csv,people.tsv,.gitattributes}`. They are
written with `printf`, because `semi.csv` has CRLF line ends and `people.tsv` tabs:

```bash
cd tests/fixtures/table
printf '* -text\n' > .gitattributes
printf 'id,Name,Plan,Notes\n1,Ana,free,first row\n2,Bea,needle,"likes ""quotes"", and commas"\n3,Carla,pro,"two\nlines with needle"\n\n4,Dan,team,needle at the end,extra\n' > plans.csv
printf 'Nombre;Plan;Precio\r\nAna;needle;0\r\nBea;pro;9,50 needle\r\n' > semi.csv
printf 'Name\tRole\tNote\n"Ana\tadmin\tneedle "quoted\nBea\tneedle\tok\n' > people.tsv
```

- [ ] **Step 1:** run it. `cat -A plans.csv | cat -n` gives:

```
     1	id,Name,Plan,Notes$
     2	1,Ana,free,first row$
     3	2,Bea,needle,"likes ""quotes"", and commas"$
     4	3,Carla,pro,"two$
     5	lines with needle"$
     6	$
     7	4,Dan,team,needle at the end,extra$
```

  `semi.csv` has 3 lines ending in `^M$`; `people.tsv` 3 lines with `^I` tabs. The `needle`s:
  `plans.csv` lines 3 (row 2, `Plan`), 5 (row 3, `Notes`, the record spans lines 4–5) and 7
  (row 4, `Notes`; line 6 is blank and is no record); `semi.csv` lines 2 (row 1, `Plan`) and 3
  (row 2, `Precio`: `9,50` holds no delimiter, since `;` is sniffed); `people.tsv` lines 2 (row 1,
  `Note`: the quotes are text) and 3 (row 2, `Role`).

- [ ] **Step 2:** `git add -- tests/fixtures/table` happens with the code, Task 3.2 Step 6; check
  before it that `git diff --cached` would keep the `\r` (`git check-attr -a semi.csv` shows
  `text: unset`).

### Task 3.2: the Processor (src/processors/csv.rs)

- [ ] **Step 1:** keep the stub's `Processor` impl and add the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn table(extra: &str) -> toml::Table {
        toml::from_str(extra).unwrap()
    }

    /// (line, Trail names) of every `needle` in `src`.
    fn needles(src: &[u8], params: &str) -> Vec<Vec<String>> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        let input = Input {
            bytes: src,
            path: Path::new("x.csv"),
            format: "csv",
            disk: None,
        };
        Csv.trails(&input, &table(params), &anchors)
            .unwrap()
            .iter()
            .map(|t| {
                t.iter()
                    .map(|s| format!("{}:{}-{}", s.name, s.line, s.end_line))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn sniffs_the_delimiter_of_the_first_line() {
        assert_eq!(sniff(b"a;b;c\n1,5;2;3\n", Some(b'"')), b';');
        assert_eq!(sniff(b"\"a;b\",c\n", Some(b'"')), b',');
        assert_eq!(sniff(b"a\tb|c\n", Some(b'"')), b'\t');
        assert_eq!(sniff(b"plain\n", Some(b'"')), b',');
    }

    #[test]
    fn rows_and_columns_with_quotes_crlf_and_blank_lines() {
        let src = b"id,Plan,Note\r\n1,free,x\r\n\r\n2,needle,\"a\r\nneedle\"\r\n3,pro,needle\r\n";
        assert_eq!(
            needles(src, ""),
            [
                vec!["row 2:4-5", "Plan:4-5"],
                vec!["row 2:4-5", "Note:4-5"],
                vec!["row 3:6-6", "Note:6-6"]
            ]
        );
    }

    #[test]
    fn header_matches_ragged_rows_and_no_headers() {
        let src = b"needle,b\n1,2,needle\n";
        assert_eq!(
            needles(src, ""),
            [
                vec![],
                vec!["row 1:2-2".to_string(), "column 3:2-2".to_string()]
            ]
        );
        assert_eq!(
            needles(src, "headers = false"),
            [
                vec!["row 1:1-1", "column 1:1-1"],
                vec!["row 2:2-2", "column 3:2-2"]
            ]
        );
    }

    #[test]
    fn an_unclosed_quote_ends_the_structure() {
        let src = b"a,b\n1,needle\n2,\"open\nneedle\n";
        assert_eq!(
            needles(src, ""),
            [vec!["row 1:2-2".to_string(), "b:2-2".to_string()], vec![]]
        );
        // A stray quote after the closing one is text: the field is closed (Fable r1 #2).
        assert_eq!(
            needles(b"a,b\nneedle,\"x\"y\"", ""),
            [vec!["row 1:2-2", "a:2-2"]]
        );
    }

    #[test]
    fn text_after_a_closed_quote_is_not_an_open_quote() {
        // csv-core reads `"needle"junk"` as `needlejunk"`: the field is closed, at the end of
        // the file or not (M3 review r1, Codex #7).
        assert_eq!(
            needles(b"a,b\n1,\"needle\"junk\"\n", ""),
            [vec!["row 1:2-2", "b:2-2"]]
        );
        assert_eq!(
            needles(b"a,b\n1,\"needle\"junk\"\n2,end\n", ""),
            [vec!["row 1:2-2", "b:2-2"]]
        );
        // `""` inside quotes keeps the field open: this one never closes.
        assert_eq!(
            needles(b"a,b\n1,\"needle\"\"x\n", ""),
            [Vec::<String>::new()]
        );
    }

    #[test]
    fn tsv_quotes_are_text() {
        let src = b"a\tb\n\"x\tneedle\n";
        assert_eq!(
            needles(src, "delimiter = \"\\t\"\nquote = \"\""),
            [vec!["row 1:2-2", "b:2-2"]]
        );
    }

    #[test]
    fn header_names_and_their_pieces() {
        let src = b" Plan ,\"Note \"\"x\"\"\",,id\n1,2,3,4\n";
        let input = Input {
            bytes: src,
            path: Path::new("x.csv"),
            format: "csv",
            disk: None,
        };
        let n = Csv.names(&input, &table("")).unwrap();
        let got: Vec<(&str, Pieces)> = n
            .iter()
            .map(|n| (n.text.as_str(), n.pieces.clone()))
            .collect();
        assert_eq!(
            got,
            [
                ("Plan", vec![(0..4, 1..5)]),
                (
                    "Note \"x\"",
                    vec![
                        (0..5, 8..13),
                        (5..6, 13..15),
                        (6..7, 15..16),
                        (7..8, 16..18)
                    ]
                ),
                ("id", vec![(0..2, 21..23)])
            ]
        );
        assert_eq!(n[0].trail_at(1), Vec::new());
    }

    #[test]
    fn walks_only_to_the_last_anchor() {
        // Everything after row 1 is never read: an unclosed quote there changes nothing.
        let src = b"a,b\nneedle,1\n2,\"open\n";
        assert_eq!(needles(src, ""), [vec!["row 1:2-2", "a:2-2"]]);
    }
}
```

- [ ] **Step 2:** `cargo test --lib processors::csv`: FAILS to compile (`sniff`).

- [ ] **Step 3:** replace everything above the tests (design note 4, D3–D5, D10, D18, D19, D21,
  D23):

```rust
//! csv: rows and columns of CSV and TSV files (spec §5.5, M3 plan design note 4). csv-core
//! reads the fields; this module keeps their byte ranges and walks only as far as the last
//! anchor, so a Match on row 3 of a 1 GB file reads three rows.
use super::Processor;
use crate::model::{Input, Name, Segment, Trail};
use std::ops::Range;

use crate::text::Pieces;

pub struct Csv;

pub const DEFAULT_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Params {
    /// None: sniffed from the first line (D4).
    delimiter: Option<u8>,
    /// None: quotes are text (TSV).
    quote: Option<u8>,
    headers: bool,
    name_chars: usize,
}

fn one_byte(v: &str, what: &str) -> anyhow::Result<u8> {
    match v.as_bytes() {
        [b] if b.is_ascii() => Ok(*b),
        _ => anyhow::bail!("csv: `{what}` must be one ASCII character, got {v:?}"),
    }
}

fn params(t: &toml::Table) -> anyhow::Result<Params> {
    let delimiter = match t
        .get("delimiter")
        .and_then(|v| v.as_str())
        .unwrap_or("auto")
    {
        "auto" => None,
        d => Some(one_byte(d, "delimiter")?),
    };
    let quote = match t.get("quote").and_then(|v| v.as_str()).unwrap_or("\"") {
        "" => None,
        q => Some(one_byte(q, "quote")?),
    };
    Ok(Params {
        delimiter,
        quote,
        headers: t.get("headers").and_then(|v| v.as_bool()).unwrap_or(true),
        name_chars: t
            .get("name_chars")
            .and_then(|v| v.as_integer())
            .filter(|&n| n > 0)
            .map_or(DEFAULT_NAME_CHARS, |n| n as usize),
    })
}

/// The delimiter of a CSV file (D4): of `,`, `;`, tab and `|`, the one that appears most often
/// in its first line outside quotes; `,` on a tie or when none appears.
pub fn sniff(src: &[u8], quote: Option<u8>) -> u8 {
    const CANDIDATES: [u8; 4] = [b',', b';', b'\t', b'|'];
    let line = &src[..memchr::memchr(b'\n', src).unwrap_or(src.len())];
    let mut counts = [0usize; 4];
    let mut quoted = false;
    for &b in line {
        if Some(b) == quote {
            quoted = !quoted;
        } else if !quoted && let Some(k) = CANDIDATES.iter().position(|&c| c == b) {
            counts[k] += 1;
        }
    }
    let mut best = 0;
    for k in 1..4 {
        if counts[k] > counts[best] {
            best = k;
        }
    }
    CANDIDATES[best]
}

/// Whether csv-core would still be inside the quotes of `field` at its end (M3 review r1,
/// Codex #7): a field quoted from its first byte, where `""` is a quote inside the field and a
/// closing quote followed by anything but a quote turns the rest into plain text, whose quotes
/// are literal (csv-core 0.1.13, `reader.rs`, states `InQuotedField` and `InDoubleEscapedQuote`).
fn ends_quoted(field: &[u8], q: u8) -> bool {
    if field.first() != Some(&q) {
        return false;
    }
    let mut quoted = true;
    let mut after_quote = false;
    for &b in &field[1..] {
        if quoted {
            if b == q {
                quoted = false;
                after_quote = true;
            }
        } else if after_quote && b == q {
            quoted = true;
            after_quote = false;
        } else {
            // Plain text from here to the end of the field.
            return false;
        }
    }
    quoted
}

/// One field: `raw` is its bytes in the Source, quotes included, without the delimiter or line
/// terminator that ends it; `end` is the byte after that delimiter or terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    raw: Range<usize>,
    end: usize,
}

/// The records of a Source, in order.
struct Records<'s> {
    src: &'s [u8],
    pos: usize,
    rdr: csv_core::Reader,
    quote: Option<u8>,
    /// The decoded text of each field of the last record, when asked for (the header).
    out: Vec<u8>,
    ends: Vec<usize>,
}

/// What `Records::next` found.
enum Next {
    Record,
    /// The file ended inside a quoted field: the structure ends at that record (spec §5.5).
    Unclosed,
    End,
}

impl<'s> Records<'s> {
    fn new(src: &'s [u8], p: &Params) -> Self {
        let delimiter = p.delimiter.unwrap_or_else(|| sniff(src, p.quote));
        let rdr = csv_core::ReaderBuilder::new()
            .delimiter(delimiter)
            .quote(p.quote.unwrap_or(b'"'))
            .quoting(p.quote.is_some())
            .build();
        Self {
            src,
            pos: 0,
            rdr,
            quote: p.quote,
            out: vec![0; 1024],
            ends: Vec::new(),
        }
    }

    /// Reads the next record into `fields`; with `text`, also the decoded text of each field
    /// (`self.out`, split at `self.ends`).
    fn next(&mut self, fields: &mut Vec<Field>, text: bool) -> Next {
        fields.clear();
        self.ends.clear();
        let src = self.src;
        let mut start = self.pos;
        let mut o = 0;
        loop {
            let (res, nin, nout) = self.rdr.read_field(&src[self.pos..], &mut self.out[o..]);
            self.pos += nin;
            if text {
                o += nout;
            }
            match res {
                csv_core::ReadFieldResult::InputEmpty => {}
                csv_core::ReadFieldResult::OutputFull => {
                    if text {
                        self.out.resize(self.out.len() * 2, 0);
                    } else {
                        o = 0;
                    }
                }
                csv_core::ReadFieldResult::End => return Next::End,
                csv_core::ReadFieldResult::Field { record_end } => {
                    // csv-core consumes the terminators of the previous record and blank lines
                    // with a record's first field.
                    if fields.is_empty() {
                        while start < self.pos && matches!(src[start], b'\n' | b'\r') {
                            start += 1;
                        }
                    }
                    let mut raw_end = self.pos;
                    if raw_end > start {
                        let last = src[raw_end - 1];
                        let ended = if record_end {
                            matches!(last, b'\n' | b'\r')
                        } else {
                            true
                        };
                        if ended {
                            raw_end -= 1;
                        }
                    }
                    let raw = start..raw_end.max(start);
                    if record_end
                        && self.pos == src.len()
                        && let Some(q) = self.quote
                        && ends_quoted(&src[raw.clone()], q)
                    {
                        return Next::Unclosed;
                    }
                    let mut end = self.pos;
                    if record_end
                        && end > 0
                        && src[end - 1] == b'\r'
                        && src.get(end) == Some(&b'\n')
                    {
                        end += 1;
                    }
                    fields.push(Field { raw, end });
                    if text {
                        self.ends.push(o);
                    }
                    start = self.pos;
                    if record_end {
                        return Next::Record;
                    }
                }
            }
        }
    }
}

/// 1-based line numbers of bytes asked in non-decreasing order.
struct Lines<'s> {
    src: &'s [u8],
    at: usize,
    line: u64,
}

impl Lines<'_> {
    fn of(&mut self, byte: usize) -> u64 {
        let to = byte.min(self.src.len());
        if to > self.at {
            self.line += memchr::memchr_iter(b'\n', &self.src[self.at..to]).count() as u64;
            self.at = to;
        }
        self.line
    }
}

/// A header cell's name (spec §5.5): its text, quotes removed and `""` decoded, trimmed of
/// ASCII white space and cut at `name_chars`, with the pieces that map it to the cell; None
/// when it is empty. `searchable` is false when csv-core read the cell in a way the pieces
/// cannot map (a lenient reading of stray quotes): the name still names the column, and
/// `--in-headings` skips it.
struct HeaderName {
    text: String,
    pieces: Pieces,
    searchable: bool,
}

fn header_name(
    src: &[u8],
    f: &Field,
    decoded: &[u8],
    quote: Option<u8>,
    name_chars: usize,
) -> Option<HeaderName> {
    let raw = &src[f.raw.clone()];
    // (decoded bytes, their Source range): verbatim runs, and one entry per `""`.
    let mut runs: Vec<(Vec<u8>, Range<usize>)> = Vec::new();
    match quote {
        Some(q) if raw.len() >= 2 && raw[0] == q && raw[raw.len() - 1] == q => {
            let (s0, s1) = (f.raw.start + 1, f.raw.end - 1);
            let mut i = s0;
            let mut run = s0;
            while i < s1 {
                if src[i] == q && i + 1 < s1 && src[i + 1] == q {
                    if run < i {
                        runs.push((src[run..i].to_vec(), run..i));
                    }
                    runs.push((vec![q], i..i + 2));
                    i += 2;
                    run = i;
                } else {
                    i += 1;
                }
            }
            if run < s1 {
                runs.push((src[run..s1].to_vec(), run..s1));
            }
        }
        _ => runs.push((raw.to_vec(), f.raw.clone())),
    }
    let mine: Vec<u8> = runs.iter().flat_map(|(b, _)| b.iter().copied()).collect();
    let searchable = mine == decoded;
    // Trim the white space at both ends, which only verbatim runs hold.
    while let Some((b, r)) = runs.first_mut() {
        let lead = b.len() - b.trim_ascii_start().len();
        if lead == b.len() && b.len() == r.len() {
            runs.remove(0);
            continue;
        }
        if b.len() == r.len() {
            b.drain(..lead);
            r.start += lead;
        }
        break;
    }
    while let Some((b, r)) = runs.last_mut() {
        let trail = b.len() - b.trim_ascii_end().len();
        if trail == b.len() && b.len() == r.len() {
            runs.pop();
            continue;
        }
        if b.len() == r.len() {
            b.truncate(b.len() - trail);
            r.end -= trail;
        }
        break;
    }
    let mut text = String::new();
    let mut pieces = Vec::new();
    for (b, r) in &runs {
        if b.len() == r.len() {
            crate::text::push_lossy(&mut text, &mut pieces, src, r.clone());
        } else {
            let at = text.len();
            text.push_str(&String::from_utf8_lossy(b));
            pieces.push((at..text.len(), r.clone()));
        }
    }
    if !searchable {
        text = String::from_utf8_lossy(decoded).trim_ascii().to_string();
        pieces.clear();
    }
    if text.is_empty() {
        return None;
    }
    crate::text::cut_name(&mut text, &mut pieces, name_chars);
    Some(HeaderName {
        text,
        pieces,
        searchable,
    })
}

impl Csv {
    /// The header names (`None` for an empty cell), the header's fields and the line range of
    /// the header row.
    #[allow(clippy::type_complexity)]
    fn header(
        recs: &mut Records,
        p: &Params,
        lines: &mut Lines,
    ) -> Option<(Vec<Option<HeaderName>>, Vec<Field>, (u64, u64))> {
        let mut fields = Vec::new();
        if !matches!(recs.next(&mut fields, true), Next::Record) {
            return None;
        }
        let mut names = Vec::new();
        let mut from = 0;
        for (f, &to) in fields.iter().zip(&recs.ends) {
            names.push(header_name(
                recs.src,
                f,
                &recs.out[from..to],
                p.quote,
                p.name_chars,
            ));
            from = to;
        }
        let first = lines.of(fields[0].raw.start);
        let last = lines.of(fields
            .last()
            .unwrap()
            .raw
            .end
            .saturating_sub(1)
            .max(fields[0].raw.start));
        Some((names, fields, (first, last)))
    }
}

fn seg(kind: &str, name: String, (line, end_line): (u64, u64)) -> Segment {
    Segment {
        kind: kind.into(),
        name,
        line,
        end_line,
        level: None,
        signature: None,
    }
}

impl Processor for Csv {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let p = self::params(params)?;
        let src = input.bytes;
        let mut out = vec![Vec::new(); anchors.len()];
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut next = 0; // first anchor of `order` not yet placed
        let mut recs = Records::new(src, &p);
        let mut lines = Lines {
            src,
            at: 0,
            line: 1,
        };
        let mut names: Vec<Option<String>> = Vec::new();
        if p.headers {
            match Csv::header(&mut recs, &p, &mut lines) {
                Some((n, fields, _)) => {
                    names = n.into_iter().map(|n| n.map(|h| h.text)).collect();
                    // Matches in the header row are in a column's name: no Trail (spec §5.5).
                    let end = fields.last().unwrap().end;
                    while next < order.len() && anchors[order[next]] < end {
                        next += 1;
                    }
                }
                None => return Ok(out),
            }
        }
        let mut fields = Vec::new();
        let mut row = 0u64;
        while next < order.len() {
            match recs.next(&mut fields, false) {
                Next::End => break,
                Next::Unclosed => break,
                Next::Record => {}
            }
            row += 1;
            let (first, last) = (&fields[0], fields.last().unwrap());
            let rec_end = last.end;
            let lines_of = (
                lines.of(first.raw.start),
                lines.of(last.raw.end.saturating_sub(1).max(first.raw.start)),
            );
            let mut k = 0;
            while next < order.len() && anchors[order[next]] < rec_end {
                let a = anchors[order[next]];
                if a >= first.raw.start {
                    while k + 1 < fields.len() && a >= fields[k].end {
                        k += 1;
                    }
                    let col = names
                        .get(k)
                        .cloned()
                        .flatten()
                        .unwrap_or_else(|| format!("column {}", k + 1));
                    out[order[next]] = vec![
                        seg("row", format!("row {row}"), lines_of),
                        seg("column", col, lines_of),
                    ];
                }
                next += 1;
            }
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let p = self::params(params)?;
        if !p.headers {
            return Ok(Vec::new());
        }
        let src = input.bytes;
        let mut recs = Records::new(src, &p);
        let mut lines = Lines {
            src,
            at: 0,
            line: 1,
        };
        let Some((names, fields, header_lines)) = Csv::header(&mut recs, &p, &mut lines) else {
            return Ok(Vec::new());
        };
        Ok(names
            .into_iter()
            .zip(&fields)
            .filter_map(|(n, f)| {
                let h = n.filter(|h| h.searchable)?;
                Some(Name {
                    trail: vec![seg("column", h.text.clone(), header_lines)],
                    labels: vec![f.raw.clone()],
                    anchor: f.raw.start,
                    text: h.text,
                    pieces: h.pieces,
                })
            })
            .collect())
    }
}
```

- [ ] **Step 4:** `cargo test --lib processors::csv`: 8 tests PASS; `cargo test --workspace` and
  clippy. `text_after_a_closed_quote_is_not_an_open_quote` is review r1's Codex #7: the first
  version counted quotes and lost `1,"needle"junk"` at the end of a file; it FAILED against it
  (D32). `an_unclosed_quote_ends_the_structure` also holds Fable #2's `a,b\nneedle,"x"y"`.

- [ ] **Step 5:** by hand: `target/debug/sfor --flat needle tests/fixtures/table` prints the rows
  of `m3_04` (Task 6.1), and `--expand 'lines with' tests/fixtures/table/plans.csv` prints lines
  4–5, the record (D10).

- [ ] **Step 6:** commit: `git add -- src/processors/csv.rs tests/fixtures/table` and
  `git commit -m "Trail CSV and TSV rows and columns over csv-core"`.

## Chunk 4: subtitles — SubRip and WebVTT

Read spec §5.6 and design note 5 first.

### Task 4.1: the fixtures

**Files:** `tests/fixtures/subs/{movie.srt,talk.vtt,.gitattributes}`. `movie.srt` starts with a
UTF-8 BOM and has CRLF line ends, as many real SRT files (of 30 SRT files on this machine, 2 had
both, 2 more mixed CRLF and LF):

```bash
cd tests/fixtures/subs
printf '* -text\n' > .gitattributes
printf '\xef\xbb\xbf1\r\n00:00:01,000 --> 00:00:03,500\r\nWhere is the needle?\r\n\r\n2\r\n00:00:04,000 --> 00:00:06,000\r\n- In the haystack.\r\n- A needle, in there?\r\n\r\n3\r\n00:01:02,500 --> 00:01:04,000\r\n<i>needle</i>\r\n' > movie.srt
printf 'WEBVTT - needle talk\n\nNOTE needle in a note\n\nintro\n00:00.000 --> 00:02.000\nWelcome.\n\n00:02.500 --> 00:05.000 align:start\n<v Ana>The needle is here.\n\n01:00:00.000 --> 01:00:01.000\nneedle at one hour\n' > talk.vtt
```

- [ ] **Step 1:** run it. `cat -A movie.srt | cat -n`: 12 lines, the first `M-oM-;M-?1^M$`; the
  `needle`s on lines 3 (cue 1, lines 1–3), 8 (cue 2, lines 5–8) and 12 (cue 3, lines 10–12).
  `talk.vtt`: 13 lines; `needle` on lines 1 (header: no cue), 3 (`NOTE`: no cue), 10 (the second
  cue, which has no identifier: `cue 2 00:02.500`, lines 9–10) and 13 (`cue 3 01:00:00.000`,
  lines 12–13). The first cue is `cue intro 00:00.000`.

### Task 4.2: the Processor (src/processors/subtitles.rs)

- [ ] **Step 1:** keep the stub's `Processor` impl and add the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn needles(src: &str) -> Vec<Vec<(String, u64, u64)>> {
        let anchors: Vec<usize> = src.match_indices("needle").map(|(i, _)| i).collect();
        let input = Input {
            bytes: src.as_bytes(),
            path: Path::new("x.srt"),
            format: "srt",
            disk: None,
        };
        Subtitles
            .trails(&input, &toml::Table::new(), &anchors)
            .unwrap()
            .iter()
            .map(|t| {
                t.iter()
                    .map(|s| (s.name.clone(), s.line, s.end_line))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn timing_lines() {
        assert_eq!(
            timing(b"00:01:02,500 --> 00:01:04,000"),
            Some((0..12, 17..29))
        );
        assert_eq!(
            timing(b"01:02.500 --> 01:04.000 align:start"),
            Some((0..9, 14..23))
        );
        assert_eq!(timing(b"  0:00:01,5-->0:00:02,25"), Some((2..11, 14..24)));
        assert_eq!(timing(b"00:01 --> 00:02"), None);
        assert_eq!(timing(b"needle --> 00:00:01,000"), None);
        assert_eq!(timing(b"00:00:01,000 --> 00:00:02,000x"), None);
    }

    #[test]
    fn srt_cues_with_crlf_and_a_missing_blank_line() {
        let src = "1\r\n00:00:01,000 --> 00:00:02,000\r\nneedle one\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nsecond\r\nneedle two\r\n00:00:05,000 --> 00:00:06,000\r\nneedle three\r\n";
        assert_eq!(
            needles(src),
            [
                vec![("cue 1 00:00:01,000".into(), 1, 3)],
                vec![("cue 2 00:00:03,000".into(), 5, 8)],
                vec![("cue 3 00:00:05,000".into(), 9, 10)],
            ]
        );
    }

    #[test]
    fn vtt_header_notes_and_identifiers() {
        let src = "WEBVTT - needle\n\nNOTE needle here\n\nintro\n00:01.000 --> 00:02.000\nneedle\n\n00:03.000 --> 00:04.000 line:0\n<v Ana>needle\n";
        assert_eq!(
            needles(src),
            [
                vec![],
                vec![],
                vec![("cue intro 00:01.000".into(), 5, 7)],
                vec![("cue 2 00:03.000".into(), 9, 10)]
            ]
        );
    }

    #[test]
    fn a_match_in_the_timing_line_is_inside_the_cue_and_names_are_searchable() {
        let src = "7\n00:00:01,000 --> 00:00:02,000\ntext\n";
        let input = Input {
            bytes: src.as_bytes(),
            path: Path::new("x.srt"),
            format: "srt",
            disk: None,
        };
        let t = Subtitles
            .trails(&input, &toml::Table::new(), &[2, 0])
            .unwrap();
        assert_eq!(t[0][0].name, "cue 7 00:00:01,000");
        assert_eq!(
            t[0][0].signature.as_deref(),
            Some("cue 7 00:00:01,000 --> 00:00:02,000")
        );
        assert_eq!(t[1][0].name, "cue 7 00:00:01,000");
        let n = Subtitles.names(&input, &toml::Table::new()).unwrap();
        assert_eq!(n[0].text, "cue 7 00:00:01,000");
        assert_eq!(n[0].pieces, vec![(4..5, 0..1), (6..18, 2..14)]);
        assert_eq!(n[0].trail_at(2).len(), 1);
    }

    #[test]
    fn vtt_blocks_hold_no_cue_even_with_a_timing_line() {
        let src = "WEBVTT\n00:00.000 --> 00:00.500\nneedle\n\nNOTE\n00:01.000 --> 00:02.000\nneedle\n\nSTYLE\n00:01.000 --> 00:02.000\n::cue { needle }\n\n00:03.000 --> 00:04.000\nneedle\n";
        assert_eq!(
            needles(src),
            [
                vec![],
                vec![],
                vec![],
                vec![("cue 1 00:03.000".into(), 13, 14)]
            ]
        );
        // Without the WEBVTT header (SubRip) a line `NOTE` is an identifier like any other.
        assert_eq!(
            needles("NOTE\n00:01,000 --> 00:02,000\nneedle\n"),
            [vec![("cue NOTE 00:01,000".into(), 1, 3)]]
        );
    }

    #[test]
    fn a_timing_line_is_never_an_identifier() {
        // M3 review r1, Fable #4: two timing lines in a row are two cues.
        assert_eq!(
            needles("00:00:01,000 --> 00:00:02,000\n00:00:03,000 --> 00:00:04,000\nneedle\n"),
            [vec![("cue 2 00:00:03,000".into(), 2, 3)]]
        );
    }

    #[test]
    fn text_outside_cues_has_no_trail() {
        assert_eq!(
            needles("needle\n\n00:00:01,000 --> 00:00:02,000\nx\n\nneedle\n"),
            [vec![], vec![]]
        );
    }
}
```

- [ ] **Step 2:** `cargo test --lib processors::subtitles`: FAILS to compile (`timing`).

- [ ] **Step 3:** replace everything above the tests (design note 5, D6):

```rust
//! subtitles: the cues of SubRip (.srt) and WebVTT (.vtt) files (spec §5.6). One pass over the
//! lines, as far as the last anchor; a cue is the block of a timing line.
use super::Processor;
use crate::model::{Input, Name, Segment, Trail};
use std::ops::Range;

pub struct Subtitles;

/// Characters of a cue identifier used in its name.
pub const ID_CHARS: usize = 40;

/// The end of a timestamp starting at `i`: 2 or 3 groups of digits joined by `:`, then `,` or
/// `.` and 1 to 3 digits (`00:01:02,500`, `01:02.500`).
fn timestamp(s: &[u8], mut i: usize) -> Option<usize> {
    let digits = |i: &mut usize, max: usize| {
        let s0 = *i;
        while *i < s.len() && s[*i].is_ascii_digit() && *i - s0 < max {
            *i += 1;
        }
        *i > s0
    };
    let mut groups = 0;
    loop {
        if !digits(&mut i, 9) {
            return None;
        }
        groups += 1;
        if i < s.len() && s[i] == b':' && groups < 3 {
            i += 1;
        } else {
            break;
        }
    }
    if groups < 2 || i >= s.len() || !matches!(s[i], b',' | b'.') {
        return None;
    }
    i += 1;
    digits(&mut i, 3).then_some(i)
}

/// A timing line `START --> END [settings]`: the ranges of START and END within `line`.
pub fn timing(line: &[u8]) -> Option<(Range<usize>, Range<usize>)> {
    let skip = |mut i: usize| {
        while i < line.len() && matches!(line[i], b' ' | b'\t') {
            i += 1;
        }
        i
    };
    let a = skip(0);
    let a_end = timestamp(line, a)?;
    let arrow = skip(a_end);
    if !line[arrow..].starts_with(b"-->") {
        return None;
    }
    let b = skip(arrow + 3);
    let b_end = timestamp(line, b)?;
    if b_end < line.len() && !matches!(line[b_end], b' ' | b'\t') {
        return None;
    }
    Some((a..a_end, b..b_end))
}

/// True when `line` starts with the word `w`, followed by white space or nothing.
fn block_word(line: &[u8], w: &[u8]) -> bool {
    line.starts_with(w) && line.get(w.len()).is_none_or(|b| b.is_ascii_whitespace())
}

/// One cue: its Section (`range`), its identifier line, its timestamps (Source ranges), its
/// 1-based position, and its lines.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Cue {
    range: Range<usize>,
    id: Option<Range<usize>>,
    start: Range<usize>,
    end: Range<usize>,
    ordinal: u64,
    lines: (u64, u64),
}

impl Cue {
    /// The identifier as the name shows it, trimmed and cut at `ID_CHARS`, with the pieces
    /// that map it to the Source (M3 review r1, Codex #6); the position when there is none.
    fn id(&self, src: &[u8]) -> (String, crate::text::Pieces) {
        let Some(r) = &self.id else {
            return (self.ordinal.to_string(), Vec::new());
        };
        let raw = &src[r.clone()];
        let lead = raw.len() - raw.trim_ascii_start().len();
        let trimmed = raw.trim_ascii();
        let s0 = r.start + lead;
        let (mut text, mut pieces) = (String::new(), Vec::new());
        crate::text::push_lossy(&mut text, &mut pieces, src, s0..s0 + trimmed.len());
        crate::text::cut_name(&mut text, &mut pieces, ID_CHARS);
        (text, pieces)
    }

    fn id_text(&self, src: &[u8]) -> String {
        self.id(src).0
    }

    fn segment(&self, src: &[u8]) -> Segment {
        let id = self.id_text(src);
        let start = String::from_utf8_lossy(&src[self.start.clone()]);
        let end = String::from_utf8_lossy(&src[self.end.clone()]);
        Segment {
            kind: "cue".into(),
            name: format!("cue {id} {start}"),
            line: self.lines.0,
            end_line: self.lines.1,
            level: None,
            signature: Some(format!("cue {id} {start} --> {end}")),
        }
    }
}

/// Calls `f` with every cue in file order until it returns false.
fn cues(src: &[u8], mut f: impl FnMut(&Cue) -> bool) {
    // The cue being read; its range ends after its last non-blank line.
    let mut open: Option<Cue> = None;
    // The previous line: (range without terminator, blank, its line number, a timing line);
    // and whether the line before it was blank or absent.
    let mut prev: Option<(Range<usize>, bool, u64, bool)> = None;
    let mut before_prev_blank = true;
    let mut ordinal = 0;
    let mut pos = 0;
    let mut line_no = 0u64;
    // WebVTT (a file that starts with `WEBVTT`): its header and its NOTE, STYLE and REGION
    // blocks hold no cue, even when a line in them looks like a timing line (M3 review r1,
    // Codex #3). `excluded` is true inside one, up to the blank line that ends it.
    let vtt = block_word(src, b"WEBVTT");
    let mut excluded = false;
    while pos < src.len() {
        line_no += 1;
        let nl = memchr::memchr(b'\n', &src[pos..]).map_or(src.len(), |k| pos + k);
        let next = (nl + 1).min(src.len());
        let content = pos..if nl > pos && src[nl - 1] == b'\r' {
            nl - 1
        } else {
            nl
        };
        let line = &src[content.clone()];
        let blank = line.iter().all(|b| matches!(b, b' ' | b'\t'));
        let block_start = prev.as_ref().is_none_or(|(_, b, _, _)| *b);
        if vtt
            && !blank
            && block_start
            && (line_no == 1
                || [&b"NOTE"[..], b"STYLE", b"REGION"]
                    .iter()
                    .any(|w| block_word(line, w)))
        {
            excluded = true;
        }
        let tm = if excluded { None } else { timing(line) };
        if excluded {
            excluded = !blank;
        } else if let Some((a, b)) = tm.clone() {
            // The line before is the identifier when it starts a block and is not a timing
            // line itself (M3 review r1, Fable #4).
            let id = prev
                .as_ref()
                .filter(|(_, pb, _, pt)| !pb && !pt && before_prev_blank)
                .map(|(r, _, n, _)| (r.clone(), *n));
            // A timing line right after a cue's text starts the next cue (a missing blank line).
            if let Some(c) = open.take()
                && !f(&c)
            {
                return;
            }
            ordinal += 1;
            let (start, start_line) = match &id {
                Some((r, n)) => (r.start, *n),
                None => (pos, line_no),
            };
            open = Some(Cue {
                range: start..next,
                id: id.map(|(r, _)| r),
                start: content.start + a.start..content.start + a.end,
                end: content.start + b.start..content.start + b.end,
                ordinal,
                lines: (start_line, line_no),
            });
        } else if blank {
            if let Some(c) = open.take()
                && !f(&c)
            {
                return;
            }
        } else if let Some(c) = open.as_mut() {
            c.range.end = next;
            c.lines.1 = line_no;
        }
        before_prev_blank = prev.as_ref().is_none_or(|(_, b, _, _)| *b);
        prev = Some((content, blank, line_no, tm.is_some()));
        pos = next;
    }
    if let Some(c) = open {
        f(&c);
    }
}

impl Processor for Subtitles {
    fn trails(
        &self,
        input: &Input,
        _: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let src = input.bytes;
        let mut out = vec![Vec::new(); anchors.len()];
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut next = 0;
        cues(src, |c| {
            while next < order.len() && anchors[order[next]] < c.range.end {
                let a = anchors[order[next]];
                if a >= c.range.start {
                    out[order[next]] = vec![c.segment(src)];
                }
                next += 1;
            }
            next < order.len()
        });
        Ok(out)
    }

    fn names(&self, input: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let src = input.bytes;
        let mut names = Vec::new();
        cues(src, |c| {
            let seg = c.segment(src);
            // "cue ", the identifier, " ", the start time: the identifier (when it is an
            // identifier line) and the time come from the Source; the rest is text of the name.
            let (id, id_pieces) = c.id(src);
            let mut pieces: crate::text::Pieces = id_pieces
                .into_iter()
                .map(|(t, s)| (4 + t.start..4 + t.end, s))
                .collect();
            let at = 4 + id.len() + 1;
            pieces.push((at..at + c.start.len(), c.start.clone()));
            names.push(Name {
                text: seg.name.clone(),
                pieces,
                trail: vec![seg],
                // A Match in a cue's name is inside the cue, as in its timing line: an empty
                // label holds nothing.
                #[allow(clippy::single_range_in_vec_init)]
                labels: vec![0..0],
                anchor: c.start.start,
            });
            true
        });
        Ok(names)
    }
}
```

- [ ] **Step 4:** `cargo test --lib processors::subtitles`: 7 tests PASS; `cargo test
  --workspace` and clippy. From review r1 (D29): `vtt_blocks_hold_no_cue_even_with_a_timing_line`
  (Codex #3, Fable #3: the first version printed `cue NOTE 00:01.000` and numbered the next cue
  4; the cue it expects is on lines 13–14) and `a_timing_line_is_never_an_identifier` (Fable #4);
  both FAILED against the first version.

- [ ] **Step 5:** by hand: `target/debug/sfor --tree needle tests/fixtures/subs/movie.srt` prints
  the tree of spec §5.6.

- [ ] **Step 6:** commit: `git add -- src/processors/subtitles.rs tests/fixtures/subs` and
  `git commit -m "Trail subtitle cues by number and start time"`.

## Chunk 5: escaped names and the labels of the new kinds

Read spec §6.5 and design note 6. This is M2's leftover: a Segment name whose escape decodes to a
line break (`"a\nb"` in JSON, TOML, YAML or JavaScript) printed the break raw and split the row.

### Task 5.1: `text::visible`

- [ ] **Step 1:** the test, first in `src/text.rs`'s `tests` module:

```rust
    #[test]
    fn visible_escapes_controls_and_line_separators_only() {
        assert_eq!(visible("plain ñ"), "plain ñ");
        assert!(matches!(visible("plain"), std::borrow::Cow::Borrowed(_)));
        assert_eq!(visible("a\nb"), "a\\nb");
        assert_eq!(visible("a\r\tb\0"), "a\\r\\tb\\0");
        assert_eq!(visible("\u{1b}[31mred"), "\\u{1b}[31mred");
        assert_eq!(
            visible("x\u{85}y\u{2028}z\u{7f}"),
            "x\\u{85}y\\u{2028}z\\u{7f}"
        );
        // A backslash and quotes are text, not escapes.
        assert_eq!(visible(r#"a\nb "q" 's'"#), r#"a\nb "q" 's'"#);
    }
```

- [ ] **Step 2:** `cargo test --lib text::`: FAILS to compile.

- [ ] **Step 3:** above the `tests` module of `src/text.rs` (D14):

```rust
/// True for the characters that would break a printed name or drive the terminal: C0 and C1
/// controls, DEL, and the Unicode line and paragraph separators.
fn breaks_display(c: char) -> bool {
    c.is_control() || c == '\u{2028}' || c == '\u{2029}'
}

/// A Segment name or signature as tree and one-line output print it (spec §6.5, M3 plan D14):
/// each character of `breaks_display` as its escape (`\n`, `\r`, `\t`, `\0`, else
/// `\u{1b}`), so a decoded `"a\nb"` key stays on its row and cannot write escape sequences to
/// the terminal. Everything else, a backslash included, is unchanged; `--json` keeps the name
/// as decoded.
pub fn visible(name: &str) -> std::borrow::Cow<'_, str> {
    if !name.chars().any(breaks_display) {
        return std::borrow::Cow::Borrowed(name);
    }
    let mut out = String::with_capacity(name.len() + 8);
    for c in name.chars() {
        if breaks_display(c) {
            out.extend(c.escape_debug());
        } else {
            out.push(c);
        }
    }
    std::borrow::Cow::Owned(out)
}
```

  (`char::escape_debug` also escapes `\` and quotes; `visible` calls it only for the characters
  of `breaks_display`.)

- [ ] **Step 4:** `cargo test --lib text::`: PASS.

### Task 5.2: the printers

- [ ] **Step 1:** the tests: `control_characters_in_names_print_escaped` in `src/output/flat.rs`
  and `labels_of_the_m3_kinds_and_escaped_names` in `src/output/tree.rs` (the `+` lines of the
  `tests` modules below).

- [ ] **Step 2:** `cargo test --lib output::`: both FAIL (`a\nb` is printed raw; `tag` gets the
  fallback label `tag li[3]`).

- [ ] **Step 3:** `fit_trail` measures and joins the escaped names; `label` escapes and knows
  `tag` and `row`:

```diff
--- a/src/output/flat.rs
+++ b/src/output/flat.rs
@@ -12,18 +12,21 @@
 /// each run of `item` Segments as `…`; else first › … › last; else cut with `…` at the end.
 fn fit_trail(trail: &[Segment], room: usize) -> String {
     let fits = |s: &str| width(s) <= room;
-    let full = trail
+    // Names print with their control characters escaped (M3 plan, D14).
+    let names: Vec<std::borrow::Cow<'_, str>> =
+        trail.iter().map(|s| text::visible(&s.name)).collect();
+    let full = names
         .iter()
-        .map(|s| s.name.as_str())
+        .map(|n| n.as_ref())
         .collect::<Vec<_>>()
         .join(" › ");
     if fits(&full) {
         return full;
     }
     let mut parts: Vec<&str> = Vec::new();
-    for s in trail {
+    for (s, n) in trail.iter().zip(&names) {
         if s.kind != "item" {
-            parts.push(&s.name);
+            parts.push(n);
         } else if parts.last() != Some(&"…") {
             parts.push("…");
         }
@@ -33,7 +36,7 @@
         return shortest;
     }
     if trail.len() > 2 {
-        let ends = format!("{} › … › {}", trail[0].name, trail[trail.len() - 1].name);
+        let ends = format!("{} › … › {}", names[0], names[names.len() - 1]);
         if width(&ends) < width(&shortest) {
             shortest = ends;
         }
@@ -285,6 +288,16 @@
         }
     }
 
+    #[test]
+    fn control_characters_in_names_print_escaped() {
+        // M3 plan, D14: a decoded line break or ESC stays on the row and drives nothing.
+        let t = [seg("key", "a\nb", 1, 1), seg("key", "\u{1b}[31m", 1, 1)];
+        assert_eq!(fit_trail(&t, 80), "a\\nb › \\u{1b}[31m");
+        // The width counts the escapes: 4 + 3 + 10 characters.
+        assert_eq!(fit_trail(&t, 17), "a\\nb › \\u{1b}[31m");
+        assert_eq!(fit_trail(&t, 16), "a\\nb › \\u{1b}[3…");
+    }
+
     fn plain(d: &Described, src: &[u8], o: &RenderOptions) -> String {
         let mut w = NoColor::new(Vec::new());
         render(d, src, o, None, &mut w).unwrap();
```

````diff
--- a/src/output/tree.rs
+++ b/src/output/tree.rs
@@ -4,12 +4,12 @@
 use std::collections::VecDeque;
 use termcolor::WriteColor;
 
-/// How a Segment is shown in the tree.
+/// How a Segment is shown in the tree, its control characters escaped (M3 plan, D14).
 pub fn label(s: &Segment) -> String {
     if let Some(sig) = &s.signature {
-        return sig.clone();
+        return crate::text::visible(sig).into_owned();
     }
-    let name = &s.name;
+    let name = crate::text::visible(&s.name);
     match s.kind.as_str() {
         "heading" => format!(
             "{} {name}",
@@ -18,7 +18,8 @@
         "code" => format!("```{name}"),
         "item" => format!("• {name}"),
         "column" => format!("column «{name}»"),
-        "document" | "key" | "element" => name.clone(),
+        // M3: markup elements (`tag`) and CSV rows (`row 12`) show their name alone.
+        "document" | "key" | "element" | "tag" | "row" => name.into_owned(),
         kind => format!("{kind} {name}"),
     }
 }
@@ -214,6 +215,21 @@
     use crate::output::tests::opts;
     use termcolor::{Ansi, NoColor};
 
+    #[test]
+    fn labels_of_the_m3_kinds_and_escaped_names() {
+        assert_eq!(label(&seg("tag", "li[3]", 1, 1)), "li[3]");
+        assert_eq!(label(&seg("row", "row 2", 1, 1)), "row 2");
+        assert_eq!(label(&seg("column", "Plan", 1, 1)), "column «Plan»");
+        let mut cue = seg("cue", "cue 1 00:00:01,000", 1, 3);
+        cue.signature = Some("cue 1 00:00:01,000 --> 00:00:02,000".into());
+        assert_eq!(label(&cue), "cue 1 00:00:01,000 --> 00:00:02,000");
+        // M3 plan, D14.
+        assert_eq!(label(&seg("key", "a\nb", 1, 1)), "a\\nb");
+        let mut sig = seg("method", "m", 1, 1);
+        sig.signature = Some("fn m(\u{1b})".into());
+        assert_eq!(label(&sig), "fn m(\\u{1b})");
+    }
+
     fn plain(d: &Described, src: &[u8], o: &RenderOptions) -> String {
         let mut w = NoColor::new(Vec::new());
         render(d, src, o, &mut w).unwrap();
````

- [ ] **Step 4:** `cargo test --workspace`: PASS, M1's and M2's output tests unchanged (no
  fixture name has a control character). Clippy.

- [ ] **Step 5:** by hand, the repro of M2's leftover: `printf '{"a\\nb": "needle"}\n' > /tmp/k.json
  && target/debug/sfor --flat needle /tmp/k.json` prints `1: a\nb │ {"a\nb": "needle"}` on one
  line (before the fix: `1: a` and `b │ …`).

- [ ] **Step 6:** commit: `git add -- src/text.rs src/output/flat.rs src/output/tree.rs` and
  `git commit -m "Print control characters in Segment names as escapes"`.

## Chunk 6: end-to-end tests and the fuzz

Runs on the merge of tanda 2. If a test here fails, the fix goes to the chunk that owns the file
(reported, then fixed in the tanda's integration), never into the test's expectation without a
reason in the report.

### Task 6.1: tests/cli_m3.rs

- [ ] **Step 1:** the whole file; every expected row was printed by the probe's binary:

```rust
//! End-to-end tests of M3's Formats (plan, Chunk 6): fixtures under
//! tests/fixtures/{markup,table,subs}, and Segment names with control characters (D14).
mod common;

use common::{Fixture, code, run, stderr, stdout};

const FORMATS: &[(&str, &str)] = &[
    ("html", "markup/page.html"),
    ("xml", "markup/feed.xml"),
    ("csv", "table/plans.csv"),
    ("tsv", "table/people.tsv"),
    ("srt", "subs/movie.srt"),
    ("vtt", "subs/talk.vtt"),
];

fn flat(fx: &Fixture, args: &[&str]) -> String {
    let o = run(&mut fx.sfor(args.iter().copied().chain(["--flat"])));
    assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
    assert_eq!(stderr(&o), "", "{args:?}");
    stdout(&o)
}

#[test]
fn m3_01_every_format_in_three_layouts() {
    // Spec §11 #2: a fixture and a snapshot of tree, one-line and JSON output per Format.
    let fx = Fixture::new();
    for (name, file) in FORMATS {
        for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
            let o = run(&mut fx.sfor(["needle", file, flag]));
            assert_eq!(code(&o), 0, "{name} {layout}: {}", stderr(&o));
            assert!(stderr(&o).is_empty(), "{name} {layout}: {}", stderr(&o));
            let mut out = stdout(&o);
            if layout == "json" {
                common::assert_valid_json_schema(&out);
                out = common::mask_json_times(&out);
            }
            insta::assert_snapshot!(format!("m3_01_{name}_{layout}"), out);
        }
    }
}

#[test]
fn m3_02_html_element_paths() {
    // Spec §11 #18.
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "markup/page.html"]),
        "4: head › title │   <title>Pricing needle</title>\n\
         5: head › script │   <script>if (a < b) { label = \"</div> needle\"; }</script>\n\
         12: nav.top.menu › ul › li[3] │       <li><a href=\"/needle\">Needle</a>\n\
         19: main#content › section#pricing.card.wide › p.note[2] │       <p class=\"note\">The needle plan is free.\n\
         23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9\n\
         26: main#content › div › div › div │     <div><div><div>deep needle</div></div></div>\n\
         27: main#content │     <img src=\"needle.png\" alt=\"\">\n"
    );
}

#[test]
fn m3_03_xml_paths_and_a_syntax_error() {
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "markup/feed.xml"]),
        "3:   <!ENTITY pub \"needle press\">\n\
         6: catalog │   <!-- needle in a comment -->\n\
         12: catalog › book[2] › title │     <title>The needle</title>\n\
         14: catalog › book[2] › note │     <note><![CDATA[ a < b needle ]]></note>\n\
         17: catalog › magazine › title │   <magazine><title>needle monthly</title></magazine>\n"
    );
    assert_eq!(
        flat(&fx, &["needle", "markup/broken.xml"]),
        "2: root › ok │   <ok>needle before</ok>\n\
         3: root › a › b │   <a><b>needle inside</a>\n\
         4:   <c>needle after</c>\n"
    );
}

#[test]
fn m3_04_csv_and_tsv_rows_and_columns() {
    // Spec §11 #19.
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "table/plans.csv"]),
        "3: row 2 › Plan │ 2,Bea,needle,\"likes \"\"quotes\"\", and commas\"\n\
         5: row 3 › Notes │ lines with needle\"\n\
         7: row 4 › Notes │ 4,Dan,team,needle at the end,extra\n"
    );
    // `;` is sniffed from the first line; the CRLF stays in the printed line, as rg's.
    assert_eq!(
        flat(&fx, &["needle", "table/semi.csv"]),
        "2: row 1 › Plan │ Ana;needle;0\r\n3: row 2 › Precio │ Bea;pro;9,50 needle\r\n"
    );
    assert_eq!(
        flat(&fx, &["needle", "table/people.tsv"]),
        "2: row 1 › Note │ \"Ana\tadmin\tneedle \"quoted\n3: row 2 › Role │ Bea\tneedle\tok\n"
    );
}

#[test]
fn m3_05_subtitle_cues() {
    // Spec §11 #20: the cue number and its start time; BOM and CRLF in movie.srt.
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "subs/movie.srt"]),
        "3: cue 1 00:00:01,000 │ Where is the needle?\r\n\
         8: cue 2 00:00:04,000 │ - A needle, in there?\r\n\
         12: cue 3 00:01:02,500 │ <i>needle</i>\r\n"
    );
    assert_eq!(
        flat(&fx, &["needle", "subs/talk.vtt"]),
        "1: WEBVTT - needle talk\n\
         3: NOTE needle in a note\n\
         10: cue 2 00:02.500 │ <v Ana>The needle is here.\n\
         13: cue 3 01:00:00.000 │ needle at one hour\n"
    );
    let o = stdout(&run(&mut fx.sfor(["needle", "subs/movie.srt", "--tree"])));
    assert!(
        o.contains("  cue 2 00:00:04,000 --> 00:00:06,000\n    8:- A needle"),
        "{o}"
    );
}

#[test]
fn m3_06_within_and_expand() {
    let fx = Fixture::new();
    assert_eq!(
        flat(
            &fx,
            &["needle", "markup/page.html", "--within", "pricing › table"]
        ),
        "23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9\n"
    );
    // A CSV column's Section is its row (D10): --expand prints the record, both lines.
    let o = stdout(&run(&mut fx.sfor([
        "lines with",
        "table/plans.csv",
        "--expand",
    ])));
    assert_eq!(
        o,
        "4-5: row 3 › Notes\n3,Carla,pro,\"two\nlines with needle\"\n\n"
    );
}

#[test]
fn m3_07_in_headings() {
    // Element names, CSV headers (no Trail: the Match is the column's name) and cue times.
    let fx = Fixture::new();
    let names = |file| flat(&fx, &["--in-headings", "pricing|^Plan$|00:01", file]);
    assert_eq!(
        names("markup/page.html"),
        "16: main#content │     <section id=\"pricing\" class=\"card wide\">\n"
    );
    assert_eq!(names("table/plans.csv"), "1: id,Name,Plan,Notes\n");
    assert_eq!(
        names("subs/movie.srt"),
        "2: cue 1 00:00:01,000 │ 00:00:01,000 --> 00:00:03,500\r\n\
         11: cue 3 00:01:02,500 │ 00:01:02,500 --> 00:01:04,000\r\n"
    );
}

#[test]
fn m3_08_a_name_with_a_decoded_line_break_stays_on_its_row() {
    // The M2 leftover (D14): `"a\nb"` decodes to a line break in every Format with escapes.
    let fx = Fixture::new();
    for (file, body) in [
        ("k.json", "{\"a\\nb\": \"needle\"}\n"),
        ("k.toml", "\"a\\nb\" = \"needle\"\n"),
        ("k.yaml", "\"a\\nb\": needle\n"),
        ("k.js", "const o = {\"a\\nb\": () => \"needle\"};\n"),
    ] {
        std::fs::write(fx.file(file), body).unwrap();
        assert_eq!(
            flat(&fx, &["needle", file]),
            format!("1: a\\nb │ {body}"),
            "{file}"
        );
        let tree = stdout(&run(&mut fx.sfor(["needle", file, "--tree"])));
        assert_eq!(tree.lines().count(), 3, "{file}: {tree}");
    }
    let json = stdout(&run(&mut fx.sfor(["needle", "k.json", "--json"])));
    assert!(
        json.contains(r#""trail":[{"kind":"key","name":"a\nb""#),
        "{json}"
    );
    // An escape sequence in a key cannot drive the terminal.
    std::fs::write(fx.file("esc.json"), "{\"\\u001b[31m\": \"needle\"}\n").unwrap();
    assert_eq!(
        flat(&fx, &["needle", "esc.json"]),
        "1: \\u{1b}[31m │ {\"\\u001b[31m\": \"needle\"}\n"
    );
}

#[test]
fn m3_09_xml_without_an_extension_by_its_declaration() {
    let fx = Fixture::new();
    std::fs::write(
        fx.file("feed"),
        "<?xml version=\"1.0\"?>\n<r><a>needle</a></r>\n",
    )
    .unwrap();
    assert_eq!(
        flat(&fx, &["needle", "feed"]),
        "2: r › a │ <r><a>needle</a></r>\n"
    );
}

#[test]
fn m3_10_no_trail_is_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    for dir in ["markup", "table", "subs"] {
        // Both walk in parallel: compare the lines in order.
        let sorted = |o: std::process::Output| {
            let mut l: Vec<String> = stdout(&o).lines().map(String::from).collect();
            l.sort();
            l
        };
        let ours = sorted(run(&mut fx.sfor(["--no-trail", "-n", "needle", dir])));
        let theirs = sorted(run(&mut fx.rg(["-n", "needle", dir])));
        assert!(!ours.is_empty(), "{dir}");
        assert_eq!(ours, theirs, "{dir}");
    }
}

#[test]
fn m3_11_list_formats_has_the_six() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor(["--list-formats"])));
    for (name, _) in FORMATS {
        assert!(
            o.lines()
                .any(|l| l.starts_with(&format!("{name} ")) && l.contains("built-in")),
            "{name}: {o}"
        );
    }
}

#[test]
fn m3_12_utf16_subtitles_keep_their_cues() {
    let fx = Fixture::new();
    let text = "1\r\n00:00:01,000 --> 00:00:02,000\r\nañá needle\r\n";
    let mut bytes = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(fx.file("w.srt"), bytes).unwrap();
    assert_eq!(
        flat(&fx, &["needle", "w.srt"]),
        "3: cue 1 00:00:01,000 │ añá needle\r\n"
    );
}

#[test]
fn m3_13_latin1_headers_are_decoded_lossily() {
    let fx = Fixture::new();
    std::fs::write(fx.file("l.csv"), b"A\xf1o,Valor\n1999,needle\n").unwrap();
    let o = run(&mut fx.sfor(["needle", "l.csv", "--flat"]));
    assert_eq!(stdout(&o), "2: row 1 › Valor │ 1999,needle\n");
    std::fs::write(fx.file("m.csv"), b"A\xf1o,Valor\nneedle,1\n").unwrap();
    let o = run(&mut fx.sfor(["needle", "m.csv", "--flat"]));
    assert_eq!(stdout(&o), "2: row 1 › A\u{FFFD}o │ needle,1\n");
}

#[test]
fn m3_14_a_cut_name_stays_searchable_up_to_its_cut() {
    // M3 review r1, Codex #6: the visible part of a cut name is still a name.
    let fx = Fixture::new();
    let a = "A".repeat(50);
    let n = "ñ".repeat(50);
    std::fs::write(fx.file("long.csv"), format!("id,{a}\n1,x\n")).unwrap();
    std::fs::write(fx.file("long.html"), format!("<div id=\"{a}\">x</div>\n")).unwrap();
    std::fs::write(
        fx.file("long.vtt"),
        format!("WEBVTT\n\n{a}\n00:01.000 --> 00:02.000\nx\n"),
    )
    .unwrap();
    std::fs::write(fx.file("longn.csv"), format!("id,{n}\n1,x\n")).unwrap();
    for (file, pattern) in [
        ("long.csv", "AAAA"),
        ("long.html", "AAAA"),
        ("long.vtt", "AAAA"),
        ("longn.csv", "ññññ"),
    ] {
        let o = run(&mut fx.sfor(["--flat", "--in-headings", pattern, file]));
        assert_eq!(code(&o), 0, "{file}: {}", stderr(&o));
        assert_eq!(stdout(&o).lines().count(), 1, "{file}");
    }
    // Latin-1 before the letters: the replaced byte is a piece of its own (M3 review r2,
    // Codex #2).
    let l1 = |file: &str, body: Vec<u8>| {
        std::fs::write(fx.file(file), body).unwrap();
        let o = run(&mut fx.sfor(["--flat", "--in-headings", "AAAA", file]));
        assert_eq!(code(&o), 0, "{file}: {}", stderr(&o));
    };
    l1(
        "l1.csv",
        [&b"id,\xe9"[..], &[b'A'; 40], b"\n1,x\n"].concat(),
    );
    l1(
        "l1.html",
        [&b"<div id=\"\xe9"[..], &[b'A'; 36], b"\">x</div>\n"].concat(),
    );
    l1(
        "l1.vtt",
        [
            &b"WEBVTT\n\n\xe9"[..],
            &[b'A'; 40],
            b"\n00:01.000 --> 00:02.000\nx\n",
        ]
        .concat(),
    );
    // The cut's `…` maps to nothing.
    let o = run(&mut fx.sfor(["--in-headings", "…", "long.csv"]));
    assert_eq!(code(&o), 1);
}
```

- [ ] **Step 2:** `INSTA_UPDATE=always cargo test --test cli_m3`: the 14 tests PASS and `m3_01`
  writes 18 snapshots. Read every `tests/snapshots/cli_m3__m3_01_*.snap` in full: each tree shows
  the Segments of the flat rows of `m3_02`–`m3_05`, each JSON line validates (the test checks it)
  and carries `"format"` and the kinds `tag`, `row`/`column` or `cue`. For example
  `cli_m3__m3_01_csv_tree.snap`:

```
table/plans.csv
  row 2
    column «Plan»
      3:2,Bea,needle,"likes ""quotes"", and commas"
  row 3
    column «Notes»
      5:lines with needle"
  row 4
    column «Notes»
      7:4,Dan,team,needle at the end,extra
```

- [ ] **Step 3:** RED check: `git show f7cccfe:src/output/flat.rs > src/output/flat.rs`, then
  `cargo test --test cli_m3 m3_08`: FAILS (the row splits at the line break); then
  `git checkout -- src/output/flat.rs` and the test PASSES again. Report it: this is the test that
  pins M2's leftover.

- [ ] **Step 3b:** `m3_14` is review r1's Codex #6: with the release binary built before
  `cut_name` (Task 1.4) the four `--in-headings` runs exit 1; with it, each prints one row.

- [ ] **Step 4:** commit: `git add -- tests/cli_m3.rs tests/snapshots` (only the
  `cli_m3__*` files are new) and `git commit -m "Test the M3 Formats end to end"`.

### Task 6.2: the M3 targets of tests/fuzz.rs

- [ ] **Step 1:** the targets: three token pools (`MARKUP`, `TABLE`, `SUBS`), three generators
  (`markup_doc`, `table_doc`, `subs_doc`), three `Shape`s, a `Proc` enum replacing
  `treesitter: bool`, `params` accepting a Definition without `[params]`, and six targets:

```diff
--- a/tests/fuzz.rs
+++ b/tests/fuzz.rs
@@ -1,4 +1,5 @@
-//! A small fuzz of the keypath and treesitter Processors (review m2-code-r1): generated inputs
+//! A small fuzz of the keypath, treesitter, markup, csv and subtitles Processors (review
+//! m2-code-r1; M3 plan, Task 6.2): generated inputs
 //! with lone CRs, CRLF, BOMs, multi-byte keys and values, and truncated files, looking for
 //! panics and for name pieces outside their text, outside their Source, or over other bytes.
 //! The generator is seeded, so a failure reproduces; `SFOR_FUZZ_SEED` and `SFOR_FUZZ_N` (inputs
@@ -7,7 +8,10 @@
 
 use searchfor::model::{Input, Name};
 use searchfor::processors::Processor;
+use searchfor::processors::csv::Csv;
 use searchfor::processors::keypath::Keypath;
+use searchfor::processors::markup::Markup;
+use searchfor::processors::subtitles::Subtitles;
 use searchfor::processors::treesitter::TreeSitter;
 use std::panic::{AssertUnwindSafe, catch_unwind};
 use std::path::Path;
@@ -73,6 +77,26 @@
     r#""a\u0062""#, r#"'\x41'"#, "\\\n", r#""\uD83D""#, r#"{ "k\u{31}": () => needle }"#,
 ];
 
+#[rustfmt::skip]
+const MARKUP: &[&str] = &[
+    "<", ">", "</", "/>", "<div>", "</div>", "<li>", "<ul>", "</ul>", "<p class=\"a b\">", "<td>",
+    "<tr>", "<table>", "<br>", "<script>", "</script>", "<style>", "<!--", "-->", "<![CDATA[",
+    "]]>", "<?xml version=\"1.0\"?>", "<!DOCTYPE x [", "]>", "<x:y>", "</x:y>", "<a id='ñ'>",
+    "</a>", " id=", "\"", "'", "=", "<é>", "</é>", "<b/>", "&lt;", "<html>", "<body>",
+];
+
+#[rustfmt::skip]
+const TABLE: &[&str] = &[
+    ",", ";", "\t", "|", "\"", "\"\"", "\n", "\r\n", "a", "ñandú", "1", "needle", "\"x,y\"",
+    "\"a\nb\"", " ", "\u{feff}",
+];
+
+#[rustfmt::skip]
+const SUBS: &[&str] = &[
+    "1", "12", "00:00:01,000", "00:01.500", " --> ", "-->", "00:00:02,000", "WEBVTT", "NOTE ",
+    "\n", "\r\n", "\n\n", "<i>", "</i>", "needle", "ñ", " align:start", ":", ",", ".",
+];
+
 /// Key spellings, plain and multi-byte; the generators quote and escape some.
 #[rustfmt::skip]
 const KEYS: &[&str] = &["a", "key", "ñandú", "é", "日本", "😀k", "k_1", "Café"];
@@ -241,6 +265,78 @@
     s
 }
 
+/// Nested elements with attributes, text and the constructs the scanner skips.
+fn markup_doc(rng: &mut Rng, depth: usize) -> String {
+    let mut s = String::new();
+    for _ in 0..1 + rng.below(4) {
+        let tag = rng.pick(&["div", "li", "p", "td", "tr", "x:y", "é", "b", "script"]);
+        let attrs = match rng.below(4) {
+            0 => format!(" id=\"{}\"", rng.pick(KEYS)),
+            1 => format!(" class='{} {}'", rng.pick(KEYS), rng.pick(KEYS)),
+            2 => " a=b".to_string(),
+            _ => String::new(),
+        };
+        let inner = if depth < 4 && rng.below(2) == 0 {
+            markup_doc(rng, depth + 1)
+        } else {
+            rng.pick(VALUES).to_string()
+        };
+        let close = if rng.below(4) == 0 {
+            String::new()
+        } else {
+            format!("</{tag}>")
+        };
+        s.push_str(&format!("<{tag}{attrs}>{inner}{close}{}", rng.br()));
+        if rng.below(5) == 0 {
+            s.push_str(rng.pick(&["<!-- c -->", "<![CDATA[ < ]]>", "<br>", "<?p?>"]));
+        }
+    }
+    s
+}
+
+/// A header and rows, quoted fields with delimiters, `""` and line breaks.
+fn table_doc(rng: &mut Rng) -> String {
+    let d = rng.pick(&[",", ";", "\t"]);
+    let mut s = String::new();
+    for _ in 0..1 + rng.below(6) {
+        let cells: Vec<String> = (0..1 + rng.below(4))
+            .map(|_| match rng.below(4) {
+                0 => format!("\"{}{d}\"\"{}\"", rng.pick(VALUES), rng.pick(KEYS)),
+                1 => format!("\"{}{}x\"", rng.pick(VALUES), rng.br()),
+                _ => rng.pick(VALUES).to_string(),
+            })
+            .collect();
+        s.push_str(&cells.join(d));
+        s.push_str(rng.br());
+    }
+    s
+}
+
+/// Cues with and without identifiers, settings and blank lines, SubRip or WebVTT timing.
+fn subs_doc(rng: &mut Rng) -> String {
+    let mut s = if rng.below(2) == 0 {
+        String::from("WEBVTT\n\n")
+    } else {
+        String::new()
+    };
+    for i in 0..1 + rng.below(5) {
+        let br = rng.br();
+        if rng.below(2) == 0 {
+            s.push_str(&format!("{}{br}", i + 1));
+        }
+        let ts = rng.pick(&[
+            "00:00:01,000 --> 00:00:02,000",
+            "00:01.000 --> 00:02.000 align:end",
+            "1:2:3.4 --> 5:6:7.8",
+        ]);
+        s.push_str(&format!("{ts}{br}{}{br}", rng.pick(VALUES)));
+        if rng.below(3) > 0 {
+            s.push_str(br);
+        }
+    }
+    s
+}
+
 /// The structured generator of a target.
 #[derive(Clone, Copy)]
 enum Shape {
@@ -248,6 +344,9 @@
     Json,
     Toml,
     Code,
+    Markup,
+    Table,
+    Subs,
 }
 
 fn structured(rng: &mut Rng, shape: Shape) -> Vec<u8> {
@@ -256,6 +355,9 @@
         Shape::Json => json_value(rng, 0),
         Shape::Toml => toml_doc(rng),
         Shape::Code => code_doc(rng),
+        Shape::Markup => markup_doc(rng, 0),
+        Shape::Table => table_doc(rng),
+        Shape::Subs => subs_doc(rng),
     }
     .into_bytes()
 }
@@ -329,7 +431,11 @@
     )
     .unwrap();
     let t: toml::Table = toml::from_str(&src).unwrap();
-    t["params"].as_table().unwrap().clone()
+    // srt and vtt have no [params].
+    t.get("params")
+        .and_then(|p| p.as_table())
+        .cloned()
+        .unwrap_or_default()
 }
 
 fn fixtures(dir: &str, ext: &[&str]) -> Vec<Vec<u8>> {
@@ -437,18 +543,28 @@
 struct Target {
     format: &'static str,
     ext: &'static str,
-    treesitter: bool,
+    proc: Proc,
     shape: Shape,
     pool: &'static [&'static str],
     seeds: Vec<Vec<u8>>,
     params: toml::Table,
 }
 
-/// (format, extension, treesitter, shape, token pool, fixture directory, fixture extensions).
+/// The Processor a target runs.
+#[derive(Clone, Copy)]
+enum Proc {
+    Keypath,
+    TreeSitter,
+    Markup,
+    Csv,
+    Subtitles,
+}
+
+/// (format, extension, Processor, shape, token pool, fixture directory, fixture extensions).
 type Row = (
     &'static str,
     &'static str,
-    bool,
+    Proc,
     Shape,
     &'static [&'static str],
     &'static str,
@@ -457,24 +573,30 @@
 
 fn targets() -> Vec<Target> {
     #[rustfmt::skip]
-    let list: [Row; 11] = [
-        ("yaml", "yaml", false, Shape::Yaml, YAML, "data", &["yaml"]),
-        ("json", "json", false, Shape::Json, JSON, "data", &["json", "jsonl"]),
-        ("toml", "toml", false, Shape::Toml, TOML, "data", &["toml"]),
-        ("rust", "rs", true, Shape::Code, CODE, "code", &["rs"]),
-        ("python", "py", true, Shape::Code, CODE, "code", &["py"]),
-        ("javascript", "js", true, Shape::Code, CODE, "code", &["js"]),
-        ("typescript", "ts", true, Shape::Code, CODE, "code", &["ts"]),
-        ("tsx", "tsx", true, Shape::Code, CODE, "code", &["tsx"]),
-        ("go", "go", true, Shape::Code, CODE, "code", &["go"]),
-        ("php", "php", true, Shape::Code, CODE, "code", &["php"]),
-        ("bash", "sh", true, Shape::Code, CODE, "code", &["sh"]),
+    let list: [Row; 17] = [
+        ("yaml", "yaml", Proc::Keypath, Shape::Yaml, YAML, "data", &["yaml"]),
+        ("json", "json", Proc::Keypath, Shape::Json, JSON, "data", &["json", "jsonl"]),
+        ("toml", "toml", Proc::Keypath, Shape::Toml, TOML, "data", &["toml"]),
+        ("rust", "rs", Proc::TreeSitter, Shape::Code, CODE, "code", &["rs"]),
+        ("python", "py", Proc::TreeSitter, Shape::Code, CODE, "code", &["py"]),
+        ("javascript", "js", Proc::TreeSitter, Shape::Code, CODE, "code", &["js"]),
+        ("typescript", "ts", Proc::TreeSitter, Shape::Code, CODE, "code", &["ts"]),
+        ("tsx", "tsx", Proc::TreeSitter, Shape::Code, CODE, "code", &["tsx"]),
+        ("go", "go", Proc::TreeSitter, Shape::Code, CODE, "code", &["go"]),
+        ("php", "php", Proc::TreeSitter, Shape::Code, CODE, "code", &["php"]),
+        ("bash", "sh", Proc::TreeSitter, Shape::Code, CODE, "code", &["sh"]),
+        ("html", "html", Proc::Markup, Shape::Markup, MARKUP, "markup", &["html"]),
+        ("xml", "xml", Proc::Markup, Shape::Markup, MARKUP, "markup", &["xml"]),
+        ("csv", "csv", Proc::Csv, Shape::Table, TABLE, "table", &["csv"]),
+        ("tsv", "tsv", Proc::Csv, Shape::Table, TABLE, "table", &["tsv"]),
+        ("srt", "srt", Proc::Subtitles, Shape::Subs, SUBS, "subs", &["srt"]),
+        ("vtt", "vtt", Proc::Subtitles, Shape::Subs, SUBS, "subs", &["vtt"]),
     ];
     list.into_iter()
-        .map(|(format, ext, treesitter, shape, pool, dir, exts)| Target {
+        .map(|(format, ext, proc, shape, pool, dir, exts)| Target {
             format,
             ext,
-            treesitter,
+            proc,
             shape,
             pool,
             seeds: fixtures(dir, exts),
@@ -502,7 +624,13 @@
     std::panic::set_hook(Box::new(|_| {}));
     for (t, set) in targets.iter().zip(inputs(&targets, count())) {
         let ts = TreeSitter::default();
-        let p: &dyn Processor = if t.treesitter { &ts } else { &Keypath };
+        let p: &dyn Processor = match t.proc {
+            Proc::Keypath => &Keypath,
+            Proc::TreeSitter => &ts,
+            Proc::Markup => &Markup,
+            Proc::Csv => &Csv,
+            Proc::Subtitles => &Subtitles,
+        };
         for bytes in &set {
             tried += 1;
             if let Err(e) = run(t, p, bytes, &mut rng) {
```

- [ ] **Step 2:** `cargo test --release --test fuzz`: PASS, `fuzz: 17000 inputs, 0 failures`.
  Then `SFOR_FUZZ_N=20000 SFOR_FUZZ_SEED=42 cargo test --release --test fuzz
  generated_inputs` (about four minutes): PASS. A failure prints the input; it goes to C2–C4's
  owner as a bug, with the input as a new unit test.

- [ ] **Step 3:** commit: `git add -- tests/fuzz.rs` and
  `git commit -m "Fuzz the markup, csv and subtitles Processors"`.

## Chunk 7: perf ceilings and the benchmark

### Task 7.1: the ceilings of tests/perf.rs

- [ ] **Step 1:** append `m3_ceiling` and its six tests, one per M3 Format (each writes a 50 MB
  file with three `needle`s — the first unit, one in the middle, the last — and checks the first
  and last Trails before timing). The diff is against `tests/perf.rs` after Tasks 1.5 and 2.5:

```diff
--- a/tests/perf.rs
+++ b/tests/perf.rs
@@ -819,3 +819,181 @@
     let body = format!("{}needle{}", "<x>".repeat(20_000), " ".repeat(200_000));
     markup_linear("trim.html", body, 256);
 }
+
+/// One M3 Format at 50 MB (M3 plan, Task 7.1): `unit(i, needle)` writes the i-th repetition,
+/// with `needle` at i = 0, in the middle and at the end. sfor must print three rows whose
+/// Trails start with `first` and `last`, within rg's time + 1 s (50 MB/s at least) and rg's
+/// RSS + 32 MiB: the walk keeps three short stacks, not the file's structure.
+fn m3_ceiling(
+    file: &str,
+    head: &str,
+    unit: impl Fn(usize, &str) -> String,
+    tail: &str,
+    first: &str,
+    last: &str,
+) {
+    let fx = Fixture::new();
+    let mut v = String::with_capacity(SIZE + 4096);
+    v.push_str(head);
+    let mut i = 0;
+    let mut middle = None;
+    while v.len() < SIZE {
+        let needle = if i == 0 || (middle.is_none() && v.len() >= SIZE / 2) {
+            if i > 0 {
+                middle = Some(i);
+            }
+            "needle"
+        } else {
+            "hay"
+        };
+        v.push_str(&unit(i, needle));
+        i += 1;
+    }
+    v.push_str(&unit(i, "needle"));
+    v.push_str(tail);
+    assert_eq!(v.matches("needle").count(), 3, "{file}");
+    std::fs::write(fx.file(file), &v).unwrap();
+    drop(v);
+    let args = ["--flat", "needle", file];
+    let (out, err) = sfor_output(&fx, &args);
+    assert_eq!(err, "", "{file}");
+    let trails: Vec<&str> = out
+        .lines()
+        .map(|l| l.split_once(" │ ").map_or("", |(t, _)| t))
+        .collect();
+    assert_eq!(trails.len(), 3, "{file}: {out:.400}");
+    // `{n}` is the 1-based position of the last unit: its book, row or cue number.
+    let last = last.replace("{n}", &(i + 1).to_string());
+    assert!(
+        trails[0].ends_with(first),
+        "{file}: {:?} ≠ …{first}",
+        trails[0]
+    );
+    assert!(
+        trails[2].ends_with(&last),
+        "{file}: {:?} ≠ …{last}",
+        trails[2]
+    );
+    let Some(((t, rss), (base, base_rss))) = against_rg(&fx, &args) else {
+        return;
+    };
+    eprintln!(
+        "50 MB {file}: sfor {t:?}, RSS {} MiB; rg {base:?}, RSS {} MiB",
+        rss / 1_048_576,
+        base_rss / 1_048_576
+    );
+    assert!(
+        t <= base + Duration::from_secs(1),
+        "{file}: {t:?} is more than rg's {base:?} + 1 s"
+    );
+    assert!(
+        rss <= base_rss + 32 * 1_048_576,
+        "{file}: RSS {rss} is more than rg's {base_rss} + 32 MiB"
+    );
+}
+
+#[test]
+#[ignore]
+fn markup_html_walks_at_fifty_megabytes_a_second() {
+    let _alone = alone();
+    m3_ceiling(
+        "big.html",
+        "<!DOCTYPE html>\n<html><body>\n<main id=\"all\">\n",
+        |i, n| {
+            format!(
+                "<section class=\"s\">\n  <h2>Part {i}</h2>\n  <ul><li>one<li>two <b>{n}</b><li>three</ul>\n  <p>text<br>\n</section>\n"
+            )
+        },
+        "</main>\n</body></html>\n",
+        ": main#all › section.s[1] › ul › li[2] › b",
+        " › ul › li[2] › b",
+    );
+}
+
+#[test]
+#[ignore]
+fn markup_xml_walks_at_fifty_megabytes_a_second() {
+    let _alone = alone();
+    m3_ceiling(
+        "big.xml",
+        "<?xml version=\"1.0\"?>\n<catalog>\n",
+        |i, n| {
+            format!(
+                "  <book id=\"b{i}\">\n    <title>{n}</title>\n    <!-- c -->\n    <price>{i}</price>\n  </book>\n"
+            )
+        },
+        "</catalog>\n",
+        ": catalog › book[1] › title",
+        ": catalog › book[{n}] › title",
+    );
+}
+
+#[test]
+#[ignore]
+fn csv_reads_at_fifty_megabytes_a_second() {
+    let _alone = alone();
+    m3_ceiling(
+        "big.csv",
+        "id,Name,Plan,Notes\n",
+        |i, n| {
+            // Only units without a needle are quoted, so the three needles are all in `Plan`
+            // (Fable r1 #9).
+            if i % 7 == 3 && n == "hay" {
+                format!("{i},Ana,pro,\"two, \"\"quoted\"\"\nlines {n}\"\n")
+            } else {
+                format!("{i},Bea,{n},plain notes here\n")
+            }
+        },
+        "",
+        ": row 1 › Plan",
+        // Unit i is row i + 1.
+        ": row {n} › Plan",
+    );
+}
+
+#[test]
+#[ignore]
+fn subtitles_read_at_fifty_megabytes_a_second() {
+    let _alone = alone();
+    m3_ceiling(
+        "big.srt",
+        "",
+        |i, n| {
+            format!(
+                "{}\r\n00:00:01,000 --> 00:00:02,000\r\nline of {n}\r\nsecond line\r\n\r\n",
+                i + 1
+            )
+        },
+        "",
+        ": cue 1 00:00:01,000",
+        ": cue {n} 00:00:01,000",
+    );
+}
+
+#[test]
+#[ignore]
+fn tsv_reads_at_fifty_megabytes_a_second() {
+    let _alone = alone();
+    m3_ceiling(
+        "big.tsv",
+        "id\tName\tPlan\tNotes\n",
+        |i, n| format!("{i}\tBea\t{n}\tplain \"notes\" here\n"),
+        "",
+        ": row 1 › Plan",
+        ": row {n} › Plan",
+    );
+}
+
+#[test]
+#[ignore]
+fn vtt_reads_at_fifty_megabytes_a_second() {
+    let _alone = alone();
+    m3_ceiling(
+        "big.vtt",
+        "WEBVTT\n\nNOTE a note\n\n",
+        |_, n| format!("00:00:01.000 --> 00:00:02.000 align:start\nline of {n}\nsecond line\n\n"),
+        "",
+        ": cue 1 00:00:01.000",
+        ": cue {n} 00:00:01.000",
+    );
+}
```

- [ ] **Step 2:** RED check, as M2's ceilings: `SFOR_BIN=<a build of f7cccfe> cargo test
  --release --test perf -- --ignored _a_second`: the six FAIL on their Trail assertions (the
  Formats are Plain there).

- [ ] **Step 3:** check the reference first: `/usr/bin/rg --version` prints `ripgrep 14.1.1`.
  Then `SFOR_REQUIRE_REFERENCE=1 cargo test --release --test perf -- --ignored --nocapture
  markup_ csv_ tsv_ subtitles_ vtt_`: PASS; without `SFOR_REQUIRE_REFERENCE=1` a missing rg or
  `/usr/bin/time` only skips the timing, and such a run does not count for criterion 8. Probe
  values (release, this machine): HTML 0.50 s, XML 0.27 s, CSV 0.26 s, TSV 0.25 s, SRT 0.14 s, VTT 0.13 s, rg about 0.02 s, RSS 57 MiB against rg's 56. Then the whole `SFOR_REQUIRE_REFERENCE=1 cargo test --release --test
  perf -- --ignored`: M1's and M2's ceilings still PASS.

- [ ] **Step 4:** commit: `git add -- tests/perf.rs` and
  `git commit -m "Bound the time and memory of the M3 Processors on 50 MB files"`.

### Task 7.2: scripts/bench-m3.sh

- [ ] **Step 1:** the script (D15, D16); it generates its corpus with `awk`, so it runs anywhere
  with hyperfine and jq:

```bash
#!/usr/bin/env bash
# M3 acceptance (spec §11 #13, M3 plan D15), on a generated corpus of HTML, XML, CSV, TSV, SRT
# and VTT files (small ones and one big file of each Format), against ripgrep 14.1.1:
#   - a pattern no file matches, and --no-trail: sfor ≤ 1.1 × rg;
#   - a pattern matching under 5 % of the files, the big ones included: sfor − rg ≤ the
#     largest matching file ÷ 50 MB/s + all matching bytes ÷ (50 MB/s × threads) + 0.05 s
#     (a file is walked by one thread);
#   - a dense pattern (every non-empty line of the CSV files matches, once): time and RSS
#     reported, not bounded (D16); sfor must still exit 0 and print every line rg prints.
# Usage: scripts/bench-m3.sh [CORPUS_DIR]  (generated there when missing; a temporary one by
# default). Needs hyperfine, jq, awk, GNU stat and /usr/bin/time. SFOR_BIN replaces the sfor
# under test (SFOR_BIN=scripts/fake-sfor-exit-2.sh must make it exit 1). The reference is
# SFOR_RG, else /usr/bin/rg, which must be ripgrep 14.1.1 (M3 review r1, Codex #8): another rg
# may come first on PATH.
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
sparse='zqsparse'; none='zzqq-searchfor-bench-none'; dense='^.'
fail=0

# gen DIR: small files (every 200th holds the sparse word) and one big file (9–19 MB) per
# Format, whose last line holds it.
gen() {
  mkdir -p "$1"
  awk -v d="$1" -v sp="$sparse" 'BEGIN {
    for (i = 0; i < 8000; i++) {
      w = (i % 200 == 0) ? sp : "word"
      f = sprintf("%s/p%04d.html", d, i)
      printf "<!DOCTYPE html>\n<html><body>\n<main id=\"m\">\n" > f
      for (j = 0; j < 40; j++) printf "<section class=\"s\"><h2>T %d</h2><ul><li>one<li>%s</ul><p>text<br>\n</section>\n", j, (j == 20 ? w : "x") > f
      printf "</main></body></html>\n" > f; close(f)
      if (i % 2) continue
      f = sprintf("%s/c%04d.xml", d, i)
      printf "<?xml version=\"1.0\"?>\n<catalog>\n" > f
      for (j = 0; j < 60; j++) printf "  <book id=\"b%d\"><title>%s</title><price>%d</price></book>\n", j, (j == 30 ? w : "t"), j > f
      printf "</catalog>\n" > f; close(f)
      f = sprintf("%s/u%04d.tsv", d, i)
      printf "id\tName\tPlan\n" > f
      for (j = 0; j < 80; j++) printf "%d\tAna\t%s plan\n", j, (j == 40 ? w : "pro") > f
      close(f)
      f = sprintf("%s/t%04d.csv", d, i)
      printf "id,Name,Plan,Notes\n" > f
      for (j = 0; j < 80; j++) printf "%d,Ana,%s plan,\"a, \"\"b\"\"\"\n", j, (j == 40 ? w : "pro") > f
      close(f)
      if (i % 4) continue
      f = sprintf("%s/s%04d.srt", d, i)
      for (j = 1; j <= 60; j++) printf "%d\r\n00:00:%02d,000 --> 00:00:%02d,500\r\nline %s\r\n\r\n", j, j % 60, j % 60, (j == 30 ? w : "x") > f
      close(f)
      f = sprintf("%s/v%04d.vtt", d, i)
      printf "WEBVTT\n\n" > f
      for (j = 1; j <= 60; j++) printf "00:%02d.000 --> 00:%02d.500\nline %s\n\n", j % 60, j % 60, (j == 30 ? w : "x") > f
      close(f)
    }
  }'
  awk -v d="$1" -v sp="$sparse" 'BEGIN {
    f = d "/big.html"; printf "<html><body>\n" > f
    for (j = 0; j < 150000; j++) printf "<div class=\"r\"><ul><li>one<li>two</ul><p>row %d<br>\n</div>\n", j > f
    printf "<p>%s</p></body></html>\n", sp > f; close(f)
    f = d "/big.xml"; printf "<catalog>\n" > f
    for (j = 0; j < 260000; j++) printf "  <book id=\"b%d\"><title>t</title><price>%d</price></book>\n", j, j > f
    printf "  <book><title>%s</title></book>\n</catalog>\n", sp > f; close(f)
    f = d "/big.csv"; printf "id,Name,Plan,Notes\n" > f
    for (j = 0; j < 500000; j++) printf "%d,Bea,pro plan,\"x, \"\"y\"\"\nz\"\n", j > f
    printf "%d,Bea,%s,end\n", j, sp > f; close(f)
    f = d "/big.tsv"; printf "id\tName\tPlan\n" > f
    for (j = 0; j < 700000; j++) printf "%d\tBea\tpro plan\n", j > f
    printf "%d\tBea\t%s\n", j, sp > f; close(f)
    f = d "/big.vtt"; printf "WEBVTT\n\nNOTE generated\n\n" > f
    for (j = 1; j <= 300000; j++) printf "00:00:01.000 --> 00:00:02.000 align:start\nline of text\n\n" > f
    printf "00:00:01.000 --> 00:00:02.000\n%s\n", sp > f; close(f)
    f = d "/big.srt"
    for (j = 1; j <= 300000; j++) printf "%d\n00:00:01,000 --> 00:00:02,000\nline of text\nsecond line\n\n", j > f
    printf "%d\n00:00:01,000 --> 00:00:02,000\n%s\n", j, sp > f; close(f)
  }'
}
[ -d "$dir" ] || gen "$dir"
export SFOR_BENCH_DIR="$dir" SFOR_BENCH_PATTERN="$sparse" SFOR_BENCH_NONE="$none"

# median NAME CMD_RG CMD_SFOR → prints sfor/rg and sets RG_S, SFOR_S (seconds).
median() {
  hyperfine --ignore-failure --warmup 3 --runs 20 --export-json "$iso/$1.json" "$2" "$3" >/dev/null
  # --ignore-failure lets a run that found nothing (exit 1) be timed; a crash, an OOM kill or an
  # error (exit 2) during the timing still fails the benchmark (M3 review r1, Codex #11).
  if ! jq -e '[.results[].exit_codes[]] | all(. == 0 or . == 1)' "$iso/$1.json" >/dev/null; then
    echo "  FAIL $1: a timed run did not exit 0 or 1"; fail=1
  fi
  RG_S=$(jq '.results[0].median' "$iso/$1.json")
  SFOR_S=$(jq '.results[1].median' "$iso/$1.json")
  awk -v a="$SFOR_S" -v b="$RG_S" -v n="$1" 'BEGIN { printf "%-14s rg %.3f s  sfor %.3f s  ratio %.2f\n", n, b, a, a / b }'
}

# check NAME FLAG RG_ARGS…: sfor exits as rg does, writes to stderr what rg writes, and prints
# at least rg's lines (exactly rg's with --no-trail).
check() {
  local name="$1" flag="$2"; shift 2
  local code=0 rgcode=0
  "$sfor" --color never "$flag" "$@" >"$iso/sfor.out" 2>"$iso/sfor.err" || code=$?
  "$rg" --color never "$@" >"$iso/rg.out" 2>"$iso/rg.err" || rgcode=$?
  local e1=0 e2=0
  [ -s "$iso/sfor.err" ] && e1=1
  [ -s "$iso/rg.err" ] && e2=1
  if [ "$code" != "$rgcode" ] || [ "$e1" != "$e2" ]; then
    echo "  FAIL $name: sfor exited $code, rg $rgcode, or stderr differs"; fail=1; return
  fi
  local ours rgs
  ours=$(wc -l <"$iso/sfor.out"); rgs=$(wc -l <"$iso/rg.out")
  if [ "$ours" -lt "$rgs" ] || { [ "$flag" = --no-trail ] && [ "$ours" != "$rgs" ]; }; then
    echo "  FAIL $name: $ours lines, rg printed $rgs"; fail=1
  fi
}

total=$("$rg" --files -- "$dir" | wc -l)
hits=$( ("$rg" -l -e "$sparse" -- "$dir" || true) | wc -l)
sizes=$( ("$rg" -l -0 -e "$sparse" -- "$dir" || true) | xargs -0 -r stat -c %s)
bytes=$(echo "$sizes" | awk '{ s += $1 } END { print s + 0 }')
largest=$(echo "$sizes" | awk '$1 > m { m = $1 } END { print m + 0 }')
echo "== $dir: files $total, matching $hits ($bytes bytes), threads $threads"
awk -v h="$hits" -v t="$total" 'BEGIN { if (h * 20 >= t) { print "the sparse pattern matches 5% or more of the files"; exit 1 } }'
check none --flat -e "$none" -- "$dir"
check sparse --flat -e "$sparse" -- "$dir"
check no-trail --no-trail -e "$sparse" -- "$dir"
[ "$fail" = 0 ] || exit 1
median none '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_NONE" -- "$SFOR_BENCH_DIR"'
awk -v a="$SFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
median no-trail '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --no-trail -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
awk -v a="$SFOR_S" -v b="$RG_S" 'BEGIN { exit !(a <= 1.1 * b) }' || { echo "  FAIL: over 1.1 × rg"; fail=1; }
median sparse '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_PATTERN" -- "$SFOR_BENCH_DIR"'
awk -v a="$SFOR_S" -v b="$RG_S" -v by="$bytes" -v big="$largest" -v t="$threads" \
  'BEGIN { budget = big / 50e6 + by / (50e6 * t) + 0.05; printf "  extra %.3f s, budget %.3f s\n", a - b, budget; exit !(a - b <= budget) }' \
  || { echo "  FAIL: over the budget"; fail=1; }
# Dense: every non-empty line of the CSV files matches `^.`, once. Checked like the others (exit
# code, stderr, lines), then timed; its RSS comes from /usr/bin/time (D16: reported only).
check dense --flat -e "$dense" -g '*.csv' -- "$dir"
export SFOR_BENCH_PATTERN="$dense"
median dense '"$SFOR_RG_PATH" --color never -e "$SFOR_BENCH_PATTERN" -g "*.csv" -- "$SFOR_BENCH_DIR"' \
  '"$SFOR_BIN_PATH" --color never --flat -e "$SFOR_BENCH_PATTERN" -g "*.csv" -- "$SFOR_BENCH_DIR"'
for prog in "$rg" "$sfor"; do
  flag=(); [ "$prog" = "$sfor" ] && flag=(--flat)
  rc=0
  /usr/bin/time -f '%M' -o "$iso/rss" "$prog" --color never "${flag[@]}" -e "$dense" -g '*.csv' -- "$dir" >/dev/null 2>"$iso/dense.err" || rc=$?
  [ "$rc" = 0 ] || { echo "  FAIL dense: $prog exited $rc"; fail=1; }
  printf '  dense RSS %s: %s MiB\n' "$(basename "$prog")" "$(( $(tail -1 "$iso/rss") / 1024 ))"
done
exit "$fail"
```

- [ ] **Step 2:** `chmod +x scripts/bench-m3.sh`; `scripts/bench-m3.sh` exits 0, twice in a row
  (probe after review r1: none 1.04, no-trail 1.02, sparse extra 0.14 s against 0.58 s, dense 14.4 × and 660 MiB reported). It refuses a reference that is not 14.1.1:
  `SFOR_RG=/nonexistent scripts/bench-m3.sh` exits 1, as does
  `SFOR_BIN=scripts/fake-sfor-exit-2.sh scripts/bench-m3.sh`. A timed run that exits with 2 or
  a signal fails it (Codex #11).

- [ ] **Step 3:** M1's `scripts/bench.sh` and M2's `scripts/bench-code.sh` call `rg` from `PATH`;
  run them as `PATH=/usr/bin:$PATH scripts/bench.sh` and `PATH=/usr/bin:$PATH
  scripts/bench-code.sh` after checking that `PATH=/usr/bin:$PATH rg --version` prints
  `ripgrep 14.1.1`. Both exit 0 — to verify in this chunk; M3 only adds work to files of the new Formats and a `Cow` check per printed name,
  so neither should move. Report their ratios and the size of `target/release/sfor` (criterion
  12).

- [ ] **Step 4:** commit: `git add -- scripts/bench-m3.sh` and
  `git commit -m "Benchmark sfor against rg on HTML, XML, CSV and subtitles"`.

## Chunk 8: documentation

Runs on the merge of tanda 2, with its binary: every example below is pasted from a real run over
`tests/fixtures/`, never typed.

### Task 8.1: README, README.es and the skill

- [ ] **Step 1:** `README.md`, table «Formats»: six rows at its end:

```
| HTML | `.html`, `.htm`, `.xhtml` | elements as `tag#id.class`, with `[n]` among siblings of the same tag; `html` and `body` hidden |
| XML | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.svg`, `.rss`, `.atom`, `.plist`, `.wsdl`, `.xlf`, `.xliff`, `.kml`, `.gpx`, `.csproj`, `.vbproj`, `.fsproj`, `.nuspec`, `.resx`, `.xaml`, and files starting with `<?xml` | elements by tag, with `[n]` among siblings of the same tag |
| CSV | `.csv` | `row N › Column`; the delimiter (`,` `;` tab `|`) is read from the first line |
| TSV | `.tsv` | `row N › Column` |
| SubRip | `.srt` | cues: `cue 12 00:01:02,500` |
| WebVTT | `.vtt` | cues: `cue intro 00:00.000` |
```

  An example block per family with real output (`page.html:23`, `plans.csv:3`, `movie.srt:8`),
  and in «Roadmap» M3 marked done with its date. The same in `README.es.md`, in Spanish (8375:
  docs also in Spanish), with the table's words translated and the output identical.

- [ ] **Step 2:** `skills/searchfor/SKILL.md`: its Format list gains HTML, XML, CSV/TSV and
  subtitles, and two recipes: `sfor --within 'pricing › table' needle site/` and
  `sfor --in-headings '01:02' subs/` (cues starting at 1:02).

### Task 8.2: the JSON schema and the spec

- [ ] **Step 1:** `docs/json-schema.json`, the description of `kind`: add «HTML and XML (from
  v0.3): tag. CSV and TSV (from v0.3): row, column. Subtitles (from v0.3): cue, with a
  signature.». `cargo test --workspace` still validates every JSON output against it.

- [ ] **Step 2:** the spec: §5.4–§5.6 and §6.5 were written for this plan; replace their
  «Real output» blocks with the merged binary's output if any byte differs, and report each
  difference. In §7, the size of the v0.3 release binary (criterion 12).

- [ ] **Step 3:** commit: `git add -- README.md README.es.md skills/searchfor/SKILL.md
  docs/json-schema.json docs/specs/2026-10-16-searchfor-design.md` and
  `git commit -m "Document the M3 Formats"`.
