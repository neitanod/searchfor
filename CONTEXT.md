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
A tree-sitter parser for one programming language, installed separately from the binary.
_Avoid_: parser, language pack

### Describing where a match is

**Processor**:
The component that reads a file of a given **Format** and produces the **Trail** of each
**Match** in it.
_Avoid_: parser, handler, plugin

**External Processor**:
A **Processor** implemented as a separate program, in any language, that searchfor talks to
over a documented protocol.
_Avoid_: plugin, extension

**Trail**:
The ordered list of **Segments** leading from the root of a file down to a **Match**.
_Avoid_: breadcrumb, path, context, scope

**Segment**:
One step of a **Trail**: a heading, a function, a key, a section of a binary, or a member of
a **Container**.
_Avoid_: crumb, node, level

**Section**:
The span of a file covered by a **Segment**, from where it opens to where it ends. The Section
of a table column Segment (`column`) is its whole table.
_Avoid_: block, region

**Container**:
A **Format** whose content is a set of other files, such as zip, docx, epub, jar or tar.gz.
_Avoid_: archive, bundle

**Member**:
A file inside a **Container**, searched and described as if it were a file on disk.
_Avoid_: entry, inner file

## Relationships

- A **Format Definition** describes exactly one **Format** and names exactly one **Processor**
- The **Detector** assigns at most one **Format** to each file
- A **Processor** produces one **Trail** per **Match**
- A **Trail** is made of zero or more **Segments**; an empty Trail means the Match sits at the
  top level of its file
- Every **Segment** delimits one **Section**
- A **Container** holds **Members**, and each Member gets its own **Format**; the Member is a
  **Segment** of the Trail
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
> **Dev:** "If a Markdown file is inside a zip, which **Processor** runs?"
> **Domain expert:** "The **Container**'s Processor opens the zip, the **Detector** gives
> the **Member** the Markdown **Format**, and the Markdown Processor builds the rest of the
> Trail: `docs.zip › guide.md › Install › Linux`."

## Flagged ambiguities

- "path" was used for both the file location and the chain of headings — resolved: the file
  location is the **path**, the chain is the **Trail**.
- "parser" was used for both the tree-sitter **Grammar** and the component that builds
  Trails — resolved: a **Processor** may use a **Grammar**, and they are distinct.
- "context" collides with ripgrep's context lines (`-A`/`-B`/`-C`) — resolved: never used for
  the **Trail**.
