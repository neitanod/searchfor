> 🇦🇷 Leer en español: [external-processors.es.md](external-processors.es.md)

# Writing an External Processor

An **External Processor** gives Trails to a Format that searchfor does not know, without
changing s4. It is a program you write, in any language, that s4 runs as a **Helper
Processor**: s4 sends it the path of a file and the positions of the matches, and the program
answers with the Trail of each match. This guide shows how to write one, in this order:

1. [The worked example](#the-worked-example-ini-files-first-with---pre-then-with-a-helper): an
   INI Format, first with `--pre` and then as a Helper Processor, to show when each fits.
2. [The Format Definition](#the-format-definition): the fields that name the program.
3. [Protocol v1](#protocol-v1-message-by-message), message by message.
4. [The three modes](#the-three-modes) and when to choose each.
5. [Failures and how s4 reports them](#failures-and-how-s4-reports-them).
6. [Memory and temporary files](#memory-and-temporary-files).
7. [Testing a helper](#testing-a-helper).

The glossary of the terms in bold is [CONTEXT.md](../CONTEXT.md). The normative description of
the protocol is [spec §4.4](specs/2026-10-16-searchfor-design.md#44-helper-processors-7-84518459);
this guide follows it. Every output below is what s4 prints over the files of
[`docs/examples/`](examples), and `tests/guide.rs` runs the two examples and checks their
output on Linux, macOS and Windows.

## The worked example: INI files, first with `--pre`, then with a helper

The example searches this file, [`docs/examples/sample.ini`](examples/sample.ini). Run the
commands of the example from `docs/examples/`, where the file is:

```ini
[server]
host = needle.example
```

s4 has no INI Format, so by default it prints the match as **Plain**, with an empty Trail.
Two tools can change that, and they solve different problems.

### `--pre`: convert the file into text

`--pre COMMAND` is ripgrep's flag, and s4 runs it as rg does: it runs `COMMAND PATH` with the
file on stdin, and searches what the command writes instead of the file. Use `--pre` when the
file is not text you can search (a binary format, a compressed or encoded one) and a program
can turn it into text.

[`ini-pre.py`](examples/ini-pre.py) writes only the values of an INI file, one per line:

```
$ s4 --no-docs --no-trail --pre ./ini-pre.py needle sample.ini
needle.example
```

The output is what rg prints with the same `--pre`. The section and the key are gone: the
converted text has no structure, so it has no Trail to give. Use `--pre` to choose **what**
is searched.

**How to run a Python `--pre` command on each system:**

- **Linux and macOS.** `--pre` runs a program, and a script is one when it starts with a
  shebang line and has the executable bit. `ini-pre.py` starts with `#!/usr/bin/env python3`
  and is committed executable, so the command above works as it is. This is how rg users
  already write their `--pre` scripts.
- **Windows.** Windows does not read shebang lines, so `--pre` must name a program (`.exe`).
  [`pre-launcher.rs`](examples/pre-launcher.rs) is a small native program that runs
  `SEARCHFOR_EXAMPLE_PYTHON SEARCHFOR_EXAMPLE_CONVERTER PATH`, shares its stdin, stdout and
  stderr with the converter, and exits with the converter's exit status. Build it with the
  Rust compiler, without a Cargo project, and give `--pre` its absolute path: on Windows rg,
  and s4 with it, looks a relative `--pre` program up in `PATH` only, never in the current
  directory.

  ```
  > rustc pre-launcher.rs -o ini-pre-launcher.exe
  > $env:SEARCHFOR_EXAMPLE_PYTHON = "python"
  > $env:SEARCHFOR_EXAMPLE_CONVERTER = "$PWD\ini-pre.py"
  > s4 --no-docs --no-trail --pre "$PWD\ini-pre-launcher.exe" needle sample.ini
  needle.example
  ```

  The commands above are PowerShell. The launcher works on Linux and macOS too: from the
  repository root, `rustc docs/examples/pre-launcher.rs -o ini-pre-launcher` builds it, and
  the two variables name the interpreter and the script as above. The test of this guide
  builds the launcher and runs this search through it on every system.

### A Helper Processor: describe the file's structure

A Helper Processor leaves the search alone: s4 searches the file's own bytes, and asks the
helper only where each match lives. Use a Helper Processor when the file is already text and
what is missing is its structure, the **Trail**.

[`ini-helper.py`](examples/ini-helper.py) reads an INI file and answers with its sections and
keys. Its Format Definition is [`ini.toml`](examples/ini.toml):

```toml
name       = "ini-guide"
extensions = ["ini"]
processor  = "external"

[params]
command    = ["python3", "/absolute/path/to/docs/examples/ini-helper.py"]
mode       = "oneshot"
timeout_ms = 5000
```

Copy `ini.toml` into the formats directory and write the absolute path of `ini-helper.py` in
`command`. The formats directory is `searchfor/formats/` under the user configuration
directory:

| System | Formats directory |
|---|---|
| Linux | `$XDG_CONFIG_HOME/searchfor/formats/` (`~/.config/searchfor/formats/` by default) |
| macOS | `~/Library/Application Support/searchfor/formats/` |
| Windows | `%APPDATA%\searchfor\formats\` |

`SEARCHFOR_FORMATS_DIR` replaces that directory, as the tests do. On Windows write `"python"`
(or `"py"`, the Python launcher) instead of `"python3"`. Then the same search gives the Trail:

```
$ s4 --flat needle sample.ini
2: server › host │ host = needle.example
```

`server › host` is the Trail: the section `server`, and in it the key `host`. In the tree
layout, the default on a terminal:

```
$ s4 --tree needle sample.ini
sample.ini
  server
    host
      2:host = needle.example
```

and `--json` carries the same Trail as data, with the Format's name:

```json
{"type":"match","data":{"path":{"text":"sample.ini"},"lines":{"text":"host = needle.example\n"},"line_number":2,"absolute_offset":9,"submatches":[{"match":{"text":"needle"},"start":7,"end":13}],"format":"ini-guide","trail":[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}],"fragment":null}}
```

### Choosing between them

| What you have | What to use |
|---|---|
| A file that is not searchable text (binary, compressed, encoded) | `--pre`, which converts it |
| A text file whose structure s4 does not know | A Helper Processor, which gives the Trail |
| Both: a binary file with structure | `--pre` for the text, and a Helper Processor for the Format the file's name says: s4 sends the helper the converted text in a temporary file |

The two can be combined: with `--pre`, a Helper Processor receives the converted text, so
`ini-helper.py` behind `--pre ini-pre.py` finds no sections and prints the match with an
empty Trail (`1: needle.example`).

## The Format Definition

A Format Definition is a TOML file in the formats directory. A Helper Processor uses these
fields:

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | The Format's name: `--format <name>` selects it, warnings and `--json` show it. A user Definition with the name of a built-in Format replaces it |
| `extensions` | no | File extensions without the dot, as `["ini"]` |
| `names` | no | Exact file names, as `["Dockerfile"]` |
| `shebangs` | no | Interpreters named on a first line `#!`, read only for a file without an extension |
| `magic` | no | Leading bytes, as `[{ offset = 0, hex = "7f454c46" }]`, read only for a file without an extension |
| `processor` | yes | `"external"` for a Helper Processor |
| `params.command` | yes | The program and its arguments, as an array. A bare name is looked up in `PATH`, and never in the current directory; a path is used as given |
| `params.mode` | no | `"pool"` (the default), `"concurrent"` or `"oneshot"`; see [the three modes](#the-three-modes) |
| `params.timeout_ms` | no | The longest s4 waits for each answer, in milliseconds, counted from when s4 queues the request; a positive integer, 5000 when absent |

A Definition without `extensions`, `names`, `shebangs` or `magic` applies only to the files
of `--format <name>`. Every key of `[params]`, including the ones s4 does not use, reaches the
helper in `hello`, so a helper can take its own options from the Definition. `s4 --list-formats` shows whether the
Definition was loaded, and an invalid one is a warning that names the file and the field.

## Protocol v1, message by message

s4 and the helper exchange UTF-8 JSON, one object per line ended by `\n`, in both directions:
s4 writes to the helper's stdin, and the helper answers on its stdout. Both sides ignore fields
they do not know. Every request has an `id`, and its answer carries the same `id`. Every line
the helper writes to stderr is shown as a warning, `s4: <format>: <line>`, unless the user
passed `--no-messages`.

This is the transcript of `s4 --flat needle sample.ini` with `ini-helper.py`, recorded on
Linux (the `params` of `hello` are the Definition's, shortened here):

```
s4 → {"op":"hello","id":0,"protocol":1,"format":"ini-guide","params":{"command":[…],"mode":"oneshot"},"threads":1}
s4 → {"op":"trails","id":1,"path":"/home/me/sample.ini","matches":[{"line":2,"byte":16}]}
   ← {"id":0,"protocol":1,"name":"ini-helper.py","version":"1.0.0"}
   ← {"id":1,"trails":[[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]]}
```

### `hello`

The first line every process receives:

```json
{"id":0,"op":"hello","protocol":1,"format":"ini-guide","params":{…},"threads":1}
```

`params` is the Definition's `[params]` table and `threads` is the most requests s4 will have
in flight to this process (1 in `pool` and `oneshot`). Answer with the protocol you speak, and
optionally your name and version:

```json
{"id":0,"protocol":1,"name":"ini-helper.py","version":"1.0.0"}
```

If the helper answers another protocol number or an `error`, does not answer within
`timeout_ms`, or exits before it answers, s4 disables the Definition for the rest of the run,
with one warning, and prints the Format's files as Plain.

### `trails`

The Trail of each match of one file:

```json
{"id":1,"op":"trails","path":"/home/me/sample.ini","matches":[{"line":2,"byte":16}]}
```

`line` is the match's 1-based line and `byte` its absolute byte offset in the file at `path`.
Answer with one Trail per match, in the order of `matches`:

```json
{"id":1,"trails":[[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]]}
```

A Trail is a list of **Segments**, outermost first. A Segment is the object of `--json`:
`kind`, `name`, `line` and `end_line` (the first and last line of the Section), and the
optional `level` and `signature`. An empty list is a match with no Trail.

**Count bytes, not characters.** `byte` counts the bytes of the file. Read the file as bytes
and compute every offset over them; in Python, `open(path, "rb")`, never a decoded `str`,
whose indexes count characters and move as soon as a line holds `é` or a BOM.
`ini-helper.py` decodes a name only to show it.

### `names`

Every Segment name of a file, for `--in-headings`, which matches the pattern against the names
instead of the lines:

```json
{"id":8,"op":"names","path":"/home/me/sample.ini"}
```

```json
{"id":8,"names":[{"text":"server","pieces":[[0,6,1,7]],"anchor":1,"trail":[{"kind":"section","name":"server","line":1,"end_line":2}]},{"text":"host","pieces":[[0,4,9,13]],"anchor":9,"trail":[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]}]}
```

- `text` is the name as shown.
- `pieces` says where each piece of `text` comes from, as `[text_start, text_end, src_start,
  src_end]` byte ranges. The lengths are equal when the name is copied verbatim (`server` is
  bytes 0–6 of the text and bytes 1–7 of the file), and differ when the helper decoded it (an
  entity like `&lt;` is 1 byte of text over 4 of the file). A name spread over two lines has
  one piece per line.
- `anchor` is the byte where an empty match (`^`, `$`) lands when no piece contains it.
- `trail` is the Trail of the named Segment, with the Segment itself last.

### Errors

```json
{"id":9,"error":"ValueError: unknown op 'unknown'"}
```

An `error` answer fails that request only, and the process goes on with the next one.
`ini-helper.py` answers an `error` to an operation it does not know and to a file it cannot
read.

## The three modes

`params.mode` says how s4 feeds the helper:

| Mode | How s4 feeds it | When to use it |
|---|---|---|
| `pool` (default) | Each copy of the helper has one request at a time. s4 starts a new copy only when every running copy is busy, up to the search's thread count (`-j`), and gives each file to a free copy | A program that reads a line, answers it, and reads the next one. Choose it unless one of the other two fits better |
| `concurrent` | One process receives up to the search's thread count of requests without waiting for the answers (`threads` in `hello` says how many), and may answer them in any order: s4 matches each answer to its request by `id` | A helper that is expensive to start (a large Grammar, a JVM) and can work on several files at once |
| `oneshot` | One process per request: s4 writes `hello` and the request, closes stdin, and reads the two answers | Small scripts: the script may read its whole stdin before it writes anything. Each file pays the start of a process |

`ini-helper.py` is a `oneshot` helper: it reads its stdin up to EOF and only then answers each
request. A `pool` helper must instead answer each line as it reads it and flush its stdout
after each answer; s4 sends the next request only after the answer.

In `concurrent` mode, the answers of a helper that runs requests in parallel may come back in
another order than the requests, as in this sketch:

```
s4 → {"id":1,"op":"trails","path":"a.ini","matches":[…]}
s4 → {"id":2,"op":"trails","path":"b.ini","matches":[…]}
   ← {"id":2,"trails":[…]}
   ← {"id":1,"trails":[…]}
```

**Lifetime.** s4 starts a helper when the first file of its Format has a match, sends it every
later file of that Format in the same run, and closes its stdin when the run ends. Exit on
that EOF: s4 waits up to one second, and then kills the helper. The next run starts the helper
again. A helper that starts processes of its own must stop them when it exits; s4 waits at
most one more second for pipes such a child keeps open. On Windows, every process s4 starts,
helpers and their children included, belongs to a Job Object that ends them when s4 ends.

## Failures and how s4 reports them

A Helper Processor never stops the search and never changes s4's exit code. A file whose
request fails prints as Plain (no Trail, `format` `null` in `--json`), with one warning that
names the file and the Definition. These warnings are real s4 output; each follows the case it
reports:

| Case | Warning | What follows |
|---|---|---|
| The request gets an `error` answer | `s4: sample.ini: ini-guide: no parser for this file` | The file prints as Plain; the process goes on. Not counted as a failure |
| No answer within `timeout_ms` | `s4: sample.ini: ini-guide: did not answer within 500 ms` | s4 kills the process, which fails the other requests it had in flight |
| The helper exits with requests in flight | `s4: sample.ini: ini-guide: the helper exited` | The next request starts a new process |
| The helper writes a line that is not JSON, or has no `id` | `s4: sample.ini: ini-guide: the helper wrote a line that is not JSON (expected value at line 1 column 1)` | s4 kills the process, and every request it had in flight fails |
| Three failures in one run (crashes, timeouts, lines without JSON or `id`) | `s4: ini-guide: the helper failed 3 times; its files print without Trails for the rest of this run` | The Definition is disabled for the run |
| `hello` fails (wrong protocol, `error`, no answer, exit) | `s4: ini-guide: did not answer hello within 300 ms; its files print without Trails for the rest of this run` | The Definition is disabled for the run |
| The command cannot start | ``s4: ini-guide: cannot start `no-such-python3 x.py`: No such file or directory (os error 2); its files print without Trails for the rest of this run`` | The Definition is disabled for the run |
| The helper writes to stderr | `s4: ini-guide: <the line>` | Nothing else; `--no-messages` silences it |

How s4 counts failures:

- Each process counts once, however many requests it failed.
- An `error` answer, and an answer with the right `id` whose fields do not fit the protocol
  (the wrong number of Trails, a Segment without `name`), fail that request only and are not
  counted.
- A process that exits between requests, with nothing in flight, is replaced without counting
  a failure, so a helper may exit after each answer if it prefers.
- While a Definition is disabled its files print as Plain, with no more warnings. The next
  run tries the helper again.

## Memory and temporary files

**Keep nothing from one file for the next.** Requests are independent: read the file, compute
the answer, write it, and free the file, its tree and the answer before reading the next
request. Keep loaded only what belongs to the Format: a Grammar, a compiled query, the
`params` of `hello`. A `pool` or `concurrent` helper lives for the whole run, which can be
thousands of files, so memory that grows per file eventually stops the search. searchfor's
own helpers are tested to keep their memory flat over 5 000 requests.

**Read the file at `path`.** s4 sends the path and the positions, never the content. `path` is
absolute, or relative to s4's working directory, which the helper inherits. s4 sends the
file's own resolved path only when the file is a regular file whose bytes s4 searched
unchanged. For anything else (stdin, a FIFO, a file whose BOM s4 removed or whose UTF-16 it
transcoded, the output of `--pre` or of `-z`) s4 writes the bytes it searched to a temporary
file and sends that path, so `byte` always counts the bytes at `path`. Never reopen a stream
yourself: in the helper, stdin is the protocol's pipe.

**Temporary files are s4's.** s4 keeps them in a directory of its own, `searchfor-<random>`,
created in the system's temporary directory (`$TMPDIR` or `/tmp` on Unix, `%TEMP%` on Windows)
when the first one is needed. Each file is named `<n>-<file name>`, so the helper still sees
the extension, and s4 removes it after the answer. Read it and leave it alone: do not modify,
move or keep it. On Unix the directory gets the permissions of the user's umask; if other
users of the machine must not read what you search from stdin or through `--pre`, point
`TMPDIR` at a directory only you can read. If the helper needs scratch files of its own,
create them with the language's safe temporary-file function (Python's `tempfile`), never
under a fixed name in a shared directory, and remove them before answering.

## Testing a helper

1. **Talk to it by hand.** Write the requests to its stdin and read the answers. This is the
   `oneshot` exchange of the example, `names` and an unknown operation included:

   ```
   $ printf '%s\n' '{"id":0,"op":"hello","protocol":1,"format":"ini-guide","params":{},"threads":1}' \
       '{"id":8,"op":"names","path":"sample.ini"}' '{"id":9,"op":"unknown"}' \
       | python3 ini-helper.py
   {"id":0,"protocol":1,"name":"ini-helper.py","version":"1.0.0"}
   {"id":8,"names":[{"text":"server","pieces":[[0,6,1,7]],"anchor":1,"trail":[{"kind":"section","name":"server","line":1,"end_line":2}]},{"text":"host","pieces":[[0,4,9,13]],"anchor":9,"trail":[{"kind":"section","name":"server","line":1,"end_line":2},{"kind":"key","name":"host","line":2,"end_line":2}]}]}
   {"id":9,"error":"ValueError: unknown op 'unknown'"}
   ```

2. **Check that s4 loads the Definition.** `s4 --list-formats` lists it with its origin:

   ```
   $ s4 --list-formats | grep ini-guide
   ini-guide    user (/home/me/.config/searchfor/formats/ini-guide.toml) ini
   ```

3. **Run it through s4.** `--format <name>` applies the Format to every file you name,
   whatever its extension, so you can try the helper on any file:

   ```
   $ s4 --flat --format ini-guide needle sample.ini
   2: server › host │ host = needle.example
   ```

   `--json` shows each Segment's fields as the helper sent them, and a warning on stderr names
   any failure. Try `--in-headings` to exercise `names`.

4. **Run the guide's own test.** `cargo test --test guide` runs the two examples of this
   guide (the `--pre` search through the compiled launcher, and the Helper Processor through
   s4) and the exchange of step 1. Where Python 3 is missing it prints that it skipped them.
