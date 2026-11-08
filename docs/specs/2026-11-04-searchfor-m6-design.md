# searchfor M6 (v1.1) — design

Status: draft · 2026-11-04 · Task 798 (Hilo 855). This document is the spec of M6, the extra
Formats of Questionnaire 49. It adds to [the v1 design](2026-10-16-searchfor-design.md) (called
«the spec» below, its sections `§N`) and to [the M5b design](2026-10-20-searchfor-m5b-design.md)
(«M5b §N»), which stay the source of truth for everything M6 does not change; M6's plan amends
the spec's §2 (the Formats row), §4.1 (the Detector's signals), §4.2 (the Definition's fields),
§4.3 (the Processor table), §5 (a pointer to this document) and §11 (criteria 43–56) to match
what this document decides. Decisions of Sebastián: [grill-respuestas.md](grill-respuestas.md)
(`#N` are its questions, four- and five-digit numbers are Hilo messages); «Q49» is
Questionnaire 49, whose answers are the last section of grill-respuestas.md. Vocabulary:
[../../CONTEXT.md](../../CONTEXT.md).

The example Trails below are the design's, written before the fixtures exist: the plan's
fixtures fix their line numbers and the tests make them real output.

## 1. Goal

s4 1.1 describes the Formats Sebastián chose in Questionnaire 49 that v1.0 left out: text
documents, configuration files, the files that live next to code, the metadata inside media
files and binaries, and everyday files such as mail, calendars and chat exports. For each one, a
Match gets a Trail that tells where it sits (`http › server example.com › location /api`,
`CREATE TABLE users › email`, `JPEG › APP1 Exif › IFD0 › Model`), in the tree, the one-line
layout and `--json`, with `--within`, `--expand` and `--in-headings` working as they work for
the v1 Formats.

## 2. Scope

### In M6

The groups of Q49 left for M6 (grill-respuestas.md, «What happened since»):

| Group (Q49) | Formats | Processor | Section |
|---|---|---|---|
| A · Text documents | AsciiDoc, reStructuredText, Org-mode, LaTeX | `headings` (new) | §6.1–§6.4 |
| | Jupyter notebooks | `keypath`, JSON with the `jupyter` view | §6.5 |
| B · Configuration and infrastructure | INI, systemd units, `.properties`, `.env`, `~/.ssh/config` | `keypath`, new syntaxes | §7.1–§7.5 |
| | nginx, Apache, Terraform and HCL | `blocks` (new) | §7.6–§7.8 |
| | Dockerfile, Makefile, justfile | `buildfile` (new) | §7.9–§7.11 |
| C · Next to the code | SQL | `sql` (new) | §8.1 |
| | Diffs and patches | `diff` (new) | §8.2 |
| | Vue and Svelte single-file components, Blade | `markup`, with scripts and template blocks | §8.3–§8.5 |
| | The scripts of HTML pages (a change to v1.0's HTML output, Open question 2 answered B) | `markup`, `scripts` on in `html.toml` | §8.3 |
| | GraphQL, Protobuf | `blocks` (new) | §8.6–§8.7 |
| | HAR captures | `keypath`, JSON with the `har` view | §8.8 |
| E · Data and binaries | Image metadata (JPEG, PNG, WebP, TIFF: EXIF, PNG text, XMP), audio and video tags (ID3, MP4), WebAssembly, Java `.class` | `object`, new layouts | §9 |
| F · Everyday | Mail (`.eml`, mbox) | `mail` (new) | §10.1 |
| | Calendars and contacts (`.ics`, `.vcf`) | `keypath`, the `ical` syntax | §10.2 |
| | BibTeX | `keypath`, the `bibtex` syntax | §10.3 |
| | WhatsApp chat exports | `chat` (new) | §10.4 |
| | Telegram chat exports (JSON and HTML) | `keypath` with the `telegram` view; `markup` with the `telegram` view | §10.5 |

Also in M6: two new Detector signals (§4), the cost and streaming requirements every new
Processor meets (§11), the docs (§14), and version 1.1.0 (12659: «the v1.0 tag goes on M5b's
commit and M6 ships in a 1.1»).

### Out of M6

- Archive Formats: zip, tar, 7z, rar, `.deb`, `.rpm`, jar (8390). A WhatsApp export shared as a
  `.zip` is searched as rg searches a zip.
- Anything rg does not find (8390): s4 decodes nothing to search it. A base64 mail part, a
  `zTXt` PNG chunk, a UTF-16 ID3 frame and the compressed streams of a `.class` stay invisible
  to the search, as they are to rg; s4 decodes only Segment names (§3).
- Formats Q49 did not tick, also when they would be cheap next to a chosen one: FLAC and Ogg
  comments, HEIC's EXIF item, Outlook `.msg`, Maildir directories, `.emlx`.
- New tree-sitter Grammars compiled into s4, and Grammar Helpers for the M6 Formats (§5.2).
- Packages and channels: the five release targets and four channels of v1.0 (Q57) stay.

## 3. Rules that hold for every M6 Format

These come from Sebastián's comments on Q49 (8385, 8390) and from the v1 rules each new Format
inherits; the sections of each Format only add to them.

1. **The search is rg's** (8385: «the search part must be as fast as ripgrep; what we add should
   be in the presentation only»). Detection and Processors run only on files with a Match
   (spec §3, point 1), and no M6 Format changes which files are searched, which bytes, or how:
   `--no-trail` prints rg's output byte for byte over every M6 fixture (criterion 44).
2. **Decorate what rg finds** (8390). A binary Format applies only where rg already finds a
   Match: a file named on the command line, `--binary` or `-a`, as the object files of spec
   §5.7.
3. **No structure, rg's way** (8385: «when our presentation finds no structure for a Match, at
   least show it the generic way ripgrep would»). A Match outside every Segment has an empty
   Trail. Input a Processor cannot read keeps the Trails of the Matches before the point where
   reading failed and gives the Matches after it an empty Trail, without a warning, as YAML and
   XML do (spec §5.2, §5.4); a Processor whose syntax has no errors (HTML's rule, spec §5.4)
   reads on. Nothing in an M6 file causes a warning, changes the exit code, or panics.
4. **Names.** A Segment name is the file's text as written, cut at the Definition's
   `name_chars` (40 when absent) with `…`, its control characters escaped in tree and one-line
   output (spec §6.5). Words s4 adds to a name (`row 12`, `cell 7`, `stage 0`, `part 1.2`,
   `[3]`) are text of the name only, so `--in-headings` searches the parts that come from the
   file and finds nothing in the added words, as cue names (spec §5.6). Where a name is decoded
   (an RFC 2047 mail header, a `.properties` escape, a JSON string), `--json`, `--within` and
   `--in-headings` see the decoded name, as JSON keys (spec §5.2).
5. **A Match in a Segment's own name is outside that Segment**, as a Match on a heading line
   (spec §5.1) or a key's text (spec §5.2): it gets the Trail of the Segment's parent. The few
   exceptions say so (cues, spec §5.6, keep theirs; the M6 exceptions are listed per Format).
   One exception holds across Formats: a Segment whose name is assembled from values read
   elsewhere in its unit (a HAR entry, page, header, cookie, `queryString` or param, §8.8; a
   Telegram chat and message, §10.5; an iCalendar or vCard component's `SUMMARY`, `FN` or
   `TZID`, §10.2) has as its label only what opens it as written: nothing for a JSON object,
   the `BEGIN:X` line for a component. A Match in one of those values stays inside the
   Segment, under its key: `GET api.example.com/v1/users › request › url`,
   `Work › 2024 Ana › from`, `VEVENT Team sync › SUMMARY`. The server name nginx and Apache
   append to a block's name follows the same rule (§7.6, §7.7).
6. **Sections.** Every Segment has a Section (its first and last line), which `--expand` prints
   and `--json --expand` carries (spec §6.4). For the binary Formats, `--expand` prints the rows
   themselves, as for object files (spec §5.7, M4 plan D8).
7. **Encodings and line ends.** As in the v1 Formats (spec §5.4): a UTF-8 BOM removed and UTF-16
   transcoded by the search, bytes that are not UTF-8 shown as U+FFFD in names, CRLF and LF read
   the same, a lone CR no line break.
8. **Depth.** Past 256 open Segments no more Segments open; deeper Matches get the Trail of the
   deepest one, and nesting of any depth never crashes s4 (spec §5.2, §5.4). The Trail inside a
   `<script>` of HTML, Vue or Svelte obeys the same cap, counting the markup's open elements
   (§8.3); v1's code Formats (a standalone `.ts` or `.js`) keep their v1 depth.
9. **`--json` adds no field.** Each M6 Format fills the existing `trail` objects (`kind`,
   `name`, `line`, `end_line`, `level` for headings, `signature` where a tree label needs one);
   `docs/json-schema.json` lists the new kinds in its description, which stays a free string
   (spec §6.4: «new ones may appear, existing ones are never renamed»).

## 4. Detection

### 4.1 Signals

The Detector keeps the signals of spec §4.1 and their order, and gains two, both decided by the
path alone. The second one, the path glob, is consulted at two places of the order, according
to whether the glob writes the file's extension or leaves it open. The order becomes:

1. `--format` (unchanged).
2. Exact file name (unchanged): `Dockerfile`, `.env`, `nginx.conf`.
3. **Compound extension (new).** An entry of `extensions` that holds a dot (`blade.php`)
   matches the last two extensions of the name, compared without case, and wins over the last
   extension alone: `cart.blade.php` is Blade, `cart.php` stays PHP. The Detector asks the last
   two extensions only of a name whose last extension ends a compound one (`php` here), so every
   other name costs what it costs today.
4. **Path glob that writes the extension (new).** A glob of `paths` (below) whose last
   component ends in a literal extension: `**/ChatExport_*/result.json`,
   `**/ChatExport_*/messages*.html`, `**/nginx/**/*.conf`, `**/systemd/**/*.{path,target}`.
   When it matches, it wins over the Definition that claims that extension:
   `ChatExport_2024/result.json` is `telegram` and `ChatExport_2024/messages.html` is
   `telegram-html`, while a `result.json` or `messages.html` anywhere else stays JSON or HTML.
   A glob that writes its extension compares it in any case, as written in the glob and in the
   path (`*.CONF` matches `app.CONF` and `app.conf`); the rest of the glob keeps its case.
5. Extension (unchanged).
6. **Path glob that leaves the extension open (new):** every other glob of `paths`, whose last
   component has no dot (`**/.ssh/config`, `**/nginx/sites-enabled/*`) or has a wildcard after
   its last dot (`Dockerfile.*`, `.env.*`, `Makefile.*`). It applies after the extension, so
   `prod.Dockerfile` is a Dockerfile by its extension, `Makefile.toml` stays TOML and
   `.env.json` stays JSON.
7. Magic bytes, then 8. shebang (unchanged: read only for a name without an extension).

**The `paths` field.** A Definition's new field `paths` lists globs (`globset`'s syntax, already
a dependency): one without a `/` is matched against the file name (`Dockerfile.*`, `.env.*`),
one with a `/` against the whole path (`**/.ssh/config`, `**/nginx/sites-enabled/*`).
Separators are `/` on every system: `globset` 0.4.15 turns `\` into `/` before matching on
Windows (`pathutil::normalize_path`, read on 04/11). The Detector sorts each glob into signal 4
or signal 6 once, when it loads the Definitions: a glob belongs to signal 4 when the text after
the last `.` of its last component is a literal extension, or a `{…}` list of literal
extensions, with no `*`, `?` or `[` in it. Among the globs of one signal that match, the first
Definition in Registry order wins, as for names and extensions.

Why the glob is split in two. A glob that writes the extension (`result.json`) says everything
the extension says and adds a name or a folder, so it is the more specific signal, the way
`blade.php` is over `php` in signal 3; letting the extension win would make the Telegram rows of
§4.2 unreachable, since `json` and `html` are claimed. A glob that leaves the extension open
(`Makefile.*`) says less than the extension does, so the extension keeps the file. Moving every
glob ahead of the extension was the rejected alternative: it turns `Makefile.toml` into a
Makefile and makes every file with an extension pay a glob lookup before the search. With the
built-in Definitions, signal 4 changes the outcome only for the two Telegram rows: the other
globs that write an extension end in `.conf`, `.txt` or systemd's unit extensions, which no
Definition claims.

Why globs and compound extensions: several M6 Formats live in files no name or extension tells
apart — `/etc/nginx/sites-enabled/default`, `~/.ssh/config`, `.env.production`,
`Dockerfile.dev`, `cart.blade.php` — and the spec forbids sniffing content (§4.1: «There is no
content-sniffing heuristic»). A user's own Definition may use `paths` too, and `--format
'GLOB=NAME'` stays the override.

**Cost.** Detection runs on files with a Match (spec §3, point 1), where a glob match costs
nothing that shows. The checks that run before the search for every file (`detect_by_path`,
`may_be_document`) keep their cost: a glob is consulted there only for a file whose literal
parts can match it, through an index the plan chooses (by name, extension, or parent directory
name); a file no glob can match costs the same lookups as today. Signal 4 runs before the
extension, so it is the one that reaches files with a claimed extension: its globs are found
through the lowercase extension the Detector already looks up, so a file whose extension no
signal-4 glob writes pays nothing more; a file whose extension one writes (with the built-ins,
every `.json`, `.html` and `.txt`) pays a comparison of its file name against the glob's literal
part (`result.json`, the prefix `messages`, the infix `WhatsApp`), and only a name that passes it
runs the glob. Criterion 51 measures it, with `.json`, `.html` and `.txt` files in its corpus.

### 4.2 Detection table

| Format (Definition) | Names | Extensions | Paths | Magic |
|---|---|---|---|---|
| `asciidoc` | | `adoc`, `asciidoc` | | |
| `rst` | | `rst`, `rest` | | |
| `org` | | `org` | | |
| `latex` | | `tex`, `ltx`, `latex` | | |
| `jupyter` | | `ipynb` | | |
| `ini` | `.gitconfig`, `.gitmodules`, `.editorconfig`, `.npmrc`, `.pypirc`, `.flake8`, `.pylintrc`, `.coveragerc`, `.hgrc` | `ini`, `cfg`, `cnf`, `inf`, `desktop` | `**/.git/config`, `**/git/config` | |
| `systemd` | | `service`, `socket`, `timer`, `mount`, `automount` | `**/systemd/**/*.{path,target,slice,scope,swap,network,netdev,link,conf}` | |
| `properties` | | `properties` | | |
| `env` | `.env` | `env` | `.env.*` | |
| `ssh-config` | `ssh_config`, `sshd_config` | | `**/.ssh/config`, `**/.ssh/config.d/*`, `**/ssh/ssh_config.d/*.conf`, `**/ssh/sshd_config.d/*.conf` | |
| `nginx` | `nginx.conf` | | `**/nginx/**/*.conf`, `**/nginx/sites-available/*`, `**/nginx/sites-enabled/*` | |
| `apache` | `httpd.conf`, `apache2.conf`, `.htaccess` | | `**/apache2/**/*.conf`, `**/httpd/**/*.conf`, `**/apache2/sites-available/*`, `**/apache2/sites-enabled/*` | |
| `hcl` | | `tf`, `tfvars`, `hcl`, `nomad` | | |
| `dockerfile` | `Dockerfile`, `Containerfile`, `dockerfile` | `dockerfile`, `containerfile` | `Dockerfile.*`, `Containerfile.*` | |
| `make` | `Makefile`, `makefile`, `GNUmakefile`, `BSDmakefile` | `mk`, `mak`, `make` | `Makefile.*` | |
| `just` | `justfile`, `Justfile`, `.justfile` | `just` | | |
| `sql` | | `sql`, `ddl`, `dml` | | |
| `diff` | | `diff`, `patch`, `rej` | | |
| `vue` | | `vue` | | |
| `svelte` | | `svelte` | | |
| `blade` | | `blade.php` | | |
| `graphql` | | `graphql`, `gql`, `graphqls` | | |
| `protobuf` | | `proto` | | |
| `har` | | `har` | | |
| `jpeg`, `png`, `webp`, `tiff` | | | | `ffd8ff`; `89504e470d0a1a0a`; `57454250` at 8; `49492a00`, `4d4d002a` |
| `id3`, `mp4` | | | | `494433`; `66747970` at 4 |
| `wasm`, `java-class` | | | | `0061736d`; `cafebabe` |
| `eml` | | `eml` | | |
| `mbox` | `mbox` | `mbox`, `mbx` | | `46726f6d20` (`From `) |
| `icalendar`, `vcard` | | `ics`, `ical`, `ifb`; `vcf`, `vcard` | | |
| `bibtex` | | `bib` | | |
| `whatsapp` | `_chat.txt` | | `*WhatsApp*.txt` | |
| `telegram` | | | `**/ChatExport_*/result.json`, `**/DataExport_*/result.json` | |
| `telegram-html` | | | `**/ChatExport_*/messages*.html`, `**/DataExport_*/chats/*/messages*.html` | |

Notes on the table, each a decision with its reason:

- **`.conf` is claimed by no extension.** nginx, Apache, systemd, sshd and dozens of programs
  use it with different syntaxes; the paths tell them apart, and any other `.conf` stays Plain.
- **`.asc` is left out of AsciiDoc**, being the extension of ASCII-armored PGP files too.
- **The binary Formats of group E name no extension**, as ELF, PE and Mach-O (spec §4.1, M4
  plan D5): a binary is told by its magic once it has a Match, so a tree of photos or songs costs
  rg's time and nothing more until one matches (8385), and a `.jpg` that holds a PNG reads as a
  PNG. Their magic is compared in every binary file with a Match whose name decides no other
  Format, after the v1 object Formats. **`cafebabe`** starts both a fat Mach-O and a Java class:
  the object Definitions whose magic matches are tried in Registry order until one reads the
  file (§9), and a class file is never a valid fat file (its version would be an arch count of
  45–69 whose arch table falls outside the file's slices, which xnu's checks refuse, spec §5.7).
- **mbox's `From ` magic** applies, as every magic, only to names without an extension
  (Thunderbird's `Inbox`, `Sent`).
- **The chat exports are told by the names their apps give them** (§10.4, §10.5), with no look
  at the content: Open question 1, answered A. Telegram's globs write the extension
  (`result.json`, `messages*.html`), so they are signal 4 of §4.1 and win over `json` and
  `html` inside a `ChatExport_*` or `DataExport_*` folder only. WhatsApp's glob,
  `*WhatsApp*.txt`, writes `.txt` and is signal 4 too, which no Definition claims.

## 5. Processors

### 5.1 Reused and new

| Processor | Status | M6 Formats | What M6 adds to it |
|---|---|---|---|
| `keypath` | reused | Jupyter, HAR, Telegram JSON (JSON views); INI, systemd, `.properties`, `.env`, ssh config, iCalendar, vCard, BibTeX | syntaxes `ini`, `properties`, `env`, `ssh`, `ical`, `bibtex`; a `view` parameter for JSON (`jupyter`, `har`, `telegram`) |
| `markup` | reused | Vue, Svelte, Blade, Telegram HTML; HTML's scripts | parameters `scripts` (Trails inside `<script>` from the compiled-in JavaScript and TypeScript Grammars, and keypath's JSON for JSON-LD), `script_max_bytes` and `script_max_line`, `templates` (`svelte`, `blade`) and `view` (`telegram`) |
| `object` | reused | JPEG, PNG, WebP, TIFF, ID3, MP4, WebAssembly, Java class | a `layout` parameter that picks the reader; ELF, PE and Mach-O keep goblin with no `layout` |
| `treesitter` | reused inside `markup` | the `<script>` of Vue, Svelte and HTML | none: it parses a byte range of the file |
| `headings` | new | AsciiDoc, reStructuredText, Org-mode, LaTeX | a line scanner per `syntax`, giving Markdown's Segment kinds |
| `blocks` | new | nginx, Apache, HCL, GraphQL, Protobuf | a block scanner per `syntax` (braces, or Apache's `<Section>` tags) |
| `buildfile` | new | Dockerfile, Makefile, justfile | a line scanner per `syntax`, with continuations and recipes |
| `sql` | new | SQL | a statement scanner |
| `diff` | new | unified diffs, patches, `git log -p`, `git format-patch` | a line scanner |
| `mail` | new | `.eml`, mbox | an RFC 5322 and MIME reader |
| `chat` | new | WhatsApp text exports | a line scanner |

Why `keypath` grows: INI-like files, calendars, contacts and BibTeX are paths of keys, which is
what keypath's Trails and Sections already model (spec §5.2), and a JSON view reuses its JSON
scanner and its streaming cursor unchanged. A view is Rust code that relabels, hides and adds
Segments over the keypath Outline of one JSON shape; it is not a user-facing mapping language
(three shapes do not justify a new contract, and an External Processor covers any other shape).
Why `markup` grows: a single-file component is HTML, and its template blocks open and close
like elements. Why `object` grows: group E is «more Formats of this kind» (spec §5.7, M4 plan
D1), with the same rows at hex offsets.

Why seven new Processors instead of fewer: each groups Formats that share one reading (lines
with headings; blocks; build files), and keeps one parameter, `syntax`, choosing the rules, as
`keypath` and `markup` do. `sql`, `diff`, `mail` and `chat` each have a structure no other
Format shares.

Every new Processor is an Outline Processor or gives a native cursor (M5b §6.1): its rows
stream, and the model path stays for none of them (§11).

### 5.2 No new Grammar is compiled in

The brief for this spec asked to prefer tree-sitter for languages with a maintained grammar.
Measured on 04/11 (crates fetched from crates.io into the cargo cache; `cc -O2 -c` over each
`parser.c` and `scanner.c`, text plus data with `size`; tree-sitter-go 0.23.4, compiled in
today, measured 210 KB the same way against its 232 KB archive in the build):

| Crate | ABI | Compiled size | Use |
|---|---|---|---|
| tree-sitter-sequel 0.3.11 (SQL) | 14 | 2 434 KB | over the binary's headroom alone |
| tree-sitter-hcl 1.1.0 (its only release) | 15 | 93 KB | does not load in tree-sitter 0.24.7 |
| tree-sitter-graphql 0.1.0, 0.2.1, 0.3.0 | 15 | 51 KB | does not load |
| tree-sitter-proto 0.2.0 / 0.4.0–0.6.0 | 14 / 15 | 74 KB | only 0.2.0 loads |
| tree-sitter-nginx 1.0.1 (its only release) | 15 | 98 KB | does not load |
| tree-sitter-make 1.1.1 | 14 | 154 KB | loads |
| tree-sitter-dockerfile 0.2.0 | 14 | 53 KB | loads, needs tree-sitter 0.20 as its dependency |
| tree-sitter-rst 0.2.0 | 14 | 57 KB | loads |
| tree-sitter-latex 0.1.0 | 15 | `parser.c` of 38 MB | does not load |
| tree-sitter-diff 0.1.0 | 14 | 22 KB | loads |

tree-sitter 0.24.7 loads ABI 13 and 14 (`TREE_SITTER_MIN_COMPATIBLE_LANGUAGE_VERSION = 13`,
`LANGUAGE_VERSION = 14` in its `api.h`), and ADR 0005 ties it to ripgrep 14.1.1's regex pins. So
HCL, GraphQL, nginx and LaTeX have no loadable crate, SQL's is larger than everything M6 may
add, Org's (1.3.3) pins tree-sitter below 0.21 and builds a C++ scanner, and INI's newest
(1.4.0) is ABI 15; AsciiDoc's (tree-sitter-asciidoc 0.9.0, with a second inline grammar) and
BibTeX's (0.1.0) were not measured. What the remaining ones would give (Makefile, Dockerfile, reStructuredText,
diff, Protobuf 0.2.0) a scanner gives too, with three more gains measured in M3 (spec §4.3): a
scanner reads at 60 MB/s or more where tree-sitter reads about 3 MB/s, a scanner has no
`max_bytes` past which a file turns Plain (a 200 MB SQL dump or HAR capture is common), and a
scanner streams Matches in dense files (M5b §6). M6 therefore adds scanners of its own for
every text Format and no Grammar, which leaves ADR 0005 as it is. A Grammar Helper for any of
these languages stays possible later, through `--install-grammar`, with no change to s4.

### 5.3 Binary size

Criterion 22 (≤ 23 000 000 bytes, 8612) stays. The installed s4 1.0.0 (`~/.cargo/bin/s4`, M5b's
code) weighs 21 386 712 bytes, so M6 has about 1.6 MB. M3 added three scanners (`markup`,
`csv`, `subtitles`) for 0.2 MB (spec §7: v0.2 16.4 MB, v0.3 16.6 MB); M6 adds seven Processors,
six keypath syntaxes, four views, two `markup` parameters and eight binary layouts, all Rust with no new crate, which by that
ratio should cost 0.5–0.8 MB (an estimate, not a measurement). The plan measures the release
binary after each tanda and records it in criterion 22; a measurement over 22.8 MB stops the
plan before the next tanda with the question of which Format family to move to a Helper
Processor, or whether to raise the ceiling (8612 set it; only Sebastián moves it).

### 5.4 Crates

**No new crate.** Every reader in M6 is code of our own over dependencies already in
`Cargo.lock` (and so in this machine's offline cache): `globset` 0.4.15 for path globs,
`memchr` for scanning, `encoding_rs` 0.8.34 for the charsets of mail names, `base64` 0.23.1
(already built through `ureq`) or a decoder of our own for RFC 2047, and the `markup` and
`keypath` code for XMP and JSON. The plan fetches nothing; if a chunk wants a crate after all,
that is a question for the orchestrator before it is added (ADR 0001's pins and the offline
build). The grammar crates fetched on 04/11 for §5.2's measurements sit in the cargo cache and
nothing uses them.

## 6. Group A · Text documents

The `headings` Processor gives AsciiDoc, reStructuredText, Org-mode and LaTeX the Segment kinds
of Markdown (spec §5.1): `document` for the title, `heading` with its `level`, `code` for a
code block named by its language, and `column` where §6.3 says. A heading's Section runs to the
next heading of the same or a higher level, as in Markdown. Every heading and code Segment has
a `signature`, its line as written (`== Install`, `\section{Install}`), which the tree shows
instead of Markdown's `#` markers (`tree::label` shows a signature when there is one). Headings
inside code, literal and comment blocks are text. List items are Markdown's alone (#11 asked
about Markdown); the A Formats have none in M6. Parameters: `syntax`, `name_chars`.

A file has no syntax errors for `headings`: a block left open runs to the end of the file, as
an unclosed Markdown fence does.

### 6.1 AsciiDoc

- **Headings.** `= Title` at the top is the document title (`document`); `==` to `======` are
  levels 1 to 5, and the Markdown-style `#` forms Asciidoctor accepts are read the same. A
  heading needs a space after its markers. Attribute entries (`:toc: left`) above `= Title`
  leave it the document title. The two-line (setext) titles of AsciiDoc's legacy syntax are
  text, and so is a two-line title's underline (one repeated `=`, `-`, `~`, `^` or `+` within
  one character of the title's length, when the title line starts a paragraph: after a blank
  line, a heading, a block's delimiter or an attribute entry; never a list item or a block
  title), which opens no block. An attribute entry may continue over lines ending in a space
  and `\`; a bare trailing `\`, as in a Windows path, ends its value.
- **Blocks.** A delimited block (`----` listing, `....` literal, `////` comment, `++++`
  passthrough, `****`, `====`, `____`, and Markdown-style ```` ``` ```` fences) runs to the same
  delimiter; a fence of three or more backticks closes at a run of backticks as long or longer.
  `[source,LANG]` names a listing, a literal block, and a fence without an info string: each is
  then a `code` Segment named `LANG`. Without it a listing is `listing`, a literal block
  `literal`, and a fence its info string or `listing`. A table (`|===`, `,===`, `:===`, `!===`)
  is a block whose lines are text. The others hold text and no Segment.
- **Example.** `guide.adoc:9: Deploy guide › Install › Linux › bash │ apt install needle-pkg`
- **Limits.** Tables give no `column` Segment in M6: their header row is optional and told by
  layout, and a wrong header names every Match wrongly. `include::` is not followed.

### 6.2 reStructuredText

- **Headings.** A line of text underlined (and optionally overlined) by one repeated punctuation
  character at least as long as the text. Levels follow the order in which each style (the
  character, with or without an overline) first appears, as docutils assigns them. The first
  style is level 1; s4 does not promote a lone top section to a document title (docutils'
  `doctitle_xform`), so the Trail keeps the same depth in every file.
- **Blocks.** `.. code-block:: LANG`, `.. code:: LANG` and `.. sourcecode:: LANG` are `code`
  Segments named `LANG`, over their options and indented body; one without LANG is named by its
  directive (`code-block`, `code`, `sourcecode`). A paragraph ending in `::` opens
  a `literal` code block over the indented lines after it. Any other directive (`.. note::`,
  `.. warning::`) is a `directive` Segment named by its type, over its indented body; a comment
  (`..` with no `::`) holds no Segment.
- **Example.** `index.rst:14: Install › Linux › bash │    apt install needle-pkg`
- **Limits.** Grid and simple tables give no `column`; substitutions and roles are text.

### 6.3 Org-mode

- **Headings.** `*` to any depth followed by a space; the level is the number of stars. The name
  is the heading's text after the stars, its TODO keyword, priority and tags kept as written, so
  `--in-headings` finds them. A heading of stars alone is named `heading`. `#+TITLE:` is the
  document title.
- **Blocks.** `#+BEGIN_SRC LANG` … `#+END_SRC` (without case) is a `code` Segment named `LANG`
  (`src` when absent); `#+BEGIN_EXAMPLE` is `example`. Other blocks hold no Segment.
- **Tables.** A table whose first row is followed by a rule (`|---+---|`) names its columns by
  the cells of that row: a Match in a later row gets a `column` Segment, whose Section is the
  table, as Markdown's (spec §5.1). `--in-headings` does not search a column's header, as
  Markdown's. A table without the rule has no header.
- **Example.** `notes.org:20: Budget › Costs › Price │ | Hosting | needle 9 |`

### 6.4 LaTeX

- **Headings.** `\part`, `\chapter`, `\section`, `\subsection`, `\subsubsection`, `\paragraph`
  and `\subparagraph`, starred or not, are levels 1 to 7; the name is the braced title as written
  (balanced braces, over several lines), the optional short title ignored. `\title{…}` is the
  document title. A `%` comment (not `\%`) is text, and so are the commands in it.
- **Environments.** `\begin{X}` … `\end{X}` is an `environment` Segment named `X`, with its
  `\begin` line as signature. `lstlisting` with `[language=L]` and `minted` with `{L}` are `code`
  Segments named `L`; `verbatim`, `Verbatim`, `comment` and the listings without a language are
  `code` Segments named by the environment; inside them nothing is read as a command. The
  Definition's `hidden_environments` (default `document`, `itemize`, `enumerate`,
  `description`) are hidden, as HTML's `body` (spec §5.4): their contents keep the Trail of
  what holds them. A heading inside an open environment ends at that environment's end at the
  latest. An `\end` with no open environment of its name closes nothing. An environment or a
  heading with an empty name (`\begin{}`, `\subsection{ }`) is named by its kind,
  `environment` or `heading`.
- **Example.** `paper.tex:41: A Study of Needles › Methods › Sampling › figure │ \caption{needle distribution}`
- **Limits.** `\input` and `\include` are not followed; macros that expand to sectioning
  commands are text, and so is the body of a macro or environment definition (`\newcommand`,
  `\DeclareRobustCommand`, `\newenvironment`, `\def`, xparse's `\NewDocumentCommand` and
  `\NewDocumentEnvironment` families, and their variants), whose parts may stand on several
  lines with comments between them, up to a blank line, and whose commands open nothing. No
  part of a definition crosses a blank line; a `%` comment eats its line break, so a blank line
  right after it is one too.

### 6.5 Jupyter notebooks

`keypath` with `syntax = "json"` and `view = "jupyter"`. A notebook is JSON (nbformat 4), so
the JSON scanner reads it and the view reshapes its Outline:

- The Markdown cells' headings (`#` to `######` at the start of a source line, decoded from the
  JSON strings) become `heading` Segments with their level, each over the cells up to the next
  heading of the same or a higher level, so the Trail of a code cell starts with the headings
  above it, as in the notebook a person reads. The headings are read over the source strings
  decoded and joined: a line may span two strings, and a `\r` before a line break, however
  escaped, is no text of it.
- Each element of `cells` is a `cell` Segment named `cell N`, 1-based, its signature
  `cell N (code)` or `(markdown)`, `(raw)`. An element of `outputs` is an `output` Segment named
  `output N`, signature with its `output_type`. The keys `cells`, `source`, `outputs`, `data`
  and `text`, and the indexes of the source lines, are hidden; the MIME keys of an output's
  `data` stay (`text/plain`, `image/png`), and so do `metadata` and every other key.
- **Example.** `analysis.ipynb:57: Analysis › Load data › cell 7 │     "df = load('needle.csv')\n",`
  and in an output: `analysis.ipynb:81: Analysis › Load data › cell 7 › output 1 › text/plain │ …`
- **`--in-headings`** finds the headings' text and the kept keys; `cell N` and `output N` are
  words of s4.
- **Failure.** A file that is not nbformat-shaped (no `cells` array at the top) keeps the plain
  JSON Trails; a JSON syntax error keeps spec §5.2's rule.

## 7. Group B · Configuration and infrastructure

### 7.1 INI

`keypath`, `syntax = "ini"`; also the `systemd` Definition (§7.2).

- **Model.** `[section]` opens a section until the next header; `key = value` and `key: value`
  are keys. A section is a `key` Segment, as a TOML table (spec §5.2), named by the text between
  the brackets as written: `[remote "origin"]` is `remote "origin"`, and `[a.b]` is one Segment
  `a.b` (INI has no dotted tables; splitting it would invent a parent). Keys before the first
  header are at the top. A repeated section or key is a Segment of its own each time.
- **Lines.** `;` and `#` at a line's start are comments. A line ending in `\` continues on the
  next. A header is one physical line: `[sec\` followed by `tion]` is text. An indented line continues the previous key's value when it holds neither `=` nor `:`,
  and is a key of its own otherwise, so the indented keys of `.gitconfig` are keys and the
  multi-line values of Python's `configparser` are values: a heuristic, documented, that only
  decides between two Trails of the same section.
- **Example.** `app.ini:3: server › host │ host = needle.example`
- **Failure.** None: a line that is neither a header nor a key is text of the key or section
  above it.

### 7.2 systemd units

The INI model with systemd's names: `needle.service:6: Service › ExecStart │ ExecStart=/usr/bin/needle --serve`.
A Definition of its own (`systemd`) so `--json`'s `format` and `--list-formats` name it; its
syntax is `ini`, whose `\` continuation is systemd's.

### 7.3 `.properties`

`keypath`, `syntax = "properties"`. A key ends at the first unescaped `=`, `:` or white space;
`\` at a line's end continues the value, and a key too: the key goes on past it, without the
line break and the blanks that start the next line; `#` and `!` start comments. The key is
decoded (`\:`, `\=`, `\ `, `\uXXXX`) and is one Segment: `spring.datasource.url` keeps its
dots, as the format has no hierarchy and a dotted parent would have no Section of its own when
its keys are apart. A key that decodes to nothing (`=value`) opens no Segment.
`app.properties:4: spring.datasource.url │ spring.datasource.url=jdbc:needle://db`

### 7.4 `.env`

`keypath`, `syntax = "env"`. `KEY=value` and `export KEY=value`; a value in quotes may span
lines, and the Section of its key covers them; `#` starts a comment outside quotes.
`.env.local:2: DATABASE_URL │ DATABASE_URL=postgres://needle@db/app`

### 7.5 `~/.ssh/config` and `sshd_config`

`keypath`, `syntax = "ssh"`. `Host …` and `Match …` open a section, named by the line as written
(`Host github.com gitlab.com`), until the next one; every other line is a keyword (`key`), with
white space or `=` before its value, compared without case as ssh does. Keywords before the
first `Host` are at the top. `Include` is not followed.
`config:12: Host github.com › IdentityFile │   IdentityFile ~/.ssh/needle_ed25519`

### 7.6 nginx

`blocks`, `syntax = "nginx"`. A directive ends at `;`; a block directive opens `{` … `}`.

- **Segments.** A block is a `block` Segment named by its directive and arguments as written
  (`location ~ \.php$`, `upstream backend`, `if ($host = x)`). A `server` block is named
  `server` plus its first `server_name` value when it has one, read from the whole block
  (preparation reads the file once, §11), since the bare word `server` tells nothing. That
  `server_name` value is read later in the block and is no label (rule 5's exception, §3): a
  Match in it keeps the whole Trail (`http › server example.com › server_name`). A directive
  is a `key` Segment named by its name, over its lines up to its `;`.
- **Lexing.** `#` comments; `'…'` and `"…"` strings with `\` escapes (nginx requires a regex
  holding `{`, `}` or `;` to be quoted, so braces outside strings are structure).
- **Example.** `nginx.conf:42: http › server example.com › location /api › proxy_pass │       proxy_pass http://needle:8080;`
- **Failure.** A `}` that closes nothing ends the structure there; a block still open at the end
  of the file ends there. `include` is not followed.

### 7.7 Apache

`blocks`, `syntax = "apache"`. `<Name args>` … `</Name>` sections, compared without case, nest;
a directive is one line, `\` continuing it. A section is a `block` Segment named by its tag's
text as written, `Name args` (`VirtualHost *:443`, `Directory /var/www`), a `VirtualHost` with
its first `ServerName` appended; as nginx's `server_name` (§7.6), that value is no label, and a
Match in it keeps the whole Trail
(`IfModule mod_ssl.c › VirtualHost *:443 example.com › ServerName`). A directive is a `key`
Segment. `#` comments.
`site.conf:9: VirtualHost *:443 example.com › Directory /var/www › Require │     Require ip 10.0.0.0/8 # needle`
A closing tag that matches no open section is ignored; an unclosed section ends at the end of
the file.

### 7.8 Terraform and HCL

`blocks`, `syntax = "hcl"`.

- **Segments.** A block (`resource "aws_instance" "web" {`) is a `block` Segment named by its
  type and labels without quotes, joined by spaces (`resource aws_instance web`): Terraform's
  `aws_instance.web` addressing is one tool's convention, and the same rule reads Packer and
  Nomad. An attribute (`ami = "…"`) is a `key` Segment whose Section runs to the end of its
  last line, an end-of-line comment included; a line starting with `=` or `:` has no key.
  An object value (`tags = { … }`) gives
  keys inside it, and a list element `[N]`, 0-based, named after its key as in keypath
  (`ingress[1]`, spec §5.2). A list element, and an object or a list after its closer, take
  their end-of-line comment, the comma before it included, when a line break comes before the
  next token; the comma between two values on one line is in neither.
- **Lexing.** `#`, `//` and `/* */` comments; strings with `${ … }` interpolations and
  `%{ … }` directives, whose braces and quotes nest inside the string; heredocs `<<EOT` and
  `<<-EOT` up to their marker line.
- **Example.** `main.tf:14: resource aws_instance web › tags › Name │     Name = "needle-web"`
- **Failure.** As nginx: a stray closer ends the structure, an open block ends with the file.

### 7.9 Dockerfile

`buildfile`, `syntax = "dockerfile"`.

- **Segments.** Each `FROM` opens a `stage` Segment to the next `FROM`, named by its `AS` name,
  or `stage N` (0-based, Docker's own numbering for `--from=N`) without one; its signature is the
  `FROM` line. Each instruction is an `instruction` Segment named by its keyword as written
  (`RUN`), over its continuation lines and its heredocs (`RUN <<EOF` … `EOF`). `ARG` before the
  first `FROM` is at the top.
- **Lexing.** The escape character is `\`, or the one a `# escape=` directive sets on the first
  lines; comment lines inside a continuation are skipped, as Docker skips them. A heredoc opens
  at a word of the line, split at blanks outside quotes and escapes, that starts with `<<`; a
  `<<` inside a quoted word or another word is text. The words of an instruction are split over
  its logical line: a quote or a word left open before a continuation's escape goes on in the
  next line, so `RUN echo "a \` + `<<EOF"` opens no heredoc. A heredoc marker split by a
  continuation goes on with the start of the next line, as Docker joins the lines first
  (`<<EOF\` + `cat` is `<<EOFcat`).
- **Example.** `Dockerfile:12: build › RUN │     needle-build --release`

### 7.10 Makefile

`buildfile`, `syntax = "make"`.

- **Segments.** A rule (`build test: deps`, `%.o: %.c`, static patterns, `::`) is a `target`
  Segment named by its targets as written, signature its rule line, over its recipe: the lines
  that follow starting with a tab (or `.RECIPEPREFIX`), with comments and blank lines among
  them. The rule's label is its targets: its prerequisites are inside the `target`, as a
  function's parameters are inside its function, so a Match in `deps` of `build test: deps` has
  the Trail `build test`. A drive letter (`C:/`, `C:\`) is part of its target, so one letter
  glued to an absolute prerequisite (`c:/x`) reads as a drive letter and opens no rule. A
  variable (`=`, `:=`, `::=`, `?=`, `+=`, `!=`, with `override` or `export`) is a `key` Segment
  over its `\` continuations; `define NAME` … `endef` is a `key` named `NAME`, and `define`
  nests: the key ends at the matching `endef`. Inside a `define`, a line that starts with the
  recipe prefix opens and closes nothing.
  `ifeq`, `ifneq`, `ifdef`, `ifndef` … `endif` is a `conditional` Segment named by its line
  as written, its `else` branches inside it. A rule opened inside a conditional keeps its recipe
  past that conditional's `endif`, as GNU make runs it: the `target` ends before the `endif`,
  and the recipe lines after it are another `target` with the same name and signature, whose
  name `--in-headings` does not search again.
- **Lexing.** `$(…)` and `${…}` are skipped when looking for a rule's `:`, so
  `$(OBJ:.o=.c)` is no rule; `#` starts a comment outside a recipe.
- **Example.** `Makefile:18: ifeq ($(OS),Windows_NT) › release │ 	cargo build --release # needle`
- **Failure.** An `endif` or `endef` that closes nothing is text; an open one ends with the file.

### 7.11 justfile

`buildfile`, `syntax = "just"`. A recipe (`name params: deps`, with `[attributes]` and `@`
before it) is a `recipe` Segment named `name`, signature its line, over its indented body.
Attributes may stand on their own lines or share the recipe's line (`[private] build:`). A
recipe's label is its name and `@`; its attributes, parameters and dependencies are inside it,
so a Match in `needle-dep` of `build +args: needle-dep` has the Trail `build`. A variable or
`alias` is a `key`. `just`'s comments above a recipe are its documentation and lie
outside it. `justfile:7: deploy │     rsync -a build/ host:/srv/needle/`

## 8. Group C · Next to the code

### 8.1 SQL

The `sql` Processor reads one permissive dialect (PostgreSQL, MySQL, SQLite, SQL Server
scripts and their dumps); a grammar would need one dialect per file and could not read the
dumps (§5.2).

- **Statements.** A statement ends at a `;` outside strings (`'…'` with `''`), quoted
  identifiers (`"…"`, `` `…` ``, `[…]`), PostgreSQL dollar quotes (`$tag$ … $tag$`) and comments
  (`--`, `#` in MySQL's style, `/* */`); at a `GO` line (SQL Server's batch separator); and at
  the delimiter a `DELIMITER` line sets (mysqldump's procedure bodies). A `#` starts a comment
  only before white space or a line's end, or first on its line (SQL Server's `#tmp` is a
  name). MySQL's conditional comments `/*!NNNNN … */` are SQL. A backslash escapes in `'…'`
  unless the file's head says PostgreSQL (`standard_conforming_strings = on`, or pg_dump's
  banner), where only `E'…'` has escapes. Each statement is a
  `statement` Segment named by its leading keywords and object, identifiers without quotes:
  `CREATE TABLE users`, `CREATE OR REPLACE VIEW active_users`, `INSERT INTO users`,
  `ALTER TABLE users`, `UPDATE users`, `DELETE FROM users`, `CREATE FUNCTION total`,
  `COPY public.users`, `SELECT`.
- **Columns.** Inside `CREATE TABLE … ( … )`, each column definition is a `column` Segment
  named by the column; table constraints are text of the statement.
- **Rows.** In `INSERT … VALUES (…), (…)`, each tuple is a `row` Segment named `row N` (1-based
  within the statement), and a Match inside a value gets a `column` Segment named from the
  statement's column list, else from the columns of the same table's `CREATE TABLE` earlier in
  the file (mysqldump writes it before its `INSERT`s, without a column list), else `column K`.
  A value's `column` Segment spans its row's lines, as a CSV cell's does.
  `OVERRIDING SYSTEM VALUE` or `OVERRIDING USER VALUE` before `VALUES` is skipped.
  PostgreSQL's `COPY t (cols) FROM stdin;` reads its data lines up to `\.` the same way, one
  `row` per line, tab-separated. This gives a dump the Trail of the SQLite Extractor
  (`users › row 12 › email`, spec §5.8).
- **Example.** `schema.sql:12: CREATE TABLE users › email │   email text NOT NULL, -- needle`;
  `dump.sql:310: INSERT INTO users › row 1234 › email │ …('needle@example.com',…` (a Fragment of
  a long line, spec §6.3); `dump.sql:88: COPY public.users › row 3 › email │ 3	needle@example.com`
- **Large files.** A dump can be gigabytes: the Processor indexes statements only (their start
  and kind) and builds rows and columns for the statements that hold Matches, and reads no
  further than the statement of the file's last Match, as CSV reads no further than its last
  Match's record (spec §5.5). Criterion 52.
- **Failure.** A string or comment left open ends the structure at its statement; the Matches
  after it get an empty Trail. A `CREATE PROCEDURE` body written without `DELIMITER` splits at
  its inner `;` into several statements: documented, the Trail still names the statement each
  piece starts.

### 8.2 Diffs and patches

The `diff` Processor reads unified diffs (`diff -u`, `git diff`), combined diffs (`@@@`),
`git log -p` and `git format-patch` series.

- **Segments.** A commit (`commit <sha>` of `git log`, `From <sha> …` of a patch mail) is a
  `commit` Segment named by the short hash and the subject (`[PATCH n/m]` removed), signature
  its first line, up to the next commit. A `From ` line opens a commit only with the whole sha
  (40 or 64 digits). A file (`diff --git`, or `---`/`+++` pairs) is a `file` Segment named by
  its new path without `b/`, or the old one when the new is `/dev/null`; a rename's signature
  is `old → new`. Inside a hunk whose counts are not done, a `---`/`+++` pair is a file's
  header only when a hunk header follows it. A hunk (`@@ -a,b +c,d @@ context`) is a `hunk`
  Segment named `+c` and its context text (`+120 fn describe(`), `-a` for a deleted file,
  signature its header line. A hunk takes one `\ No newline` line (`\ No newline at end of
  file`) after its last counted line; another one after it is text.
- **Rule 5 here.** A file's own name runs from `diff --git` over its extended header lines
  (`index`, modes, renames, copies, similarity) to `+++`: a Match in those lines has the Trail
  above the file. A Match in a hunk header is in the file Segment.
- **Example.** `fix.patch:23: src/app.rs › +120 fn describe( │ +    let needle = 1;` and in a
  series: `series.patch:40: 3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe( │ …`
- **Stdin.** `git diff | s4 --format diff needle` gives the Trails of a diff with no name; the
  docs show it.
- **Failure.** A hunk whose counts run past the next header ends at that header; lines outside
  every file are at the top, or in their commit.

### 8.3 Vue single-file components

`markup`, `syntax = "html"`, `scripts = true`. The template is HTML read by spec §5.4's rules,
so `template › ul.items › li[2]`. With `scripts`, the content of each `<script>` is parsed by
the compiled-in Grammar its `lang` names (`ts` and `typescript` TypeScript, `tsx` TSX, none or
`js` JavaScript), with the query of that Format's Definition, and its Segments go under the
`script` element: `Cart.vue:22: script › addItem │       this.needle++`. A script over the
Grammar Definition's `max_bytes`, or of a `lang` with no compiled-in Grammar, keeps the `script`
Segment alone. Inside a `<script>` of HTML, Vue or Svelte, the 256 open Segments of rule 8 (§3)
count the markup's open elements, the script's included; the script's deeper Segments do not
open. `<style>` is text (no CSS Grammar is compiled in).

**Which scripts are read** (Sebastián, Hilo 855 #12726, the three limits of #12697), in every
markup Format with `scripts`, HTML included:

1. **JavaScript:** a `<script>` with no `type`, an empty one, `module`, or a JavaScript MIME type
   (`text/javascript`, `application/javascript`, `text/ecmascript`, `application/ecmascript`,
   their `x-` forms, `text/jscript`, `text/livescript` and `text/javascript1.0`–`1.5`, the list
   of WHATWG HTML's «JavaScript MIME type», quoted from memory and not checked against the
   standard here), compared after trimming spaces and without case, as browsers decide what to
   run. Its `lang` then picks TypeScript or TSX as above, in HTML too (browsers ignore the
   attribute, and a page that writes one means it).
2. **JSON-LD:** `type="application/ld+json"` is read by keypath's JSON (spec §5.2), its key
   Segments under the `script` element: `page.html:12: head › script › offers › price │ …`.
3. **The rest keeps v1.0's Trail:** any other `type` (`text/x-template`, `text/plain`,
   `importmap`, `speculationrules`…) is the `script` element's text, as in 1.0; and so is a
   script whose content passes `script_max_bytes` or holds a line longer than
   `script_max_line`.

`html.toml` sets `scripts = true`, `script_max_bytes = 1048576` (1 MiB) and
`script_max_line = 1000`; Vue and Svelte leave both caps unset, so the Grammar Definition's
`max_bytes` is their only one. Why 1 000 bytes: the project had no minification cap (`src/` has
only the Fragments' width rules); hand-written code keeps its lines under the 80–120 columns
formatters and style guides break at, while a minifier writes a bundle on one line or a few
very long ones, whose Trails (`a`, `e`, `t`) would say nothing and whose parse is the costly
part of a page. Why 1 MiB: an inline script past it is generated (a framework's state or a
bundle), and the cap bounds what one Match makes s4 parse.

### 8.4 Svelte components

`markup`, `syntax = "html"`, `scripts = true`, `templates = "svelte"`. Svelte's blocks
(`{#if …}`, `{#each …}`, `{#await …}`, `{#key …}`, `{#snippet …}`) open a `block` Segment named
by their opening tag without braces (`#each items as item`), and `{/if}` and the others close the
innermost open block of their name and every element inside it, as an HTML end tag closes
(spec §5.4); `{:else}` and `{:then}` stay inside. A closer with no open block is text, and so
is an opener whose expression does not close within 4 KiB.
`Cart.svelte:14: ul › #each items as item › li │   <li>{item.needle}</li>`

### 8.5 Blade templates

`markup`, `syntax = "html"`, `templates = "blade"`. A paired directive (`@section`, `@if`,
`@foreach`, `@forelse`, `@for`, `@while`, `@switch`, `@push`, `@component`, `@slot`, `@php`,
`@once`, `@auth`, `@guest`, `@isset`, `@empty`, `@unless`, `@verbatim`) opens a `block` Segment
named by the directive and its argument as written (`@foreach($items as $item)`), up to its
`@end…` (`@show`, `@stop` and `@overwrite` close a `@section`), with the closing rule of Svelte's
blocks; `@section` with two arguments is inline and opens nothing. `{{-- … --}}` is a comment and
`@verbatim` holds text. PHP inside `@php` is text (scripts apply to `<script>` alone). An
opener whose argument list does not close within 4 KiB (`@foreach(` with no `)`) opens a block
named by the directive alone (`@foreach`); its arguments are text and its `@end…` closes it.
A Blade directive in a `class` attribute is no class of the element's name:
`<div class="@if($x == 1) a @endif b">` is `div.a.b`.
`cart.blade.php:8: @section('content') › div.cart › @foreach($items as $item) › li │ …`

**HTML's output changes from v1.0 in its scripts only** (Open question 2, answered B):
`html.toml` turns `scripts` on with the limits above, so a Match in a function of a page's
script gets `script › onClick` under the element's Trail, where 1.0 stopped at `script`.
`templates` stays off. The v1 snapshots of spec §5.4 do not change: their one script
(`tests/fixtures/markup/page.html`) declares no function, so the JavaScript query names nothing
in it. The change is recorded in `docs/rg-compat.md` (§14); `--no-trail` is untouched.

### 8.6 GraphQL

`blocks`, `syntax = "graphql"`. Definitions are Segments with their keyword as kind: `type`,
`interface`, `input`, `enum`, `union`, `scalar`, `schema`, `directive`, `extend`, `query`,
`mutation`, `subscription`, `fragment` (an anonymous `{ … }` operation is a `query` named
`query`). A field with arguments or a selection set, and every field of a type, is a `field`
Segment named by its alias or name. Strings, block strings (`"""`) and `#` comments are lexed.
`schema.graphql:12: User › email │   email: String! # needle` and
`ops.graphql:4: GetUser › user › posts │       title(needle: true)`. The tree shows `type User`,
`field email` (the kind before the name).

### 8.7 Protobuf

`blocks`, `syntax = "protobuf"`. `message`, `enum`, `service`, `rpc`, `oneof` and `extend` are
Segments with their keyword as kind, nested; a field (`string email = 1;`) and an enum value are
`field` Segments; `package`, `import` and `option` statements are text. `//` and `/* */`
comments, strings. `users.proto:9: User › email │   string email = 1 [json_name = "needle"];`

### 8.8 HAR captures

`keypath`, `syntax = "json"`, `view = "har"`. `log` is hidden; each element of `entries` is an
`entry` Segment named by its request's method and URL without the scheme
(`GET api.example.com/v1/users`), cut at `name_chars` (60 for HAR), signature with the full URL
and the response status; `pages` elements are named by their `title`. The elements of
`headers`, `cookies`, `queryString` and `postData.params` are named by their `name` value
(`header Authorization`) instead of their index. Every other key is keypath's.
`session.har:812: GET api.example.com/v1/users › response › content › text │       "text": "{\"needle\":1}",`
The values a name needs may come after the Match in the file; the view reads them during the
preparation (§11). These names are rule 5's exception (§3): an entry, a page and an element of
those arrays have no label, and a Match in the URL, `title` or `name` value that names them
stays inside, under its key: `GET api.example.com/v1/users › request › url`,
`GET api.example.com/v1/users › request › header Authorization › name`, `Login › title`.

## 9. Group E · Data and binaries

The `object` Processor reads these files with the rules of spec §5.7, unchanged: where rg finds
a Match (a named file, `--binary`, `-a`), s4 prints one row per printable string that holds
Matches, at its hex offset, with its Trail; a walked file without `--binary` prints what rg
prints; `-v`, `-c`, `-l`, `-q`, `--no-trail` print rg's output; `--within` filters rows;
`--expand` prints the rows; `--json` is §5.7's. The Definition's `layout` parameter names the
reader; the image Segment (`object`) names the Format, a container (`section`) names a chunk,
segment, box, frame, IFD or section, and a field (`key`) or a member names what holds the bytes.
A file whose magic matches but whose layout does not read prints as rg prints it, without a
warning (spec §5.7, D9). Each reader is code of our own, a walk over length-prefixed units that
never reads past a unit's end nor the file's (§5.4).

| Layout | Trail | Example row |
|---|---|---|
| `jpeg` | `JPEG › APP1 Exif › IFD0 › Model`; `JPEG › APP1 XMP › …`; `JPEG › COM` | `photo.jpg:0xbc: JPEG › APP1 Exif › IFD0 › Model │ needle-cam 3000` |
| `png` | `PNG › tEXt Comment` (chunk type and keyword), `PNG › iTXt XML:com.adobe.xmp › …`, `PNG › eXIf › IFD0 › Artist` | `shot.png:0x29: PNG › tEXt Comment │ needle` |
| `webp` | `WebP › EXIF › IFD0 › Artist`, `WebP › XMP › …` | |
| `tiff` | `TIFF › IFD0 › ImageDescription`, `› Exif IFD`, `› GPS IFD` | `scan.tif:0x8a: TIFF › IFD0 › ImageDescription │ needle scan` |
| `id3` | `ID3v2.4 › TIT2`, `› TXXX MusicBrainz Album Id`, `› COMM`; `ID3v1 › title` for the 128-byte tag at the end | `song.mp3:0x2b: ID3v2.4 › TIT2 │ needle song` |
| `mp4` | `MP4 M4A › moov › udta › meta › ilst › ©nam` (the box path, the image named by the major brand) | `clip.m4a:0x1c3: MP4 M4A › moov › udta › meta › ilst › ©nam │ needle` |
| `wasm` | `WebAssembly › export › needle_fn`, `› import › env.log`, `› data › data[0]`, `› code › total` (a function named by the `name` section, else `func[N]`), `› custom .debug_str` | `app.wasm:0x3f0: WebAssembly › data › data[0] │ needle not found` |
| `class` | `Java 17 › com.example.Cart › addItem` (method), `› NEEDLE` (field), `› constant pool` | `Cart.class:0x8f: Java 17 › com.example.Cart › addItem │ needle merged` |

Details, each a decision:

- **EXIF** is a TIFF structure wherever it sits (JPEG APP1, PNG `eXIf`, WebP `EXIF`, a TIFF
  file, and the TIFF-based raw formats, which read as TIFF): IFD0, IFD1, the Exif, GPS and
  Interoperability IFDs are `section` Segments, and an entry whose value holds the Match is a
  `key` Segment named from a table of the standard tags (`Model`, `Artist`, `DateTimeOriginal`),
  or `tag 0x9c9b` for any other. Values in the IFD entry itself (four bytes or fewer) are inside
  the entry.
- **XMP** is XML: inside its packet, the Trail continues with the element path `markup`'s XML
  rules give (spec §5.4), below `rdf:Description` (whose ancestors every packet repeats):
  `JPEG › APP1 XMP › dc:title › rdf:Alt › rdf:li`. A packet that is not well-formed keeps the
  `XMP` Segment for the Matches after the error. Entries that name one XMP packet read it once,
  with the first entry's Trail; the XMP read inside a TIFF adds up to at most its size. An XMP
  entry that shares bytes with a packet read before owns the rest of its bytes, unread.
- **ID3** frames are named by their four-character id; `TXXX` and `WXXX` add their description,
  `COMM` stays `COMM`. ID3v2.2's three-character ids read the same. An ID3v2 tag that declares
  more than the file holds reads up to the file's end, and stops before an ID3v1 tag that ends
  the file. An MP3 with only an ID3v1
  tag has no magic at its start and is not detected. A text frame in UTF-16 is
  found only by a pattern rg finds in UTF-16, which is rg's behaviour (§2, Out).
- **MP4** box types are four bytes shown as Latin-1 (`©nam`); the box tree is walked through
  the container boxes (`moov`, `trak`, `mdia`, `minf`, `stbl`, `udta`, `meta` with its 4-byte
  header, `ilst` and its items), and `mdat` is a `section` with nothing inside. The same walk
  reads QuickTime `.mov`, 3GP and the HEIF/AVIF image brands, named by their brand; a file with
  no `ftyp` at offset 4 (old QuickTime) is not detected.
- **WebAssembly** sections are named by their id (`type`, `import`, `function`, `table`,
  `memory`, `global`, `export`, `start`, `element`, `code`, `data`, `datacount`, `tag`) and
  custom sections `custom NAME`; the `symbol` inside is the export or import name
  (`module.field` for an import), the function of a code body from the `name` section, or
  `data[N]`.
- **Java classes.** The image is named `Java N` from the class version (`major − 44` from 49 on,
  `Java 1.x` below, `Java class M.m` when unknown); the `class` Segment names the class in
  dotted form. A constant that names a field or method is that member's name first: a Match in
  it has the class's Trail (rule 5), even when an `ldc` loads the same Utf8 entry. Any other
  constant-pool string is assigned to the method that loads it (`ldc`, `ldc_w` in its `Code`,
  found by walking each method's bytecode with the JVM's instruction lengths), else to the
  field whose constant value it is, else to the field or method it describes, else to
  `constant pool` (`section`). The constant pool stores modified UTF-8, which rg searches as
  bytes.
- **Fat Mach-O against Java** (`cafebabe`): §4.2.

## 10. Group F · Everyday

### 10.1 Mail

The `mail` Processor reads one RFC 5322 message (`.eml`) or an mbox (messages each starting at a
`From ` line at the start of the file or after a blank line; mboxo and mboxrd alike). In an
mbox, a `From ` line opens a message only when a header field follows it; one with no header
field after it is text of the message before it and cuts nothing. An `.eml` skips a
first `From ` line that is no field (Thunderbird's `From - …`).

- **Segments.** In an mbox, a message is a `message` Segment named by its decoded `Subject`
  (`(no subject)` without one), signature `From · Date · Subject`; an `.eml` file is one message
  and has no `message` Segment. Each header field is a `header` Segment named by the field's
  name (`Subject`, `Received`), over its folded lines. A non-multipart body is a `part` Segment
  named `body`. In a multipart message each part is a `part` Segment named `part N` in IMAP's
  numbering (`part 1.2`) and its media type (`part 1.2 text/plain`), or `attachment NAME` when
  it has a file name, nested as the MIME tree is; the part's headers are `header` Segments
  inside it.
- **Decoding names.** RFC 2047 encoded words (`=?UTF-8?B?…?=`, `?Q?`) in a subject or a file
  name are decoded, their charset through `encoding_rs`, and RFC 2231 file names too (rule 4).
  Bodies are never decoded (8390).
- **Example.** `bill.eml:3: Subject │ Subject: Your needle invoice`;
  `inbox.mbox:2210: Re: the needle budget › part 1 text/plain │ …`
- **Large files.** An mbox can be gigabytes: the Processor indexes messages only and builds
  headers and parts for the messages that hold Matches, reading no further than the message of
  the last Match (criterion 52).
- **Failure.** A boundary never closed ends its part at the end of the message; a header block
  without its blank line ends at the first line that is not a header.

### 10.2 Calendars and contacts

`keypath`, `syntax = "ical"`, for iCalendar (RFC 5545) and vCard (RFC 6350), which share the
content-line syntax.

- **Model.** `BEGIN:X` … `END:X` is a `component` Segment named `X` and its `SUMMARY` (events,
  to-dos, journals), `FN` (cards) or `TZID` (time zones) when it has one (`VEVENT Team sync`,
  `VCARD Ana Pérez`), else `X[N]` among its siblings of that kind when there are two or more,
  `X` alone otherwise; the value may come after the Match (§11). These names are rule 5's
  exception (§3): a component's label is its `BEGIN:X` line alone, and a Match in its
  `SUMMARY`, `FN` or `TZID` stays inside, under that key (`VEVENT Team sync › SUMMARY`). A
  single `VCALENDAR` around the whole file is hidden, as HTML's `body`. A property is a `key`
  Segment named by its name without parameters (`DTSTART` of `DTSTART;TZID=Europe/Madrid:…`),
  over its folded lines (a line starting with a space or tab continues the one before); its
  parameters may fold before its `:`.
- **Example.** `team.ics:14: VEVENT Team sync › DESCRIPTION │ DESCRIPTION:Bring the needle`;
  `contacts.vcf:22: VCARD Ana Pérez › EMAIL │ EMAIL;TYPE=work:ana@needle.example`
- **Failure.** An `END` closes the innermost open component of its name and those inside it; an
  `END` with none open is text; components open at the end close there.

### 10.3 BibTeX

`keypath`, `syntax = "bibtex"`. An entry (`@article{knuth1984, …}` or with parentheses) is an
`entry` Segment named by its key, signature `@article{knuth1984,`; an entry without a key (its
first item is a field) is named by its type (`@article`). Each field is a `key` Segment, its
value in braces (nesting), quotes or bare, `#` concatenations included. `@string` and
`@preamble` entries are named `@string` and `@preamble`; `@comment` and the text between
entries are text with an empty Trail (BibTeX ignores it), and `@comment` ends at its closing
brace or parenthesis.
`refs.bib:5: knuth1984 › title │   title = {Literate needle programming},`
Where an entry closes follows its quoted values: a `}` or `)` between quotes closes nothing.
An entry whose braces or a quote never close ends at the next line that starts with `@`; in one
that closes, such an `@` is text of its value.

### 10.4 WhatsApp

The `chat` Processor reads WhatsApp's text export: a message starts at a line that begins with
a date and a time in one of the export's forms — Android's `31/12/23, 21:41 - ` and
`12/31/23, 9:41 PM - `, iOS's `[31/12/23, 21:41:05] `, with the U+200E and U+202F characters
newer exports put in them — and runs over the lines up to the next such line. The forms also
take 12-hour times written `a. m.`/`p. m.` (`31/12/23, 9:41 p. m. - `, its spaces plain,
U+202F or U+00A0) and dates separated by `.` or `-` (`31.12.23`). A U+200E before the sender
is left out of its name.

- **Segments.** Consecutive messages of one date form a `day` Segment named by the date as
  written (`31/12/23`: the export does not say whether it is day-first, so s4 does not reorder
  it). A message is a `message` Segment named by its time as written and its sender
  (`21:41 Ana`), or its time alone for a system message.
- **Example.** `WhatsApp Chat with Ana.txt:120: 31/12/23 › 21:41 Ana │ 31/12/23, 21:41 - Ana: where's the needle?`
- **Detection.** By name and path, with no look at the content (Open question 1, answered A):
  `_chat.txt`, the name of the text inside the export's zip, and one glob, `*WhatsApp*.txt`.
  The app's name is in the export's file name in every language (`WhatsApp Chat with Ana.txt`,
  `Chat de WhatsApp con Ana.txt`), so one glob replaces lists of localized names, which would
  need sources this repository does not have. A `.txt` that matches the glob and is no export
  costs nothing visible: it fails the first-line check below and its Matches show as rg shows
  them (8385).
- **Failure.** The first line, after a BOM and a U+200E, must open a message; a file whose first
  line does not is no export, and every Match in it has an empty Trail. After that, a line that
  opens no message continues the message before it.

### 10.5 Telegram

Telegram Desktop exports a chat as JSON (`result.json`) or HTML (`messages.html`,
`messages2.html`…), in a folder named `ChatExport_<date>` (a whole account in
`DataExport_<date>`).

- **JSON**: `keypath`, `syntax = "json"`, `view = "telegram"`. A chat (the root of a chat
  export, or an element of `chats.list`) is a `chat` Segment named by its `name`; an element of
  `messages` is a `message` Segment named by its `date` as written and its `from`
  (`2024-01-31T10:04:12 Ana`); the arrays `text` and `text_entities` keep their keys and hide
  their indexes. `result.json:5521: Ana › 2024-01-31T10:04:12 Ana › text │       "text": "needle",`
  These names are rule 5's exception (§3): a chat and a message have no label, and a Match in
  the `name`, `date` or `from` that names them stays inside, under its key (`Work › name`,
  `Work › 2024 Ana › from`).
- **HTML**: `markup`, `syntax = "html"`, `view = "telegram"`. Each `div.message` is a `message`
  Segment named by the `title` of its date element and its sender's name; a message without a
  sender name (Telegram's `joined` messages) takes the sender of the one before. A service
  message (a date separator, «X joined the group»), with no date title and no sender, is named
  `service`, a word s4 adds. The page's wrapper elements are hidden.
  `messages.html:880: 31.01.2024 10:04:12 Ana │ <div class="text">needle</div>`
  The view depends on the class names of Telegram's export template: when they are absent the
  file keeps the HTML Trails of spec §5.4, so a change on Telegram's side costs the decoration,
  never the Match.

## 11. Cost and streaming

These requirements hold for every M6 Processor and are what criteria 49–52 measure.

- **Files that do not match cost what they cost today** (spec §11 #26, M5b §7): no M6 Format
  reads, maps or detects anything before a file has a Match, except the path checks of §4.1,
  which stay at today's cost. The M5b §7 criteria (`s4 --no-docs --no-trail`, `s4 --no-docs`
  and the default, each ≤ 1.05 × its reference over the M4 text corpus) hold with every M6
  Definition loaded, and M6's s4 against M5b's over that corpus stays within 1.03 ×.
- **One reading per file with a Match.** A Processor prepares the file once (its Outline, or an
  index of units, M5b §6.1); the values a Segment's name takes from later in the file (nginx's
  `server_name`, a vCard's `FN`, a HAR entry's URL) are read in that preparation. Scanners read
  at 50 MB/s or more on this machine, the rate the v1 scanners meet (spec §4.3, §11 #13).
- **Streaming.** Each new Processor gives a native cursor (an Outline Processor gets one,
  `OutlinePrepared`), so its rows print as they are computed and none takes the model path
  (M5b §6.1); each cursor implements `open_from`, so `--expand` keeps no more Matches than the
  open Section needs (the M5b code reviews found markup and subtitles without it).
- **Large files.** `sql`, `mail` and `chat` keep an index of their top-level units (statement,
  message, day), a few dozen bytes each, build inner Segments only for units with Matches, and
  read no further than the unit of the last Match.
- **Dense files.** M5b §6.3's criterion extends to one Format of each new Processor family
  (criterion 50).
- **Scripts in HTML.** A script is parsed only when a Match (or `--in-headings`' names) falls in
  it, once, and never past `script_max_bytes`; finding its longest line is a `memchr` pass over
  those bytes. A page whose Matches are outside its scripts parses none, so a large HTML file
  costs the markup scanner's rate, and each parsed script tree-sitter's rate for a `.js` file
  (spec §4.3). Criterion 53 measures a 50 MB page.
- **Binary layouts** read only the file a Match was found in, as §5.7's object files, at
  100 MB/s or more (spec §11 #26's rate).

## 12. Windows and macOS

M6 adds no system-specific code: every reader is a byte scanner over the Source. The four CI
systems (M5b §5) run every new test. What needs care, each with a test that runs on all four:

- **Paths.** The globs of §4 see `/` on Windows through `globset`'s normalization; a test
  detects `C:\Users\x\.ssh\config` and `…\nginx\sites-enabled\default` on Windows and their `/`
  forms elsewhere.
- **Names compared without case.** The Detector compares exact names with case, as today; the
  Definitions list the spellings in use (`Makefile`, `makefile`), so a case-insensitive file
  system changes nothing.
- **Line ends.** Every fixture of M6 exists with LF; each Processor has a CRLF copy of one
  fixture (generated by the test from the LF one, so `.gitattributes`' `-text` keeps both
  byte-exact) whose Trails equal the LF ones.
- **Binary fixtures** are written by the tests, with writers next to `object::fixture` (M4
  plan, D19), so no system's checkout changes them.

## 13. Tests

For every M6 Format:

- **A fixture** under `tests/fixtures/<group>/` (`docs-a/`, `config/`, `code-c/`, `mail/`,
  `chat/`; the binary ones written by the tests), small, written for the purpose (no third-party
  file), holding a Match at each kind of Segment its section describes, plus a broken copy that
  exercises §3 rule 3.
- **Snapshots** of the tree, one-line and `--json` output (criterion 2's rule, extended), and a
  test per example Trail of §6–§10.
- **The rg differential**: `s4 --no-docs --no-trail` and `s4 --no-docs --json --no-trail` equal
  rg 14.1.1 over every M6 fixture, with the timings masked (criteria 1 and 12), in
  `tests/rg_compat_m6.rs`.
- **The structural flags**: `--within` with a Trail of that Format, `--expand` printing its
  Sections, `--in-headings` finding a name and not an added word (§3 rule 4).
- **The fuzz**: `tests/fuzz_m6.rs` (the text Formats) and `tests/cli_m6_object.rs` (the binary
  layouts) run each new Processor and layout over mutated fixtures (bytes
  changed, cut, repeated): nothing panics, a file ends within 2 s, every Segment lies inside
  the file and every name inside its Section's lines or generated.
- **Detection**: a unit test per row of §4.2, and the cases of §4.1 (compound extension over
  PHP, a glob that writes the extension before the extension, a glob that leaves it open after
  the extension, `cafebabe` read as a class and as a fat Mach-O).

## 14. Docs

- **README and README.es**: the Formats section becomes one table of every Format with one
  example Trail each, and its per-Format detail moves to a new **`docs/formats.md`** and
  **`docs/formats.es.md`** (8375: English first, Spanish too), which the README links; the
  README would otherwise pass 1 200 lines.
- **The External Processor guide** (`docs/external-processors.md` and its Spanish copy,
  `docs/examples/`, `tests/guide.rs`) uses INI as the Format «s4 has no» (M5b §10). With M6's
  `ini`, its worked example moves to gettext `.po` files, a Format M6 leaves out, so the guide
  keeps showing a Plain file becoming a described one; the README's Helper Processors example
  moves with it. The tests that write a user `ini` Definition (`tests/cli_m2.rs`,
  `tests/cli_m5.rs`, `tests/helpers.rs`) keep working, since a user Definition replaces the
  built-in with its name (spec §4.2). `--list-formats` shows that `ini` as `overridden` instead
  of `user`, and none of the three asserts its origin (checked on `efabcde`), so they need no
  change.
- **The skill** (`skills/searchfor/SKILL.md`): its description and Formats list name the M6
  Formats (the description is what makes an agent load it), with three examples: a config
  Trail, a SQL dump row, a diff hunk.
- **`docs/json-schema.json`**: the `kind` description lists the M6 kinds.
- **`docs/rg-compat.md`** (generated from `src/cli/rg_flags.rs`), a section «Trails that changed
  since 1.0»: HTML's scripts (§8.3), with
  a before/after example and the three limits. It lists the one change to an existing Format's
  Trails; `--no-trail` and rg's own output are unaffected, so no flag row changes. The README's
  HTML row and `docs/formats{,.es}.md` say the same.
- **CONTEXT.md**: Segment's definition adds «a statement, block, directive, hunk, message part,
  cell or field of the M6 Formats»; no new term is needed.
- **`--help`** and the man page list `paths` and compound extensions where they describe
  Definitions; `--list-formats` and `--export-formats` cover the new Definitions with no change.

## 15. Milestone parts

Four tandas of twelve chunks (the plan's chunk graph); within each, the chunks touch separate
files, except the registration points every chunk adds a line to (`src/formats/builtin.rs`,
`processors::by_name`, the tree's label kinds, the schema's description), which the
orchestrator merges by hand. Before tanda 1, the orchestrator commits one preparatory change
(the plan's Task 0) that frees the existing tests from every name, extension, count and fixture
directory M6 adds, so every chunk starts from a suite that stays green when it registers its
Formats.

| Tanda | Chunks | Formats |
|---|---|---|
| 1 · Detection and configuration | Detector: compound extensions and `paths` (§4) · keypath syntaxes `ini`, `properties`, `env`, `ssh`, `ical`, `bibtex` · `blocks` (nginx, Apache, HCL, GraphQL, Protobuf) · `buildfile` (Dockerfile, Makefile, justfile) | 16 |
| 2 · Documents and code | `headings` (AsciiDoc, rST, Org, LaTeX) · JSON views (Jupyter, HAR, Telegram) · `markup` scripts, templates and the Telegram view (Vue, Svelte, Blade, Telegram HTML) · `sql` · `diff` · `mail` (`.eml`, mbox) · `chat` (WhatsApp) | 16 |
| 3 · Binaries and cost | `object` layouts (JPEG, PNG, WebP, TIFF, ID3, MP4, WebAssembly, Java class) · cost and dense criteria over every new Processor (§11) · fuzz | 8 |
| 4 · Closing | docs (§14) · version 1.1.0 | 0 |

Tanda 1 goes first because the Detector's `paths` and compound extensions detect Formats of
tanda 1 itself (nginx, `.env`, ssh config, Dockerfile) and of tanda 2 (Blade, the chat exports);
the other chunks of tanda 1 can start with `--format` in their tests and switch to detection when
the Detector chunk lands.

## 16. Acceptance criteria

New, numbered after M5b's #42:

43. **Detection (M6).** Every row of §4.2 detects its Format; `cart.blade.php` is Blade and
    `cart.php` PHP; `Dockerfile.dev`, `.env.production`, `~/.ssh/config` and
    `/etc/nginx/sites-enabled/default` are detected by `paths` on the four systems (with `\` on
    Windows); `Makefile.toml` is TOML; `ChatExport_2024/result.json` is `telegram` and
    `ChatExport_2024/messages.html` `telegram-html`, while a `result.json` or `messages.html`
    outside a `ChatExport_*` folder is JSON or HTML (§4.1, signal 4); `WhatsApp Chat with
    Ana.txt` and `Chat de WhatsApp con Ana.txt` are `whatsapp`, and a `.txt` without `WhatsApp`
    in its name Plain; a `.conf` outside the paths of §4.2 is Plain; a `cafebabe` file reads as
    a Java class or a fat Mach-O, whichever it is.
44. **Every M6 Format** has a fixture, a snapshot of its tree, one-line and JSON output, and a
    test of each example Trail of §6–§10; `s4 --no-docs --no-trail` and `--json --no-trail` equal
    rg 14.1.1 over every M6 fixture (`tests/rg_compat_m6.rs`).
45. **Rules of §3.** Each Format's broken fixture keeps the Trails before its error, gives an
    empty Trail after it, prints no warning and exits as rg; `--in-headings` finds a name from
    the file and nothing in an added word; a Match in a Segment's own name has its parent's
    Trail, except where §6–§10 say otherwise.
46. **Binaries (M6).** A walked JPEG, MP3 or `.class` without `--binary` prints what rg prints;
    named on the command line, its rows carry the Trails of §9's table; `-v`, `-c` and
    `--no-trail` print rg's output.
47. **Single-file components (M6).** A Match in a Vue method gets `script › addItem`, in a Svelte
    `{#each}` its `block`, in a Blade `@foreach` its `block`. In HTML (§8.3): a function in a
    script with no `type` and in a `module` script gets its function Trail, a JSON-LD script its
    key Trails, and a `text/x-template` script, one over 1 MiB and one with a line over 1 000
    bytes keep the `script` element's Trail; every v1 snapshot of `html.toml`'s fixtures is
    unchanged.
48. **The fuzz (M6)** of §13 runs every new Processor and layout: no panic, each case within
    2 s, every Segment inside the file.
49. **Per-file cost (M6).** `scripts/bench-m6.sh`: over a generated corpus of the M6 text
    Formats, with a pattern no file matches and with `--no-trail`, s4 ≤ 1.1 × rg; with a pattern
    matching under 5 % of the files, s4's time over rg's ≤ the largest matching file ÷ 50 MB/s
    plus all matching files ÷ (50 MB/s × threads) plus 0.05 s (criterion 13's M3 rule); over
    the M4 text corpus, M5b §7's three ceilings hold with the M6 Definitions loaded, and M6's s4
    against M5b's (`--flat` and `--no-trail`, `--no-docs`) ≤ 1.03 ×, the median of three
    alternated series.
50. **Dense files (M6).** M5b §6.3's criterion (memory bound; `--json` time bound; one-line time
    measured and reported) in `tests/perf.rs` over 50 MiB files where every line matches, one per
    new Processor family: AsciiDoc, INI, nginx, Makefile, SQL (an `INSERT` dump), a diff, an
    mbox, a WhatsApp export, and HAR through the JSON view.
51. **Detection cost (M6).** Over 100 000 files whose names no M6 signal claims, among them
    `.json`, `.html` and `.txt` files (some named `result.json`, `messages.html` and
    `notes.txt`) outside Telegram's folders, which signal 4 of §4.1 consults, s4 with the M6 Definitions ≤ 1.02 × M5b's s4
    (`--no-docs`, a pattern no file matches, median of three alternated series).
52. **Large files (M6).** A generated 1 GiB SQL dump, mbox and WhatsApp export, each with one
    Match in its first unit, get their Trails within rg's time plus 0.5 s and rg's peak RSS plus
    64 MiB.
53. **Fifty-megabyte files (M6).** A 50 MB file of each new text Processor family with three
    Matches gets its Trails within rg's time plus 1 s (criterion 13's rule); so does a 50 MB
    HTML page of scripts, with its three Matches inside three scripts and, separately, outside
    every script.
54. **Binary size (M6).** Criterion 22 holds (≤ 23 MB) and the plan records M6's size.
55. **Docs (M6).** `docs/formats.md` and `docs/formats.es.md` describe every M6 Format with its
    example; the README and README.es list every Format; the guide's worked example runs with
    `.po` files (`tests/guide.rs`); the skill names the M6 Formats.
56. **Version 1.1.0 (M6).** `s4 --version` prints 1.1.0, and criterion 34 holds with the M6
    tests on the four systems.

## Open questions

1. **How chat exports are detected.** *Answered A (Sebastián, Hilo 855 #12694); WhatsApp's
   names became one glob, `*WhatsApp*.txt` (§10.4), and the chat Processor's first-line check
   decides structure, never the Format, so detection stays by name and path.*
   WhatsApp names an export `WhatsApp Chat with <name>.txt` (localized), and people rename it
   or unpack `_chat.txt` from its zip; Telegram's `result.json` and `messages.html` are told
   apart by their `ChatExport_*` folder. The spec rules out sniffing content (spec §4.1).
   Options:
   - **A** (recommended): names and paths only, as §4.2; a renamed export is read with
     `--format 'chats/*.txt=whatsapp'`, which the docs show. Keeps the rule and its zero cost.
   - **B**: for a `.txt` with a Match (and only then, so files without Matches cost nothing),
     look at its first line for WhatsApp's date pattern. Finds renamed exports, and makes the
     first exception to «no content sniffing».
2. **Scripts inside HTML.** *Answered B with three limits (Sebastián, Hilo 855 #12726,
   accepting #12697): JavaScript and `module` scripts only, JSON-LD by keypath's JSON, and
   scripts past 1 MiB or with a minified line keep 1.0's Trail (§8.3).* §8.3's `scripts` gives a `<script>` in a Vue file the Trails of its
   JavaScript; HTML pages hold scripts too. Options:
   - **A** (recommended): off for HTML in 1.1, so v1.0's HTML output and snapshots stay as they
     are; a user Definition can turn it on (`scripts = true` in a copy of `html.toml`).
   - **B**: on for HTML too, so `page.html:40: script › onClick │ …`; changes the Trails of
     every Match inside an HTML script, which a script reading 1.0's `--json` would see.
