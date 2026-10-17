# searchfor

searchfor (command: `sfor`) is a ripgrep-compatible search tool that tells you *where* each
match lives inside the structure of its file: the headings above it in Markdown, the class
and function around it in code, the key path in YAML/JSON/TOML, the section and symbol in a
binary.

## Language

### Finding things

**Match**:
A span of a file that satisfies the search pattern.
_Avoid_: result, hit, occurrence

**Fragment**:
The part of a long line shown around one or more nearby **Matches**, with markers for the
omitted text on each side.
_Avoid_: snippet, preview, excerpt

### Knowing what a file is

**Format**:
A kind of file that searchfor knows how to describe, such as Markdown, YAML, PHP or ELF.
_Avoid_: type, file type, language

**Format Definition**:
A declarative TOML description of one **Format**: how the **Detector** recognises it, which
**Processor** handles it, and that Processor's parameters.
_Avoid_: format config, format spec

**Detector**:
The part that decides the **Format** of a file from its extension, exact name, shebang,
leading magic bytes, or an explicit `--format` override.
_Avoid_: sniffer, classifier

**Grammar**:
A tree-sitter parser for one programming language. The Grammars of the v1 code Formats are
compiled into sfor; any other one comes as a **Grammar Helper**.
_Avoid_: parser, language pack

### Describing where a match is

**Processor**:
The component that reads a file of a given **Format** and produces the **Trail** of each
**Match** in it.
_Avoid_: parser, handler, plugin

**Helper Processor**:
A **Processor** that runs as a separate program and talks to sfor over protocol v1, JSON lines
on its stdin and stdout. It is started once per search, when the first file of its **Format**
needs a **Trail**, and runs in one of three modes its **Format Definition** names: `pool`
(several copies, one file each at a time), `concurrent` (one process, several files at once,
answers matched by id) or `oneshot` (one process per file).
_Avoid_: plugin, sidecar, daemon, "helper" for anything else

**External Processor**:
A **Helper Processor** written by anyone, in any language, and named by a **Format
Definition** with `processor = "external"`.
_Avoid_: plugin, extension

**Grammar Helper**:
A **Helper Processor** built by searchfor that carries one **Grammar** not compiled into sfor,
installed with `--install-grammar` together with its **Format Definition**.
_Avoid_: grammar plugin, grammar module

**Trail**:
The ordered list of **Segments** leading from the root of a file down to a **Match**.
_Avoid_: breadcrumb, path, context, scope

**Segment**:
One step of a **Trail**: a heading, a function, a key, an element of HTML or XML, a row or column
of a CSV file, a subtitle cue, a section of a binary, or a page, sheet or table of a document read
by an **Extractor**.
_Avoid_: crumb, node, level

**Section**:
The span of a file covered by a **Segment**, from where it opens to where it ends. The Section
of a Markdown table column Segment (`column`) is its whole table; in a CSV file, where the table
is the whole file, a column's Section is its row.
_Avoid_: block, region

**Extractor**:
A built-in component, enabled with `--docs`, that turns a document ripgrep cannot read (PDF,
Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB, SQLite) into searchable text plus its structure.
_Avoid_: converter, preprocessor (ripgrep's `--pre` is the external variant)

## Relationships

- A **Format Definition** describes exactly one **Format** and names exactly one **Processor**
- The **Detector** assigns at most one **Format** to each file
- A **Processor** produces one **Trail** per **Match**
- A **Trail** is made of zero or more **Segments**; an empty Trail means the Match sits at the
  top level of its file
- Every **Segment** delimits one **Section**
- An **Extractor** gives the text it extracts a **Trail** of its own (page, sheet, slide,
  table, row, column)
- A **Helper Processor** serves the files of one **Format** during one search
- A **Grammar Helper** carries exactly one **Grammar**; the `treesitter` **Processor** uses it
  when its Grammar is not compiled in
- A **Fragment** shows one or more **Matches** from the same line

## Example dialogue

> **Dev:** "When a **Match** falls inside a fenced code block in a Markdown file, does the
> **Trail** end at the heading?"
> **Domain expert:** "No — the code block is its own **Segment**, labelled with its
> language, so the Trail is `Install › Linux › bash block`."
>
> **Dev:** "And `--expand` prints what, exactly?"
> **Domain expert:** "The **Section** of the last **Segment** in the Trail: the whole bash
> block, not the whole Install chapter."
>
> **Dev:** "And a match inside a Lua file?"
> **Domain expert:** "Lua's **Grammar** is not compiled in. Once `--install-grammar lua` put
> its **Grammar Helper** in place, the `treesitter` **Processor** hands the file to that helper
> and gets back `Player › update`; without it, the file is Plain."
>
> **Dev:** "And a match inside a PDF?"
> **Domain expert:** "Only with `--docs`: the PDF **Extractor** turns it into text and gives
> each line a **Trail** like `page 42 › Chapter 3`; without `--docs`, searchfor treats the PDF
> as ripgrep does."

## Flagged ambiguities

- "path" was used for both the file location and the chain of headings — resolved: the file
  location is the **path**, the chain is the **Trail**.
- "parser" was used for both the tree-sitter **Grammar** and the component that builds
  Trails — resolved: a **Processor** may use a **Grammar**, and they are distinct.
- "context" collides with ripgrep's context lines (`-A`/`-B`/`-C`) — resolved: never used for
  the **Trail**.
- "helper" named both the install flag of grill #8 («un subcomando de sfor ayuda en la
  instalación») and the separate programs of 8451–8459 — resolved: a **Helper Processor** is the
  program; `--install-grammar` is the install flag.
