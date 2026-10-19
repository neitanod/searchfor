# searchfor M5a (v0.5: every ripgrep flag, and an MCP server) Implementation Plan

> **For agentic workers:** each chunk below is implemented by one subagent in its own git
> worktree, following `test-driven-development`: every task writes its tests first, runs them
> and sees them fail (RED), implements, and runs them green (GREEN). Steps use checkbox
> (`- [ ]`) syntax. Read `CONTEXT.md` (vocabulary), `docs/specs/2026-10-16-searchfor-design.md`
> (the spec; the «M5 in two parts» note, §8, §9 and §11 #14, #22 and #29–#33 are M5a's) and this
> plan's «Design notes» before starting. The spec wins over this plan if they disagree, and the
> disagreement is reported. Everything is written in English: code, comments, docs, commit
> messages. Commit messages carry no attribution lines.

**Limits every chunk keeps** (they hold for M5b too, whose outline is at the end):

- **Nothing is published without Sebastián's OK.** No `cargo publish`, no `npm publish`, no
  Homebrew tap created or updated, no GitHub Release or tag, no binary signed with a key. M5a
  publishes nothing; M5b's plan prepares each publication and puts a step «Ask Sebastián» right
  before it, and stops there until he answers.
- **Nothing is pushed until the orchestrator says so.** Commits stay on the work branch. A CI
  workflow under `.github/workflows/` may be committed (the repository is public) and is not
  pushed with the rest until then.
- **No network in tests.** Every test, benchmark and fuzz run works offline; the Grammar index
  stays a `file://` URL, as since M2.
- **Tests never touch `~/.config`.** Every `s4` a test runs gets `SEARCHFOR_FORMATS_DIR`,
  `SEARCHFOR_GRAMMARS_DIR` and `SEARCHFOR_GRAMMAR_INDEX` inside its fixture, and
  `SEARCHFOR_CONFIG_PATH` removed (C1); a test of the configuration file writes it in the
  fixture.
- **Low priority on the shared machine** (Sebastián, 19/10): every cargo command runs as
  `CARGO_BUILD_JOBS=8 nice -n 19 ionice -c3 cargo …`, and every binary under test, benchmarks
  included, under `nice -n 19 ionice -c3`, rg too, with `uptime` written beside each
  measurement.
- **ripgrep 14.1.1's pins stay.** The `=` versions of ADR 0001 (grep-searcher, grep-printer,
  grep-regex, grep-matcher, ignore, globset…) do not move; the new PCRE2 crates are pinned at
  rg 14.1.1's lock (D7), and acceptance criterion 8 checks the lock.

**Goal:** `s4` v0.5, which implements every flag of ripgrep 14.1.1 — in `--no-trail` exactly as
rg prints it, and in the tree and one-line layouts with each row keeping its Trail — and serves
its search to AI agents as an MCP server (`s4 --mcp`).

**Architecture:** the M4 pipeline stays: search first, describe later. The command line gains
the 90 flags M1–M4 refused, resolved in command-line order as rg applies them (`cli::order`)
and from an optional configuration file (`cli::config`). The matcher becomes an enum over Rust's
regex engine and PCRE2 (`matcher::PatternMatcher`). The walker and the searcher take rg's walk
and search flags (`search`), and a file is decoded as `-E` says (`source::Decoding`). For
`--no-trail`, the summaries and `--files`, grep-printer's printers are configured as rg
configures them (`output::rg`); for the Trail layouts, a `Look` carries rg's output flags into
the rows (`output::Row`, `flat`, `tree`, `summary`): columns, byte offsets, one row per Match
(`-o`, `--vimgrep`), replacements, a row per line of a multi-line Match or record, separators,
palette and hyperlinks. `--generate`, `--debug`, `--stats`, `--files` and `--mcp` are new
actions of `app`. The MCP server runs each search as a child `s4 --json`.

**Tech stack:** Rust 1.90 (edition 2024), as M4. New dependencies (verified, «Verified for this
plan»): `grep-pcre2` =0.1.8, `pcre2` =0.2.9 and `pcre2-sys` =0.2.9 (ripgrep 14.1.1's `pcre2`
feature, at the versions of its Cargo.lock; PCRE2's C built into the binary), `clap_complete`
4.5 and `clap_mangen` 0.2 (`--generate`), and `log` 0.4 as a direct dependency (`--debug`; it
was already in the lock). tree-sitter stays at `=0.24.7` and every ripgrep 14.1.1 pin of ADR
0001 stays where it is.

**Reference ripgrep:** 14.1.1 at `/usr/bin/rg` (another `rg`, such as 15.x, may come first on
`PATH`), built with PCRE2 (`rg --pcre2-version`), as M3 and M4.

**Branch:** work branch `t-768-searchfor-m5`, worktree
`/home/sebas/robotin/apps/mdgrep/.workspace/t-768-mdgrep/wt/m5`, from `main` at `522bf96` (M1–M4
closed). The diffs below are against `522bf96`, each against the state of the tanda before its
chunk. Each chunk branches `t-768-searchfor-m5-c<N>` from the work branch and is merged back after
its tanda, as `ciclo` stage 7 describes (`using-git-worktrees`, `finishing-a-development-branch`).
Line numbers in the diffs are those of the state they apply to and only hints: locate code by
the names given. The code of this plan was built and tested in a probe copy of the repository,
frozen as `/tmp/subtask/m5-plan/m5tree-7631b14` (see «Verified for this plan»).

---

## Why M5a, and what goes to M5b

M4 closed the Formats of «what ripgrep reads as bytes». The roadmap left M5 everything that
makes s4 a v1.0: «the rest of ripgrep's flags», the MCP server, Windows and macOS, CI,
releases, Homebrew and npm, the Grammar Helper binaries, signed releases, the External
Processor guide and the streaming of dense files. The flags alone take eight of this plan's
eleven chunks, with their code written and tested; the rest of M5, estimated the same way
without its code, takes about fifteen more, well past the size of one plan. This plan cuts M5 in
two (D1):

| Milestone | Release | Content |
|---|---|---|
| M1–M4 (done) | v0.1–v0.4 | Markdown, keypath, code, markup, CSV, subtitles, object files, documents; 56 of ripgrep's flags; `--pre` and `-z` |
| **M5a (this plan)** | v0.5 | the other 90 flag names of ripgrep 14.1.1, in `--no-trail` as rg prints them and in the Trail layouts (D2–D16, D19–D21); `s4 --mcp` (D17); the M5a benchmark, perf ceilings and fuzz (D18) |
| M5b | v1.0 | Windows and macOS (the `cfg(unix)` paths: isolated readers, signals, the readers' `RLIMIT_DATA`, paths), the Windows Job Object over each document reader (M4 plan, D31), CI in GitHub Actions on Linux, macOS and Windows, releases with `dist` (GitHub Releases, Homebrew tap, npm, #18), Grammar Helper binaries in the Grammar index (§7), signed releases, the External Processor guide (§4.4), the streaming of dense files (§3, item 5) and the per-file cost s4 adds over rg on small files (Hilo, 18/10) |
| M6 | v1.x | the Formats of Questionnaire 49 that 8385 accepted |

**Why this cut.**

- **The order Sebastián set.** 8375: «keep going without stopping until every ripgrep flag and
  every Format discussed is supported. Afterwards, make sure it works on Windows and macOS».
  The flags come first; the platforms, and the releases that need them, after.
- **The flags are one piece of work.** Most of them change the same files: the command line,
  the searcher, the printers' rows. Split across two milestones they would be designed twice.
- **The MCP server goes with the flags.** It is one module (`src/mcp.rs`) and one action,
  independent of every other chunk, and it is the part of #19 that agents use; it needs no
  platform work.
- **Dense streaming goes to M5b.** It rewrites how the Trail pipeline holds a file (the
  Processors' Outlines, the model of a file's Matches), which is performance work best done once
  the rows of every flag are settled (this plan) and measured on the three systems (M5b's CI).
  The spec's note that it «touches the printers anyway» is honoured by M5a settling the rows;
  the streaming has its own criterion in M5b.
- **Distribution goes to M5b.** A release needs the Windows and macOS builds and their CI;
  publishing anything needs Sebastián's go-ahead at each step.

**What M5a leaves out, and why:**

- **rg's own usage-error messages.** A flag without its value or an unknown flag keeps clap's
  message (exit 2, as rg), as since M1: matching rg's 146 messages byte for byte is a parser of
  its own (`rg-compat.md`, «Usage errors»; D21).
- **Streaming stdin in the Trail layouts.** A file whose Format its name does not decide, stdin
  among them, is still read whole before its rows print; `--no-trail` streams it, line-buffered
  on a terminal (D13). The streaming of the Trail pipeline is M5b's.
- **`RIPGREP_CONFIG_PATH`.** s4 reads `SEARCHFOR_CONFIG_PATH` only (D3; a question for
  Sebastián below).

## Grill decisions this plan implements

| # | Decision | Where in M5a |
|---|---|---|
| 3 | Full compatibility with rg's flags, plus flags of s4's own | C1 (every flag parsed, in rg's order), C2–C4, C7–C9 (each implemented), C9 (`tests/rg_compat_m5.rs`) |
| 8375 | Every ripgrep flag and every Format discussed; afterwards Windows and macOS; everything in English, docs also in Spanish | C1–C9 (the flags); M5b (the platforms); C11 (README.es) |
| 8362 / 8365 | `--json` is ripgrep's messages plus s4's fields | C9: rg's JSON ignores the output flags, so s4's does; `--passthru` and `--stats` with `--json` as rg's |
| 19 | An MCP server (`sfor mcp` in the grill; `s4 --mcp`, a flag, as Sebastián decided on 19/10, 8837) | C1, C6, C9 |
| 18 | crates.io, GitHub Releases, a Homebrew tap, npm | M5b (nothing is published without Sebastián's go-ahead) |
| 8381 | Coloured output | C4 (the palette and `--colors`), C7, C8 |
| 8385 | As fast as rg; the extra cost in the presentation only; a Match without structure prints as rg prints it | C2 (a PCRE2 matcher per thread), C9 (`--files` from the walker's threads), C10 (`bench-m5.sh`, perf ceilings) |
| 8612 | The release binary at most 23 MB | C10 (measured: 21.0 MB) |
| 8750 | Documents searched by default | C3 (`--max-filesize` applies to them as to any file), C9 (`--passthru`, `--stats`, `-o` over their text) |
| Hilo, 18/10 | The command is `s4`; environment variables `SEARCHFOR_*` | C1 (`SEARCHFOR_CONFIG_PATH`, D3) |

Decisions 1, 2, 4–17, 20 and the Hilo's 8355–8750 that M1–M4 implemented stay as they are; the
platform and distribution part of 8375, #18 and 8366 is M5b's.

## Decisions delegated to this plan

Sebastián left these open; each one is taken here, with its reason, and the spec carries the
ones it describes marked «M5a plan». The ones that need his word are listed again at the end,
in «Questions for Sebastián».

| Id | Decision | Reason |
|---|---|---|
| D1 | M5 is cut in two: M5a (this plan, v0.5) is every flag of ripgrep 14.1.1 and the MCP server; M5b (v1.0) is Windows and macOS with the Job Object of the M4 plan's D31, CI on the three systems, the releases and their channels, the Grammar Helper binaries in the index, signed releases, the External Processor guide, the streaming of dense files and the per-file cost over rg | 8375 orders the flags before the platforms; the flags touch the same files and are designed once here; the MCP server is one independent module; the releases need the platforms; dense streaming rewrites how the pipeline holds a file, after the rows are settled. The whole of M5 in one plan would be about twenty-six chunks (an estimate: this plan's eleven are written and tested, the other fifteen counted without code) |
| D2 | Flags that set more than one thing are resolved by their place on the command line (`cli::order`): `--no-ignore`/`--ignore`/the first `-u` against each `--no-ignore-*`; `--sort`, `--sortr`, `--sort-files`, `--no-sort-files`; `-P`, `--no-pcre2`, `--engine`, `--auto-hybrid-regex`; `--no-unicode` and `--no-pcre2-unicode` with their negations; `--crlf`/`--null-data`; `-U`/`--stop-on-nonmatch`; `-E`/`--no-encoding`; the buffering flags; `--debug`/`--trace`; `--context-separator`/`--no-context-separator`; `--passthru` against `-A`, `-B`, `-C`; `--heading`, `--no-heading`, `--tree`, `--flat`, `--pretty` (and `--pretty` against `--color`, `-n`, `-N`); `--files`, `--type-list`, `--generate`. A switch is replayed at the place of its last occurrence; a flag with values at each | ripgrep 14.1.1 applies each flag's `update` in order to its `LowArgs` (defs.rs), so a later flag undoes part of an earlier one: `--no-ignore --ignore-vcs` reads `.gitignore` again but not `.git/info/exclude` (measured with `rg --files`). A switch sets the same values every time, so its last occurrence is the only one that can be the last to set any of them, and clap keeps that place. clap's `overrides_with` stays for plain pairs |
| D3 | `SEARCHFOR_CONFIG_PATH` names a configuration file in the format of rg's `RIPGREP_CONFIG_PATH` (one argument per line, trimmed, `#` comments and blank lines skipped), its arguments before the command line's; the command line is parsed alone first (`--no-config`, `--pcre2-version`, `--help`, `--version` never depend on the file); `--mcp` reads no file, and neither do the searches it runs (D17); no default location; a file that cannot be read is a message, the search goes on | rg 14.1.1's `config.rs` and `parse.rs`. The variable is s4's own, as the Hilo of 18/10 named every variable `SEARCHFOR_*`: a user's `RIPGREP_CONFIG_PATH` was written for rg and s4 is not rg (it prints Trails, searches documents by default); and with no default location no test can read the user's files. Reading `RIPGREP_CONFIG_PATH` as well is a question for Sebastián. A file's `--generate` would win over the command line's `--mcp` by the order of the actions, and its `--pre` would run on every call of the tool (review r1, Fable #1) |
| D4 | rg's output flags print in the Trail layouts as rg's standard printer prints them, row by row, each row keeping its Trail: `--column` the 1-based column of the row's first Match (rg: the line's first), `-b` the byte offset rg gives (the line's, with `-o` and `--vimgrep` the Match's), both after the line number with the field separator; `-o` a row per Match, its text the matched bytes, an empty Match or an empty replacement an empty row, as rg's printer of one line prints them, and only the active terminator left out (review r2, Codex #4); `--vimgrep` a row per Match with its line (rg's `per_match_one_line`), with its Trail; `-r` the line with every Match replaced over the Source as grep-printer's `Replacer` does, each row's replacements highlighted, columns those of the replacements (rg's), and in an object file's string each Match replaced where the search found it in the file's bytes, its anchors and groups read there, its replacement made by the search of the file in one pass over the Match's line (with `-U`, its block), as grep-printer's `Replacer` makes it, so a `\K` Match too (review r3, Codex #3) with the groups the search found, each line read once however many strings it holds (review r4, Codex #1 and #2), the string made printable after (review r2, Codex #2); `--trim`; `--max-columns-preview`; the field, context and path separators; `-0`; `--colors`; `--hyperlink-format`; `--include-zero` in s4's summaries. Every row ends with `\n`, with `--crlf` and `--null-data` too, where rg ends a line with the search's terminator (`\r\n`, NUL), and `--no-trail` prints rg's (review r4, Fable #1). `--json` keeps rg's JSON, which ignores them | A row of the Trail layouts is the part of a line under one Trail (M1), so its column is its own; the rest is rg's printer applied to that row. `--vimgrep` keeps the Trail because an editor's quickfix shows it as the message (`%f:%l:%c:%m`): `--no-trail --vimgrep` gives rg's exact lines. rg's JSON printer has no `-o`, `-r`, `--column`… (measured: `rg --json -o -r X --column` prints the plain match), and 8365 made s4's JSON rg's plus fields |
| D5 | `--heading`, `--pretty` and `--tree` choose the tree, `--no-heading` and `--flat` the one-line layout, the last one given winning; `--vimgrep` forces the one-line layout whatever comes before or after it; `--pretty` also sets `--color always` and `-n` where it comes after them. With `--no-trail` the same choice is rg's heading | The tree is s4's heading (grouped by file, the default on a terminal as rg's heading), the one-line layout rg's `--no-heading`. rg's `Pretty` sets color, heading and line numbers (defs.rs); rg's heading is `!vimgrep` whatever `--heading` says (hiargs.rs) |
| D6 | A `-U` Match that spans lines prints a row per line, numbered as rg numbers each line of the block, under the Trail of the first Match that touches the line; `-o` a row per line of a Match's text that is not empty (rg's `sink_slow_multi_line_only_matching`), while a `-U` search whose pattern cannot match a line break prints as a search of lines, as grep-printer's printers choose by `multi_line_with_matcher` (review r2, Codex #4); `--vimgrep` the first line of each Match, and in a search of several lines none for an empty Match at a line's start, as rg's printer of several lines (`sink_slow_multi_per_match`: review r3, Codex #4). A `--null-data` record prints a row per `\n` line of it with the record's number, its NUL left out, a record of one line too (review r4). A row's highlight is clipped to its text | Every row of the Trail layouts is one line of text, the invariant the layouts' widths, Fragments and Trails rest on; rg prints a block line by line (`sink_slow_multi_line`) and a record whole. The fuzz of C10 found a `\n` Match inside a record whose highlight ran past its row's text and panicked in the Fragments; the clip fixes it |
| D7 | PCRE2 through `grep-pcre2` =0.1.8, `pcre2` =0.2.9, `pcre2-sys` =0.2.9 (rg 14.1.1's lock), PCRE2's C built into the binary (`PCRE2_SYS_STATIC=1` in `.cargo/config.toml`), configured as rg's `matcher_pcre2` (JIT with a 10 MiB stack on 64 bits, `utf` and `ucp` unless `--no-unicode`). Each search thread matches with a clone of its own of the PCRE2 matcher (`matcher::Pcre2::with`) | rg's `pcre2` feature. pkg-config finds the system's libpcre2 (10.45 here) and pcre2-sys links it unless asked not to: the release binary must not depend on it, and the tests must search with one PCRE2. A `cargo install searchfor` does not read `.cargo/config.toml` and links the system's, as ripgrep's. The pcre2 crate keeps a PCRE2 matcher's scratch space in a pool behind a lock; twelve threads sharing one matcher took 1.55 × rg over the M4 corpus, where rg builds a matcher per thread; with a clone per thread, 1.02–1.05 × (the ratio s4 has with the default engine). A clone shares the compiled pattern |
| D8 | The walker takes each kind of ignore file on its own (`.ignore`/`.rgignore`, `.git/info/exclude`, git's global excludes, parent directories, `.gitignore`), `--ignore-file` (unless `--no-ignore-files`; an unreadable one is an ignore message), `--ignore-file-case-insensitive`, `--no-require-git`, `--one-file-system`, `--max-filesize` (walked files only, as rg), `--glob-case-insensitive` (every `-g` case-insensitive, then the `--iglob`s), `--type-add` and `--type-clear` in order with `-t` and `-T`. `--sort`/`--sortr` other than ascending path collect the walk and sort it stably, a file whose time cannot be read after the others (before them with `--sortr`); `--sort path` stays the walker's, siblings by name. The errors of an ignore file the walk reads (a glob it cannot parse) are printed, unless `--no-messages` or `--no-ignore-messages`, and do not change the exit code | rg 14.1.1's `walk_builder` and `sort` (hiargs.rs) and `Haystack::build`. s4 printed no error of a `.gitignore` since M1 (rg prints `rg: ./.gitignore: line 1: error parsing glob …`), a gap this plan closes |
| D9 | The searcher takes rg's line terminator (CRLF with `--crlf`, NUL with `--null-data`), `-U`, `--stop-on-nonmatch` and `--passthru` (which replaces `-A`/`-B`/`-C` before it); `--null-data` turns binary detection off, as rg's; `--mmap` maps every file searched, walked ones too, `--no-mmap` none, the `Source` of a file of 8 MiB or more included (`source::Mapping`; review r2, Codex #1); a document's reader maps its document in its own process, which a truncation can end without ending s4. The submatches the sinks record follow the searcher's terminator and, with `-U`, look 128 bytes past the block, as grep-printer's `find_iter_at_in_context`; a line's numbers count the search's terminator. With `--passthru` the Trail layouts print a file without a Match too | rg's `searcher` (hiargs.rs) and `BinaryDetection::from_low_args`. rg with `--passthru` prints every file it searches; its exit code says whether one matched. grep-searcher never memory-maps on macOS, which `rg_in_memory` must follow there: M5b's |
| D10 | `-E LABEL` decodes as grep-searcher decodes for rg: a BOM wins over the label, UTF-8 passes through as it is, any other label transcodes with U+FFFD for what does not decode, and the search runs through a reader as rg's; `-E none` sniffs no BOM and searches the bytes as they are; `-E auto` and `--no-encoding` are the default. The Source holds the decoded text, so Trails and rows read it | grep-searcher 0.1.14's `DecodeReaderBytesBuilder` settings and `slice_needs_transcoding` (a set encoding always transcodes, through a reader). A Latin-1 Markdown file gets its headings' Trails (`Café`) |
| D11 | `--colors` applies rg's specs over rg's defaults (grep-printer's `ColorSpecs`), and takes a type of s4's own, `trail`, with the same attributes (`trail:fg:yellow`) over the Trail's cyan (8381); s4's markers stay dimmed. With `--no-trail` a `trail:` spec is left out | rg rejects an unknown type, so an rg command line never has one; the Trail's colour was fixed since M1 and a user who changes the others wants that one too |
| D12 | `--hyperlink-format` in the Trail layouts: the one-line layout links each row's path, line, column and offset as one OSC 8 link (rg's no-heading prelude), the tree its header's path (line 1) and, when the format names `{line}` or `{column}`, each row's numbers; the URL as grep-printer builds it (canonical path, percent-encoded, `{host}` from `--hostname-bin` or the system, `{wslprefix}`); only with colour on a writer that takes links | grep-printer 0.2.2's `Interpolator` and `HyperlinkPath` are `pub(crate)`; `output::hyperlink` copies their rules, about 150 lines. rg prints no link without colour |
| D13 | stdout is flushed per line on a terminal and with `--line-buffered`, per block in a pipe and with `--block-buffered`: where one file's output is printed as it grows (`Ctx::spill`), each line goes out as it is written; `--files` writes through a block-buffered or line-buffered stream as rg's. A file whose Format its name does not decide, stdin among them, is still read whole in the Trail layouts before its rows print | rg's `stdout` (hiargs.rs). Before, s4 printed one file's output every 64 KiB or at its end, on a terminal too; `tail -f log | s4 --no-trail error` now prints each line as rg. The Trail pipeline's model of stdin (read to detect its Format) is M5b's streaming work |
| D14 | `--stats`: with `--no-trail` and the summaries, the sum of grep-printer's statistics of every file, as rg adds them; in the Trail layouts every file goes through the model's path, which counts every search (matched or not) as `--json -q` already counted them, the bytes printed being s4's own. `-q --stats` searches every file. With `--json` the summary message, as rg | rg's `print_stats` (main.rs) and `quit_after_match` (`-q` stops at the first Match unless `--stats`). The JSON summary of `--no-trail --json` counted only the files with a Match; grep-printer's JSON sink counts each file it printed a `begin` for, which `--passthru` makes every file (measured `searches 1` for a file without a Match): fixed with it |
| D15 | `--debug` and `--trace` install a `log` logger that prints in rg's format, `s4: LEVEL|target|file:line: message`; the crates s4 shares with rg log through `log`, so their messages (why a file is ignored, the globs built) read as rg's | rg's `logger.rs`. s4's own messages differ from rg's; a test checks the format and that nothing is logged without the flags |
| D16 | `--generate man|complete-bash|complete-zsh|complete-fish|complete-powershell` writes s4's man page (clap_mangen) and completions (clap_complete) from s4's own clap command | rg generates them from its flag definitions; s4's flags are clap's, so its man page and completions list s4's own flags and help too. Two small crates (`roff` with them) |
| D17 | `s4 --mcp` (a flag: Sebastián, 8837): JSON-RPC 2.0, a message per line on stdin and stdout; protocol versions 2025-06-18, 2025-03-26 and 2024-11-05 (the client's when it is one, else the newest); `initialize`, `ping`, `tools/list`, `tools/call`, notifications and responses ignored; a message JSON-RPC 2.0 does not allow (not an object, a batch, no `"jsonrpc": "2.0"`, an `id` that is not a string or a number, a `method` that is not a string) is -32600, a line that is not JSON or not UTF-8 -32700, a request over 1 MiB -32600, and the server goes on. One tool, `search` (spec §9's arguments, each of its schema's type and nothing else; `max_results` 100 by default, at most 1 000), which runs `s4 --no-config --json --color never [flags] --regexp=PATTERN -- [paths]` as a child of the same binary, with neither `SEARCHFOR_CONFIG_PATH` nor `RIPGREP_CONFIG_PATH` in its environment and every value joined to its flag (`--within=…`, `--glob=…`), so no configuration file and nothing the client sends is read as a flag: no `--pre` and no `-z` ever run (the tool has no argument for them). Its stdout is read a line at a time, at most 4 MiB a line (a longer `match` is left out and its file named in `errors`, `truncated_by: "long_line"`; at most 20 files named, `MAX_LONG_LINES_NAMED`, each paid for in both copies of `errors` out of the Matches' share, and the rest counted in one line: review r3, Codex #2, Fable #2), up to `max_results` Matches (`"max_results"`) and 256 KiB of result (`"bytes"`); at either the child is killed, `truncated: true`. No Match is left out for its size alone, which counts its escapes: the first, or one that would not fit in an empty answer, has its Section and then its text cut by halves until it fits (review r3, Fable #1). The 256 KiB bound the tool's result; the response's line adds the JSON-RPC envelope, 33 bytes, and the id as the client sent it, at most a request's 1 MiB (review r2, Codex #6). A request over 1 MiB gets -32600 with the id a member of its first MiB holds, else null (review r2, Fable #3). A line whose submatches lie under different Trails is a Match per Trail, in the order they come, each with the Section of its first submatch (review r2, Fable #2), the Trails read one at a time from the message and never copied (review r3, Codex #1). The answer is `{"matches": [{"path", "line", "trail": [names], "text", "format", "section"?}], "truncated", "errors"?}` as `structuredContent` and as text: `text` is the line, or s4's Fragment of it when the line is longer than the Fragment width (`omitted_before`, `omitted_after`), at most 2 KiB; a name at most 256 bytes, the 32 innermost names (`trail_omitted`); a Section at most 16 KiB, and once per answer (a later Match of it in the same file, told by the exact path s4 gave, has `same_as`, the index of the Match that holds it: review r2, Codex #5); `cut: true` on a Match something of which was cut; bytes that are not UTF-8 decoded with U+FFFD (`lossy: true`), and a path that is not UTF-8 also exact in base64 (`path_base64`). s4's stderr (at most 4 KiB) is `errors`; a child that exits 2 with no Match, exits with another code or ends by a signal is a tool result with `isError` and what s4 printed | The flag form because `s4 mcp` must search for `mcp` (§8); Sebastián chose it (8837, after the grill's #19 had said a subcommand). A child process keeps the server alive whatever a search does, and its output is exactly `s4 --json`'s for the flags of the call. The client's arguments are the whole search: a user's configuration file holds flags for the terminal, and its `--pre` ran on every call (review r1, Codex #1, Fable #1). The limits protect the agent's context: one minified line or one Section could fill it whatever `max_results` said (r1, Codex #2, Fable #2); the Fragment is what s4 already computes for `--json`. Exit 2 with Matches is a search with errors, not a failed one: the Matches and the errors both go to the model, and a signal is never a success (r1, Codex #3). The envelope's checks are JSON-RPC 2.0's (r1, Codex #6; Fable #8); the bytes of grep-printer's `{"bytes": base64}` are decoded, never shown as base64 text (r1, Codex #7). A protocol error (an unknown method, bad arguments) is JSON-RPC's (-32601, -32602, -32700). Review r2: an answer that leaves Matches out says which limit did, since the advice differs (raise `max_results`, narrow the search, or search a 5 MiB minified line from a shell: Fable #1); a model reading one Match per line would think a second cell had no Match (Fable #2); two paths that read alike with U+FFFD are two files (Codex #5). Review r3: a line of 240 Trails copied its message 240 times, 212 MiB of RSS for one Match (Codex #1); hundreds of files with a line past 4 MiB made `errors` overflow the 256 KiB (Codex #2, Fable #2); a Match too long for any answer coming second ended the answer as full (Fable #1) |
| D18 | Performance (`scripts/bench-m5.sh`, `tests/perf.rs`): over the M4 corpus, `--no-trail` with `-P`, `-U`, `--column -b -o`, `--vimgrep` and `-r --trim` at most 1.1 × rg; `--files` and `--files --sort path` at most 1.1 × rg over a generated tree of 100 000 files (over the corpus, 31 ms for rg, 3 ms were the 10 %: review r1, Fable #5); the one-line layout with `--column -b`, `-o`, `--vimgrep`, `-r`, `--trim`, `--stats` at most 1.1 × without them; with `--no-docs`, M5a against M4's s4 (`SEARCHFOR_BASE_BIN`, 522bf96), `--flat` and `--no-trail`, at most 1.03 ×, the median of three series. Perf tests: `-P` and `-U` over 10 MB within 1.25 × rg; `--files` over 20 000 files within 1.25 × rg + 10 ms; on 50 MB of Markdown each output flag within 1.25 × the default's time and its RSS + 16 MiB | The probe measured (12 threads, a machine with other sessions): M5a/M4 1.017 (0.984, 1.017, 1.023); `-P` 1.02–1.05 × rg after D7, `-U` 1.05; `--files` 1.02 (47.2 against 48.0 ms over 32 430 files), sorted 1.02; the output flags 0.98–1.01 × the default over the corpus and 1.00–1.02 on the 50 MB file. 1.1 leaves the room M4's ceilings left |
| D19 | The matcher's errors are rg's: `matcher::build` adds rg's suggestions (PCRE2 for look-around and backreferences with the default engine, `-U` for a literal line terminator, `-a` for a NUL), `--engine auto` reports both engines' errors under rg's heading; a pattern that can match a NUL is refused while binary data is detected (rg's `ban_byte`); a search of the current directory that read no file prints rg's `No files were searched …` (with `s4` for `ripgrep`) and exits 2 | M1–M4 had the suggestions in `app.rs` and neither the NUL ban (`s4 'a\x00b'` exited 1 where rg exits 2 with its message) nor the message of an empty search (`s4 -g '*.zzz' x` exited 1 silently, rg exits 2): gaps this plan closes |
| D20 | `--files`: every positional argument is a path (the pattern too), the current directory without one even with a readable stdin, a file listed when named or walked, printed by grep-printer's path printer; from the walker's threads through one printing thread, or in `--sort`'s order from one thread; `-q` stops at the first file. `--files`, `--type-list` and `--generate`: the last one given runs, as rg's `Mode::update`. `--type-list` prints the types after `--type-add` and `--type-clear`, each with its globs sorted, and exits 1 when none is left (review r2, Codex #3) | rg's `files` and `files_parallel` (main.rs) and `Paths::from_low_args`; rg's `types` prints `HiArgs::types()`, the built types. The first version walked with one thread: 6 × rg's time over 32 430 files |
| D21 | Usage errors stay clap's messages (exit 2, as rg), the deviation of `rg-compat.md` since M1 | Matching rg's message for each flag is a parser of its own, for stderr text only; the exit codes and the order of rg's validations (types, globs, pre-globs, patterns) hold since M4 |
| D22 | The release binary grows from 19.3 MB (19 298 032 bytes at `522bf96`, 17 859 024 stripped) to 21.0 MB (20 953 944 bytes, 19 402 656 stripped), under 23 MB (8612) | PCRE2 linked statically (`PCRE2_SYS_STATIC`, D7), clap_complete, clap_mangen and the new code; the probe did not measure how much each one adds |

---
## Verified for this plan (2026-10-18 and 2026-10-19)

Outputs in `/tmp/subtask/m5-plan/` (round 0), `/tmp/subtask/m5-plan-r1-fix/` (review round
1), `/tmp/subtask/m5-plan-r2-fix/` (review round 2), `/tmp/subtask/m5-plan-r3-fix/` (review
round 3) and `/tmp/subtask/m5-plan-r4-fix/` (review round 4). The probe `/tmp/subtask/m5-plan/m5v` is a clone of the work branch at `522bf96` with M5a's
code in commits named after the chunk each belongs to. Its history rebuilt in tanda order
(`/tmp/subtask/m5-plan/states/rebuild.sh`) was the frozen copy
`/tmp/subtask/m5-plan/m5tree-5c6226b`: branch `t0` is `522bf96`, one branch per chunk (`c1` …
`c11`) from the state of the tanda before it, and `t1` … `t5` the state after each tanda
(`t1` is `c1`; `t2` merges `c2`–`c6`; `t3` `c7` and `c8`; `t4` `c9`; `t5` `c10` and `c11`),
which added up to the probe (`git diff probe t5` was empty). Review round 1 fixed a copy of it:
each fix a commit on the branch of its chunk (`c1`, `c4`, `c6`, `c9`, `c10`, `c11`), merged
forward into every later chunk and tanda, so each chunk's branch is still its tanda's state plus
its own commits. That copy was frozen as `/tmp/subtask/m5-plan/m5tree-005ea08`; review round 2
fixed a copy of it the same way (commits on `c3`, `c4`, `c6`, `c9` and `c11`), frozen as
`/tmp/subtask/m5-plan/m5tree-4b029c4`; review round 3 fixed a copy of that one (commits on
`c4`, `c6`, `c9` and `c11`), frozen as `/tmp/subtask/m5-plan/m5tree-63003b3`; review round 4
fixed a copy of that one (commits on `c1`, `c4`, `c9` and `c11`), frozen as `/tmp/subtask/m5-plan/m5tree-7631b14` (`t5` is
`7631b14`), and every code block below was generated from it
(`/tmp/subtask/m5-plan-r4-fix/plan/gen.py`), each diff from the state of the tanda before its
chunk.

- `rustc 1.90.0`, `cargo 1.90.0`, 20 cores. `/usr/bin/rg` 14.1.1 is linked to the system's
  `libpcre2-8.so.0` (the package is 10.45) and prints `PCRE2 10.42 is available (JIT is
  available)`; s4 prints `PCRE2 10.43 is available (JIT is available)` and `ldd` shows no
  libpcre2: the C of `pcre2-sys` 0.2.9, linked statically (D7).
- **Dependencies.** The lock gains eight packages (`clap_complete`, `clap_mangen`, `grep-pcre2`,
  `jobserver`, `pcre2`, `pcre2-sys`, `pkg-config`, `roff`) and loses none; `cargo tree -i`
  shows `regex-automata` 0.4.7, `regex-syntax` 0.8.4, `memchr` 2.7.4 and `encoding_rs` 0.8.34
  only. tree-sitter stays at 0.24.7.
- **Each tanda and each chunk alone** (`/tmp/subtask/m5-plan/states/verify.sh`: `cargo build
  --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`,
  `SEARCHFOR_REQUIRE_REFERENCE=1 cargo test --workspace`; a chunk alone is its branch, on the
  state of the tanda before it):

| State | build | clippy | fmt | tests passed | ignored |
|---|---|---|---|---|---|
| `t1`, tanda 1 (C1) | ok | ok | ok | 784 | 50 |
| `c2`, C2 alone | ok | ok | ok | 790 | 50 |
| `c3`, C3 alone | ok | ok | ok | 792 | 50 |
| `c4`, C4 alone | ok | ok | ok | 808 | 50 |
| `c5`, C5 alone | ok | ok | ok | 788 | 50 |
| `c6`, C6 alone | ok | ok | ok | 798 | 50 |
| `t2`, tanda 2 | ok | ok | ok | 840 | 50 |
| `c7`, C7 alone | ok | ok | ok | 844 | 50 |
| `c8`, C8 alone | ok | ok | ok | 842 | 50 |
| `t3`, tanda 3 | ok | ok | ok | 846 | 50 |
| `c9`, C9 alone | ok | ok | ok | 876 | 50 |
| `t4`, tanda 4 (C9) | ok | ok | ok | 876 | 50 |
| `c10`, C10 alone | ok | ok | ok | 878 | 53 |
| `c11`, C11 alone | ok | ok | ok | 877 | 50 |
| `t5`, tanda 5, M5a | ok | ok | ok | 879 | 53 |

  The table is review round 4's (`/tmp/subtask/m5-plan-r4-fix/scripts/verify.sh`, the same
  steps under `nice -n 19 ionice -c3`, in a worktree and a target directory of its own);
  round 0's gave 783 for `t1` and 854 for `t5`, round 1's 784 and 866, round 2's 784 and 874,
  round 3's 784 and 878:
  round 1 adds twelve tests (one in C1, two in C4, six in `mcp::tests`, two in `cli_m5.rs`,
  `benchmarks_run_isolated` in C10), round 2 eight (one in C3, two in C4, three in `mcp::tests`,
  `m5_19` and `m5_20` in C9), round 3 four (two in C4, `many_long_lines_keep_the_answer_bounded`
  in `mcp::tests`, `m5_21` in C9), round 4 one (`m5_22` in C9; in C4,
  `the_replacements_of_a_line_in_one_pass` takes the place of round 3's test of `replace_each`);
  the other checks of rounds 2, 3 and 4 extend existing tests.
  With `SEARCHFOR_REQUIRE_REFERENCE=1` no comparison with rg skipped.
- **Two chunks alone that failed, and their fixes.** The first run of the script found `c3` and `c4`
  red alone: C1's matcher set the line terminator to `\n` always, so the searcher C3 builds
  with `--null-data` or `--crlf` refused it (`mismatched line terminators`), and C4's test of a
  `-U` replacement could not compile `\n`. C1's `build` now sets the terminator as rg's
  `matcher_rust` does (Task 1.4), and C2 keeps it. The second run found `c3` red on one case
  of `submatches_follow_the_searchers_terminator`, a `-P` look-ahead, which C1's matcher cannot
  compile: the case moves to C9, after C2 is merged.
- **Against rg.** `tests/rg_compat_m5.rs` compares about 120 command lines (its `vec!` literals) of M5a's
  flags with `--no-trail` against `/usr/bin/rg`, byte for byte: all equal. Review round 3 ran
  Codex's differential of `-U` with `-o`, `-r` and `--vimgrep` in `--flat` (six texts, twelve
  patterns, seven flag sets, both engines: 924 command lines, `scripts/diff-U.py` of the round's
  outputs) against `rg -n`: no difference; and `-P -o -r X 'GLIBC_\K2\.\d+'` over an ELF of
  the corpus prints three `X` as `rg -a`, where round 2's binary printed the versions. Review
  round 4 compared `--flat -a -P -o -r '<$1|$2>'` against `rg -a -o` over three gcc ELFs (the
  strings `\0A\0B\0`, the same with line breaks between them, and 1 000 strings `\0B`), seven
  patterns with a group before `\K` or a look-behind, with and without `-U`
  (`scripts/objdiff.py`): 42 command lines, no difference; round 3's binary differed on 12. The
  differences the probe found
  on the way are in C9's code (the JSON summary of `--passthru`, hyperlinks with `--no-trail`
  through `Spill`, `--line-buffered`).
- **Performance** (12 threads of 20, a machine with other sessions; the corpus of
  `gen-m4-corpus.py`): M5a's s4 against M4's (`522bf96`) with `--no-docs`, the sparse pattern,
  three alternated series: 0.984, 1.017, 1.023, median 1.017 (ceiling 1.03). `-P` with
  `--no-trail`: 1.55 × rg with one shared PCRE2 matcher, 1.02–1.05 × with a clone per thread
  (D7). `-U`: 1.05 × rg. `--files` over 32 430 files: 6 × rg walking sequentially, 1.02 × (47.2
  against 48.0 ms) from the walker's threads (D20). The output flags in the Trail layouts:
  0.98–1.01 × the default over the corpus. `tests/perf.rs`' three new tests pass: `--files`
  21.6 ms against rg's 18.8; the output flags 1.00–1.02 × the default on 50 MB; `-P` 55.3
  against 53.7 ms and `-U` 47.5 against 45.4 ms on 10 MB. `scripts/bench-m5.sh` over the final probe
  (2026-10-19, after the suite, the machine otherwise quiet): exit 0; `--no-trail` with `-P`, `-U`, `--column -b -o`,
  `--vimgrep` and `-r X --trim` 1.03–1.04 × rg; `--files` 1.10 × rg (s4 34 ms, rg 31, at the
  ceiling: the 32 430-file tree above gave 1.02) and sorted 1.02; the output flags in `--flat`
  0.97–1.03 × without them; M5a against M4, medians of three series, 1.023 with `--flat` and
  1.025 with `--no-trail` (ceiling 1.03). Review round 1's `bench-m5.sh` (`--files` over the
  generated 100 000 files; load average 7–10, other sessions running), three runs: `--files`
  0.96, 0.99 and 1.00 × rg (41–43 ms), sorted 0.99–1.01; `--no-trail` with M5a's flags 0.98–1.07
  × rg; the output flags in `--flat` 0.97–1.04; M5a against M4 0.993, 1.017 and 1.028 with
  `--flat` and 1.035, 1.020 and 1.026 with `--no-trail`: the first run exited 1 on that 1.035,
  the next two exited 0. Round 1's binary against round 0's over the same corpus, 40 runs each:
  `--no-trail` 47.9 against 48.6 ms, `--flat` 50.1 against 50.2 ms, so round 1 adds no cost to a
  search. Review round 2's `bench-m5.sh`, three runs under `nice -n 19 ionice -c3` (load average 7–12,
  other sessions running; `bench-m5*.log` in `/tmp/subtask/m5-plan-r2-fix/`): M5a against M4
  1.032, 1.023 and 1.025 with `--flat`, 1.025, 1.027 and 1.024 with `--no-trail`; `--files`
  0.95, 1.01 and 0.94 × rg, sorted 1.09, 1.03 and 1.04; `--no-trail` with M5a's flags 1.02–1.10
  × rg but one 1.13 (`-r X --trim`, third run); the output flags in `--flat` 0.96–1.02. The first
  run exited 1 on the 1.032 and the third on the 1.13, each a gate over its ceiling once in three
  runs, which Task 10.3 does not count as a finding. Side by side, 40 runs each: round 2's binary
  against round 1's, `--flat` 49.0 against 51.0 ms and `--no-trail` 47.1 against 47.5; with
  `-r X --trim --no-trail` rg 46.6, round 1 47.1 and round 2 47.5 ms: round 2 adds no cost to a
  search. Review round 3, side by side under `nice -n 19` (load average 4–6, `outputs/bench-side2.txt`),
  20 runs each, round 2's binary against round 3's: `--flat --binary -r X 'e[a-z]{2}'` over the
  corpus's 300 object files 322.6 ± 11.3 against 335.1 ± 15.3 ms, the same output; `--flat -U
  --vimgrep 'e[a-z]{2}'` over its text files 103.1 ± 5.6 against 106.1 ± 7.6 ms. Round 3 did not
  rerun `bench-m5.sh`: its changes are on paths the sparse pattern does not reach. Review round
  4, under `nice -n 19` (load average 2.9–4.1, `outputs/bench-r4.txt`), round 3's binary
  against round 4's, the same output: `--flat --binary -r X 'e[a-z]{2}'` over the corpus's 300
  object files, 15 runs, 333.4 ± 11.4 against 331.9 ± 8.9 ms; over Fable's ELF of 20 000
  strings `needle NNNNN` on one line of 260 KB, 10 runs, `-P -r X 'needle \K\d+'` 381.4 ± 7.7
  against 68.0 ± 1.8 ms and `-r X needle` 138.7 ± 2.8 against 62.4 ± 2.6 ms. Round 4 did not
  rerun `bench-m5.sh` either.
- **M1–M4's benchmarks** with M5a's binary: `bench.sh` exit 0 (1.05 × rg with Trails, 1.04
  with `--no-trail`); `bench-m3.sh` exit 0; `bench-code.sh` exit 1, its `--no-trail` check
  printing 669 lines against rg's 667 over `~/robotin/apps`: two documents there (a `.docx` and
  a `.pdf` of another project) match `TODO`, and M4's binary prints the same 669; `bench-m4.sh`
  exit 1 twice, `noext-passthru` at 1.05 and 1.06 (gate 1.05) and `noext-L-passthru` at 1.05
  once, where M4's binary passed in the run beside; 40 runs of each binary side by side gave
  the same pair 1.05 ± 0.05 (M4) and 1.06 ± 0.06 (M5a). C10, Step 3 says how to read both.
  Those runs were round 0's, before Task 10.3's isolation; they read no configuration file
  because none existed on the machine (neither variable set, no `~/.config/searchfor` nor
  `~/.local/share/searchfor`), checked in round 1. Round 1 did not rerun them.
- **Binary.** `target/release/s4` of `522bf96`: 19 298 032 bytes (17 859 024 stripped); of the
  probe: 20 953 944 bytes (19 402 656 stripped), 1.66 MB more, under 23 MB (8612); after review
  round 1, 21 002 720 bytes; after review round 2, 21 031 896 bytes; after review round 3,
  21 037 240 bytes; after review round 4, 21 033 312 bytes.
- **Fuzz.** `tests/fuzz_m5.rs` with its default count and with `SEARCHFOR_FUZZ_N=2000` at seeds
  12345 (199 s) and 777 (194 s): PASS. An earlier run found a panic (`range end index 716 out
  of range` in the Fragments) with `-a --null-data -e '\n'` over a long Markdown file: a Match
  of the record's line break crossed the end of its row. C4 clips each row's highlights to its
  text (`clip_line`, the clamp in `write_line_text`) and keeps the case as
  `a_match_of_a_records_line_break_stays_in_its_row`.
- **MCP.** `claude -p` (Claude Code) with a temporary `--mcp-config` naming the probe's `s4
  --mcp`, from `tests/fixtures`, asked to find `needle-pkg` under `md` with the `search` tool,
  answered `md/guide.md`, line 12, `Deploy guide › Install › Linux › bash` (Task 9.4, Step 4);
  with round 1's server, the same three facts (`md/guide.md, línea 12, trail: Deploy guide ›
  Install › Linux › bash`); with round 2's, asked also for `needle-[ab]` in `md/guide.md`, the
  same three facts and both Trails of line 24 (`Deploy guide › Install › Plans › Plan` and
  `… › Plans › Price`). Round 3, the release servers of rounds 2 and 3 side by side
  (`scripts/mcp-mem.py`, `mcp-long.py`): `max_results: 1` over an XML line of 10, 100, 240 and
  1 000 Trails took round 2's to 12 680, 43 240, 216 844 and 3 634 892 KiB of peak RSS (7.1 s for
  the last) and round 3's to 13 028 KiB each (0.03 s at most); thirty files of a 4 MiB line and a
  small one: round 2's `errors` named all thirty, round 3's twenty and `10 more Matches were
  left out`, a 6 302-byte result with the small file's Match. Round 3 did not repeat the
  `claude -p` check.
- **Documentation.** The new examples of `README.md`, `README.es.md` and the skill, run from
  `tests/fixtures`, print what the files show; the skill's `-o` example had lost its `…`, fixed.

---
## Review round 1

`reviews/m5-plan-r1/{codex,fable}.md`, over `f793587`: Codex 5 MAJOR and 3 MINOR, Fable 2 MAJOR,
4 MINOR and 2 NIT, no BLOCKER; every finding fixed. Each fix was written test first in a copy
of the frozen tree (`/tmp/subtask/m5-plan/m5tree-5c6226b`) and seen RED over round 0's code
(`mcp::tests`, whose `Searcher` changed, does not compile over it: each of its new checks was
seen failing with its fix undone instead), then GREEN, as a commit on the branch of its chunk,
merged forward into every later state; the copy is frozen again as
`/tmp/subtask/m5-plan/m5tree-005ea08`, which review round 2 fixed in turn.
No chunk or tanda was added or moved and no dependency changed; C10 gains M1–M4's four
benchmark scripts. Sebastián's decision of 19/10 (8837) settled the server as the flag
`s4 --mcp`, and that question left the plan.

| Finding | What changed | Where | RED over round 0 |
|---|---|---|---|
| Codex #1, Fable #1 (MAJOR): the server's searches read `SEARCHFOR_CONFIG_PATH`, so a configured `--pre` ran on every call and a configured path was searched; a configured `--generate` replaced the server | The child gets `--no-config` first and neither configuration variable in its environment (`mcp::command`); `--mcp` reads no file (`cli::parse_with_config`); D3, D17, spec §8, §9 | C1, C6, C9 | `the_configuration_file_is_not_read_for_mcp`; `m5_11`, whose first search found nothing: the configured `--pre` had replaced the file's text; `the_search_runs_without_configuration` |
| Codex #2, Fable #2 (MAJOR): no limit of bytes: a long line, a Section repeated per Match, an unbounded `max_results`, stderr and the read of a line | `max_results` at most 1 000; each stdout line read into at most 4 MiB (`read_line_capped`), a longer `match` left out; `text` the Fragment s4 computes, at most 2 KiB; a Section at most 16 KiB and once per answer (`same_as`); names at most 256 bytes, the 32 innermost; the answer at most 256 KiB, the child killed where it fills (`truncated`); stderr at most 4 KiB; D17, spec §9 | C6, C9 | `long_texts_come_cut`, `the_answer_has_a_size_limit` (Fable measured a 6 006-character line returned whole, and 24 494 bytes for one Match with `expand`) |
| Codex #3 (MAJOR): exit 2 with Matches was a success with `truncated: false`, and a signal was a success | Every end: 0, 1 and the server's own kill are answers; 2 with Matches is an answer with s4's messages as `errors`; 2 without Matches, another code or a signal is a tool error with s4's message | C6, C9 | `how_a_search_ends`; `m5_11` with `["md/guide.md", "missing"]` |
| Codex #4 (MAJOR): an object file's string skipped `-o`, `--vimgrep` and `-r` | `match_rows` takes the string through the paths of a line, as one line, its rows at the hex offset of their Match and under its Trail; spec §5.7 | C4, C9 | `object_rows_take_o_vimgrep_and_r`; `m5_17` over ELF, PE and Mach-O |
| Codex #5 (MAJOR): M1–M4's benchmarks read the user's configuration and, in `bench.sh`, the user's Definitions and Grammars | Each script sets its own Definitions, Grammars and index and unsets both configuration variables before its first `hyperfine`; Task 10.3, Step 1b and Step 3 | C10 | `benchmarks_run_isolated` over the four scripts of round 0 |
| Codex #6 (MINOR), Fable #8 (NIT): the JSON-RPC envelope was not checked; `expand: "true"` was false; a batch got no answer; a line that is not UTF-8 ended the server; `100.0` was refused | -32600 for `{}`, a batch, a non-object, no `"jsonrpc": "2.0"`, an `id` neither string nor number, a `method` that is not a string, `params` that are not an object; -32700 for a line that is not JSON or not UTF-8, which no longer ends the server; every argument of its schema's type (null too is refused), `max_results` an integer, `100.0` included; a request over 1 MiB refused | C6 | `invalid_requests_are_answered`, `search_arguments_become_flags` |
| Codex #7 (MINOR): `{"bytes": base64}` reached the client as its base64 | `compact` decodes it with U+FFFD (`lossy`), and a path that is not UTF-8 also comes exact as `path_base64`; the line's exact bytes are the file's at `path` and `line` | C6, C9 | `bytes_are_decoded`; `m5_11` with `needle\xff` |
| Codex #8 (MINOR): `--trim` kept a leading `\r` and `\x0B` that rg trims | `trimmed` is grep-printer's `trim_ascii_prefix`: tab, line feed, vertical tab, form feed, carriage return and space, less the terminator's bytes; spec §8 | C4, C9 | `trim_leaves_out_greps_white_space_but_the_terminator`; `m5_18` |
| Fable #3 (MINOR): eleven negations in no test | Each after its flag in `rg_compat_m5.rs` (`--no-ignore --ignore-dot`, `-b --no-byte-offset`, `--no-pcre2-unicode --pcre2-unicode -P`…), equal to rg | C9 | The eleven names were absent from every test (no RED: each already matched rg) |
| Fable #4 (MINOR): `cargo test --lib A B` fails with cargo's usage | The filters after `--` in Tasks 1.2, 1.3, 3.1, 3.4 and 7.1 | plan | — |
| Fable #5 (MINOR): the `--files` gate sat at 1.10 for 3 ms | `bench-m5.sh` times `--files` over a generated tree of 100 000 files; Task 10.3 says three runs over a ceiling are a finding for every ratio; D18, spec §11 #32 | C10 | See «Verified for this plan» |
| Fable #6 (MINOR): what the subcommand would change was not said | Settled by Sebastián (8837): the flag; the question left the plan | plan, spec §9 | — |
| Fable #7 (NIT): Task 10.2 cited §11 #31 for the fuzz | #33 | plan | — |
| A `within` that starts with `-` (found while fixing Codex #1) | `--within -x` was clap's usage error, exit 2 (`-g -x` and `-e -x` already took their value): every value is joined to its flag (`--within=`, `--glob=`, `--regexp=`) | C6, C9 | `search_arguments_become_flags`; `m5_11` with `"within": "-x"` |

---
## Review round 2

`reviews/m5-plan-r2/{codex,fable}.md`, over `224a92d`: Codex 2 MAJOR and 4 MINOR, Fable 2 MINOR
and 1 NIT, no BLOCKER; every finding fixed. Each fix was written test first in a copy of round
1's frozen tree (`/tmp/subtask/m5-plan/m5tree-005ea08`), seen RED, then GREEN, as a commit on the
branch of its chunk (`c3`, `c4`, `c6`, `c9`, `c11`), merged forward into every later state; the
copy was frozen again as `/tmp/subtask/m5-plan/m5tree-4b029c4` («Verified for this plan»). No chunk or tanda was added or
moved and no dependency changed; C4 gains one function in M4's `src/processors/object/rows.rs`,
which no other chunk touches.

| Finding | What changed | Where | RED |
|---|---|---|---|
| Codex #1 (MAJOR): `--no-mmap` turned the searcher's maps off, but `Source::open_as` still mapped every file of 8 MiB or more | `source::Mapping` (`Large`, `Never`), taken by `open_as` and set from `--no-mmap` in `Ctx` for both places that open a file by name; D9, spec §8. A document's reader still maps its document in its own process (M4's `docs::isolated`), which a truncation can end without ending s4: said in D9 and rg-compat | C3, C9, C11 | `no_mmap_maps_no_file` (does not compile over round 1); `m5_19_no_mmap_maps_no_file` reads `/proc/PID/maps` of s4 while it waits on a full pipe: with the fix undone, `--no-mmap` maps `big.md` |
| Codex #2 (MAJOR): an object file's `-r` ran the regex again over the printable string: `^needle\|target` replaced `needle` too (`X X`), and `ab\x01cd` was not replaced (`ab.cd`) | `sunk` replaces each Match of an object file's string where the search found it in the file's bytes (`Replacer::replace_each`: the Match found again at its own offset, its anchors and groups reading the bytes up to its line's end), then makes the string printable (`object::rows::shown` over each replacement); D4, spec §5.7, rg-compat | C4, C9, C11 | `object_rows_replace_over_the_files_bytes` (`X X` over round 1); `m5_17` over ELF, PE and Mach-O with `^`, a control byte in the Match and in a group |
| Codex #3 (MINOR): `--type-list` printed rg's catalogue whatever `--type-add` and `--type-clear` said | `search::file_types`, the walker's types, which `--type-list` prints too (exit 1 with none, as rg's `types`); D20 | C9 | Three cases of `m5_walk_flags_equal_ripgrep` against rg (`--type-add 'review:*.review'`, `--type-clear rust`, a merged and an `include:` type) |
| Codex #4 (MINOR): `-o` dropped a Match whose text or replacement was empty, and the `\r` of a CRLF line without `--crlf` | `-o` prints a row for every Match, an empty one as an empty row, unless a `-U` search left it empty, as rg's printers of one line and of several; rows clip at the active terminator only (`cut_of` in `match_rows` and `clip_line`). Found on the way: rg prints a `-U` search whose pattern cannot match a line break with its printer of one line (`multi_line_with_matcher`), so `-U -o 'x*'` printed one row where rg prints eleven; `output::rg::look` takes the same choice. D4, D6, spec §8, rg-compat | C4, C9, C11 | `only_matching_rows_as_ripgrep`; `m5_20` against rg (`-o -r '' needle`, `-o -r '' e`, `-o 'x*'`, `-o '\r'`, and with `-U`); `the_look_of_the_command_line` |
| Codex #5 (MINOR): `same_as` keyed a Section by the path decoded with U+FFFD, so `a\xfe.md` pointed at `a\xff.md`'s Section | `SectionKey` holds the path's JSON as s4 wrote it (`{"bytes": …}`); D17, spec §9 | C6 | `sections_of_files_whose_names_read_alike` |
| Codex #6 (MINOR): the 256 KiB excluded the id without saying so (a 300 000-character id made a 303 082-byte line) | The limit is the result's, both copies; the line adds `ENVELOPE_BYTES` (33) and the id as sent, at most a request's 1 MiB; D17, spec §9 | C6 | None: round 1 already bounded the result so; `the_answer_has_a_size_limit` now pins it with a 300 000-character id |
| Fable #1 (MINOR): a Match past 4 MiB, or a first Match whose escaped Section did not fit, left `{"matches":[],"truncated":true}` and advice that did not help | `truncated_by` (`max_results`, `bytes`, `long_line`) in the answer, its schema and the tool's description; a `match` past `MAX_MESSAGE_BYTES` named in `errors` by the path its first bytes hold (`path_of_prefix`); the first Match cut by halves until it fits (`shrink`); D17, spec §9, README, README.es, skill | C6, C9, C11 | `left_out_matches_say_why`; `m5_11` with a 5 MiB line |
| Fable #2 (MINOR): a line with Matches under two Trails kept only the first Trail | `by_trail`: a Match per distinct Trail of the line's submatches, each with the Section of its first submatch; D17, spec §9 | C6, C9 | `a_match_per_trail_of_the_line`; `m5_11` with `needle-[ab]` on line 24 of `md/guide.md` (`Plan`, `Price`) |
| Fable #3 (NIT): a request over 1 MiB was answered with `id: null` | `id_of_prefix` reads the object's members before the cut, so an `"id"` inside `params` is never taken; null when the id comes after the cut | C6 | `invalid_requests_are_answered` with both orders |

---
## Review round 3

`reviews/m5-plan-r3/{codex,fable}.md`, over `94924fa`: Codex 2 MAJOR and 2 MINOR, Fable 2 MINOR
and 1 NIT, no BLOCKER; every finding fixed. Each fix was written test first in a copy of round
2's frozen tree (`/tmp/subtask/m5-plan/m5tree-4b029c4`), seen RED, then GREEN, as a commit on the
branch of its chunk (`c4`, `c6`, `c9`, `c11`), merged forward into every later state; the copy
is frozen again as `/tmp/subtask/m5-plan/m5tree-63003b3` («Verified for this plan»). No chunk or tanda was added or moved,
no dependency changed and no file changed chunk. Codex's #1 and #2 and Fable's #1 and #2 were
fixed together in `mcp::run`.

| Finding | What changed | Where | RED |
|---|---|---|---|
| Codex #1 (MAJOR): `by_trail` copied the whole message, its submatches included, once per Trail before `max_results` or the bytes were looked at: an XML line of 240 Trails took the server to 216 676 KiB of RSS for one Match | `by_trail` yields each Trail and its Section borrowed from the message, one at a time, and `compact` reads the line from the message with them, so only the Matches the answer takes are built; D17 | C6, C9 | `a_match_per_trail_of_the_line` with 240 Trails and `max_results: 1` (does not compile over round 2: `by_trail` was a `Vec`); `m5_21` reads the server's `VmHWM` after a call on a 240-element XML line: 230 640 kB over round 2, under the test's 64 MiB after |
| Codex #2 (MAJOR) and Fable #2 (MINOR): the notices of `match` messages past 4 MiB had no bound and were paid for by no limit, twice in the result | `errors` names at most `MAX_LONG_LINES_NAMED` (20) of them, each named one paid for in both copies (`message_cost`) out of the Matches' share and its path cut to `MAX_NAME_BYTES`; the rest are counted in one line, whose bytes, with s4's longest stderr escaped in both copies, `RESERVED_BYTES` holds; the tool's description, D17, spec §9, README, README.es and the skill say so | C6, C11 | `many_long_lines_keep_the_answer_bounded`: forty lines past 4 MiB whose names are all control characters, from a searcher that makes its output as it is read (`Repeated`), then Matches, with a stderr of 4 096 control characters: a result of 391 032 bytes over round 2 |
| Codex #3 (MINOR): an object file's `-r` searched again at the Match's own start, so a `-P` Match after `\K` was not found and kept its bytes (`-P -o -r X 'ab\Kcd'` printed `cd`; `rg -a` prints `X`) | `replace_each` iterates the Matches of a string from the string's start in the file's bytes and takes the one at each hit's range with its groups; a hit not found there is looked for again from its line's start, where the search and grep-printer's `Replacer` start. A first version started every string at its line's start: on the 300 object files of the corpus, `--flat --binary -r X 'e[a-z]{2}'` took 7.85 s against round 2's 0.36 s, a line of an object file being as long as its bytes go without a `\n`; this one 335 ms against 323 (20 runs each, same output); D4, spec §5.7, rg-compat | C4, C9, C11 | `m5_17` over ELF, PE and Mach-O with `-P -r X 'needle \Ktarget'`, `-r '<$1>' '(nee)dle \Ktarget'` and with `-o`: `needle target` over round 2. C4 has no PCRE2 (C2 brings it), so its unit test `each_match_is_found_again_from_its_strings_start` covers the strings, the groups and the look from the line's start (`^x`) with Rust's regex |
| Codex #4 (MINOR): `-U --vimgrep '^'` printed a row per line where rg prints none | A `--vimgrep` row is the first line a Match touches; in a search of several lines an empty Match at a line's start touches none, as grep-printer's `sink_slow_multi_per_match` skips it; D6, spec §8, rg-compat | C4, C9, C11 | `per_match_rows_of_a_multi_line_search`; `m5_20` against rg with `-U --vimgrep` and `^`, `^\|e` and `-P 'e?'`: two rows over round 2 where rg prints none |
| Fable #1 (MINOR): only the first Match was cut until it fit, so a later Match too long for any answer ended it as full (`truncated_by: "bytes"`) and the files after it went unsearched | Every Match that would not fit in an empty answer is cut until it fits (`ROOM_BYTES`); the first is cut to what is left; then the check against the room left, which still ends a full answer with `bytes`; D17, spec §9 | C6 | `left_out_matches_say_why` with `guide()` and then the Match of a Section of control characters: one Match over round 2, two after, the second `cut` |
| Fable #3 (NIT): criterion 5 counted ten tests of `mcp::tests` | Criteria 4 and 5 count the tests there are now: twenty-one in `tests/cli_m5.rs` (criterion 4 said eighteen), fourteen in `mcp::tests` | Acceptance | None |

---
## Review round 4

`reviews/m5-plan-r4/{codex,fable}.md`, over `293c885`: Codex 1 MAJOR and 1 MINOR, Fable 1
MINOR, no BLOCKER; every finding fixed. Each fix was written test first in a copy of round 3's
frozen tree (`/tmp/subtask/m5-plan/m5tree-63003b3`), seen RED, then GREEN, as a commit on the
branch of its chunk (`c1`, `c4`, `c9`, `c11`), merged forward into every later state; the copy
is frozen again as `/tmp/subtask/m5-plan/m5tree-7631b14` («Verified for this plan»). No chunk or tanda was added or moved,
and no dependency changed. C1 gains two files, `src/model.rs` and `src/filter.rs`, which no
other chunk touches: the field `Described::replacements` and a test's literal. Codex's #1 and
#2 have one fix: the search of an object file makes its replacements.

| Finding | What changed | Where | RED |
|---|---|---|---|
| Codex #1 (MAJOR): `replace_each` looked each string of an object file up again, and from its line's start when not found from its own: a line of many strings was read once per string, `-P -r X '\x00\KB'` over 1 000, 2 000 and 4 000 NUL-separated strings `B` took 82, 245 and 826 ms | The search of an object file (`App::object`) makes the replacement of each Match in one pass over each line (with `-U`, each block) it reported, as grep-printer's `Replacer` makes it (`Replacer::replacements`), and keeps them in `Described::replacements`, by the Match's range in the file; `sunk` takes each Match's from there, and a Match without one keeps its bytes. `replace_each` and its look-ups are gone. Over 1 000, 2 000, 4 000 and 8 000 strings: 89, 215, 770 and 2 515 ms before, 15, 18, 25 and 39 ms after (best of three, release, `outputs/scale-*.txt`); D4, spec §5.7, rg-compat | C1, C4, C9, C11 | `m5_22`: 4 000 strings in less than four times the time of 1 000, debug build: 244 ms and 2.50 s over round 3. C4's `the_replacements_of_a_line_in_one_pass` (does not compile over round 3) |
| Codex #2 (MINOR): a Match found again from its string's start, at the same range, kept the groups of another alternative: `-P -o -r '<$1\|$2>' '(A)\x00\KB\|(B)'` printed `<\|B>` where `rg -a` prints `<A\|>` | The same fix: each Match prints the replacement the search's own pass made, with the groups it found | C4, C9, C11 | `m5_17` with `(needle)\x00\Ktarget\|(target)` over ELF, PE and Mach-O, and against `rg -a`: `<\|target>` over round 3; `object_rows_replace_over_the_files_bytes` with a replacement given in `Described::replacements`: `needle <t\|>` over round 3's `sunk`, which made it again |
| Fable #1 (MINOR): with `--crlf` rg ends each line with `\r\n`, `--flat` and `--tree` with `\n`, and no deviation said so | Documented, as Fable proposed: the Trail layouts end every row with `\n`, with `--crlf` and `--null-data` too, and `--no-trail` prints rg's terminator; D4, spec §8, rg-compat | C9, C11 | `m5_deviations_are_known` looks for «end every row with `\n`»: FAILS before the paragraph. `m5_20` compares `--crlf` (with `-o` and `-U --vimgrep`) and `--null-data` rows against rg's, rg's terminator turned into `\n` |
| Found checking Fable #1: a `--null-data` record of one line printed its NUL (`1: a needle\0`), against D6 | `match_rows` prints a row of one line from that line's range, which leaves the NUL out, as it already did for a record of several lines | C4, C9 | `null_data_records` with `a needle\0`: `a needle\0` over round 3; `m5_20`'s `--null-data` case: `1:a needle\0` over round 3 |

## M5a acceptance criteria

M5a is done when all of these hold; each names the test or script that measures it.

1. `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo
   fmt --check` and `SEARCHFOR_REQUIRE_REFERENCE=1 cargo test --workspace` pass, every M1–M4
   test included (`t11` of `tests/cli.rs` rewritten, C1).
2. Every flag: `docs/rg-compat.md` says `yes` for the 146 names, and `every_ripgrep_flag_is_implemented`,
   `table_covers_reference_rg` and `table_has_nothing_reference_rg_lacks` pass (C1).
3. Spec §11 #29: the five tests of `tests/rg_compat_m5.rs` pass, run as
   `SEARCHFOR_REQUIRE_REFERENCE=1 cargo test --test rg_compat_m5` so they cannot skip.
4. Spec §11 #30 and #31: the twenty-two tests of `tests/cli_m5.rs` pass.
5. Spec §11 #14: `m5_11`, `m5_21` and the fourteen tests of `mcp::tests` pass, and the manual
   check of Task 9.4, Step 4 (a real Claude Code client calling `search` through a temporary
   MCP configuration) is written in the chunk report with its output.
6. Spec §11 #32: `SEARCHFOR_BASE_BIN=<s4 of 522bf96> scripts/bench-m5.sh` exits 0; and
   `SEARCHFOR_REQUIRE_REFERENCE=1 cargo test --release --test perf -- --ignored pcre2_and
   files_lists output_flags` passes. A run that printed `skipping:` does not count.
   `scripts/bench.sh`, `bench-code.sh`, `bench-m3.sh` and `bench-m4.sh`, isolated from the
   configuration files (`benchmarks_run_isolated`), exit 0, or each failure is shown to be M4's
   by M4's binary beside it (C10, Task 10.3, Step 3).
7. Spec §11 #33: `tests/fuzz_m5.rs` passes with its default count and once with
   `SEARCHFOR_FUZZ_N=2000`, the seed printed in the chunk report.
8. `Cargo.lock` gains only the packages listed in Task 1.1; `cargo tree -i regex-automata`,
   `-i regex-syntax`, `-i memchr` and `-i encoding_rs` show only 0.4.7, 0.8.4, 2.7.4 and 0.8.34.
9. Spec §11 #22: `target/release/s4` is at most 23 MB (measured in C10 and reported; 21.0 MB in
   the probe).
10. Nothing is published: no `cargo publish`, no tag, no GitHub release, no push of the branch
    (the orchestrator pushes once Sebastián says so).

## Design notes the chunks share

1. **Every flag is parsed in C1.** C1 adds the 90 flags to `cli::Args`, resolves the ones that
   depend on their place (`cli::order`, D2), reads the configuration file (`cli::config`, D3)
   and marks every name of `rg_flags::RG_FLAGS` implemented. Until a chunk implements a flag's
   behaviour, the flag parses and does nothing; no tanda is released in between, as in M4.
2. **One matcher type.** `matcher::PatternMatcher` (Rust's engine or PCRE2) implements
   `grep_matcher::Matcher`; C1 retypes every `RegexMatcher` of `search.rs`, `app.rs` and
   `output/stream.rs` to it, with only the Rust engine; C2 adds PCRE2 inside `matcher.rs`.
3. **The searcher's terminator.** The submatches the sinks record (`search::submatches_in` and
   its two callers) take the searcher, so they trim CRLF and NUL terminators and look past a
   `-U` block as grep-printer does; until C9, `output/stream.rs` keeps M4's `\n` helpers, which
   C3 keeps as wrappers so the tandas build.
4. **A `Look` carries rg's output flags into the Trail layouts.** `output::Look` (C1, in
   `RenderOptions`) holds them; C4 makes the rows (`output::Row` gains `column` and
   `byte_offset`; `match_rows` makes one row per Match for `-o` and `--vimgrep`, one per line of
   a multi-line item, and replaces with `-r`, an object file's string included, as one line at
   hex offsets; `write_line_text` trims, previews, colours), C7 and
   C8 print the prelude and the separators in the one-line layout, the summaries and the tree.
   C9 builds the `Look` from the command line (`output::rg::look`) and keeps the streaming
   printer (`output::stream`) to the flags it handles (`output::rg::streams`): `-o`,
   `--vimgrep`, `-r`, `-U`, `--null-data`, `--crlf`, `--passthru` and `--stats` take the model's
   path.
5. **grep-printer prints `--no-trail`.** `output::rg` (C9) configures the standard, summary and
   path printers as rg's `printer_standard`, `printer_summary` and `path_printer_builder`, so
   with `--no-trail` every output flag is rg's by construction, and `tests/rg_compat_m5.rs`
   compares them byte for byte.
6. **Stubs keep every tanda building.** C1 writes every new `mod` line and a stub for
   `src/{log,mcp}.rs`, `src/cli/generate.rs`, `src/output/{hyperlink,replace}.rs` and
   `style::Palette`, with the final signatures; each chunk of tanda 2 fills its own files, and
   no file is shared within a tanda (the chunk graph). A chunk that changes a function another
   file calls keeps the old signature as a wrapper until the chunk that owns the caller
   (`search::match_submatches`, note 3; `output::write_path` until C9).
7. **Tests against rg** run rg 14.1.1 at `/usr/bin/rg` with `--sort path` (one walk order),
   over the fixture copy and the files each test writes into it; a test of the Trail layouts
   states its expected output whole.
8. After each tanda: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D
   warnings`, `cargo test --workspace` with `SEARCHFOR_REQUIRE_REFERENCE=1`; `cargo fmt` before
   every commit.

## File structure

```
Cargo.toml, Cargo.lock, .cargo/config.toml     grep-pcre2, pcre2, pcre2-sys, clap_complete, clap_mangen, log; PCRE2_SYS_STATIC (C1)
src/cli/mod.rs                                 every rg flag in Args, the resolved types, parse_all; no configuration file for --mcp (C1)
src/cli/order.rs                               flags resolved by their place on the command line (C1)
src/cli/values.rs                              rg's value parsers: sizes, separators, encodings, color specs, hyperlink formats (C1)
src/cli/config.rs                              SEARCHFOR_CONFIG_PATH (C1)
src/cli/rg_flags.rs, docs/rg-compat.md         every flag implemented (C1); M5a's deviations (C11)
src/lib.rs                                     `log`, `matcher`, `mcp` (C1)
src/matcher.rs                                 PatternMatcher: Rust's engine (stub C1), PCRE2 and rg's matcher flags (C2)
src/search.rs                                  the retyped matcher, type changes, sort (C1); walker, searcher, submatches, sort_entries (C3); old helpers dropped, `file_types` (C9)
src/source.rs                                  Decoding: -E; Mapping: --no-mmap (C3)
src/model.rs, src/filter.rs                    Described::replacements, an object file's -r, and its literals (C1)
src/processors/object/rows.rs                  `shown`, a Match's byte as its row shows it (C4)
src/app.rs                                     parse_all, suggestions moved, retyped (C1); call sites of C3; summary call (C7); the pipeline, an object file's replacements (C9)
src/output/mod.rs                              Look in RenderOptions (C1); rows, line writing, paths, links (C4); `rg` module (C9)
src/output/{style,hyperlink,replace}.rs        stubs (C1); palette, hyperlinks, replacements (C4)
src/output/stream.rs                           Look literal (C1); Row's new fields (C4); tree header (C8); the searcher's submatches (C9)
src/output/flat.rs, src/output/summary.rs      the one-line layout and s4's summaries with rg's output flags (C7)
src/output/tree.rs                             the tree with rg's output flags (C8)
src/output/rg.rs                               grep-printer's printers, the Look and --stats as rg configures them (C9)
src/log.rs, src/cli/generate.rs                stubs (C1); --debug/--trace and --generate (C5)
src/mcp.rs                                     stub (C1); the MCP server, its child without configuration and its limits (C6)
tests/cli.rs, tests/cli_m4.rs, tests/perf.rs, tests/common/mod.rs   t11 rewritten; SEARCHFOR_CONFIG_PATH removed from every s4 the tests run (C1)
tests/cli_m5.rs, tests/rg_compat_m5.rs         end-to-end tests and the comparisons with rg (C9)
tests/perf.rs, tests/fuzz_m5.rs, scripts/bench-m5.sh   ceilings, fuzz and benchmark (C10)
scripts/bench{,-code,-m3,-m4}.sh               isolated from the configuration files (C10)
README.md, README.es.md, skills/searchfor/SKILL.md     (C11)
```

## Chunk graph

| Chunk | Content | Depends on | Files and folders (exclusive within its tanda) | Tanda |
|---|---|---|---|---|
| C1 | Dependencies; every flag parsed and resolved; the configuration file; `PatternMatcher` with the Rust engine; `Look`; the stubs | — | `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml`, `src/cli/{mod,order,values,config,generate,rg_flags}.rs`, `docs/rg-compat.md`, `src/lib.rs`, `src/matcher.rs`, `src/log.rs`, `src/mcp.rs`, `src/output/{mod,style,hyperlink,replace,stream}.rs`, `src/search.rs`, `src/app.rs`, `src/model.rs`, `src/filter.rs`, `tests/{cli,cli_m4,perf}.rs`, `tests/common/mod.rs` | 1 |
| C2 | The matcher: PCRE2, the engines, rg's matcher flags and errors | C1 | `src/matcher.rs` | 2 |
| C3 | The walker, the searcher and the decoding | C1 | `src/search.rs`, `src/source.rs`, `src/app.rs` | 2 |
| C4 | The output core: rows, line writing, palette, hyperlinks, replacements | C1 | `src/output/{mod,style,hyperlink,replace,stream}.rs`, `src/output/flat.rs` (one call), `src/processors/object/rows.rs` (`shown`) | 2 |
| C5 | `--generate` and the logger of `--debug`/`--trace` | C1 | `src/cli/generate.rs`, `src/log.rs` | 2 |
| C6 | The MCP server | C1 | `src/mcp.rs` | 2 |
| C7 | The one-line layout and s4's summaries with rg's output flags | C4 | `src/output/{flat,summary}.rs`, `src/output/mod.rs` (two attributes), `src/app.rs` (the summary call) | 3 |
| C8 | The tree with rg's output flags | C4 | `src/output/tree.rs`, `src/output/stream.rs` (the header) | 3 |
| C9 | The pipeline; end-to-end tests and comparisons with rg | C2–C8 | `src/app.rs`, `src/output/rg.rs`, `src/output/{mod,hyperlink,stream}.rs`, `src/search.rs`, `tests/{cli_m5,rg_compat_m5}.rs` | 4 |
| C10 | Perf ceilings, benchmark, fuzz; M1–M4 benchmarks isolated and rerun | C9 | `tests/perf.rs`, `tests/fuzz_m5.rs`, `scripts/bench-m5.sh`, `scripts/bench{,-code,-m3,-m4}.sh` | 5 |
| C11 | rg-compat deviations, README, README.es, skill | C9 | `src/cli/rg_flags.rs`, `docs/rg-compat.md`, `README.md`, `README.es.md`, `skills/searchfor/SKILL.md` | 5 |

Eleven chunks in five tandas. No file is shared within a tanda. A file edited by two chunks is
edited in different tandas: `src/app.rs` by C1, C3, C7 and C9; `src/search.rs` by C1, C3 and C9;
`src/output/mod.rs` by C1, C4, C7 and C9; `src/output/stream.rs` by C1, C4, C8 and C9;
`src/output/flat.rs` by C4 (one call) and C7; `src/output/hyperlink.rs` by C1, C4 and C9;
`src/cli/rg_flags.rs` and `docs/rg-compat.md` by C1 and C11; `tests/perf.rs` by C1 and C10. The
tanda-2 chunks depend only on C1's types and stubs: C4's rows read `Look`, which C1 defines, and
build their tests with C1's matcher; C7 and C8 print the rows C4 makes; C9 wires everything.
C10 and C11 measure and document the merged result in parallel.

---
## Chunk 1: dependencies, every flag, the configuration file, the matcher type and the stubs

### Task 1.1: the dependencies

**Files:** `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml` (new). This is the only task of M5a
that edits them.

- [ ] **Step 1:** add the dependencies after `lopdf` in `[dependencies]`, keeping every pinned
  line (D7, D15, D16):

```diff
diff --git a/Cargo.toml b/Cargo.toml
index c29c51b..db42543 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -76,6 +76,17 @@ rustc-demangle = "0.1"
 # PDF's objects, fonts, streams and encryption. Their XML and SQLite are read by our own code.
 zip = { version = "9", default-features = false, features = ["deflate-flate2"] }
 lopdf = { version = "0.45", default-features = false }
+# PCRE2 (`-P`, `--engine`; M5a plan, D7): ripgrep 14.1.1's `pcre2` feature, at the versions of
+# its Cargo.lock. PCRE2's C is built into s4 (`PCRE2_SYS_STATIC`, .cargo/config.toml), so the
+# binary depends on no system library.
+grep-pcre2 = "=0.1.8"
+pcre2 = "=0.2.9"
+pcre2-sys = "=0.2.9"
+# `--generate`: the shell completions and the man page, from s4's own command line (D16).
+clap_complete = "4.5"
+clap_mangen = "0.2"
+# `--debug` and `--trace`: the messages of ripgrep's crates, which log through `log` (D15).
+log = "0.4"
 
 # Ctrl-C with Helper Processors running (spec §4.4, Lifetime; M2 plan, D42).
 [target.'cfg(unix)'.dependencies]
```

- [ ] **Step 2:** `.cargo/config.toml`, so every build of the repository builds PCRE2 from the C
  that pcre2-sys bundles (D7):

```toml
# PCRE2 is built from the C that pcre2-sys bundles, never linked from the system (M5a plan,
# D7): every build of the repository, its tests and its releases search with the same PCRE2.
# A `cargo install searchfor` does not read this file, and links the system's libpcre2-8 when
# pkg-config finds one, as ripgrep's does.
[env]
PCRE2_SYS_STATIC = "1"
```

- [ ] **Step 3:** `cargo build`; the lock gains exactly these packages and loses none:

`clap_complete` 4.6.11, `clap_mangen` 0.2.33, `grep-pcre2` 0.1.8, `jobserver` 0.1.35, `pcre2` 0.2.9, `pcre2-sys` 0.2.9, `pkg-config` 0.3.34, `roff` 1.1.1

  `cargo tree -i regex-automata`, `-i regex-syntax`, `-i memchr` and `-i encoding_rs` still
  show only 0.4.7, 0.8.4, 2.7.4 and 0.8.34. If cargo picks newer `pcre2`/`pcre2-sys`
  (0.2.11/0.2.10 exist), the `=` pins above are missing.

### Task 1.2: the failing tests

**Files:** `src/cli/mod.rs`, `src/cli/rg_flags.rs`, `tests/cli.rs` (tests only).

- [ ] **Step 1:** in `src/cli/mod.rs`'s `mod tests`, delete `not_yet_flags_are_reported_by_name`,
  `not_yet_flags_consume_their_value` and `sort_path_and_none_are_accepted` (the flags they
  refuse now parse), and add the four tests below (D2, D3):

```rust
/// D3: the configuration file's arguments go before the command line's, and `--no-config`
/// and `--mcp` read no file (review r1 of the M5a plan, Fable #1).
#[test]
fn the_configuration_file_is_not_read_for_mcp() {
    let file = || (os(&["--generate=man", "--pre=cat"]), Vec::new());
    let a = parse_with_config(os(&["s4", "x"]), file).unwrap();
    assert!(a.generate.is_some() && a.pre.is_some());
    for argv in [&["s4", "--mcp"][..], &["s4", "--no-config", "x"]] {
        let a = parse_with_config(os(argv), file).unwrap();
        assert!(a.generate.is_none() && a.pre.is_none(), "{argv:?}");
    }
}
```

```rust
/// M5: the flags that were «not supported yet» parse, with their values (M5a plan, D1).
#[test]
fn every_ripgrep_flag_parses() {
    let a = p(&["s4", "--pcre2", "x"]);
    assert_eq!(a.engine, Engine::Pcre2);
    assert_eq!(p(&["s4", "-nP", "x"]).engine, Engine::Pcre2);
    assert_eq!(
        p(&["s4", "--max-filesize", "10K", "x"]).max_filesize,
        Some(10 << 10)
    );
    let a = p(&["s4", "-r", "--json", "x"]);
    assert_eq!(a.replace.as_deref(), Some("--json"));
    assert!(!a.json);
    assert_eq!(
        p(&["s4", "-E=utf-16le", "x"]).encoding,
        EncodingMode::Label("utf-16le".into())
    );
    assert!(!p(&["s4", "--no-unicode", "--unicode", "x"]).no_unicode);
    assert!(p(&["s4", "--unicode", "--no-pcre2-unicode", "x"]).no_unicode);
    assert!(p(&["s4", "--maxdepth", "2", "x"]).max_depth == Some(2));
    assert!(p(&["s4", "--passthrough", "x"]).passthru);
    for argv in [
        &["s4", "--max-filesize", "1k", "x"][..],
        &["s4", "-E", "klingon", "x"],
        &["s4", "--colors", "nope:fg:red", "x"],
        &["s4", "--path-separator", "ab", "x"],
        &["s4", "--engine", "fast", "x"],
        &["s4", "--sort", "size", "x"],
        &["s4", "--hyperlink-format", "{nope}", "x"],
    ] {
        assert!(
            matches!(parse(os(argv)), Err(CliError::Clap(_))),
            "{argv:?}"
        );
    }
}
```

```rust
/// rg 14.1.1 applies each flag in order (defs.rs `update`): a later flag undoes the part of
/// an earlier one it shares with it.
#[test]
fn order_dependent_flags_resolve_as_ripgrep() {
    let r = p(&["s4", "--no-ignore", "--ignore-vcs", "x"]).ignores;
    assert!(r.no_dot && r.no_parent && r.no_global && r.no_exclude && !r.no_vcs);
    let r = p(&["s4", "--no-ignore-vcs", "--ignore", "x"]).ignores;
    assert_eq!(r, IgnoreRules::default());
    let r = p(&["s4", "--no-ignore-dot", "-u", "--ignore-parent", "x"]).ignores;
    assert!(r.no_dot && r.no_vcs && !r.no_parent);
    assert_eq!(p(&["s4", "-u", "x"]).ignores, IgnoreRules::all(true));
    let sort = |v: &[&str]| p(v).sort;
    assert_eq!(sort(&["s4", "x"]), None);
    let path = Some(SortMode {
        kind: SortKind::Path,
        reverse: false,
    });
    assert_eq!(sort(&["s4", "--sort-files", "x"]), path);
    assert_eq!(
        sort(&["s4", "--sortr", "modified", "--sort", "path", "x"]),
        path
    );
    assert_eq!(
        sort(&["s4", "--sort", "path", "--sortr", "created", "x"]),
        Some(SortMode {
            kind: SortKind::Created,
            reverse: true
        })
    );
    assert_eq!(
        sort(&["s4", "--sort", "path", "--no-sort-files", "x"]),
        None
    );
    assert_eq!(sort(&["s4", "--sort-files", "--sort", "none", "x"]), None);
    assert_eq!(
        p(&["s4", "-P", "--engine", "auto", "x"]).engine,
        Engine::Auto
    );
    assert_eq!(
        p(&["s4", "--engine", "pcre2", "--no-pcre2", "x"]).engine,
        Engine::Default
    );
    assert_eq!(
        p(&["s4", "-P", "--no-auto-hybrid-regex", "x"]).engine,
        Engine::Default
    );
    assert_eq!(
        p(&["s4", "-E", "latin1", "--no-encoding", "x"]).encoding,
        EncodingMode::Auto
    );
    assert_eq!(
        p(&["s4", "-E", "none", "x"]).encoding,
        EncodingMode::Disabled
    );
    let a = p(&["s4", "--crlf", "--null-data", "x"]);
    assert!(!a.crlf && a.null_data);
    let a = p(&["s4", "--null-data", "--crlf", "x"]);
    assert!(a.crlf && !a.null_data);
    let a = p(&["s4", "-U", "--stop-on-nonmatch", "x"]);
    assert!(!a.multiline && a.stop_on_nonmatch);
    let a = p(&["s4", "--stop-on-nonmatch", "-U", "x"]);
    assert!(a.multiline && !a.stop_on_nonmatch);
    let a = p(&["s4", "--line-buffered", "--block-buffered", "x"]);
    assert_eq!(a.buffer, BufferMode::Block);
    let a = p(&["s4", "--block-buffered", "--no-block-buffered", "x"]);
    assert_eq!(a.buffer, BufferMode::Auto);
    assert_eq!(
        p(&["s4", "--trace", "--debug", "x"]).logging,
        Some(Logging::Debug)
    );
    assert_eq!(
        p(&["s4", "--debug", "--trace", "x"]).logging,
        Some(Logging::Trace)
    );
    assert_eq!(
        p(&["s4", "x"]).context_separator.as_deref(),
        Some(&b"--"[..])
    );
    let a = p(&[
        "s4",
        "--context-separator",
        "##",
        "--no-context-separator",
        "x",
    ]);
    assert_eq!(a.context_separator, None);
    let a = p(&[
        "s4",
        "--no-context-separator",
        "--context-separator",
        r"\t",
        "x",
    ]);
    assert_eq!(a.context_separator.as_deref(), Some(&b"\t"[..]));
    assert_eq!(p(&["s4", "--mmap", "x"]).mmap_mode, MmapMode::Always);
    assert_eq!(
        p(&["s4", "--mmap", "--no-mmap", "x"]).mmap_mode,
        MmapMode::Never
    );
    assert_eq!(p(&["s4", "x"]).mmap_mode, MmapMode::Auto);
}
```

```rust
#[test]
fn every_sort_is_accepted() {
    for k in ["path", "modified", "accessed", "created"] {
        assert!(p(&["s4", "--sort", k, "x"]).sort.is_some(), "{k}");
        assert!(
            p(&["s4", "--sortr", k, "x"])
                .sort
                .is_some_and(|s| s.reverse),
            "{k}"
        );
    }
    assert_eq!(p(&["s4", "--sort=none", "x"]).sort, None);
}
```

- [ ] **Step 2:** the help test now sees the implemented flags, and hides only their negations:

```rust
#[test]
fn help_shows_unrestricted_without_a_placeholder_and_hides_negations() {
    let help = command().render_help().to_string();
    assert!(help.contains("-u, --unrestricted"), "{help}");
    assert!(!help.contains("--unrestricted <"), "{help}");
    assert!(
        help.contains("Reduce the level of \"smart\" filtering."),
        "{help}"
    );
    assert!(help.contains("-P, --pcre2"), "{help}");
    assert!(
        !help.contains("--no-pcre2 ") && !help.contains("--no-pcre2\n"),
        "{help}"
    );
    assert!(!help.contains("--no-hidden"), "{help}");
}
```

- [ ] **Step 3:** in `src/cli/rg_flags.rs`'s tests, replace `implemented_flags_are_the_m1_set`
  with the test below, and in `markdown_has_header_and_one_row_per_flag` replace the `--pcre2`
  assertion with `assert!(md.contains("| `--pcre2` | `-P` | no | yes |"));` and
  `assert!(!md.contains("| not yet |"));`:

```rust
/// M5: every flag of ripgrep 14.1.1 is implemented (grill, 8375; M5a plan, D1).
#[test]
fn every_ripgrep_flag_is_implemented() {
    let not_yet: Vec<_> = RG_FLAGS
        .iter()
        .filter(|f| f.status != Status::Implemented)
        .map(|f| f.long)
        .collect();
    assert!(not_yet.is_empty(), "{not_yet:?}");
    assert_eq!(RG_FLAGS.len(), 146);
}
```

- [ ] **Step 4:** in `tests/cli.rs`, `t11_not_yet_flags_fail_loudly` becomes:

```rust
/// M5: no ripgrep flag is «not supported yet» any more (M5a plan, D1).
#[test]
fn t11_every_ripgrep_flag_is_accepted() {
    for argv in [&["--pcre2", "needle", "md"][..], &["-nP", "needle", "md"]] {
        let o = s4(argv);
        assert_eq!(code(&o), 0, "{argv:?}: {}", stderr(&o));
        assert!(!stderr(&o).contains("not supported yet"), "{argv:?}");
    }
}
```

- [ ] **Step 5:** `cargo test --lib -- cli::` does not compile (`Engine`, `IgnoreRules`,
  `SortMode`… are unknown): RED.

### Task 1.3: the flags

**Files:** `src/cli/mod.rs`, `src/cli/order.rs` (new), `src/cli/values.rs` (new),
`src/cli/config.rs` (new).

- [ ] **Step 1:** `src/cli/values.rs`, the parsers of the values clap cannot check by itself,
  with rg 14.1.1's rules (each a usage error with clap's message, D21):

```rust
//! Parsers of the values of ripgrep's flags that clap cannot check by itself, with rg 14.1.1's
//! rules (crates/core/flags/defs.rs, `convert` and the `update` of each flag). A value that does
//! not parse is a usage error, exit 2, with clap's message (rg-compat.md, «Usage errors»).

/// Bytes a flag gives, kept as a type of their own: clap reads a `Vec<u8>` field as a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

/// `--path-separator`: one byte, or none (an empty value, which rg treats as no separator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathSeparator(pub Option<u8>);

/// rg's sizes: digits and an optional `K`, `M` or `G` (grep-cli's `parse_human_readable_size`).
pub fn human_u64(s: &str) -> Result<u64, String> {
    grep_cli::parse_human_readable_size(s).map_err(|e| format!("invalid size: {e}"))
}

/// `human_u64` for a limit that must fit a `usize` (`--dfa-size-limit`, `--regex-size-limit`).
pub fn human_usize(s: &str) -> Result<usize, String> {
    usize::try_from(human_u64(s)?).map_err(|_| "size is too big".to_string())
}

/// A separator with rg's escapes (`\t`, `\n`, `\x7F`, …), as bstr's `unescape_bytes` reads them.
pub fn separator(s: &str) -> Result<Bytes, String> {
    Ok(Bytes(<Vec<u8> as bstr::ByteVec>::unescape_bytes(s)))
}

/// `--path-separator`: exactly one byte once unescaped, or empty (rg 14.1.1's message).
pub fn path_separator(s: &str) -> Result<PathSeparator, String> {
    let raw = <Vec<u8> as bstr::ByteVec>::unescape_bytes(s);
    match raw.len() {
        0 => Ok(PathSeparator(None)),
        1 => Ok(PathSeparator(Some(raw[0]))),
        len => Err(format!(
            "A path separator must be exactly one byte, but the given separator is {len} \
             bytes: {s}\nIn some shells on Windows '/' is automatically expanded. Use '//' \
             instead."
        )),
    }
}

/// `-E`: `auto`, `none`, or a label the WHATWG Encoding Standard knows (grep-searcher's
/// `Encoding::new`, which `encoding_rs` backs).
pub fn encoding(s: &str) -> Result<String, String> {
    if s == "auto" || s == "none" {
        return Ok(s.to_string());
    }
    grep_searcher::Encoding::new(s)
        .map(|_| s.to_string())
        .map_err(|e| e.to_string())
}

/// `--colors`: a spec grep-printer reads (`{type}:{attribute}:{value}`), checked here and kept
/// as written; s4 also takes the type `trail` (M5a plan, D11), which grep-printer does not know.
pub fn color_spec(s: &str) -> Result<String, String> {
    if let Some(rest) = s.strip_prefix("trail:") {
        // Checked as the same spec for a type grep-printer knows.
        return format!("path:{rest}")
            .parse::<grep_printer::UserColorSpec>()
            .map(|_| s.to_string())
            .map_err(|e| e.to_string());
    }
    s.parse::<grep_printer::UserColorSpec>()
        .map(|_| s.to_string())
        .map_err(|e| e.to_string())
}

/// `--hyperlink-format`: a format or an alias grep-printer knows (`default`, `vscode`, …),
/// kept as written.
pub fn hyperlink_format(s: &str) -> Result<String, String> {
    s.parse::<grep_printer::HyperlinkFormat>()
        .map(|_| s.to_string())
        .map_err(|e| format!("invalid hyperlink format: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_as_ripgrep() {
        assert_eq!(human_u64("123"), Ok(123));
        assert_eq!(human_u64("2K"), Ok(2048));
        assert_eq!(human_u64("3M"), Ok(3 << 20));
        assert_eq!(human_u64("1G"), Ok(1 << 30));
        assert!(human_u64("1k").is_err());
        assert!(human_u64("").is_err());
        assert!(human_u64("K").is_err());
        assert!(human_u64("99999999999999999999G").is_err());
    }

    #[test]
    fn separators_read_escapes() {
        assert_eq!(separator("--"), Ok(Bytes(b"--".to_vec())));
        assert_eq!(separator(r"\t\x00"), Ok(Bytes(b"\t\0".to_vec())));
        assert_eq!(separator(""), Ok(Bytes(Vec::new())));
    }

    #[test]
    fn path_separator_is_one_byte_or_none() {
        assert_eq!(path_separator("/"), Ok(PathSeparator(Some(b'/'))));
        assert_eq!(path_separator(r"\\"), Ok(PathSeparator(Some(b'\\'))));
        assert_eq!(path_separator(""), Ok(PathSeparator(None)));
        let e = path_separator("ab").unwrap_err();
        assert!(e.starts_with("A path separator must be exactly one byte, but the given"));
        assert!(e.contains("is 2 bytes: ab"));
    }

    #[test]
    fn encodings_are_whatwg_labels() {
        assert_eq!(encoding("utf-16le").as_deref(), Ok("utf-16le"));
        assert_eq!(encoding("latin1").as_deref(), Ok("latin1"));
        assert_eq!(encoding("auto").as_deref(), Ok("auto"));
        assert_eq!(encoding("none").as_deref(), Ok("none"));
        assert!(encoding("klingon").is_err());
    }

    #[test]
    fn color_specs_and_the_trail_type() {
        assert!(color_spec("match:fg:blue").is_ok());
        assert!(color_spec("path:none").is_ok());
        assert!(color_spec("line:style:bold").is_ok());
        assert!(color_spec("trail:fg:yellow").is_ok());
        assert!(color_spec("nope:fg:blue").is_err());
        assert!(color_spec("match:fg:nocolor").is_err());
    }

    #[test]
    fn hyperlink_formats_and_aliases() {
        assert!(hyperlink_format("default").is_ok());
        assert!(hyperlink_format("vscode").is_ok());
        assert!(hyperlink_format("file://{host}{path}:{line}").is_ok());
        assert!(hyperlink_format("{nope}").is_err());
    }
}
```

- [ ] **Step 2:** `src/cli/order.rs`, the flags resolved by their place (D2):

```rust
//! Flags whose effect depends on their place on the command line (M5a plan, D2). ripgrep
//! 14.1.1 applies each flag in order to its `LowArgs` (crates/core/flags/defs.rs, `update`), so
//! a later flag undoes part of an earlier one: `--no-ignore --ignore-vcs` reads `.gitignore`
//! again, `--pretty --no-heading` prints no heading. clap keeps, for each switch, the place of
//! its last occurrence, which is enough: a switch sets the same values every time, so only its
//! last occurrence can be the last to set any of them. Flags with values keep every place.
use super::{
    Args, BufferMode, ColorWhen, EncodingMode, Engine, IgnoreRules, Logging, MmapMode, SortKind,
    SortMode, TypeChange, values,
};
use clap::ArgMatches;
use clap::parser::ValueSource;

/// The place of the last occurrence of a switch given on the command line.
fn at(m: &ArgMatches, id: &str) -> Option<usize> {
    if m.value_source(id) != Some(ValueSource::CommandLine) {
        return None;
    }
    m.index_of(id)
}

/// Every value of a flag given on the command line, with its place.
fn values<T: Clone + Send + Sync + 'static>(m: &ArgMatches, id: &str) -> Vec<(usize, T)> {
    if m.value_source(id) != Some(ValueSource::CommandLine) {
        return Vec::new();
    }
    match (m.get_many::<T>(id), m.indices_of(id)) {
        (Some(vals), Some(idx)) => idx.zip(vals.cloned()).collect(),
        _ => Vec::new(),
    }
}

/// Applies `steps` in command-line order to `state`.
fn replay<S, E>(state: &mut S, mut steps: Vec<(usize, E)>, apply: impl Fn(&mut S, E)) {
    steps.sort_by_key(|(i, _)| *i);
    for (_, e) in steps {
        apply(state, e);
    }
}

/// Resolves every order-dependent flag of `a` from the matches it was parsed from. `first_u`
/// is the place of the first -u, which acts as --no-ignore there.
pub(super) fn resolve(m: &ArgMatches, a: &mut Args, first_u: Option<usize>) {
    a.ignores = ignores(m, first_u);
    a.sort = sort(m);
    a.engine = engine(m);
    a.encoding = encoding(m);
    a.type_changes = type_changes(m);
    a.no_unicode = no_unicode(m);
    (a.crlf, a.null_data) = terminators(m);
    (a.multiline, a.stop_on_nonmatch) = multiline(m);
    a.buffer = buffer(m);
    a.logging = logging(m);
    a.context_separator = context_separator(m);
    passthru(m, a);
    headings(m, a);
    modes(m, a);
}

fn ignores(m: &ArgMatches, first_u: Option<usize>) -> IgnoreRules {
    #[derive(Clone, Copy)]
    enum Kind {
        All,
        Dot,
        Exclude,
        Global,
        Parent,
        Vcs,
    }
    let mut steps: Vec<(usize, (Kind, bool))> = Vec::new();
    let mut push = |id: &str, kind: Kind, no: bool| {
        if let Some(i) = at(m, id) {
            steps.push((i, (kind, no)));
        }
    };
    push("no_ignore", Kind::All, true);
    push("ignore", Kind::All, false);
    push("no_ignore_dot", Kind::Dot, true);
    push("ignore_dot", Kind::Dot, false);
    push("no_ignore_exclude", Kind::Exclude, true);
    push("ignore_exclude", Kind::Exclude, false);
    push("no_ignore_global", Kind::Global, true);
    push("ignore_global", Kind::Global, false);
    push("no_ignore_parent", Kind::Parent, true);
    push("ignore_parent", Kind::Parent, false);
    push("no_ignore_vcs", Kind::Vcs, true);
    push("ignore_vcs", Kind::Vcs, false);
    if let Some(i) = first_u {
        steps.push((i, (Kind::All, true)));
    }
    let mut r = IgnoreRules::default();
    replay(&mut r, steps, |r, (kind, no)| match kind {
        Kind::All => *r = IgnoreRules::all(no),
        Kind::Dot => r.no_dot = no,
        Kind::Exclude => r.no_exclude = no,
        Kind::Global => r.no_global = no,
        Kind::Parent => r.no_parent = no,
        Kind::Vcs => r.no_vcs = no,
    });
    r
}

/// rg's `--sort`, `--sortr`, `--sort-files` (`--sort path`) and `--no-sort-files` (none).
fn sort(m: &ArgMatches) -> Option<SortMode> {
    let mut steps: Vec<(usize, Option<SortMode>)> = Vec::new();
    for (id, reverse) in [("sort_flags", false), ("sortr_flags", true)] {
        for (i, v) in values::<String>(m, id) {
            let kind = match v.as_str() {
                "path" => SortKind::Path,
                "modified" => SortKind::Modified,
                "accessed" => SortKind::Accessed,
                "created" => SortKind::Created,
                _ => {
                    steps.push((i, None));
                    continue;
                }
            };
            steps.push((i, Some(SortMode { kind, reverse })));
        }
    }
    if let Some(i) = at(m, "sort_files") {
        steps.push((
            i,
            Some(SortMode {
                kind: SortKind::Path,
                reverse: false,
            }),
        ));
    }
    if let Some(i) = at(m, "no_sort_files") {
        steps.push((i, None));
    }
    let mut s = None;
    replay(&mut s, steps, |s, v| *s = v);
    s
}

/// `-P`, `--no-pcre2`, `--engine`, `--auto-hybrid-regex` and its negation.
fn engine(m: &ArgMatches) -> Engine {
    let mut steps: Vec<(usize, Engine)> = values::<String>(m, "engine_flags")
        .into_iter()
        .map(|(i, v)| {
            let e = match v.as_str() {
                "pcre2" => Engine::Pcre2,
                "auto" => Engine::Auto,
                _ => Engine::Default,
            };
            (i, e)
        })
        .collect();
    for (id, e) in [
        ("pcre2", Engine::Pcre2),
        ("no_pcre2", Engine::Default),
        ("auto_hybrid_regex", Engine::Auto),
        ("no_auto_hybrid_regex", Engine::Default),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, e));
        }
    }
    let mut e = Engine::Default;
    replay(&mut e, steps, |e, v| *e = v);
    e
}

/// `-E` and `--no-encoding` (which is `auto`, rg's default).
fn encoding(m: &ArgMatches) -> EncodingMode {
    let mut steps: Vec<(usize, EncodingMode)> = values::<String>(m, "encoding_flags")
        .into_iter()
        .map(|(i, v)| {
            let e = match v.as_str() {
                "auto" => EncodingMode::Auto,
                "none" => EncodingMode::Disabled,
                label => EncodingMode::Label(label.to_string()),
            };
            (i, e)
        })
        .collect();
    if let Some(i) = at(m, "no_encoding") {
        steps.push((i, EncodingMode::Auto));
    }
    let mut e = EncodingMode::Auto;
    replay(&mut e, steps, |e, v| *e = v);
    e
}

/// -t, -T, --type-add and --type-clear, in command-line order.
fn type_changes(m: &ArgMatches) -> Vec<TypeChange> {
    let mut v: Vec<(usize, TypeChange)> = Vec::new();
    for (id, make) in [
        ("types", TypeChange::Select as fn(String) -> TypeChange),
        ("types_not", TypeChange::Negate),
        ("type_add", TypeChange::Add),
        ("type_clear", TypeChange::Clear),
    ] {
        v.extend(
            values::<String>(m, id)
                .into_iter()
                .map(|(i, t)| (i, make(t))),
        );
    }
    v.sort_by_key(|e| e.0);
    v.into_iter().map(|(_, c)| c).collect()
}

/// `--no-unicode`, `--no-pcre2-unicode` and their negations set one thing.
fn no_unicode(m: &ArgMatches) -> bool {
    let mut steps = Vec::new();
    for (id, no) in [
        ("no_unicode", true),
        ("unicode", false),
        ("no_pcre2_unicode", true),
        ("pcre2_unicode", false),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, no));
        }
    }
    let mut no = false;
    replay(&mut no, steps, |n, v| *n = v);
    no
}

/// `--crlf` turns `--null-data` off and `--null-data` turns `--crlf` off.
fn terminators(m: &ArgMatches) -> (bool, bool) {
    #[derive(Clone, Copy)]
    enum T {
        Crlf(bool),
        NullData,
    }
    let mut steps = Vec::new();
    for (id, t) in [
        ("crlf", T::Crlf(true)),
        ("no_crlf", T::Crlf(false)),
        ("null_data", T::NullData),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, t));
        }
    }
    let mut s = (false, false);
    replay(&mut s, steps, |s, t| match t {
        T::Crlf(on) => {
            s.0 = on;
            if on {
                s.1 = false;
            }
        }
        T::NullData => *s = (false, true),
    });
    s
}

/// `-U` turns `--stop-on-nonmatch` off and `--stop-on-nonmatch` turns `-U` off.
fn multiline(m: &ArgMatches) -> (bool, bool) {
    #[derive(Clone, Copy)]
    enum T {
        Multiline(bool),
        Stop,
    }
    let mut steps = Vec::new();
    for (id, t) in [
        ("multiline", T::Multiline(true)),
        ("no_multiline", T::Multiline(false)),
        ("stop_on_nonmatch", T::Stop),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, t));
        }
    }
    let mut s = (false, false);
    replay(&mut s, steps, |s, t| match t {
        T::Multiline(on) => {
            s.0 = on;
            if on {
                s.1 = false;
            }
        }
        T::Stop => *s = (false, true),
    });
    s
}

fn buffer(m: &ArgMatches) -> BufferMode {
    let mut steps = Vec::new();
    for (id, b) in [
        ("line_buffered", BufferMode::Line),
        ("no_line_buffered", BufferMode::Auto),
        ("block_buffered", BufferMode::Block),
        ("no_block_buffered", BufferMode::Auto),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, b));
        }
    }
    let mut b = BufferMode::Auto;
    replay(&mut b, steps, |b, v| *b = v);
    b
}

fn logging(m: &ArgMatches) -> Option<Logging> {
    let mut steps = Vec::new();
    for (id, l) in [("debug", Logging::Debug), ("trace", Logging::Trace)] {
        if let Some(i) = at(m, id) {
            steps.push((i, l));
        }
    }
    let mut l = None;
    replay(&mut l, steps, |l, v| *l = Some(v));
    l
}

fn context_separator(m: &ArgMatches) -> Option<Vec<u8>> {
    let mut steps: Vec<(usize, Option<Vec<u8>>)> =
        values::<values::Bytes>(m, "context_separator_flag")
            .into_iter()
            .map(|(i, b)| (i, Some(b.0)))
            .collect();
    if let Some(i) = at(m, "no_context_separator") {
        steps.push((i, None));
    }
    let mut s = Some(b"--".to_vec());
    replay(&mut s, steps, |s, v| *s = v);
    s
}

/// `--passthru` replaces the context, and a later -A, -B or -C replaces `--passthru` with only
/// the context flags that come after it (rg's `ContextMode::set_*`).
fn passthru(m: &ArgMatches, a: &mut Args) {
    let Some(p) = at(m, "passthru") else {
        return;
    };
    let after_it = |id: &str| at(m, id).is_some_and(|i| i > p);
    let (b, af, c) = (
        after_it("before_context"),
        after_it("after_context"),
        after_it("context"),
    );
    if !b {
        a.before_context = None;
    }
    if !af {
        a.after_context = None;
    }
    if !c {
        a.context = None;
    }
    a.passthru = !(b || af || c);
}

/// The layout and what `--pretty` sets: `--heading`, `--tree` and `--pretty` ask for the tree
/// (rg's heading), `--no-heading` and `--flat` for the one-line layout, the last one winning;
/// `--pretty` also sets `--color always` and `-n` where it comes after them (M5a plan, D5).
fn headings(m: &ArgMatches, a: &mut Args) {
    let mut steps = Vec::new();
    for (id, h) in [
        ("heading", true),
        ("tree", true),
        ("pretty", true),
        ("no_heading", false),
        ("flat", false),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, h));
        }
    }
    let mut h = None;
    replay(&mut h, steps, |h, v| *h = Some(v));
    a.tree = h == Some(true);
    a.flat = h == Some(false);
    let Some(p) = at(m, "pretty") else {
        return;
    };
    if at(m, "color").is_none_or(|i| i < p) {
        a.color = ColorWhen::Always;
    }
    // -n and -N override each other; -p acts as a -n at its place.
    let n = at(m, "line_number").or(at(m, "no_line_number"));
    if n.is_none_or(|i| i < p) {
        a.line_number = true;
        a.no_line_number = false;
    }
}

/// `--mmap`/`--no-mmap`; and the actions, of which the last one given runs as rg's
/// `Mode::update` (`--files`, `--type-list`, `--generate`).
fn modes(m: &ArgMatches, a: &mut Args) {
    a.mmap_mode = match (at(m, "mmap"), at(m, "no_mmap")) {
        (Some(_), _) => MmapMode::Always,
        (_, Some(_)) => MmapMode::Never,
        _ => MmapMode::Auto,
    };
    let last = [
        at(m, "files"),
        at(m, "type_list"),
        values::<String>(m, "generate").last().map(|(i, _)| *i),
    ];
    if let Some(max) = last.iter().flatten().max().copied() {
        a.files = last[0] == Some(max);
        a.type_list = last[1] == Some(max);
        if last[2] != Some(max) {
            a.generate = None;
        }
    }
}
```

- [ ] **Step 3:** `src/cli/config.rs` (D3):

```rust
//! The configuration file (M5a plan, D3): `SEARCHFOR_CONFIG_PATH` names a file with one
//! argument per line, read as ripgrep 14.1.1 reads `RIPGREP_CONFIG_PATH` (crates/core/flags/
//! config.rs): each line trimmed, empty lines and lines starting with `#` skipped, the arguments
//! put before the command line's, so the command line wins. `--no-config` skips it. There is
//! no default location: without the variable nothing is read, so a run never reads a file the
//! user did not name.
use std::ffi::OsString;
use std::io::BufRead;
use std::path::Path;

/// The variable that names the configuration file.
pub const ENV: &str = "SEARCHFOR_CONFIG_PATH";

/// The arguments of the file `SEARCHFOR_CONFIG_PATH` names, and the messages to print: a file
/// that cannot be read gives one and no argument; a line that is not valid UTF-8 (on Windows)
/// gives one and is skipped, as in rg.
pub fn args() -> (Vec<OsString>, Vec<String>) {
    let Some(path) = std::env::var_os(ENV).filter(|p| !p.is_empty()) else {
        return (Vec::new(), Vec::new());
    };
    let path = Path::new(&path);
    match std::fs::File::open(path) {
        Err(e) => (
            Vec::new(),
            vec![format!(
                "failed to read the file specified in {ENV}: {}: {e}",
                path.display()
            )],
        ),
        Ok(f) => {
            let (args, errs) = parse_reader(std::io::BufReader::new(f));
            let msgs = errs
                .into_iter()
                .map(|e| format!("{}:{e}", path.display()))
                .collect();
            (args, msgs)
        }
    }
}

/// One argument per line: trimmed, with blank lines and `#` comments skipped. Errors are
/// `line: reason`.
pub fn parse_reader(mut r: impl BufRead) -> (Vec<OsString>, Vec<String>) {
    let (mut args, mut errs) = (Vec::new(), Vec::new());
    let mut line = Vec::new();
    let mut n = 0;
    loop {
        line.clear();
        match r.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                errs.push(format!("{}: {e}", n + 1));
                break;
            }
        }
        n += 1;
        let t = line.trim_ascii();
        if t.is_empty() || t[0] == b'#' {
            continue;
        }
        match os_string(t) {
            Some(a) => args.push(a),
            None => errs.push(format!("{n}: invalid UTF-8")),
        }
    }
    (args, errs)
}

#[cfg(unix)]
fn os_string(b: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;
    Some(OsString::from_vec(b.to_vec()))
}

#[cfg(not(unix))]
fn os_string(b: &[u8]) -> Option<OsString> {
    std::str::from_utf8(b).ok().map(OsString::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ripgrep 14.1.1's own test of its reader (config.rs, `basic`).
    #[test]
    fn lines_trimmed_comments_and_blanks_skipped() {
        let (args, errs) = parse_reader(
            &b"\
# Test
--context=0
   --smart-case
-u


   # --bar
--foo
"[..],
        );
        assert!(errs.is_empty());
        let args: Vec<_> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args, ["--context=0", "--smart-case", "-u", "--foo"]);
    }

    #[test]
    fn a_last_line_without_terminator_counts() {
        let (args, _) = parse_reader(&b"-i\n--hidden"[..]);
        assert_eq!(args, ["-i", "--hidden"]);
    }
}
```

- [ ] **Step 4:** `src/cli/mod.rs`: the module lines, the resolved types, every flag in `Args`
  (rg's short help as their doc, the negations hidden), `--mcp`, `--maxdepth` and
  `--passthrough` as rg's aliases, `parse_all` before `parse` (through `parse_with_config`, which
  reads no file for `--no-config`, `--pcre2-version` and `--mcp`), and `parse` resolving through
  `order::resolve`. The diff without the test hunks of Task 1.2:

```diff
diff --git a/src/cli/mod.rs b/src/cli/mod.rs
index 55d47f6..0ac7a12 100644
--- a/src/cli/mod.rs
+++ b/src/cli/mod.rs
@@ -1,5 +1,9 @@
 //! Command line: ripgrep-compatible flags plus searchfor's own (spec §8).
+pub mod config;
+pub mod generate;
+mod order;
 pub mod rg_flags;
+pub mod values;
 
 use clap::{
     Arg, ArgAction, CommandFactory, FromArgMatches, Parser, ValueEnum, parser::ValueSource,
@@ -39,6 +43,101 @@ pub enum BinaryChoice {
     Text,
 }
 
+/// The ignore files the walker reads, after --no-ignore, --ignore, the first -u and the
+/// --no-ignore-* flags in command-line order (rg 14.1.1, defs.rs `NoIgnore`): true turns that
+/// kind off.
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+pub struct IgnoreRules {
+    /// `.ignore` and `.rgignore`.
+    pub no_dot: bool,
+    /// `.git/info/exclude`.
+    pub no_exclude: bool,
+    /// git's global excludes file.
+    pub no_global: bool,
+    /// Ignore files in the parents of a path given.
+    pub no_parent: bool,
+    /// `.gitignore`, and with it the exclude and global files.
+    pub no_vcs: bool,
+}
+
+impl IgnoreRules {
+    fn all(no: bool) -> Self {
+        IgnoreRules {
+            no_dot: no,
+            no_exclude: no,
+            no_global: no,
+            no_parent: no,
+            no_vcs: no,
+        }
+    }
+}
+
+/// One of -t, -T, --type-add and --type-clear, applied in command-line order as rg's
+/// `TypeChange`s.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub enum TypeChange {
+    Select(String),
+    Negate(String),
+    Add(String),
+    Clear(String),
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum SortKind {
+    Path,
+    Modified,
+    Accessed,
+    Created,
+}
+
+/// `--sort` (ascending) and `--sortr` (descending).
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub struct SortMode {
+    pub kind: SortKind,
+    pub reverse: bool,
+}
+
+/// The regex engine: Rust's (`default`), PCRE2, or Rust's falling back to PCRE2 (`auto`).
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+pub enum Engine {
+    #[default]
+    Default,
+    Pcre2,
+    Auto,
+}
+
+/// `-E`: a BOM decides (`auto`), a WHATWG label, or no transcoding at all (`none`).
+#[derive(Debug, Clone, Default, PartialEq, Eq)]
+pub enum EncodingMode {
+    #[default]
+    Auto,
+    Label(String),
+    Disabled,
+}
+
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+pub enum MmapMode {
+    #[default]
+    Auto,
+    Always,
+    Never,
+}
+
+/// How stdout is flushed: per line on a terminal and per block in a pipe (`Auto`), as rg.
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+pub enum BufferMode {
+    #[default]
+    Auto,
+    Line,
+    Block,
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum Logging {
+    Debug,
+    Trace,
+}
+
 #[derive(Debug, Parser)]
 #[command(
     name = "s4",
@@ -89,10 +188,6 @@ pub struct Args {
     pub types: Vec<String>,
     #[arg(short = 'T', long = "type-not", action = ArgAction::Append)]
     pub types_not: Vec<String>,
-    /// `-t` (true) and `-T` (false) in command-line order, resolved in `parse`: ripgrep applies
-    /// them in that order, so the last selection of a type wins.
-    #[arg(skip)]
-    pub type_selections: Vec<(bool, String)>,
     #[arg(long)]
     pub type_list: bool,
     #[arg(short = '.', long, overrides_with = "no_hidden")]
@@ -112,18 +207,16 @@ pub struct Args {
     pub unrestricted_flags: Vec<u8>,
     #[arg(skip)]
     pub unrestricted: u8,
-    /// Resolved in `parse` by position, like `binary_choice`: the first -u acts as --no-ignore
-    /// and the second as --hidden at the place where they appear (rg 14.1.1: `-u --ignore`
-    /// keeps ignores, `--ignore -u` drops them, `-uu --no-hidden` hides).
-    #[arg(skip)]
-    pub no_ignore_resolved: bool,
+    /// Resolved in `parse` by position, like `binary_choice`: the second -u acts as --hidden at
+    /// the place where it appears (rg 14.1.1: `-uu --no-hidden` hides nothing), as the first
+    /// acts as --no-ignore (`ignores`: `-u --ignore` keeps ignores, `--ignore -u` drops them).
     #[arg(skip)]
     pub hidden_resolved: bool,
     #[arg(short = 'L', long, overrides_with = "no_follow")]
     pub follow: bool,
     #[arg(long, hide = true)]
     pub no_follow: bool,
-    #[arg(short = 'd', long)]
+    #[arg(short = 'd', long, alias = "maxdepth")]
     pub max_depth: Option<usize>,
     #[arg(short = 'A', long)]
     pub after_context: Option<usize>,
@@ -159,9 +252,6 @@ pub struct Args {
     pub json: bool,
     #[arg(long, hide = true)]
     pub no_json: bool,
-    /// Only `path` and `none` in M1; any other value is «not supported yet».
-    #[arg(long)]
-    pub sort: Option<String>,
     #[arg(short = 'j', long)]
     pub threads: Option<usize>,
     // The four binary-mode flags override each other (last wins); the third -u competes with
@@ -203,6 +293,297 @@ pub struct Args {
     #[arg(long, hide = true, overrides_with = "no_messages")]
     pub messages: bool,
 
+    // M5: the rest of ripgrep 14.1.1's flags (M5a plan, D2). Pairs override each other, the
+    // last one given winning; flags that set more than one thing are resolved in `parse` by
+    // their place on the command line (`order::resolve`), as rg applies them in order.
+    /// Don't use .ignore or .rgignore files.
+    #[arg(long, overrides_with = "ignore_dot")]
+    pub no_ignore_dot: bool,
+    #[arg(long, hide = true, overrides_with = "no_ignore_dot")]
+    pub ignore_dot: bool,
+    /// Don't use local exclusion files.
+    #[arg(long, overrides_with = "ignore_exclude")]
+    pub no_ignore_exclude: bool,
+    #[arg(long, hide = true, overrides_with = "no_ignore_exclude")]
+    pub ignore_exclude: bool,
+    /// Don't use --ignore-file arguments.
+    #[arg(long, overrides_with = "ignore_files")]
+    pub no_ignore_files: bool,
+    #[arg(long, hide = true, overrides_with = "no_ignore_files")]
+    pub ignore_files: bool,
+    /// Don't use global ignore files.
+    #[arg(long, overrides_with = "ignore_global")]
+    pub no_ignore_global: bool,
+    #[arg(long, hide = true, overrides_with = "no_ignore_global")]
+    pub ignore_global: bool,
+    /// Suppress gitignore parse error messages.
+    #[arg(long, overrides_with = "ignore_messages")]
+    pub no_ignore_messages: bool,
+    #[arg(long, hide = true, overrides_with = "no_ignore_messages")]
+    pub ignore_messages: bool,
+    /// Don't use ignore files in parent directories.
+    #[arg(long, overrides_with = "ignore_parent")]
+    pub no_ignore_parent: bool,
+    #[arg(long, hide = true, overrides_with = "no_ignore_parent")]
+    pub ignore_parent: bool,
+    /// Don't use ignore files from source control.
+    #[arg(long, overrides_with = "ignore_vcs")]
+    pub no_ignore_vcs: bool,
+    #[arg(long, hide = true, overrides_with = "no_ignore_vcs")]
+    pub ignore_vcs: bool,
+    /// Use .gitignore outside of git repositories.
+    #[arg(long, overrides_with = "require_git")]
+    pub no_require_git: bool,
+    #[arg(long, hide = true, overrides_with = "no_require_git")]
+    pub require_git: bool,
+    /// The ignore rules after --no-ignore, the first -u and the --no-ignore-* flags, in order.
+    #[arg(skip)]
+    pub ignores: IgnoreRules,
+    /// Specify additional ignore files.
+    #[arg(long = "ignore-file", value_name = "PATH", action = ArgAction::Append)]
+    pub ignore_file: Vec<PathBuf>,
+    /// Process ignore files case insensitively.
+    #[arg(long, overrides_with = "no_ignore_file_case_insensitive")]
+    pub ignore_file_case_insensitive: bool,
+    #[arg(long, hide = true, overrides_with = "ignore_file_case_insensitive")]
+    pub no_ignore_file_case_insensitive: bool,
+    /// Process all glob patterns case insensitively.
+    #[arg(long, overrides_with = "no_glob_case_insensitive")]
+    pub glob_case_insensitive: bool,
+    #[arg(long, hide = true, overrides_with = "glob_case_insensitive")]
+    pub no_glob_case_insensitive: bool,
+    /// Skip directories on other file systems.
+    #[arg(long, overrides_with = "no_one_file_system")]
+    pub one_file_system: bool,
+    #[arg(long, hide = true, overrides_with = "one_file_system")]
+    pub no_one_file_system: bool,
+    /// Ignore files larger than NUM in size.
+    #[arg(long, value_name = "NUM+SUFFIX?", value_parser = values::human_u64)]
+    pub max_filesize: Option<u64>,
+    /// Add a new glob for a file type.
+    #[arg(long = "type-add", value_name = "TYPESPEC", action = ArgAction::Append)]
+    pub type_add: Vec<String>,
+    /// Clear globs for a file type.
+    #[arg(long = "type-clear", value_name = "TYPE", action = ArgAction::Append)]
+    pub type_clear: Vec<String>,
+    /// -t, -T, --type-add and --type-clear in command-line order, resolved in `parse`.
+    #[arg(skip)]
+    pub type_changes: Vec<TypeChange>,
+    /// Sort results in ascending order.
+    #[arg(long = "sort", value_name = "SORTBY", action = ArgAction::Append,
+          value_parser = ["path", "modified", "accessed", "created", "none"])]
+    pub sort_flags: Vec<String>,
+    /// Sort results in descending order.
+    #[arg(long = "sortr", value_name = "SORTBY", action = ArgAction::Append,
+          value_parser = ["path", "modified", "accessed", "created", "none"])]
+    pub sortr_flags: Vec<String>,
+    /// (DEPRECATED) Sort results by file path.
+    #[arg(long, overrides_with = "no_sort_files")]
+    pub sort_files: bool,
+    #[arg(long, hide = true, overrides_with = "sort_files")]
+    pub no_sort_files: bool,
+    /// --sort, --sortr, --sort-files and --no-sort-files, the last one winning.
+    #[arg(skip)]
+    pub sort: Option<SortMode>,
+    /// Print each file that would be searched.
+    #[arg(long)]
+    pub files: bool,
+
+    /// Enable PCRE2 matching.
+    #[arg(short = 'P', long, overrides_with = "no_pcre2")]
+    pub pcre2: bool,
+    #[arg(long, hide = true, overrides_with = "pcre2")]
+    pub no_pcre2: bool,
+    /// Specify which regex engine to use.
+    #[arg(long = "engine", value_name = "ENGINE", action = ArgAction::Append,
+          value_parser = ["default", "pcre2", "auto"])]
+    pub engine_flags: Vec<String>,
+    /// (DEPRECATED) Use PCRE2 if appropriate.
+    #[arg(long, overrides_with = "no_auto_hybrid_regex")]
+    pub auto_hybrid_regex: bool,
+    #[arg(long, hide = true, overrides_with = "auto_hybrid_regex")]
+    pub no_auto_hybrid_regex: bool,
+    /// -P, --engine and --auto-hybrid-regex, the last one winning.
+    #[arg(skip)]
+    pub engine: Engine,
+    /// Print the version of PCRE2 that s4 uses.
+    #[arg(long)]
+    pub pcre2_version: bool,
+    /// Disable Unicode mode.
+    #[arg(long)]
+    pub no_unicode: bool,
+    #[arg(long, hide = true)]
+    pub unicode: bool,
+    /// (DEPRECATED) Disable Unicode mode for PCRE2.
+    #[arg(long)]
+    pub no_pcre2_unicode: bool,
+    #[arg(long, hide = true)]
+    pub pcre2_unicode: bool,
+    /// Use CRLF line terminators (nice for Windows).
+    #[arg(long)]
+    pub crlf: bool,
+    #[arg(long, hide = true)]
+    pub no_crlf: bool,
+    /// Use NUL as a line terminator.
+    #[arg(long)]
+    pub null_data: bool,
+    /// Enable searching across multiple lines.
+    #[arg(short = 'U', long)]
+    pub multiline: bool,
+    #[arg(long, hide = true)]
+    pub no_multiline: bool,
+    /// Make '.' match line terminators.
+    #[arg(long, overrides_with = "no_multiline_dotall")]
+    pub multiline_dotall: bool,
+    #[arg(long, hide = true, overrides_with = "multiline_dotall")]
+    pub no_multiline_dotall: bool,
+    /// Stop searching after a non-match.
+    #[arg(long)]
+    pub stop_on_nonmatch: bool,
+    /// The upper size limit of the regex DFA.
+    #[arg(long, value_name = "NUM+SUFFIX?", value_parser = values::human_usize)]
+    pub dfa_size_limit: Option<usize>,
+    /// The size limit of the compiled regex.
+    #[arg(long, value_name = "NUM+SUFFIX?", value_parser = values::human_usize)]
+    pub regex_size_limit: Option<usize>,
+    /// Specify the text encoding of files to search.
+    #[arg(short = 'E', long = "encoding", value_name = "ENCODING", action = ArgAction::Append,
+          value_parser = values::encoding)]
+    pub encoding_flags: Vec<String>,
+    #[arg(long, hide = true)]
+    pub no_encoding: bool,
+    /// -E and --no-encoding, the last one winning.
+    #[arg(skip)]
+    pub encoding: EncodingMode,
+    /// Search with memory maps when possible.
+    #[arg(long, overrides_with = "no_mmap")]
+    pub mmap: bool,
+    #[arg(long, hide = true, overrides_with = "mmap")]
+    pub no_mmap: bool,
+    /// --mmap and --no-mmap, the last one winning.
+    #[arg(skip)]
+    pub mmap_mode: MmapMode,
+
+    /// Print both matching and non-matching lines.
+    #[arg(long, alias = "passthrough")]
+    pub passthru: bool,
+    /// Print the byte offset for each matching line.
+    #[arg(short = 'b', long, overrides_with = "no_byte_offset")]
+    pub byte_offset: bool,
+    #[arg(long, hide = true, overrides_with = "byte_offset")]
+    pub no_byte_offset: bool,
+    /// Show column numbers.
+    #[arg(long, overrides_with = "no_column")]
+    pub column: bool,
+    #[arg(long, hide = true, overrides_with = "column")]
+    pub no_column: bool,
+    /// Print matches grouped by each file (the tree layout with Trails).
+    #[arg(long)]
+    pub heading: bool,
+    /// Print the file path on every line (the one-line layout with Trails).
+    #[arg(long)]
+    pub no_heading: bool,
+    /// Alias for colors, headings and line numbers.
+    #[arg(short = 'p', long)]
+    pub pretty: bool,
+    /// Print a NUL byte after file paths.
+    #[arg(short = '0', long)]
+    pub null: bool,
+    /// Print only matched parts of a line.
+    #[arg(short = 'o', long)]
+    pub only_matching: bool,
+    /// Replace matches with the given text.
+    #[arg(
+        short = 'r',
+        long,
+        value_name = "REPLACEMENT",
+        allow_hyphen_values = true
+    )]
+    pub replace: Option<String>,
+    /// Trim prefix whitespace from matches.
+    #[arg(long, overrides_with = "no_trim")]
+    pub trim: bool,
+    #[arg(long, hide = true, overrides_with = "trim")]
+    pub no_trim: bool,
+    /// Print results in a vim compatible format.
+    #[arg(long)]
+    pub vimgrep: bool,
+    /// Show preview for lines exceeding the limit.
+    #[arg(long, overrides_with = "no_max_columns_preview")]
+    pub max_columns_preview: bool,
+    #[arg(long, hide = true, overrides_with = "max_columns_preview")]
+    pub no_max_columns_preview: bool,
+    /// Set the path separator for printing paths.
+    #[arg(long, value_name = "SEPARATOR", allow_hyphen_values = true,
+          value_parser = values::path_separator)]
+    pub path_separator: Option<values::PathSeparator>,
+    /// Set the separator for contextual chunks.
+    #[arg(long = "context-separator", value_name = "SEPARATOR", allow_hyphen_values = true,
+          value_parser = values::separator)]
+    pub context_separator_flag: Option<values::Bytes>,
+    #[arg(long, hide = true)]
+    pub no_context_separator: bool,
+    /// The separator between context groups, None with --no-context-separator.
+    #[arg(skip = Some(b"--".to_vec()))]
+    pub context_separator: Option<Vec<u8>>,
+    /// Set the field context separator.
+    #[arg(long, value_name = "SEPARATOR", allow_hyphen_values = true,
+          value_parser = values::separator, default_value = "-")]
+    pub field_context_separator: values::Bytes,
+    /// Set the field match separator.
+    #[arg(long, value_name = "SEPARATOR", allow_hyphen_values = true,
+          value_parser = values::separator, default_value = ":")]
+    pub field_match_separator: values::Bytes,
+    /// Configure color settings and styles.
+    #[arg(long, value_name = "COLOR_SPEC", action = ArgAction::Append,
+          value_parser = values::color_spec)]
+    pub colors: Vec<String>,
+    /// Set the format of hyperlinks.
+    #[arg(long, value_name = "FORMAT", value_parser = values::hyperlink_format)]
+    pub hyperlink_format: Option<String>,
+    /// Run a program to get this system's hostname.
+    #[arg(long, value_name = "COMMAND")]
+    pub hostname_bin: Option<PathBuf>,
+    /// Include zero matches in summary output.
+    #[arg(long, overrides_with = "no_include_zero")]
+    pub include_zero: bool,
+    #[arg(long, hide = true, overrides_with = "include_zero")]
+    pub no_include_zero: bool,
+    /// Print statistics about the search.
+    #[arg(long, overrides_with = "no_stats")]
+    pub stats: bool,
+    #[arg(long, hide = true, overrides_with = "stats")]
+    pub no_stats: bool,
+    /// Force line buffering.
+    #[arg(long)]
+    pub line_buffered: bool,
+    #[arg(long, hide = true)]
+    pub no_line_buffered: bool,
+    /// Force block buffering.
+    #[arg(long)]
+    pub block_buffered: bool,
+    #[arg(long, hide = true)]
+    pub no_block_buffered: bool,
+    /// --line-buffered, --block-buffered and their negations, the last one winning.
+    #[arg(skip)]
+    pub buffer: BufferMode,
+    /// Generate man pages and completion scripts.
+    #[arg(long, value_name = "KIND",
+          value_parser = ["man", "complete-bash", "complete-zsh", "complete-fish", "complete-powershell"])]
+    pub generate: Option<String>,
+    /// Show debug messages.
+    #[arg(long)]
+    pub debug: bool,
+    /// Show trace messages.
+    #[arg(long)]
+    pub trace: bool,
+    /// --debug and --trace, the last one winning.
+    #[arg(skip)]
+    pub logging: Option<Logging>,
+    /// Never read configuration files.
+    #[arg(long)]
+    pub no_config: bool,
+
     // searchfor's own flags (spec §8)
     /// Keep Matches whose Trail contains these Segment names, in order ("A › B", "A > B", "A/B").
     #[arg(long)]
@@ -247,6 +628,9 @@ pub struct Args {
     /// Install a Grammar Helper from the Grammar index (spec §7).
     #[arg(long, value_name = "LANG")]
     pub install_grammar: Option<String>,
+    /// Run as an MCP server over stdin and stdout (spec §9).
+    #[arg(long)]
+    pub mcp: bool,
 }
 
 #[derive(Debug, thiserror::Error)]
@@ -286,6 +670,38 @@ pub fn command() -> clap::Command {
     cmd
 }
 
+/// The command line with the configuration file's arguments before it (`config`), as rg 14.1.1
+/// reads `RIPGREP_CONFIG_PATH` (flags/parse.rs): the command line is parsed alone first, so
+/// `--help`, `--version` and `--pcre2-version` never depend on the file, and `--no-config` is
+/// seen; the file's messages are printed on stderr, prefixed `s4: `.
+pub fn parse_all(argv: Vec<OsString>) -> Result<Args, CliError> {
+    parse_with_config(argv, config::args)
+}
+
+/// `parse_all` with the reader of the configuration file given. `--mcp` reads no file either:
+/// the server and the searches it runs take only what the client asks for (review r1 of the
+/// M5a plan, Fable #1), so a `--generate` or a `--pre` in the file changes neither.
+fn parse_with_config(
+    argv: Vec<OsString>,
+    read: impl FnOnce() -> (Vec<OsString>, Vec<String>),
+) -> Result<Args, CliError> {
+    let alone = parse(argv.clone())?;
+    if alone.no_config || alone.pcre2_version || alone.mcp {
+        return Ok(alone);
+    }
+    let (extra, msgs) = read();
+    for m in msgs {
+        eprintln!("s4: {m}");
+    }
+    if extra.is_empty() {
+        return Ok(alone);
+    }
+    let mut all: Vec<OsString> = argv.iter().take(1).cloned().collect();
+    all.extend(extra);
+    all.extend(argv.into_iter().skip(1));
+    parse(all)
+}
+
 pub fn parse(args: Vec<OsString>) -> Result<Args, CliError> {
     let m = command().try_get_matches_from(args)?;
     for f in rg_flags::RG_FLAGS
@@ -309,21 +725,14 @@ pub fn parse(args: Vec<OsString>) -> Result<Args, CliError> {
         m.indices_of("unrestricted_flags")
             .and_then(|mut it| it.nth(n))
     };
-    a.no_ignore_resolved = last_by_index(&m, &[("no_ignore", true), ("ignore", false)], u(0), true);
     a.hidden_resolved = last_by_index(&m, &[("hidden", true), ("no_hidden", false)], u(1), true);
     a.binary_choice = binary_choice(&m);
     a.docs = !a.no_docs;
     (a.pre, a.search_zip) = pre_and_zip(&m);
-    a.type_selections = type_selections(&m);
     a.pattern_sources = pattern_sources(&m);
+    order::resolve(&m, &mut a, u(0));
     // `-M0` turns the limit off, as in ripgrep 14.1.1 (code review r2, Codex #3).
     a.max_columns = a.max_columns.filter(|&n| n > 0);
-    if let Some(s) = &a.sort
-        && s != "path"
-        && s != "none"
-    {
-        return Err(CliError::NotYet(format!("sort={s}")));
-    }
     if a.no_trail && a.structural() {
         return Err(CliError::Usage(
             "--no-trail cannot be combined with --within, --expand or --in-headings".into(),
@@ -442,18 +851,6 @@ fn binary_choice(m: &clap::ArgMatches) -> BinaryChoice {
     last.map(|(_, c)| c).unwrap_or_default()
 }
 
-/// `-t` and `-T` values, each with whether it selects, sorted by their place on the command line.
-fn type_selections(m: &clap::ArgMatches) -> Vec<(bool, String)> {
-    let mut v: Vec<(usize, bool, String)> = Vec::new();
-    for (id, select) in [("types", true), ("types_not", false)] {
-        if let (Some(vals), Some(idx)) = (m.get_many::<String>(id), m.indices_of(id)) {
-            v.extend(idx.zip(vals).map(|(i, t)| (i, select, t.clone())));
-        }
-    }
-    v.sort_by_key(|e| e.0);
-    v.into_iter().map(|(_, s, t)| (s, t)).collect()
-}
-
 /// One `-e` or `-f`.
 #[derive(Debug, Clone, PartialEq, Eq)]
 pub enum PatternSource {
@@ -567,10 +964,11 @@ impl Args {
     pub fn hidden(&self) -> bool {
         self.hidden_resolved
     }
-    /// --no-ignore after --ignore and the first -u, by position (see `hidden`).
+    /// Every kind of ignore file is off: --no-ignore or the first -u, after --ignore, by
+    /// position, and no --ignore-* after them (`ignores`).
     #[allow(clippy::misnamed_getters)]
     pub fn no_ignore(&self) -> bool {
-        self.no_ignore_resolved
+        self.ignores == IgnoreRules::all(true)
     }
     /// ripgrep's three binary modes for one file: quit at NUL, «binary» (search, stop at NUL
     /// and print the binary message), text.
```

- [ ] **Step 5:** `cargo test --lib -- cli::values cli::config`: PASS (`cargo test` takes one
  filter before `--`; two go after it).

### Task 1.4: the matcher type and the stubs

**Files:** `src/matcher.rs` (new), `src/log.rs` (new), `src/mcp.rs` (new),
`src/cli/generate.rs` (new), `src/output/{hyperlink,replace}.rs` (new), `src/output/mod.rs`,
`src/output/style.rs`, `src/lib.rs`.

- [ ] **Step 1:** `src/matcher.rs`: the enum over both engines, the `Matcher` implementation that
  sends each method to the engine's own, and `build` with the Rust engine, M1–M4's flags and
  the line terminator of `-U`, `--crlf` and `--null-data` (C3's searcher and C4's tests build
  their matchers with them, and grep-searcher refuses a matcher whose terminator is not the
  searcher's), the suggestions of `app.rs` moved here (D19). C2 replaces `build` and
  `pcre2_version`, and adds `Pcre2`:

```rust
//! The pattern matcher of a run (M5a plan, D7): Rust's regex engine, as ripgrep's default, or
//! PCRE2 (`-P`, `--engine`). One type for both, so the searcher, the sinks and the printers
//! take a `&Matcher` whatever the engine; each method goes to the engine's own.
use crate::cli::{Args, Case};
use grep_matcher::{ByteSet, LineMatchKind, LineTerminator, Match, NoError};

/// The matcher of a run.
#[derive(Clone, Debug)]
pub enum PatternMatcher {
    Rust(grep_regex::RegexMatcher),
    Pcre2(grep_pcre2::RegexMatcher),
}

/// The capture groups of one match, of either engine.
#[derive(Clone, Debug)]
pub enum Captures {
    Rust(grep_regex::RegexCaptures),
    Pcre2(grep_pcre2::RegexCaptures),
}

impl grep_matcher::Captures for Captures {
    fn len(&self) -> usize {
        match self {
            Captures::Rust(c) => c.len(),
            Captures::Pcre2(c) => c.len(),
        }
    }

    fn get(&self, i: usize) -> Option<Match> {
        match self {
            Captures::Rust(c) => c.get(i),
            Captures::Pcre2(c) => c.get(i),
        }
    }
}

/// A search error: Rust's engine has none; PCRE2's (a match limit, for one) is reported as an
/// error of the file, as rg reports it.
#[derive(Debug)]
pub enum Error {
    Pcre2(grep_pcre2::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Pcre2(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<NoError> for Error {
    fn from(_: NoError) -> Error {
        unreachable!("Rust's regex engine reports no error")
    }
}

impl grep_matcher::Matcher for PatternMatcher {
    type Captures = Captures;
    type Error = Error;

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.find_at(haystack, at)?),
            PatternMatcher::Pcre2(m) => m.find_at(haystack, at).map_err(Error::Pcre2),
        }
    }

    fn new_captures(&self) -> Result<Captures, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(Captures::Rust(m.new_captures()?)),
            PatternMatcher::Pcre2(m) => m.new_captures().map(Captures::Pcre2).map_err(Error::Pcre2),
        }
    }

    fn capture_count(&self) -> usize {
        match self {
            PatternMatcher::Rust(m) => m.capture_count(),
            PatternMatcher::Pcre2(m) => m.capture_count(),
        }
    }

    fn capture_index(&self, name: &str) -> Option<usize> {
        match self {
            PatternMatcher::Rust(m) => m.capture_index(name),
            PatternMatcher::Pcre2(m) => m.capture_index(name),
        }
    }

    fn captures_at(&self, haystack: &[u8], at: usize, caps: &mut Captures) -> Result<bool, Error> {
        match (self, caps) {
            (PatternMatcher::Rust(m), Captures::Rust(c)) => Ok(m.captures_at(haystack, at, c)?),
            (PatternMatcher::Pcre2(m), Captures::Pcre2(c)) => {
                m.captures_at(haystack, at, c).map_err(Error::Pcre2)
            }
            _ => unreachable!("captures of another engine"),
        }
    }

    fn shortest_match_at(&self, haystack: &[u8], at: usize) -> Result<Option<usize>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.shortest_match_at(haystack, at)?),
            PatternMatcher::Pcre2(m) => m.shortest_match_at(haystack, at).map_err(Error::Pcre2),
        }
    }

    fn non_matching_bytes(&self) -> Option<&ByteSet> {
        match self {
            PatternMatcher::Rust(m) => m.non_matching_bytes(),
            PatternMatcher::Pcre2(m) => m.non_matching_bytes(),
        }
    }

    fn line_terminator(&self) -> Option<LineTerminator> {
        match self {
            PatternMatcher::Rust(m) => m.line_terminator(),
            PatternMatcher::Pcre2(m) => m.line_terminator(),
        }
    }

    fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.find_candidate_line(haystack)?),
            PatternMatcher::Pcre2(m) => m.find_candidate_line(haystack).map_err(Error::Pcre2),
        }
    }
}

/// The matcher the command line asks for (M5a plan, Task 1.4: the Rust engine with the flags
/// of M1–M4; chunk 2 adds PCRE2 and the rest of rg's matcher flags).
pub fn build(args: &Args, patterns: &[String]) -> anyhow::Result<PatternMatcher> {
    let mut b = grep_regex::RegexMatcherBuilder::new();
    match args.case() {
        Case::Insensitive => {
            b.case_insensitive(true);
        }
        Case::Smart => {
            b.case_smart(true);
        }
        Case::Sensitive => {}
    }
    b.word(args.word_regexp)
        .whole_line(args.line_regexp)
        .fixed_strings(args.fixed_strings && !args.no_fixed_strings)
        .multi_line(true); // as ripgrep: ^ and $ match at line boundaries inside a buffer
    // The line terminator agrees with the searcher's (C3), as rg's `matcher_rust` sets it:
    // none with -U, NUL with --null-data, CRLF-aware with --crlf.
    if args.multiline {
        b.dot_matches_new_line(args.multiline_dotall);
        if args.crlf {
            b.crlf(true).line_terminator(None);
        }
    } else {
        b.line_terminator(Some(b'\n'));
        if args.crlf {
            b.crlf(true);
        }
        if args.null_data {
            b.line_terminator(Some(b'\0'));
        }
    }
    // An empty pattern set never reaches here: C9 exits 1 before searching.
    b.build_many(patterns)
        .map(PatternMatcher::Rust)
        .map_err(|e| anyhow::anyhow!(suggestion(e.to_string())))
}

/// The message of a pattern that does not compile, with ripgrep 14.1.1's suggestions when it
/// has them (hiargs.rs, `suggest_multiline` and then `suggest_pcre2` on its result): multiline
/// mode for a literal `\n`, PCRE2 for a look-around or a backreference, both when the message,
/// which repeats the pattern, names both (Fable M4 code r9 #4). s4 has neither yet, as
/// rg-compat.md says, and prints rg's message whole (Fable M4 code r8 #4).
fn suggestion(msg: String) -> String {
    let msg = if msg.contains("the literal") && msg.contains("not allowed") {
        format!(
            "{msg}\n\nConsider enabling multiline mode with the --multiline flag (or -U for \
             short).\nWhen multiline mode is enabled, new line characters can be matched."
        )
    } else {
        msg
    };
    if msg.contains("backreferences") || msg.contains("look-around") {
        format!(
            "{msg}\n\nConsider enabling PCRE2 with the --pcre2 flag, which can handle \
             backreferences\nand look-around."
        )
    } else {
        msg
    }
}

/// What `--pcre2-version` prints (chunk 2).
pub fn pcre2_version() -> String {
    String::new()
}
```

- [ ] **Step 2:** the stubs, with their final signatures (design note 6):

```rust
//! `--debug` and `--trace` (M5a plan, D15): the messages of s4 and of ripgrep's crates on
//! stderr, in ripgrep's format (chunk 5).
use crate::cli::Logging;

/// Starts the logger at the level asked for; nothing is logged without one.
pub fn init(_level: Option<Logging>) {}
```

```rust
//! `s4 --mcp`: an MCP server over stdin and stdout (spec §9; M5a plan, D17; chunk 6).
use std::io::{BufRead, Write};
use std::path::Path;

/// Serves MCP requests from stdin until it closes; `s4` is the binary each search runs.
pub fn serve(_s4: &Path) -> anyhow::Result<()> {
    anyhow::bail!("--mcp is not ready")
}

/// `serve` over any reader and writer, for the tests.
pub fn serve_io(_input: impl BufRead, _output: impl Write, _s4: &Path) -> anyhow::Result<()> {
    anyhow::bail!("--mcp is not ready")
}
```

```rust
//! `--generate`: the man page and the shell completions of s4 (M5a plan, D16; chunk 5).
use std::io::Write;

/// Writes what `kind` names (`man`, `complete-bash`, `complete-zsh`, `complete-fish`,
/// `complete-powershell`) to `w`.
pub fn generate(_kind: &str, _w: &mut dyn Write) -> std::io::Result<()> {
    Ok(())
}
```

```rust
//! `--hyperlink-format` in the Trail layouts (M5a plan, D12): a path, and a line number, as an
//! OSC 8 link on a terminal with colour, the URL built as grep-printer builds it (chunk 4).
use std::path::Path;

/// The run's hyperlink format with the values it reads from the system.
#[derive(Debug)]
pub struct Hyperlinks {}

impl Hyperlinks {
    /// None when the format is empty: no links.
    pub fn new(_format: &str, _hostname_bin: Option<&Path>) -> Option<Hyperlinks> {
        None
    }

    /// The URL of `path` at `line` and `column`, or None when the path cannot be made absolute.
    pub fn url(&self, _path: &Path, _line: Option<u64>, _column: Option<u64>) -> Option<Vec<u8>> {
        None
    }
}
```

```rust
//! `-r`/`--replace` in the Trail layouts (M5a plan, D4): each Match of a line replaced by the
//! replacement, its `$1`, `$name` and `${name}` read from the Match's capture groups, as rg's
//! standard printer replaces them (chunk 4).
use crate::matcher::PatternMatcher;

/// The replacement of a run and the matcher whose groups it reads.
#[derive(Debug)]
pub struct Replacer {
    #[allow(dead_code)]
    matcher: PatternMatcher,
    #[allow(dead_code)]
    template: Vec<u8>,
}

impl Replacer {
    pub fn new(matcher: PatternMatcher, template: Vec<u8>) -> Self {
        Replacer { matcher, template }
    }

    /// `line` with each range of `subs` replaced, and the ranges the replacements take in it.
    pub fn replace(&self, line: &[u8], subs: &[(usize, usize)]) -> (Vec<u8>, Vec<(usize, usize)>) {
        (line.to_vec(), subs.to_vec())
    }
}
```

- [ ] **Step 3:** `Look` in `RenderOptions` (design note 4), and the two new modules of
  `output`:

```diff
diff --git a/src/output/mod.rs b/src/output/mod.rs
index 37e7d39..98fd971 100644
--- a/src/output/mod.rs
+++ b/src/output/mod.rs
@@ -1,7 +1,9 @@
 //! Rendering of Described files (spec §6).
 pub mod flat;
 pub mod fragment;
+pub mod hyperlink;
 pub mod json;
+pub mod replace;
 pub mod stream;
 pub mod style;
 pub mod summary;
@@ -40,6 +42,64 @@ pub struct RenderOptions {
     pub expand: bool,
     /// --binary / --text / ripgrep's default: picks the binary message (design note 3).
     pub binary: BinaryChoice,
+    /// How each row looks: ripgrep's output flags in the Trail layouts (M5a plan, D4).
+    pub look: Look,
+}
+
+/// ripgrep's output flags as the tree and one-line layouts print them (M5a plan, D4). The
+/// default is what M1–M4 print.
+#[derive(Debug, Clone)]
+pub struct Look {
+    /// `--column`: the 1-based column of the row's first Match after its line number.
+    pub column: bool,
+    /// `-b`: the byte offset of the row's line (with `-o`, of its Match) after the column.
+    pub byte_offset: bool,
+    /// `-o`: one row per Match, its text the matched bytes alone.
+    pub only_matching: bool,
+    /// `--vimgrep`: one row per Match, its text the whole line.
+    pub per_match: bool,
+    /// `--trim`: the text without its leading ASCII white space.
+    pub trim: bool,
+    /// `--max-columns-preview`: with `-M`, the start of a long line before the omission.
+    pub max_columns_preview: bool,
+    /// `-r`: the Matches of each line replaced (`replace::Replacer`).
+    pub replace: Option<std::sync::Arc<replace::Replacer>>,
+    /// `--field-match-separator`: after the path and each number of a Match row (`:`).
+    pub sep_match: Vec<u8>,
+    /// `--field-context-separator`: the same for a context row (`-`).
+    pub sep_context: Vec<u8>,
+    /// `--context-separator` (`--`), None with `--no-context-separator`.
+    pub sep_group: Option<Vec<u8>>,
+    /// `-0`: the byte printed after each path in place of the field separator (one-line) or of
+    /// the line break (tree's header).
+    pub path_terminator: Option<u8>,
+    /// `--path-separator`: the byte printed in place of each `/` of a path.
+    pub path_separator: Option<u8>,
+    /// `--colors`: the colours of path, line, column, match, Trail and markers.
+    pub palette: style::Palette,
+    /// `--hyperlink-format`: paths and line numbers as links on a terminal with colour.
+    pub hyperlinks: Option<std::sync::Arc<hyperlink::Hyperlinks>>,
+}
+
+impl Default for Look {
+    fn default() -> Self {
+        Look {
+            column: false,
+            byte_offset: false,
+            only_matching: false,
+            per_match: false,
+            trim: false,
+            max_columns_preview: false,
+            replace: None,
+            sep_match: b":".to_vec(),
+            sep_context: b"-".to_vec(),
+            sep_group: Some(b"--".to_vec()),
+            path_terminator: None,
+            path_separator: None,
+            palette: style::Palette::default(),
+            hyperlinks: None,
+        }
+    }
 }
 
 /// True when -M omits this line: ripgrep compares the whole line, terminator included
@@ -758,6 +818,7 @@ pub(crate) mod fixture {
             trails,
             sections: Vec::new(),
             offsets: false,
+            replacements: Vec::new(),
         }
     }
 }
@@ -777,6 +838,7 @@ pub(crate) mod tests {
             context: false,
             expand: false,
             binary: BinaryChoice::Auto,
+            look: Look::default(),
         }
     }
```

- [ ] **Step 4:** the `Palette` stub, which C4 fills:

```diff
diff --git a/src/output/style.rs b/src/output/style.rs
index f25f0a3..1daa7f9 100644
--- a/src/output/style.rs
+++ b/src/output/style.rs
@@ -1,6 +1,22 @@
 //! Colours of the printers.
 use termcolor::{Color, ColorSpec, WriteColor};
 
+/// The colours of a run: ripgrep's defaults and `--colors` (M5a plan, D11; chunk 4).
+#[derive(Debug, Clone, Default)]
+pub struct Palette {}
+
+impl Palette {
+    /// The palette of the `--colors` specs, in order, over the defaults.
+    pub fn new(_specs: &[String]) -> Palette {
+        Palette {}
+    }
+
+    /// The colour of `kind`: `path`, `line`, `column`, `match`, `trail`, or the markers' dim.
+    pub fn spec(&self, kind: &str) -> ColorSpec {
+        spec(kind)
+    }
+}
+
 /// ripgrep's default colours (path magenta, line number green, match bold red), plus cyan
 /// Trails and dim markers (grill decision 8381).
 pub fn spec(kind: &str) -> ColorSpec {
```

- [ ] **Step 5:** the modules:

```diff
diff --git a/src/lib.rs b/src/lib.rs
index 10983d9..0191a7f 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -6,6 +6,9 @@ pub mod filter;
 pub mod formats;
 pub mod grammars;
 pub mod helper;
+pub mod log;
+pub mod matcher;
+pub mod mcp;
 pub mod model;
 pub mod output;
 pub mod preprocess;
```

### Task 1.5: the callers

**Files:** `src/search.rs`, `src/app.rs`, `src/output/stream.rs`, `src/model.rs`,
`src/filter.rs`, `tests/common/mod.rs`, `tests/cli_m4.rs`, `tests/perf.rs`.

- [ ] **Step 1:** `src/search.rs`: the matcher through `matcher::build`, `PatternMatcher` for
  `RegexMatcher`, the type changes (`--type-add` and `--type-clear` in order with `-t` and `-T`,
  D8) and the resolved `--sort` (any sort searches with one thread, as rg):

```diff
diff --git a/src/search.rs b/src/search.rs
index 0790796..f90f79a 100644
--- a/src/search.rs
+++ b/src/search.rs
@@ -1,10 +1,10 @@
 //! Walking and searching with ripgrep's crates (ADR 0001, design notes 3 and 4).
-use crate::cli::{Args, BinaryMode, Case};
+use crate::cli::{Args, BinaryMode, SortKind, SortMode, TypeChange};
+use crate::matcher::PatternMatcher;
 use crate::model::{FileMatches, FileStats, Item, Line, Submatch};
 use crate::source::Source;
 use anyhow::Result;
 use grep_matcher::Matcher;
-use grep_regex::{RegexMatcher, RegexMatcherBuilder};
 use grep_searcher::{
     BinaryDetection, MmapChoice, Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind,
     SinkFinish, SinkMatch,
@@ -13,24 +13,9 @@ use ignore::{WalkBuilder, overrides::OverrideBuilder, types::TypesBuilder};
 use std::ops::Range;
 use std::path::{Path, PathBuf};
 
-pub fn matcher(args: &Args, patterns: &[String]) -> Result<RegexMatcher> {
-    let mut b = RegexMatcherBuilder::new();
-    match args.case() {
-        Case::Insensitive => {
-            b.case_insensitive(true);
-        }
-        Case::Smart => {
-            b.case_smart(true);
-        }
-        Case::Sensitive => {}
-    }
-    b.word(args.word_regexp)
-        .whole_line(args.line_regexp)
-        .fixed_strings(args.fixed_strings && !args.no_fixed_strings)
-        .multi_line(true) // as ripgrep: ^ and $ match at line boundaries inside a buffer
-        .line_terminator(Some(b'\n'));
-    // An empty pattern set never reaches here: C9 exits 1 before searching.
-    Ok(b.build_many(patterns)?)
+/// The run's matcher (`matcher::build`).
+pub fn matcher(args: &Args, patterns: &[String]) -> Result<PatternMatcher> {
+    crate::matcher::build(args, patterns)
 }
 
 /// ripgrep 14.1.1's memory-map choice, which is also its in-memory vs reader choice: the
@@ -131,12 +116,20 @@ pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder>
     }
     // The file types before the globs, as ripgrep 14.1.1 validates them (code review r3).
     let mut tb = rg_types()?;
-    // In command-line order, as ripgrep: the last selection of a type wins.
-    for (select, t) in &args.type_selections {
-        if *select {
-            tb.select(t);
-        } else {
-            tb.negate(t);
+    // In command-line order, as ripgrep (hiargs.rs `types`): the last selection of a type wins,
+    // and --type-add and --type-clear change the types the selections after them see.
+    for change in &args.type_changes {
+        match change {
+            TypeChange::Select(t) => {
+                tb.select(t);
+            }
+            TypeChange::Negate(t) => {
+                tb.negate(t);
+            }
+            TypeChange::Add(def) => tb.add_def(def)?,
+            TypeChange::Clear(t) => {
+                tb.clear(t);
+            }
         }
     }
     w.types(tb.build()?);
@@ -153,17 +146,23 @@ pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder>
         }
     }
     w.overrides(ob.build()?);
-    if args.sort.as_deref() == Some("path") {
+    if args.sort == Some(PATH) {
         w.sort_by_file_path(|a, b| a.cmp(b));
     }
     w.threads(walker_threads(args));
     Ok(w)
 }
 
-/// Whether ripgrep 14.1.1 searches with one thread (hiargs.rs:167): under `--sort path`, with
-/// one file, or when the walker's threads are one.
+/// `--sort path`, the order the walker itself can give.
+const PATH: SortMode = SortMode {
+    kind: SortKind::Path,
+    reverse: false,
+};
+
+/// Whether ripgrep 14.1.1 searches with one thread (hiargs.rs:167): under any `--sort` or
+/// `--sortr`, with one file, or when the walker's threads are one.
 pub fn one_thread(args: &Args, one_file: bool) -> bool {
-    args.sort.as_deref() == Some("path") || one_file || walker_threads(args) == 1
+    args.sort.is_some() || one_file || walker_threads(args) == 1
 }
 
 /// The walker's threads: `-j N`, or ripgrep 14.1.1's default when `-j` is absent or 0, the
@@ -287,7 +286,7 @@ impl MaxCount {
 /// ranges of the Source) except after binary data in a reader search, whose NUL conversion makes
 /// them differ from the Source (`reader`).
 pub struct CollectSink<'m> {
-    matcher: &'m RegexMatcher,
+    matcher: &'m PatternMatcher,
     invert: bool,
     /// `-q` without structural flags: stop the file at its first Match (design note 10).
     quiet: bool,
@@ -299,7 +298,7 @@ pub struct CollectSink<'m> {
 }
 
 impl<'m> CollectSink<'m> {
-    pub fn new(matcher: &'m RegexMatcher, invert: bool, quiet: bool) -> Self {
+    pub fn new(matcher: &'m PatternMatcher, invert: bool, quiet: bool) -> Self {
         Self {
             matcher,
             invert,
@@ -361,14 +360,14 @@ pub fn number_lines(items: &mut [Item], bytes: &[u8]) {
 /// `range.start`, so `\A` sees what ripgrep's printer sees (`bar\nfoo needle\n` with
 /// `\Afoo|needle` reports only `needle`). Matches starting at or after `range.end` stop
 /// the search; offsets are relative to `range.start`.
-pub fn submatches(matcher: &RegexMatcher, buffer: &[u8], range: Range<usize>) -> Vec<Submatch> {
+pub fn submatches(matcher: &PatternMatcher, buffer: &[u8], range: Range<usize>) -> Vec<Submatch> {
     let mut end = range.end;
     if buffer[..end].ends_with(b"\n") {
         end -= 1;
     }
     let haystack = &buffer[..end];
     let mut subs = Vec::new();
-    // RegexMatcher's error type is NoError: the search cannot fail.
+    // PatternMatcher's error type is NoError: the search cannot fail.
     let _ = matcher.find_iter_at(haystack, range.start, |m| {
         if m.start() >= range.end {
             return false;
@@ -392,7 +391,11 @@ pub fn submatches(matcher: &RegexMatcher, buffer: &[u8], range: Range<usize>) ->
 
 /// The submatches of a reported line as ripgrep's printers record them: none on an inverted
 /// Match; on a context line, only with `-v`, searched in the line's own bytes.
-pub fn match_submatches(matcher: &RegexMatcher, invert: bool, m: &SinkMatch<'_>) -> Vec<Submatch> {
+pub fn match_submatches(
+    matcher: &PatternMatcher,
+    invert: bool,
+    m: &SinkMatch<'_>,
+) -> Vec<Submatch> {
     // grep-printer records matches on inverted lines too; none are found there.
     if invert {
         Vec::new()
@@ -401,7 +404,7 @@ pub fn match_submatches(matcher: &RegexMatcher, invert: bool, m: &SinkMatch<'_>)
     }
 }
 
-pub fn context_submatches(matcher: &RegexMatcher, invert: bool, bytes: &[u8]) -> Vec<Submatch> {
+pub fn context_submatches(matcher: &PatternMatcher, invert: bool, bytes: &[u8]) -> Vec<Submatch> {
     // grep-printer's context path searches the line's own bytes (range 0..len).
     if invert {
         submatches(matcher, bytes, 0..bytes.len())
@@ -569,7 +572,7 @@ impl<R> Reads<R> {
 /// decide (code review r9, Codex #1).
 pub fn binary_probe(
     searcher: &mut Searcher,
-    matcher: &RegexMatcher,
+    matcher: &PatternMatcher,
     reader: impl std::io::Read,
 ) -> Result<BinarySink> {
     let mut sink = BinarySink::default();
@@ -607,7 +610,7 @@ pub fn binary_probe(
 /// result has a binary offset, C9 skips the Processor.
 pub fn search_source(
     searcher: &mut Searcher,
-    matcher: &RegexMatcher,
+    matcher: &PatternMatcher,
     src: &Source,
     in_memory: bool,
     quiet: bool,
@@ -620,7 +623,7 @@ pub fn search_source(
 /// summary of `--json -q`).
 pub fn search_source_stats(
     searcher: &mut Searcher,
-    matcher: &RegexMatcher,
+    matcher: &PatternMatcher,
     src: &Source,
     in_memory: bool,
     limit: Option<u64>,
@@ -630,7 +633,7 @@ pub fn search_source_stats(
 
 fn search_with(
     searcher: &mut Searcher,
-    matcher: &RegexMatcher,
+    matcher: &PatternMatcher,
     src: &Source,
     in_memory: bool,
     quiet: bool,
```

- [ ] **Step 2:** `src/app.rs`: `parse_all`, the suggestions gone to `matcher::build`,
  `PatternMatcher`, `Look::default()` in the Trail options (C9 builds it), and the new field of
  `Described` (Step 3b) empty in its literals:

```diff
diff --git a/src/app.rs b/src/app.rs
index 918f44e..91485b0 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -4,6 +4,7 @@ use crate::cli::{self, Args, CliError, ColorWhen};
 use crate::docs;
 use crate::filter;
 use crate::formats::{self, Registry, detector::Detector};
+use crate::matcher::PatternMatcher;
 use crate::model::{Described, FileMatches, FileStats, Input, Item, Line, Name, Submatch, Trail};
 use crate::output::{
     self, Layout, RenderOptions,
@@ -22,7 +23,6 @@ use grep_printer::{
     ColorSpecs, JSON, JSONBuilder, Standard, StandardBuilder, Stats, Summary, SummaryBuilder,
     SummaryKind,
 };
-use grep_regex::RegexMatcher;
 use grep_searcher::{BinaryDetection, Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
 use ignore::{DirEntry, WalkBuilder, WalkState};
 use std::ffi::OsString;
@@ -35,7 +35,7 @@ use std::time::Instant;
 use termcolor::{Buffer, BufferWriter, ColorChoice, WriteColor};
 
 pub fn run(argv: Vec<OsString>) -> i32 {
-    let args = match cli::parse(argv) {
+    let args = match cli::parse_all(argv) {
         Ok(a) => a,
         Err(CliError::Clap(e)) => {
             let _ = e.print();
@@ -86,30 +86,6 @@ fn stopped() -> bool {
     crate::helper::host::cancel::stopped()
 }
 
-/// The message of a pattern that does not compile, with ripgrep 14.1.1's suggestions when it
-/// has them (hiargs.rs, `suggest_multiline` and then `suggest_pcre2` on its result): multiline
-/// mode for a literal `\n`, PCRE2 for a look-around or a backreference, both when the message,
-/// which repeats the pattern, names both (Fable M4 code r9 #4). s4 has neither yet, as
-/// rg-compat.md says, and prints rg's message whole (Fable M4 code r8 #4).
-fn suggestion(msg: String) -> String {
-    let msg = if msg.contains("the literal") && msg.contains("not allowed") {
-        format!(
-            "{msg}\n\nConsider enabling multiline mode with the --multiline flag (or -U for \
-             short).\nWhen multiline mode is enabled, new line characters can be matched."
-        )
-    } else {
-        msg
-    };
-    if msg.contains("backreferences") || msg.contains("look-around") {
-        format!(
-            "{msg}\n\nConsider enabling PCRE2 with the --pcre2 flag, which can handle \
-             backreferences\nand look-around."
-        )
-    } else {
-        msg
-    }
-}
-
 /// The built-in Definitions, those of the installed Grammar Helpers and the user's (D26).
 fn load_registry(msgs: &Messages, user_dir: Option<&Path>) -> Registry {
     let (registry, warnings) = Registry::load_all(
@@ -256,8 +232,8 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         }
         return Ok(1);
     }
-    let matcher = search::matcher(args, &pats.patterns)
-        .map_err(|e| anyhow::anyhow!(suggestion(e.to_string())))?;
+    // The error carries rg 14.1.1's suggestions (`matcher::build`).
+    let matcher = search::matcher(args, &pats.patterns)?;
     let one_file = paths.len() == 1 && (paths[0] == Path::new("-") || !paths[0].is_dir());
     let with_filename = if args.with_filename {
         true
@@ -431,6 +407,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
                 context,
                 expand: args.expand,
                 binary: args.binary_choice,
+                look: output::Look::default(),
             };
             ctx.walk(
                 || TrailState {
@@ -642,7 +619,7 @@ enum Printer<'a> {
 
 struct Ctx<'a> {
     args: &'a Args,
-    matcher: RegexMatcher,
+    matcher: PatternMatcher,
     msgs: Messages,
     paths: Vec<PathBuf>,
     /// Walks `paths`, with the file types and the globs (`search::walker`).
@@ -1829,6 +1806,7 @@ impl<'a> Ctx<'a> {
                     format: Some(name.clone()),
                     sections: Vec::new(),
                     offsets: false,
+                    replacements: Vec::new(),
                 };
                 return self.filter(st, t, src, d);
             }
@@ -1871,6 +1849,7 @@ impl<'a> Ctx<'a> {
                     trails,
                     sections: Vec::new(),
                     offsets: false,
+                    replacements: Vec::new(),
                 };
                 (src, d)
             })
@@ -2008,6 +1987,7 @@ impl<'a> Ctx<'a> {
             trails,
             sections: Vec::new(),
             offsets: true,
+            replacements: Vec::new(),
         }))
     }
 
@@ -2173,6 +2153,7 @@ impl<'a> Ctx<'a> {
                 file,
                 format: Some(format),
                 offsets: false,
+                replacements: Vec::new(),
                 trails,
                 // With --expand, the Section of each named Segment (code review r1, Fable #4).
                 sections: if self.args.expand {
@@ -2433,7 +2414,7 @@ impl<'a> Ctx<'a> {
         for n in names {
             // --expand prints the Section of the named Segment.
             let section = n.trail.last().map(|s| (s.line, s.end_line));
-            // RegexMatcher's error type is NoError: the search cannot fail.
+            // PatternMatcher's error type is NoError: the search cannot fail.
             let _ = self.matcher.find_iter(n.text.as_bytes(), |m| {
                 // One Match over contiguous pieces (`a&lt;b`, a replaced byte between letters)
                 // is one hit: its Source ranges are joined where they touch, never across two
@@ -2613,7 +2594,7 @@ type HeadingLine = (u64, Vec<(usize, usize)>, Vec<Trail>, Vec<Section>);
 /// (the Processor) only when it found one. None: no Match.
 fn if_matched<T>(
     searcher: &mut Searcher,
-    matcher: &RegexMatcher,
+    matcher: &PatternMatcher,
     invert: bool,
     bytes: &[u8],
     then: impl FnOnce() -> T,
@@ -2970,7 +2951,7 @@ impl WriteColor for SpillLines<'_> {
 
 /// The counting part of grep-printer 0.2.2's summary sink with statistics, for `--json -q`.
 struct StatsSink<'m> {
-    matcher: &'m RegexMatcher,
+    matcher: &'m PatternMatcher,
     invert: bool,
     max: MaxCount,
     stats: FileStats,
@@ -3034,7 +3015,7 @@ mod tests {
         assert!(!builds_outline(processors::by_name("keypath").unwrap()));
     }
 
-    fn searcher_for(argv: &[&str]) -> (Searcher, RegexMatcher, bool) {
+    fn searcher_for(argv: &[&str]) -> (Searcher, PatternMatcher, bool) {
         let mut v = vec!["s4"];
         v.extend_from_slice(argv);
         let a = cli::parse(v.iter().map(Into::into).collect()).unwrap();
```

- [ ] **Step 3:** `src/output/stream.rs`: `PatternMatcher`, and the literal of its test:

```diff
diff --git a/src/output/stream.rs b/src/output/stream.rs
index 4ac6160..aaeb898 100644
--- a/src/output/stream.rs
+++ b/src/output/stream.rs
@@ -5,10 +5,10 @@
 //! Described file.
 use super::json::{JsonFile, JsonOptions, RunStats};
 use super::{Layout, LineText, RenderOptions, Row, flat, tree};
+use crate::matcher::PatternMatcher;
 use crate::model::{FileStats, Line, Segment, Submatch};
 use crate::search::{MaxCount, context_submatches, match_submatches};
 use crate::trail::{Outline, TrailCursor};
-use grep_regex::RegexMatcher;
 use grep_searcher::{Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
 use std::path::Path;
 use std::rc::Rc;
@@ -43,7 +43,7 @@ struct Pending {
 
 pub struct StreamSink<'a, W: WriteColor> {
     w: &'a mut W,
-    matcher: &'a RegexMatcher,
+    matcher: &'a PatternMatcher,
     invert: bool,
     o: &'a RenderOptions,
     file: StreamFile<'a>,
@@ -79,7 +79,7 @@ pub struct StreamSink<'a, W: WriteColor> {
 impl<'a, W: WriteColor> StreamSink<'a, W> {
     pub fn new(
         w: &'a mut W,
-        matcher: &'a RegexMatcher,
+        matcher: &'a PatternMatcher,
         invert: bool,
         o: &'a RenderOptions,
         jo: &'a JsonOptions,
@@ -486,6 +486,7 @@ mod tests {
             context: a.before() > 0 || a.after() > 0,
             expand: false,
             binary: a.binary_choice,
+            look: crate::output::Look::default(),
         };
         let jo = JsonOptions {
             line_numbers: ro.line_numbers,
@@ -561,6 +562,7 @@ mod tests {
                 trails,
                 sections: Vec::new(),
                 offsets: false,
+                replacements: Vec::new(),
             };
             match layout {
                 Layout::Flat => {
```

- [ ] **Step 3b:** `Described::replacements`, an object file's `-r`: C9's search of the file
  fills it, one pass over each line, and C4's `sunk` prints it (D4; review r4, Codex #1 and
  #2). Every literal of `Described` gets it empty, the fixtures of `output` (Task 1.4, Step 3)
  and a test of `filter` too:

```diff
diff --git a/src/model.rs b/src/model.rs
index 84a9889..6f27e0d 100644
--- a/src/model.rs
+++ b/src/model.rs
@@ -137,6 +137,11 @@ pub struct Described {
     /// The rows are an object file's printable strings (spec §5.7): tree and one-line output
     /// print each at the hex offset of its first Match, where they print a line number.
     pub offsets: bool,
+    /// An object file's `-r` (M5a plan, D4): each Match of the search, by its range in the
+    /// Source, with its replacement, made in one pass over the Match's line as grep-printer's
+    /// `Replacer` makes it, so with the groups the search found (review r4 of the plan, Codex
+    /// #1 and #2). Empty otherwise.
+    pub replacements: Vec<((usize, usize), Vec<u8>)>,
 }
 
 /// What a Processor receives about one file (design note 6). External Processors (M5) need
```

```diff
diff --git a/src/filter.rs b/src/filter.rs
index 5ef347e..c262aaf 100644
--- a/src/filter.rs
+++ b/src/filter.rs
@@ -270,6 +270,7 @@ mod tests {
             trails,
             sections: vec![],
             offsets: false,
+            replacements: Vec::new(),
         }
     }
```

- [ ] **Step 4:** no test reads a configuration file of the user's (D3): every s4 the tests run
  has `SEARCHFOR_CONFIG_PATH` removed, as `RIPGREP_CONFIG_PATH`:

```diff
diff --git a/tests/common/mod.rs b/tests/common/mod.rs
index c617cd8..f7ee3f8 100644
--- a/tests/common/mod.rs
+++ b/tests/common/mod.rs
@@ -55,6 +55,7 @@ impl Fixture {
             .env("SEARCHFOR_FORMATS_DIR", self.formats.path())
             .env("SEARCHFOR_GRAMMARS_DIR", self.grammars.path())
             .env("SEARCHFOR_GRAMMAR_INDEX", self.no_index())
+            .env_remove("SEARCHFOR_CONFIG_PATH")
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(Stdio::null());
         let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
```

```diff
diff --git a/tests/cli_m4.rs b/tests/cli_m4.rs
index c8ec452..2c50512 100644
--- a/tests/cli_m4.rs
+++ b/tests/cli_m4.rs
@@ -992,6 +992,7 @@ fn m4_17_the_readers_flag_is_no_pattern() {
         std::process::Command::new(assert_cmd::cargo::cargo_bin("s4"))
             .args(["__extract-document", "m4/words.txt", "--no-trail"])
             .current_dir(fx.path())
+            .env_remove("SEARCHFOR_CONFIG_PATH")
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(std::process::Stdio::null()),
     );
@@ -1001,6 +1002,7 @@ fn m4_17_the_readers_flag_is_no_pattern() {
         std::process::Command::new(assert_cmd::cargo::cargo_bin("s4"))
             .args(["--__extract-document", "m4/words.txt"])
             .current_dir(fx.path())
+            .env_remove("SEARCHFOR_CONFIG_PATH")
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(std::process::Stdio::null()),
     );
```

```diff
diff --git a/tests/perf.rs b/tests/perf.rs
index a6c44bf..e6a8a85 100644
--- a/tests/perf.rs
+++ b/tests/perf.rs
@@ -122,6 +122,7 @@ fn measure_to(
         .env("SEARCHFOR_FORMATS_DIR", fx.formats.path())
         .env("SEARCHFOR_GRAMMARS_DIR", fx.grammars.path())
         .env("SEARCHFOR_GRAMMAR_INDEX", fx.no_index())
+        .env_remove("SEARCHFOR_CONFIG_PATH")
         .env_remove("RIPGREP_CONFIG_PATH")
         .stdin(match stdin {
             None => Stdio::null(),
@@ -538,6 +539,7 @@ fn sfor_output(fx: &Fixture, args: &[&str]) -> (String, String) {
             .env("SEARCHFOR_FORMATS_DIR", fx.formats.path())
             .env("SEARCHFOR_GRAMMARS_DIR", fx.grammars.path())
             .env("SEARCHFOR_GRAMMAR_INDEX", fx.no_index())
+            .env_remove("SEARCHFOR_CONFIG_PATH")
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(Stdio::null()),
     );
@@ -1503,6 +1505,7 @@ fn a_zip_bomb_is_refused_within_its_limit() {
         Command::new(sfor_bin())
             .args(args)
             .current_dir(fx.path())
+            .env_remove("SEARCHFOR_CONFIG_PATH")
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(Stdio::null()),
     );
@@ -1644,6 +1647,7 @@ fn refuses_the_bomb(name: &str, bytes: &[u8], extra_mib: u64, warning: &str, cod
         Command::new(sfor_bin())
             .args(args)
             .current_dir(fx.path())
+            .env_remove("SEARCHFOR_CONFIG_PATH")
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(Stdio::null()),
     );
@@ -1836,6 +1840,7 @@ fn a_pdf_past_the_readers_memory_is_refused_within_its_limit() {
         Command::new(sfor_bin())
             .args(args)
             .current_dir(fx.path())
+            .env_remove("SEARCHFOR_CONFIG_PATH")
             .env_remove("RIPGREP_CONFIG_PATH")
             .stdin(Stdio::null()),
     );
@@ -1857,6 +1862,7 @@ fn command(bin: &Path, fx: &Fixture, args: &[&str]) -> Command {
         .env("SEARCHFOR_FORMATS_DIR", fx.formats.path())
         .env("SEARCHFOR_GRAMMARS_DIR", fx.grammars.path())
         .env("SEARCHFOR_GRAMMAR_INDEX", fx.no_index())
+        .env_remove("SEARCHFOR_CONFIG_PATH")
         .env_remove("RIPGREP_CONFIG_PATH")
         .stdin(Stdio::null())
         .stdout(Stdio::null())
```

### Task 1.6: every flag implemented

**Files:** `src/cli/rg_flags.rs`, `docs/rg-compat.md`.

- [ ] **Step 1:** every entry of `RG_FLAGS` is implemented:

```bash
sed -i 's/status: NotYet }/status: Implemented }/' src/cli/rg_flags.rs
sed -i 's/^use Status::{Implemented, NotYet};$/use Status::Implemented;/' src/cli/rg_flags.rs
```

  `Status::NotYet` and `command()`'s hidden arguments stay: they are the mechanism for a flag a
  later ripgrep adds.
- [ ] **Step 2:** `cargo test --lib rg_compat_doc -- --ignored` regenerates `docs/rg-compat.md`;
  `git diff docs/rg-compat.md` shows the 90 rows turned to `yes` and nothing else.

### Task 1.7: green, and the commit

- [ ] **Step 1:** `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo fmt --check`, `SEARCHFOR_REQUIRE_REFERENCE=1 cargo test --workspace`: PASS
  (the counts of «Verified for this plan»).
- [ ] **Step 2:** commit: `Parse every ripgrep flag, in rg's order, with a configuration file`.

---
## Chunk 2: the matcher

### Task 2.1: the tests

**Files:** `src/matcher.rs` (tests only).

- [ ] **Step 1:** the tests module at the end of `src/matcher.rs` (D7, D19). The expected messages
  are rg 14.1.1's, measured: `rg -P '('` says `offset 5` (grep-pcre2 wraps each pattern in a
  group), `rg --null-data 'a\x00'` refuses the literal NUL with the multi-line suggestion,
  `rg 'a\x00b'` refuses a NUL with the `-a` suggestion:

```diff
diff --git a/src/matcher.rs b/src/matcher.rs
index 68eea6c..33d078a 100644
--- a/src/matcher.rs
+++ b/src/matcher.rs
@@ -161,37 +231,272 @@ pub fn build(args: &Args, patterns: &[String]) -> anyhow::Result<PatternMatcher>
             b.line_terminator(Some(b'\0'));
         }
     }
-    // An empty pattern set never reaches here: C9 exits 1 before searching.
+    if let Some(n) = args.regex_size_limit {
+        b.size_limit(n);
+    }
+    if let Some(n) = args.dfa_size_limit {
+        b.dfa_size_limit(n);
+    }
+    if detects_binary(args) {
+        b.ban_byte(Some(b'\0'));
+    }
     b.build_many(patterns)
         .map(PatternMatcher::Rust)
-        .map_err(|e| anyhow::anyhow!(suggestion(e.to_string())))
+        .map_err(|e| suggest_text(suggest_multiline(e.to_string())))
+}
+
+/// PCRE2 (hiargs.rs `matcher_pcre2`): JIT on 64-bit systems with a 10 MiB stack, Unicode
+/// unless `--no-unicode`.
+fn pcre2(args: &Args, patterns: &[String]) -> Result<PatternMatcher, String> {
+    let mut b = grep_pcre2::RegexMatcherBuilder::new();
+    b.multi_line(true)
+        .fixed_strings(args.fixed_strings && !args.no_fixed_strings);
+    match args.case() {
+        Case::Insensitive => b.caseless(true),
+        Case::Smart => b.case_smart(true),
+        Case::Sensitive => b.caseless(false),
+    };
+    if args.line_regexp {
+        b.whole_line(true);
+    } else if args.word_regexp {
+        b.word(true);
+    }
+    if cfg!(target_pointer_width = "64") {
+        b.jit_if_available(true).max_jit_stack_size(Some(10 << 20));
+    }
+    if !args.no_unicode {
+        b.utf(true).ucp(true);
+    }
+    if args.multiline {
+        b.dotall(args.multiline_dotall);
+    }
+    if args.crlf {
+        b.crlf(true);
+    }
+    b.build_many(patterns)
+        .map(|m| PatternMatcher::Pcre2(Pcre2::new(m)))
+        .map_err(|e| e.to_string())
+}
+
+/// rg bans a NUL in the pattern unless binary detection is off everywhere: `-a`, or
+/// `--null-data` (hiargs.rs `BinaryDetection::is_none`).
+fn detects_binary(args: &Args) -> bool {
+    args.binary_choice != crate::cli::BinaryChoice::Text && !args.null_data
+}
+
+/// rg's `suggest_pcre2`, applied to the default engine's errors (`suggest_other_engine`).
+fn suggest_pcre2(msg: String) -> String {
+    if msg.contains("backreferences") || msg.contains("look-around") {
+        format!(
+            "{msg}\n\nConsider enabling PCRE2 with the --pcre2 flag, which can handle \
+             backreferences\nand look-around."
+        )
+    } else {
+        msg
+    }
 }
 
-/// The message of a pattern that does not compile, with ripgrep 14.1.1's suggestions when it
-/// has them (hiargs.rs, `suggest_multiline` and then `suggest_pcre2` on its result): multiline
-/// mode for a literal `\n`, PCRE2 for a look-around or a backreference, both when the message,
-/// which repeats the pattern, names both (Fable M4 code r9 #4). s4 has neither yet, as
-/// rg-compat.md says, and prints rg's message whole (Fable M4 code r8 #4).
-fn suggestion(msg: String) -> String {
-    let msg = if msg.contains("the literal") && msg.contains("not allowed") {
+/// rg's `suggest_multiline`: a literal line terminator in the pattern.
+fn suggest_multiline(msg: String) -> String {
+    if msg.contains("the literal") && msg.contains("not allowed") {
         format!(
             "{msg}\n\nConsider enabling multiline mode with the --multiline flag (or -U for \
              short).\nWhen multiline mode is enabled, new line characters can be matched."
         )
     } else {
         msg
-    };
-    if msg.contains("backreferences") || msg.contains("look-around") {
+    }
+}
+
+/// rg's `suggest_text`: a NUL in the pattern while binary data is detected.
+fn suggest_text(msg: String) -> String {
+    if msg.contains("pattern contains \"\\0\"") {
         format!(
-            "{msg}\n\nConsider enabling PCRE2 with the --pcre2 flag, which can handle \
-             backreferences\nand look-around."
+            "{msg}\n\nConsider enabling text mode with the --text flag (or -a for short). \
+             Otherwise,\nbinary detection is enabled and matching a NUL byte is impossible."
         )
     } else {
         msg
     }
 }
 
-/// What `--pcre2-version` prints (chunk 2).
+/// What `--pcre2-version` prints: PCRE2's version and whether its JIT is available, as rg
+/// 14.1.1's `special` prints them.
 pub fn pcre2_version() -> String {
-    String::new()
+    let (major, minor) = pcre2::version();
+    let jit = if pcre2::is_jit_available() {
+        "JIT is available"
+    } else {
+        "JIT is unavailable"
+    };
+    format!("PCRE2 {major}.{minor} is available ({jit})")
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+    use grep_matcher::Matcher as _;
+
+    fn args(v: &[&str]) -> Args {
+        let mut argv = vec!["s4"];
+        argv.extend_from_slice(v);
+        crate::cli::parse(argv.iter().map(Into::into).collect()).unwrap()
+    }
+
+    fn find(argv: &[&str], pat: &str, hay: &[u8]) -> Option<(usize, usize)> {
+        let m = build(&args(argv), &[pat.to_string()]).unwrap();
+        m.find(hay).unwrap().map(|m| (m.start(), m.end()))
+    }
+
+    fn error(argv: &[&str], pat: &str) -> String {
+        build(&args(argv), &[pat.to_string()])
+            .unwrap_err()
+            .to_string()
+    }
+
+    #[test]
+    fn the_engine_the_flags_choose() {
+        assert!(matches!(
+            build(&args(&["x"]), &["x".into()]).unwrap(),
+            PatternMatcher::Rust(_)
+        ));
+        assert!(matches!(
+            build(&args(&["-P", "x"]), &["x".into()]).unwrap(),
+            PatternMatcher::Pcre2(_)
+        ));
+        // auto: Rust's when it compiles, PCRE2's for look-around.
+        let auto = |p: &str| build(&args(&["--engine", "auto", "x"]), &[p.into()]).unwrap();
+        assert!(matches!(auto("a+"), PatternMatcher::Rust(_)));
+        assert!(matches!(auto("a(?=b)"), PatternMatcher::Pcre2(_)));
+    }
+
+    #[test]
+    fn pcre2_matches_what_rust_cannot() {
+        assert_eq!(
+            find(&["-P", "x"], r"needle(?= two)", b"a needle two"),
+            Some((2, 8))
+        );
+        assert_eq!(find(&["-P", "x"], r"(\w)\1", b"abccd"), Some((2, 4)));
+        assert_eq!(
+            find(&["-P", "-i", "x"], "NEEDLE", b"a needle"),
+            Some((2, 8))
+        );
+        assert_eq!(
+            find(&["-P", "-w", "x"], "need", b"needle need"),
+            Some((7, 11))
+        );
+        assert_eq!(
+            find(&["-P", "-x", "x"], "need", b"needle\nneed\n"),
+            Some((7, 11))
+        );
+        // Unicode classes unless --no-unicode.
+        assert_eq!(find(&["-P", "x"], r"\w+", "ñandú".as_bytes()), Some((0, 7)));
+        assert_eq!(
+            find(&["-P", "--no-unicode", "x"], r"^\w+", "ñandú".as_bytes()),
+            None
+        );
+    }
+
+    /// Each thread matches with its own clone of a PCRE2 matcher, which the next matcher of the
+    /// thread replaces: two matchers in one thread keep their own patterns, and threads give
+    /// what one thread gives.
+    #[test]
+    fn pcre2_clones_per_thread() {
+        let a = build(&args(&["-P", "x"]), &["a(?=b)".into()]).unwrap();
+        let b = build(&args(&["-P", "x"]), &["b(?=c)".into()]).unwrap();
+        for _ in 0..2 {
+            assert_eq!(a.find(b"xabc").unwrap().map(|m| m.start()), Some(1));
+            assert_eq!(b.find(b"xabc").unwrap().map(|m| m.start()), Some(2));
+        }
+        std::thread::scope(|s| {
+            for _ in 0..4 {
+                s.spawn(|| {
+                    for _ in 0..100 {
+                        assert_eq!(a.find(b"xxab").unwrap().map(|m| m.start()), Some(2));
+                    }
+                });
+            }
+        });
+    }
+
+    #[test]
+    fn rust_engine_flags() {
+        // --no-unicode: \w is ASCII, and a non-ASCII byte is matched only by its bytes.
+        assert_eq!(
+            find(&["--no-unicode", "x"], r"\w+", "ñx".as_bytes()),
+            Some((2, 3))
+        );
+        // --crlf: $ before \r\n.
+        assert_eq!(
+            find(&["--crlf", "x"], r"needle$", b"needle\r\n"),
+            Some((0, 6))
+        );
+        assert_eq!(find(&["x"], r"needle$", b"needle\r\n"), None);
+        // -U: \n in a pattern, and --multiline-dotall makes `.` cross lines.
+        assert_eq!(find(&["-U", "x"], r"a\nb", b"a\nb"), Some((0, 3)));
+        assert_eq!(find(&["-U", "x"], r"a.b", b"a\nb"), None);
+        assert_eq!(
+            find(&["-U", "--multiline-dotall", "x"], r"a.b", b"a\nb"),
+            Some((0, 3))
+        );
+        // --null-data: NUL is the line terminator, which only a -U pattern may match, as in rg;
+        // and binary detection is off, so no NUL is banned.
+        let e = error(&["--null-data", "x"], r"a\x00");
+        assert!(
+            e.starts_with("the literal \"\\0\" is not allowed in a regex"),
+            "{e}"
+        );
+        assert_eq!(
+            find(&["-U", "--null-data", "x"], r"a\x00b", b"a\x00b"),
+            Some((0, 3))
+        );
+    }
+
+    #[test]
+    fn errors_with_ripgreps_suggestions() {
+        let e = error(&["x"], r"a(?=b)");
+        assert!(e.contains("look-around"), "{e}");
+        assert!(e.ends_with("Consider enabling PCRE2 with the --pcre2 flag, which can handle backreferences\nand look-around."), "{e}");
+        let e = error(&["x"], r"a\nb");
+        assert!(
+            e.contains(
+                "Consider enabling multiline mode with the --multiline flag (or -U for short)."
+            ),
+            "{e}"
+        );
+        let e = error(&["x"], r"a\x00b");
+        assert!(
+            e.starts_with("pattern contains \"\\0\" but it is impossible to match"),
+            "{e}"
+        );
+        assert!(
+            e.ends_with("binary detection is enabled and matching a NUL byte is impossible."),
+            "{e}"
+        );
+        assert!(build(&args(&["-a", "x"]), &[r"a\x00b".into()]).is_ok());
+        let e = error(&["-P", "x"], "(");
+        // rg's offset: grep-pcre2 wraps each pattern in a group.
+        assert_eq!(
+            e,
+            "PCRE2: error compiling pattern at offset 5: missing closing parenthesis"
+        );
+        let e = error(&["--engine", "auto", "x"], "(?<=a)(");
+        assert!(e.starts_with("regex could not be compiled with either the default regex engine or with PCRE2.\n\ndefault regex engine error:\n~~~"), "{e}");
+        assert!(
+            e.contains("\n\nPCRE2 regex engine error:\nPCRE2: error compiling pattern"),
+            "{e}"
+        );
+        let e = error(&["--regex-size-limit", "10", "x"], "a{1000}");
+        assert!(e.contains("exceeds size limit"), "{e}");
+    }
+
+    #[test]
+    fn pcre2_version_as_ripgrep_prints_it() {
+        let v = pcre2_version();
+        assert!(v.starts_with("PCRE2 10."), "{v}");
+        assert!(
+            v.ends_with(" is available (JIT is available)") || v.ends_with("(JIT is unavailable)"),
+            "{v}"
+        );
+    }
 }
```

- [ ] **Step 2:** `cargo test --lib matcher`: the engine and PCRE2 tests FAIL (C1's `build` has
  the Rust engine only, ignores `--no-unicode`, `--crlf`, `-U` and the size limits, and
  `pcre2_version` is empty): RED. `Pcre2` does not exist yet: comment `pcre2_clones_per_thread`
  out until Step 3.

### Task 2.2: both engines

**Files:** `src/matcher.rs`.

- [ ] **Step 1:** `Pcre2`, a PCRE2 matcher whose search threads each match with a clone of their
  own (D7: one shared matcher took 1.55 × rg over the M4 corpus, the pcre2 crate's pool of
  scratch space behind a lock); `build` as rg's `matcher` (`--engine default` with rg's
  suggestions, `pcre2`, `auto` with both errors), `rust` as rg's `matcher_rust` (with
  `ban_byte`, D19), `pcre2` as rg's `matcher_pcre2`, rg's three suggestions, and
  `pcre2_version`:

```diff
diff --git a/src/matcher.rs b/src/matcher.rs
index 68eea6c..33d078a 100644
--- a/src/matcher.rs
+++ b/src/matcher.rs
@@ -1,14 +1,51 @@
 //! The pattern matcher of a run (M5a plan, D7): Rust's regex engine, as ripgrep's default, or
 //! PCRE2 (`-P`, `--engine`). One type for both, so the searcher, the sinks and the printers
 //! take a `&Matcher` whatever the engine; each method goes to the engine's own.
-use crate::cli::{Args, Case};
+use crate::cli::{Args, Case, Engine};
 use grep_matcher::{ByteSet, LineMatchKind, LineTerminator, Match, NoError};
 
 /// The matcher of a run.
 #[derive(Clone, Debug)]
 pub enum PatternMatcher {
     Rust(grep_regex::RegexMatcher),
-    Pcre2(grep_pcre2::RegexMatcher),
+    Pcre2(Pcre2),
+}
+
+/// A PCRE2 matcher shared by the search threads. PCRE2 needs scratch space per search, which
+/// the pcre2 crate keeps in a pool behind a lock: twelve threads searching with one matcher
+/// waited on it, 1.55 × rg's time over the M4 corpus (rg builds a matcher per thread). Each
+/// thread here matches with a clone of its own, made on its first search and kept for the run
+/// (`with`): a clone shares the compiled pattern and has a pool of its own.
+#[derive(Clone, Debug)]
+pub struct Pcre2 {
+    shared: grep_pcre2::RegexMatcher,
+    /// Tells this matcher's clones from another matcher's in a thread's cache.
+    id: u64,
+}
+
+impl Pcre2 {
+    fn new(m: grep_pcre2::RegexMatcher) -> Self {
+        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
+        Pcre2 {
+            shared: m,
+            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
+        }
+    }
+
+    /// `f` over this thread's clone of the matcher.
+    fn with<R>(&self, f: impl FnOnce(&grep_pcre2::RegexMatcher) -> R) -> R {
+        thread_local! {
+            static LOCAL: std::cell::RefCell<Option<(u64, grep_pcre2::RegexMatcher)>> =
+                const { std::cell::RefCell::new(None) };
+        }
+        LOCAL.with(|l| {
+            let mut l = l.borrow_mut();
+            if l.as_ref().is_none_or(|(id, _)| *id != self.id) {
+                *l = Some((self.id, self.shared.clone()));
+            }
+            f(&l.as_ref().expect("set above").1)
+        })
+    }
 }
 
 /// The capture groups of one match, of either engine.
@@ -64,37 +101,41 @@ impl grep_matcher::Matcher for PatternMatcher {
     fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, Error> {
         match self {
             PatternMatcher::Rust(m) => Ok(m.find_at(haystack, at)?),
-            PatternMatcher::Pcre2(m) => m.find_at(haystack, at).map_err(Error::Pcre2),
+            PatternMatcher::Pcre2(m) => m.with(|m| m.find_at(haystack, at)).map_err(Error::Pcre2),
         }
     }
 
     fn new_captures(&self) -> Result<Captures, Error> {
         match self {
             PatternMatcher::Rust(m) => Ok(Captures::Rust(m.new_captures()?)),
-            PatternMatcher::Pcre2(m) => m.new_captures().map(Captures::Pcre2).map_err(Error::Pcre2),
+            PatternMatcher::Pcre2(m) => m
+                .shared
+                .new_captures()
+                .map(Captures::Pcre2)
+                .map_err(Error::Pcre2),
         }
     }
 
     fn capture_count(&self) -> usize {
         match self {
             PatternMatcher::Rust(m) => m.capture_count(),
-            PatternMatcher::Pcre2(m) => m.capture_count(),
+            PatternMatcher::Pcre2(m) => m.shared.capture_count(),
         }
     }
 
     fn capture_index(&self, name: &str) -> Option<usize> {
         match self {
             PatternMatcher::Rust(m) => m.capture_index(name),
-            PatternMatcher::Pcre2(m) => m.capture_index(name),
+            PatternMatcher::Pcre2(m) => m.shared.capture_index(name),
         }
     }
 
     fn captures_at(&self, haystack: &[u8], at: usize, caps: &mut Captures) -> Result<bool, Error> {
         match (self, caps) {
             (PatternMatcher::Rust(m), Captures::Rust(c)) => Ok(m.captures_at(haystack, at, c)?),
-            (PatternMatcher::Pcre2(m), Captures::Pcre2(c)) => {
-                m.captures_at(haystack, at, c).map_err(Error::Pcre2)
-            }
+            (PatternMatcher::Pcre2(m), Captures::Pcre2(c)) => m
+                .with(|m| m.captures_at(haystack, at, c))
+                .map_err(Error::Pcre2),
             _ => unreachable!("captures of another engine"),
         }
     }
@@ -102,58 +143,87 @@ impl grep_matcher::Matcher for PatternMatcher {
     fn shortest_match_at(&self, haystack: &[u8], at: usize) -> Result<Option<usize>, Error> {
         match self {
             PatternMatcher::Rust(m) => Ok(m.shortest_match_at(haystack, at)?),
-            PatternMatcher::Pcre2(m) => m.shortest_match_at(haystack, at).map_err(Error::Pcre2),
+            PatternMatcher::Pcre2(m) => m
+                .with(|m| m.shortest_match_at(haystack, at))
+                .map_err(Error::Pcre2),
         }
     }
 
     fn non_matching_bytes(&self) -> Option<&ByteSet> {
         match self {
             PatternMatcher::Rust(m) => m.non_matching_bytes(),
-            PatternMatcher::Pcre2(m) => m.non_matching_bytes(),
+            PatternMatcher::Pcre2(m) => m.shared.non_matching_bytes(),
         }
     }
 
     fn line_terminator(&self) -> Option<LineTerminator> {
         match self {
             PatternMatcher::Rust(m) => m.line_terminator(),
-            PatternMatcher::Pcre2(m) => m.line_terminator(),
+            PatternMatcher::Pcre2(m) => m.shared.line_terminator(),
         }
     }
 
     fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, Error> {
         match self {
             PatternMatcher::Rust(m) => Ok(m.find_candidate_line(haystack)?),
-            PatternMatcher::Pcre2(m) => m.find_candidate_line(haystack).map_err(Error::Pcre2),
+            PatternMatcher::Pcre2(m) => m
+                .with(|m| m.find_candidate_line(haystack))
+                .map_err(Error::Pcre2),
         }
     }
 }
 
-/// The matcher the command line asks for (M5a plan, Task 1.4: the Rust engine with the flags
-/// of M1–M4; chunk 2 adds PCRE2 and the rest of rg's matcher flags).
+/// The matcher the command line asks for, as rg 14.1.1 builds it (hiargs.rs `matcher`):
+/// `--engine default` (Rust's, with the suggestions of rg's errors), `pcre2`, or `auto`, which
+/// tries Rust's and then PCRE2 and reports both errors when neither compiles.
 pub fn build(args: &Args, patterns: &[String]) -> anyhow::Result<PatternMatcher> {
+    match args.engine {
+        Engine::Default => rust(args, patterns).map_err(|e| anyhow::anyhow!(suggest_pcre2(e))),
+        Engine::Pcre2 => pcre2(args, patterns).map_err(|e| anyhow::anyhow!(e)),
+        Engine::Auto => {
+            let rust_err = match rust(args, patterns) {
+                Ok(m) => return Ok(m),
+                Err(e) => e,
+            };
+            let pcre_err = match pcre2(args, patterns) {
+                Ok(m) => return Ok(m),
+                Err(e) => e,
+            };
+            let divider = "~".repeat(79);
+            anyhow::bail!(
+                "regex could not be compiled with either the default regex engine or with \
+                 PCRE2.\n\ndefault regex engine error:\n{divider}\n{rust_err}\n{divider}\n\n\
+                 PCRE2 regex engine error:\n{pcre_err}"
+            )
+        }
+    }
+}
+
+/// Rust's engine (hiargs.rs `matcher_rust`). A pattern that cannot match a NUL while binary
+/// data is detected is refused, as rg refuses it (`ban_byte`).
+fn rust(args: &Args, patterns: &[String]) -> Result<PatternMatcher, String> {
     let mut b = grep_regex::RegexMatcherBuilder::new();
+    b.multi_line(true)
+        .unicode(!args.no_unicode)
+        .octal(false)
+        .fixed_strings(args.fixed_strings && !args.no_fixed_strings);
     match args.case() {
-        Case::Insensitive => {
-            b.case_insensitive(true);
-        }
-        Case::Smart => {
-            b.case_smart(true);
-        }
-        Case::Sensitive => {}
+        Case::Insensitive => b.case_insensitive(true),
+        Case::Smart => b.case_smart(true),
+        Case::Sensitive => b.case_insensitive(false),
+    };
+    if args.line_regexp {
+        b.whole_line(true);
+    } else if args.word_regexp {
+        b.word(true);
     }
-    b.word(args.word_regexp)
-        .whole_line(args.line_regexp)
-        .fixed_strings(args.fixed_strings && !args.no_fixed_strings)
-        .multi_line(true); // as ripgrep: ^ and $ match at line boundaries inside a buffer
-    // The line terminator agrees with the searcher's (C3), as rg's `matcher_rust` sets it:
-    // none with -U, NUL with --null-data, CRLF-aware with --crlf.
     if args.multiline {
         b.dot_matches_new_line(args.multiline_dotall);
         if args.crlf {
             b.crlf(true).line_terminator(None);
         }
     } else {
-        b.line_terminator(Some(b'\n'));
+        b.line_terminator(Some(b'\n')).dot_matches_new_line(false);
         if args.crlf {
             b.crlf(true);
         }
```

- [ ] **Step 2:** `cargo test --lib matcher`: PASS, `pcre2_clones_per_thread` included.
- [ ] **Step 3:** commit: `Match with PCRE2 and with rg's matcher flags and errors`.

---
## Chunk 3: the walker, the searcher and the decoding

### Task 3.1: the tests

**Files:** `src/search.rs`, `src/source.rs` (tests only).

- [ ] **Step 1:** the tests of `src/search.rs` (D8, D9). The orders and the file lists are rg
  14.1.1's (`rg --files --no-ignore --ignore-vcs` lists the exclude file's match but not the
  `.ignore`'s):

```diff
diff --git a/src/search.rs b/src/search.rs
index f90f79a..bdd3181 100644
--- a/src/search.rs
+++ b/src/search.rs
@@ -800,7 +961,7 @@ mod tests {
                 };
                 let counted = collect(&mut streaming_searcher(&a, false, false));
                 let mut numbered = collect(&mut printing_searcher(&a, false, false, false));
-                number_lines(&mut numbered, src.bytes());
+                number_lines(&mut numbered, src.bytes(), b'\n');
                 assert!(!counted.is_empty(), "{flags:?} {bytes:?}");
                 assert_eq!(numbered, counted, "{flags:?} {bytes:?}");
             }
@@ -1168,14 +1329,25 @@ mod tests {
                 p
             })
             .collect();
-        assert!(rg_in_memory(&files[..1], false));
-        assert!(rg_in_memory(&files[..10], false));
-        assert!(!rg_in_memory(&files, false));
-        assert!(!rg_in_memory(&files[..1], true));
-        assert!(!rg_in_memory(&[], false));
-        assert!(!rg_in_memory(&[d.path().into()], false));
-        assert!(!rg_in_memory(&[files[0].clone(), d.path().into()], false));
-        assert!(!rg_in_memory(&[d.path().join("missing")], false));
+        let a = args(&["s4", "x"]);
+        assert!(rg_in_memory(&a, &files[..1], false));
+        assert!(rg_in_memory(&a, &files[..10], false));
+        assert!(!rg_in_memory(&a, &files, false));
+        assert!(!rg_in_memory(&a, &files[..1], true));
+        assert!(!rg_in_memory(&a, &[], false));
+        assert!(!rg_in_memory(&a, &[d.path().into()], false));
+        assert!(!rg_in_memory(
+            &a,
+            &[files[0].clone(), d.path().into()],
+            false
+        ));
+        assert!(!rg_in_memory(&a, &[d.path().join("missing")], false));
+        // M5a plan, D9: --mmap maps every file, walked ones too; --no-mmap none.
+        let always = args(&["s4", "--mmap", "x"]);
+        assert!(rg_in_memory(&always, &files, false));
+        assert!(rg_in_memory(&always, &[d.path().into()], false));
+        let never = args(&["s4", "--no-mmap", "x"]);
+        assert!(!rg_in_memory(&never, &files[..1], false));
     }
 
     fn walk_names(a: &Args, paths: &[PathBuf], cwd: &Path) -> Vec<String> {
@@ -1196,6 +1368,214 @@ mod tests {
         v
     }
 
+    /// M5a plan, D8: each kind of ignore file is turned off by its own flag, and
+    /// --no-ignore-vcs also stops reading the exclude and global files.
+    #[test]
+    fn walker_ignore_kinds_and_files() {
+        let d = tempfile::tempdir().unwrap();
+        std::fs::create_dir_all(d.path().join(".git/info")).unwrap();
+        std::fs::create_dir(d.path().join("sub")).unwrap();
+        std::fs::write(d.path().join(".gitignore"), "vcs.md\n").unwrap();
+        std::fs::write(d.path().join(".git/info/exclude"), "excluded.md\n").unwrap();
+        std::fs::write(d.path().join(".ignore"), "dot.md\n").unwrap();
+        let outside = tempfile::tempdir().unwrap();
+        std::fs::write(outside.path().join("extra.ignore"), "extra.md\n").unwrap();
+        for n in ["keep.md", "vcs.md", "excluded.md", "dot.md", "extra.md"] {
+            std::fs::write(d.path().join(n), "x").unwrap();
+        }
+        let names = |v: &[&str]| {
+            let mut argv = vec!["s4"];
+            argv.extend_from_slice(v);
+            argv.push("x");
+            walk_names(&args(&argv), &[d.path().into()], d.path())
+        };
+        assert_eq!(names(&[]), ["extra.md", "keep.md"]);
+        assert_eq!(
+            names(&["--no-ignore-vcs"]),
+            ["excluded.md", "extra.md", "keep.md", "vcs.md"]
+        );
+        assert_eq!(
+            names(&["--no-ignore-exclude"]),
+            ["excluded.md", "extra.md", "keep.md"]
+        );
+        assert_eq!(
+            names(&["--no-ignore-dot"]),
+            ["dot.md", "extra.md", "keep.md"]
+        );
+        let extra = outside.path().join("extra.ignore");
+        let extra = extra.to_str().unwrap();
+        assert_eq!(names(&["--ignore-file", extra]), ["keep.md"]);
+        assert_eq!(
+            names(&["--ignore-file", extra, "--no-ignore-files"]),
+            ["extra.md", "keep.md"]
+        );
+        // --no-ignore then --ignore-vcs: .gitignore is read again, the exclude file stays off
+        // (`rg --files --no-ignore --ignore-vcs` lists the same four).
+        assert_eq!(
+            names(&["--no-ignore", "--ignore-vcs"]),
+            ["dot.md", "excluded.md", "extra.md", "keep.md"]
+        );
+        // A missing --ignore-file is an error rg prints, and the walk goes on.
+        let a = args(&["s4", "--ignore-file", "/nonexistent/s4-ignore", "x"]);
+        let (_, errors) = walker_with(&a, &[d.path().into()], d.path()).unwrap();
+        assert_eq!(errors.len(), 1);
+        assert!(errors[0].to_string().contains("/nonexistent/s4-ignore"));
+    }
+
+    #[test]
+    fn walker_size_globs_and_types() {
+        let d = tempfile::tempdir().unwrap();
+        std::fs::write(d.path().join("small.TXT"), "x").unwrap();
+        std::fs::write(d.path().join("large.txt"), "x".repeat(2048)).unwrap();
+        std::fs::write(d.path().join("a.foo"), "x").unwrap();
+        let names = |v: &[&str]| {
+            let mut argv = vec!["s4"];
+            argv.extend_from_slice(v);
+            argv.push("x");
+            walk_names(&args(&argv), &[d.path().into()], d.path())
+        };
+        assert_eq!(names(&["--max-filesize", "1K"]), ["a.foo", "small.TXT"]);
+        assert_eq!(names(&["-g", "*.txt"]), ["large.txt"]);
+        assert_eq!(
+            names(&["--glob-case-insensitive", "-g", "*.txt"]),
+            ["large.txt", "small.TXT"]
+        );
+        assert_eq!(names(&["--type-add", "foo:*.foo", "-t", "foo"]), ["a.foo"]);
+        // --type-clear empties a type: selecting it then selects nothing, as in rg.
+        let a = args(&["s4", "--type-clear", "txt", "-t", "txt", "x"]);
+        assert!(walker(&a, &[d.path().into()], d.path()).is_err());
+        let a = args(&["s4", "--type-add", "nocolon", "-t", "nocolon", "x"]);
+        let e = walker(&a, &[d.path().into()], d.path())
+            .unwrap_err()
+            .to_string();
+        assert!(e.contains("invalid definition"), "{e}");
+    }
+
+    #[test]
+    fn sorted_by_time_and_in_reverse() {
+        let d = tempfile::tempdir().unwrap();
+        let t = |secs: u64| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
+        for (n, at) in [("b", 300), ("a", 100), ("c", 200)] {
+            let p = d.path().join(n);
+            std::fs::write(&p, "x").unwrap();
+            let f = std::fs::File::options().write(true).open(&p).unwrap();
+            f.set_modified(t(at)).unwrap();
+        }
+        let sorted = |v: &[&str]| {
+            let mut argv = vec!["s4"];
+            argv.extend_from_slice(v);
+            argv.push("x");
+            let a = args(&argv);
+            let mut entries: Vec<_> = walker(&a, &[d.path().into()], d.path())
+                .unwrap()
+                .build()
+                .flatten()
+                .filter(|e| e.depth() > 0)
+                .collect();
+            sort_entries(&mut entries, a.sort.unwrap());
+            entries
+                .iter()
+                .map(|e| e.file_name().to_string_lossy().into_owned())
+                .collect::<Vec<_>>()
+        };
+        assert_eq!(sorted(&["--sort", "modified"]), ["a", "c", "b"]);
+        assert_eq!(sorted(&["--sortr", "modified"]), ["b", "c", "a"]);
+        assert_eq!(sorted(&["--sortr", "path"]), ["c", "b", "a"]);
+        assert!(walker_sorts(&args(&["s4", "--sort", "path", "x"])));
+        assert!(!walker_sorts(&args(&["s4", "--sortr", "path", "x"])));
+    }
+
+    /// M5a plan, D9: the searcher's line terminator, multi-line mode, passthru and
+    /// --stop-on-nonmatch, as rg 14.1.1's `searcher`.
+    #[test]
+    fn searcher_flags_as_ripgrep() {
+        let lines = |argv: &[&str], pat: &str, text: &[u8]| -> Vec<(char, u64, Vec<u8>)> {
+            let mut v = vec!["s4"];
+            v.extend_from_slice(argv);
+            v.push(pat);
+            let a = args(&v);
+            let src = Source::from_bytes("a".into(), text.to_vec(), true);
+            let Some(fm) = run(&a, &[pat], &src) else {
+                return Vec::new();
+            };
+            fm.items
+                .iter()
+                .map(|i| match i {
+                    Item::Match(l) => ('m', l.line_number, l.bytes(src.bytes()).to_vec()),
+                    Item::Context(l) => ('c', l.line_number, l.bytes(src.bytes()).to_vec()),
+                })
+                .collect()
+        };
+        assert_eq!(
+            lines(
+                &["--null-data"],
+                "needle",
+                b"one needle\ntwo\0three\0x needle"
+            ),
+            [
+                ('m', 1, b"one needle\ntwo\0".to_vec()),
+                ('m', 3, b"x needle".to_vec())
+            ]
+        );
+        assert_eq!(
+            lines(&["-U"], r"one\ntwo", b"one\ntwo\nthree\n"),
+            [('m', 1, b"one\ntwo\n".to_vec())]
+        );
+        assert_eq!(
+            lines(&["--passthru"], "two", b"one\ntwo\nthree\n")
+                .iter()
+                .map(|l| l.0)
+                .collect::<String>(),
+            "cmc"
+        );
+        assert_eq!(
+            lines(
+                &["--stop-on-nonmatch"],
+                "needle",
+                b"needle\nneedle\nx\nneedle\n"
+            )
+            .len(),
+            2
+        );
+        assert_eq!(lines(&["--crlf"], "needle$", b"a needle\r\n").len(), 1);
+    }
+
+    /// The submatches grep-printer records with each line terminator and in multi-line mode.
+    #[test]
+    fn submatches_follow_the_searchers_terminator() {
+        let subs = |argv: &[&str], pat: &str, text: &[u8]| -> Vec<(usize, usize)> {
+            let mut v = vec!["s4"];
+            v.extend_from_slice(argv);
+            v.push(pat);
+            let a = args(&v);
+            let src = Source::from_bytes("a".into(), text.to_vec(), true);
+            run(&a, &[pat], &src)
+                .map(|fm| summary(&fm).into_iter().flat_map(|r| r.2).collect())
+                .unwrap_or_default()
+        };
+        // CRLF: the \r is not part of the line's text.
+        assert_eq!(subs(&["--crlf"], r"\s$", b"a \r\n"), [(1, 2)]);
+        // Multi-line: one submatch across the lines of its block.
+        assert_eq!(subs(&["-U"], r"a\nb", b"a\nb\n"), [(0, 3)]);
+    }
+
+    #[test]
+    fn lines_are_numbered_by_the_searchs_terminator() {
+        let src = b"a\0needle\0b\nc\0needle\0".to_vec();
+        let mut items = vec![
+            Item::Match(Line::owned(0, 2, b"needle\0")),
+            Item::Match(Line::owned(0, 15, b"needle\0")),
+        ];
+        number_lines(&mut items, &src, 0);
+        let numbers: Vec<u64> = items
+            .iter()
+            .map(|i| match i {
+                Item::Match(l) | Item::Context(l) => l.line_number,
+            })
+            .collect();
+        assert_eq!(numbers, [2, 4]);
+    }
+
     #[test]
     fn walker_respects_gitignore_and_rgignore() {
         let d = tempfile::tempdir().unwrap();
```

- [ ] **Step 2:** the tests of `src/source.rs` (D10, and D9's `--no-mmap`, which maps no file
  however large: review r2, Codex #1):

```diff
diff --git a/src/source.rs b/src/source.rs
index a64fa04..a90077d 100644
--- a/src/source.rs
+++ b/src/source.rs
@@ -174,6 +267,40 @@ mod tests {
         assert!(src.had_bom);
     }
 
+    /// M5a plan, D10: `-E` as grep-searcher decodes for rg 14.1.1.
+    #[test]
+    fn explicit_encodings_and_none() {
+        let latin1 = Decoding::Label(encoding_rs::WINDOWS_1252);
+        let s = Source::from_bytes_as("a".into(), b"caf\xe9 needle\n".to_vec(), false, latin1);
+        assert_eq!(s.bytes(), "café needle\n".as_bytes());
+        assert!(
+            s.had_bom,
+            "an explicit encoding is searched through a reader"
+        );
+        // A BOM wins over the label.
+        let mut bom = b"\xEF\xBB\xBF".to_vec();
+        bom.extend_from_slice("é".as_bytes());
+        let s = Source::from_bytes_as("a".into(), bom.clone(), false, latin1);
+        assert_eq!(s.bytes(), "é".as_bytes());
+        // UTF-8 passes through, invalid bytes included.
+        let utf8 = Decoding::Label(encoding_rs::UTF_8);
+        let s = Source::from_bytes_as("a".into(), b"a\xffb".to_vec(), false, utf8);
+        assert_eq!(s.bytes(), b"a\xffb");
+        // UTF-16 without a BOM.
+        let le: Vec<u8> = "né".encode_utf16().flat_map(u16::to_le_bytes).collect();
+        let s = Source::from_bytes_as(
+            "a".into(),
+            le,
+            false,
+            Decoding::Label(encoding_rs::UTF_16LE),
+        );
+        assert_eq!(s.bytes(), "né".as_bytes());
+        // -E none: the BOM stays, nothing is transcoded.
+        let s = Source::from_bytes_as("a".into(), bom.clone(), false, Decoding::Raw);
+        assert_eq!(s.bytes(), &bom[..]);
+        assert!(!s.had_bom);
+    }
+
     #[test]
     fn plain_file_is_unchanged() {
         let src = Source::open(&fixture("misc/bin.dat"), false).unwrap();
@@ -192,6 +319,28 @@ mod tests {
         assert_eq!(src.bytes().len(), size);
     }
 
+    /// Review r2 of the M5a plan, Codex #1: with `--no-mmap` no file is mapped, however large,
+    /// as ripgrep reads every file then; a truncation while s4 reads it cannot end the process.
+    #[test]
+    fn no_mmap_maps_no_file() {
+        let d = tempfile::tempdir().unwrap();
+        let p = d.path().join("big.txt");
+        let size = MMAP_THRESHOLD as usize;
+        std::fs::write(&p, vec![b'x'; size]).unwrap();
+        let read = Source::open_as(&p, false, Decoding::Auto, Mapping::Never).unwrap();
+        assert!(matches!(read.data, Data::Owned(_)));
+        assert_eq!(read.bytes().len(), size);
+        let mapped = Source::open_as(&p, false, Decoding::Auto, Mapping::Large).unwrap();
+        assert!(matches!(mapped.data, Data::Mapped(_)));
+        let args = |v: &[&str]| crate::cli::parse(v.iter().map(Into::into).collect()).unwrap();
+        assert_eq!(
+            Mapping::of(&args(&["s4", "--no-mmap", "x"])),
+            Mapping::Never
+        );
+        assert_eq!(Mapping::of(&args(&["s4", "--mmap", "x"])), Mapping::Large);
+        assert_eq!(Mapping::of(&args(&["s4", "x"])), Mapping::Large);
+    }
+
     #[test]
     fn from_bytes_strips_a_utf8_bom() {
         let src = Source::from_bytes("<stdin>".into(), b"\xEF\xBB\xBFhi\n".to_vec(), true);
```

- [ ] **Step 3:** `cargo test --lib -- search:: source::` does not compile (`walker_with`,
  `sort_entries`, `walker_sorts`, `Source::from_bytes_as`, `Decoding`, `Mapping`… are
  unknown): RED.

### Task 3.2: the walker and the searcher

**Files:** `src/search.rs`.

- [ ] **Step 1:** `rg_in_memory` with `--mmap` and `--no-mmap`, `binary_detection` off with
  `--null-data`, `line_terminator`, the searcher's terminator, `-U`, `--stop-on-nonmatch`,
  `--passthru` and `-E` (`searcher_builder`), the walker of every ignore kind, `--ignore-file`
  (its errors returned by `walker_with`), the size, file-system and glob flags, `sort_entries`
  and `walker_sorts`, `submatches_in` and its callers with the searcher (grep-printer's
  `find_iter_at_in_context` with `trim_line_terminator`), `number_lines` with the terminator,
  and M4's two helpers kept for `output/stream.rs` until C9 (design note 3):

```diff
diff --git a/src/search.rs b/src/search.rs
index f90f79a..bdd3181 100644
--- a/src/search.rs
+++ b/src/search.rs
@@ -1,10 +1,10 @@
 //! Walking and searching with ripgrep's crates (ADR 0001, design notes 3 and 4).
-use crate::cli::{Args, BinaryMode, SortKind, SortMode, TypeChange};
+use crate::cli::{Args, BinaryMode, EncodingMode, MmapMode, SortKind, SortMode, TypeChange};
 use crate::matcher::PatternMatcher;
 use crate::model::{FileMatches, FileStats, Item, Line, Submatch};
 use crate::source::Source;
 use anyhow::Result;
-use grep_matcher::Matcher;
+use grep_matcher::{LineTerminator, Matcher};
 use grep_searcher::{
     BinaryDetection, MmapChoice, Searcher, SearcherBuilder, Sink, SinkContext, SinkContextKind,
     SinkFinish, SinkMatch,
@@ -24,8 +24,16 @@ pub fn matcher(args: &Args, patterns: &[String]) -> Result<PatternMatcher> {
 /// M1). C9 verifies this rule by behaviour against rg: `--json` on `misc/bin.dat` as one
 /// explicit file (second Match on line 2, offset 13), walked inside `misc`, as 11 explicit
 /// files, and through stdin (line 3, offset 18).
-pub fn rg_in_memory(paths: &[PathBuf], stdin: bool) -> bool {
-    !stdin && !paths.is_empty() && paths.len() <= 10 && paths.iter().all(|p| p.is_file())
+///
+/// `--mmap` maps every file searched, walked ones too, and `--no-mmap` none (M5a plan, D9).
+pub fn rg_in_memory(args: &Args, paths: &[PathBuf], stdin: bool) -> bool {
+    match args.mmap_mode {
+        MmapMode::Never => false,
+        MmapMode::Always => true,
+        MmapMode::Auto => {
+            !stdin && !paths.is_empty() && paths.len() <= 10 && paths.iter().all(|p| p.is_file())
+        }
+    }
 }
 
 /// `explicit`: the file was named on the command line, or is stdin (design note 3).
@@ -59,7 +67,13 @@ pub fn printing_searcher(args: &Args, explicit: bool, mmap: bool, line_number: b
 
 /// ripgrep's binary policy for one file (design note 3). A searcher built for one kind of
 /// file is reused for the other with `Searcher::set_binary_detection`.
+///
+/// `--null-data` turns it off, as `-a` does: a NUL ends a line (rg 14.1.1, hiargs.rs
+/// `BinaryDetection::from_low_args`).
 pub fn binary_detection(args: &Args, explicit: bool) -> BinaryDetection {
+    if args.null_data {
+        return BinaryDetection::none();
+    }
     match args.binary_mode(explicit) {
         BinaryMode::Text => BinaryDetection::none(),
         BinaryMode::Binary => BinaryDetection::convert(b'\0'),
@@ -67,16 +81,47 @@ pub fn binary_detection(args: &Args, explicit: bool) -> BinaryDetection {
     }
 }
 
+/// The line terminator of the search: CRLF with `--crlf`, NUL with `--null-data`, else `\n`.
+pub fn line_terminator(args: &Args) -> LineTerminator {
+    if args.crlf {
+        LineTerminator::crlf()
+    } else if args.null_data {
+        LineTerminator::byte(b'\0')
+    } else {
+        LineTerminator::byte(b'\n')
+    }
+}
+
+/// `decoded`: the input is a Source already decoded (`Source::open_as`), so the searcher reads
+/// it as it is; else it decodes as rg's: by its BOM, with `-E`'s encoding, or not at all with
+/// `-E none` (hiargs.rs `searcher`).
 fn searcher_builder(args: &Args, explicit: bool, decoded: bool) -> SearcherBuilder {
     let binary = binary_detection(args, explicit);
     let mut b = SearcherBuilder::new();
     b.line_number(true) // Trails need line numbers even when they are not printed.
-        .before_context(args.before())
-        .after_context(args.after())
+        .line_terminator(line_terminator(args))
         .invert_match(args.invert_match)
+        .multi_line(args.multiline)
+        .stop_on_nonmatch(args.stop_on_nonmatch)
         // -m is counted by the sinks, as ripgrep 14.1.1's printers count it (`MaxCount`).
         .binary_detection(binary)
         .bom_sniffing(!decoded);
+    if args.passthru {
+        b.passthru(true);
+    } else {
+        b.before_context(args.before()).after_context(args.after());
+    }
+    if !decoded {
+        match &args.encoding {
+            EncodingMode::Auto => {}
+            EncodingMode::Label(label) => {
+                b.encoding(grep_searcher::Encoding::new(label).ok());
+            }
+            EncodingMode::Disabled => {
+                b.bom_sniffing(false);
+            }
+        }
+    }
     b
 }
 
@@ -92,8 +137,19 @@ thread_local! {
     pub static WALKERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
 }
 
-/// `cwd` anchors the globs, as ripgrep does (`-g 'docs/*.md' x docs` works).
+/// `walker_with` without the errors of `--ignore-file`, for the actions that only validate.
 pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder> {
+    Ok(walker_with(args, paths, cwd)?.0)
+}
+
+/// The walker of rg 14.1.1 (hiargs.rs `walk_builder`), and the errors of the `--ignore-file`
+/// files it could not read, which rg prints as ignore messages. `cwd` anchors the globs, as
+/// ripgrep does (`-g 'docs/*.md' x docs` works).
+pub fn walker_with(
+    args: &Args,
+    paths: &[PathBuf],
+    cwd: &Path,
+) -> Result<(WalkBuilder, Vec<ignore::Error>)> {
     #[cfg(test)]
     WALKERS.with(|n| n.set(n.get() + 1));
     let first = paths.first().cloned().unwrap_or_else(|| PathBuf::from("."));
@@ -101,17 +157,29 @@ pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder>
     for p in paths.iter().skip(1) {
         w.add(p);
     }
-    let no_ignore = args.no_ignore();
+    let mut errors = Vec::new();
+    if !args.no_ignore_files {
+        for p in &args.ignore_file {
+            if let Some(e) = w.add_ignore(p) {
+                errors.push(e);
+            }
+        }
+    }
+    let ig = args.ignores;
     w.hidden(!args.hidden())
-        .ignore(!no_ignore)
-        .git_ignore(!no_ignore)
-        .git_global(!no_ignore)
-        .git_exclude(!no_ignore)
-        .parents(!no_ignore)
+        .parents(!ig.no_parent)
+        .ignore(!ig.no_dot)
+        .git_global(!ig.no_vcs && !ig.no_global)
+        .git_ignore(!ig.no_vcs)
+        .git_exclude(!ig.no_vcs && !ig.no_exclude)
+        .require_git(!args.no_require_git)
+        .ignore_case_insensitive(args.ignore_file_case_insensitive)
         .follow_links(args.follow && !args.no_follow)
         .max_depth(args.max_depth)
+        .max_filesize(args.max_filesize)
+        .same_file_system(args.one_file_system)
         .skip_stdout(true);
-    if !no_ignore {
+    if !ig.no_dot {
         w.add_custom_ignore_filename(".rgignore");
     }
     // The file types before the globs, as ripgrep 14.1.1 validates them (code review r3).
@@ -133,9 +201,13 @@ pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder>
         }
     }
     w.types(tb.build()?);
-    // As ripgrep 14.1.1: every -g first, then every --iglob (case-insensitive); the relative
-    // order of the two kinds on the command line does not matter (measured in review r2).
+    // As ripgrep 14.1.1: every -g first, case-insensitive with --glob-case-insensitive, then
+    // every --iglob (case-insensitive); the relative order of the two kinds on the command line
+    // does not matter (measured in review r2).
     let mut ob = OverrideBuilder::new(cwd);
+    if args.glob_case_insensitive {
+        ob.case_insensitive(true)?;
+    }
     for g in &args.globs {
         ob.add(g)?;
     }
@@ -150,7 +222,44 @@ pub fn walker(args: &Args, paths: &[PathBuf], cwd: &Path) -> Result<WalkBuilder>
         w.sort_by_file_path(|a, b| a.cmp(b));
     }
     w.threads(walker_threads(args));
-    Ok(w)
+    Ok((w, errors))
+}
+
+/// The files of a walk in the order of `--sort`/`--sortr` when the walker cannot give it (all
+/// but `--sort path`), as rg 14.1.1 sorts its haystacks (hiargs.rs `sort`): by path, or by a
+/// time of each file's metadata, a file whose time cannot be read after the others (before
+/// them with `--sortr`), equal times keeping the walk's order.
+pub fn sort_entries(entries: &mut [ignore::DirEntry], sort: SortMode) {
+    use std::time::SystemTime;
+    let time = |e: &ignore::DirEntry| -> Option<SystemTime> {
+        let md = e.path().metadata().ok()?;
+        match sort.kind {
+            SortKind::Path => None,
+            SortKind::Modified => md.modified().ok(),
+            SortKind::Accessed => md.accessed().ok(),
+            SortKind::Created => md.created().ok(),
+        }
+    };
+    use std::cmp::Reverse;
+    // Stable sorts whose comparison `--sortr` reverses, as rg's: equal keys keep the walk's
+    // order either way.
+    match (sort.kind, sort.reverse) {
+        (SortKind::Path, false) => entries.sort_by(|a, b| a.path().cmp(b.path())),
+        (SortKind::Path, true) => entries.sort_by(|a, b| b.path().cmp(a.path())),
+        (_, false) => entries.sort_by_cached_key(|e| {
+            let t = time(e);
+            (t.is_none(), t)
+        }),
+        (_, true) => entries.sort_by_cached_key(|e| {
+            let t = time(e);
+            Reverse((t.is_none(), t))
+        }),
+    }
+}
+
+/// Whether the walker gives the order of `--sort` itself: only `--sort path`, ascending.
+pub fn walker_sorts(args: &Args) -> bool {
+    args.sort.is_none_or(|s| s == PATH)
 }
 
 /// `--sort path`, the order the walker itself can give.
@@ -336,9 +445,10 @@ impl<'m> CollectSink<'m> {
 /// Numbers the lines of `items`, reported in file order by a search that did not count lines
 /// (`printing_searcher` without line numbers, as ripgrep searches without `-n`), from
 /// `bytes`, the Source the items' offsets point into: a line's number is one more than the
-/// line terminators before it. Only for a search that converted no binary data, whose NUL
-/// bytes the searcher would have counted as line terminators.
-pub fn number_lines(items: &mut [Item], bytes: &[u8]) {
+/// line terminators before it, `term` being the search's (`\n`, or NUL with `--null-data`).
+/// Only for a search that converted no binary data, whose NUL bytes the searcher would have
+/// counted as line terminators.
+pub fn number_lines(items: &mut [Item], bytes: &[u8], term: u8) {
     let (mut counted, mut line) = (0usize, 1u64);
     for item in items {
         let (Item::Match(l) | Item::Context(l)) = item;
@@ -346,7 +456,7 @@ pub fn number_lines(items: &mut [Item], bytes: &[u8]) {
         if at < counted {
             (counted, line) = (0, 1);
         }
-        line += memchr::memchr_iter(b'\n', &bytes[counted..at]).count() as u64;
+        line += memchr::memchr_iter(term, &bytes[counted..at]).count() as u64;
         counted = at;
         l.line_number = line;
     }
@@ -354,20 +464,33 @@ pub fn number_lines(items: &mut [Item], bytes: &[u8]) {
 
 /// grep-printer 0.2.2's JSON sink (`record_matches`, json.rs:611–644, and
 /// `find_iter_at_in_context`, util.rs:467–522), copied because those helpers are
-/// `pub(crate)`. Only the single-line branch: our searcher never runs in multi-line mode
-/// (`-U` is not in M1). The line terminator is trimmed from the end of `range` (a `\r`
-/// stays: CRLF mode is not in M1) and the matcher runs over the buffer up to there, from
+/// `pub(crate)`. On a line, the line terminator of `searcher` (a CRLF's `\r` too) is trimmed
+/// from the end of `range` and the matcher runs over the buffer up to there, from
 /// `range.start`, so `\A` sees what ripgrep's printer sees (`bar\nfoo needle\n` with
-/// `\Afoo|needle` reports only `needle`). Matches starting at or after `range.end` stop
-/// the search; offsets are relative to `range.start`.
-pub fn submatches(matcher: &PatternMatcher, buffer: &[u8], range: Range<usize>) -> Vec<Submatch> {
-    let mut end = range.end;
-    if buffer[..end].ends_with(b"\n") {
-        end -= 1;
-    }
-    let haystack = &buffer[..end];
+/// `\Afoo|needle` reports only `needle`); in multi-line mode (`-U`) it runs over the buffer up
+/// to 128 bytes past `range`, for a look-ahead (grep-printer's `MAX_LOOK_AHEAD`). Matches
+/// starting at or after `range.end` stop the search; offsets are relative to `range.start`.
+/// A search error (PCRE2's match limit) ends the list: the search itself reported it.
+pub fn submatches_in(
+    searcher: &Searcher,
+    matcher: &PatternMatcher,
+    buffer: &[u8],
+    range: Range<usize>,
+) -> Vec<Submatch> {
+    let haystack = if searcher.multi_line_with_matcher(matcher) {
+        &buffer[..buffer.len().min(range.end + MAX_LOOK_AHEAD)]
+    } else {
+        let mut end = range.end;
+        let term = searcher.line_terminator();
+        if end > 0 && term.is_suffix(&buffer[..end]) {
+            end -= 1;
+            if term.is_crlf() && end > 0 && buffer[end - 1] == b'\r' {
+                end -= 1;
+            }
+        }
+        &buffer[..end]
+    };
     let mut subs = Vec::new();
-    // PatternMatcher's error type is NoError: the search cannot fail.
     let _ = matcher.find_iter_at(haystack, range.start, |m| {
         if m.start() >= range.end {
             return false;
@@ -389,9 +512,13 @@ pub fn submatches(matcher: &PatternMatcher, buffer: &[u8], range: Range<usize>)
     subs
 }
 
+/// grep-printer 0.2.2's `MAX_LOOK_AHEAD` (lib.rs:87).
+const MAX_LOOK_AHEAD: usize = 128;
+
 /// The submatches of a reported line as ripgrep's printers record them: none on an inverted
 /// Match; on a context line, only with `-v`, searched in the line's own bytes.
-pub fn match_submatches(
+pub fn match_submatches_in(
+    searcher: &Searcher,
     matcher: &PatternMatcher,
     invert: bool,
     m: &SinkMatch<'_>,
@@ -400,19 +527,53 @@ pub fn match_submatches(
     if invert {
         Vec::new()
     } else {
-        submatches(matcher, m.buffer(), m.bytes_range_in_buffer())
+        submatches_in(searcher, matcher, m.buffer(), m.bytes_range_in_buffer())
     }
 }
 
-pub fn context_submatches(matcher: &PatternMatcher, invert: bool, bytes: &[u8]) -> Vec<Submatch> {
+pub fn context_submatches_in(
+    searcher: &Searcher,
+    matcher: &PatternMatcher,
+    invert: bool,
+    bytes: &[u8],
+) -> Vec<Submatch> {
     // grep-printer's context path searches the line's own bytes (range 0..len).
     if invert {
-        submatches(matcher, bytes, 0..bytes.len())
+        submatches_in(searcher, matcher, bytes, 0..bytes.len())
+    } else {
+        Vec::new()
+    }
+}
+
+/// `match_submatches_in` for a search with `\n` lines and no `-U`, M1–M4's: the streaming
+/// printer's until chunk 9 moves it to the searcher's terminator.
+pub fn match_submatches(
+    matcher: &PatternMatcher,
+    invert: bool,
+    m: &SinkMatch<'_>,
+) -> Vec<Submatch> {
+    if invert {
+        Vec::new()
+    } else {
+        lines_submatches(matcher, m.buffer(), m.bytes_range_in_buffer())
+    }
+}
+
+/// `context_submatches_in` as `match_submatches` is `match_submatches_in`.
+pub fn context_submatches(matcher: &PatternMatcher, invert: bool, bytes: &[u8]) -> Vec<Submatch> {
+    if invert {
+        lines_submatches(matcher, bytes, 0..bytes.len())
     } else {
         Vec::new()
     }
 }
 
+/// `submatches_in` for `\n` lines without `-U`.
+fn lines_submatches(matcher: &PatternMatcher, buffer: &[u8], range: Range<usize>) -> Vec<Submatch> {
+    let searcher = SearcherBuilder::new().build();
+    submatches_in(&searcher, matcher, buffer, range)
+}
+
 impl Sink for CollectSink<'_> {
     type Error = std::io::Error;
 
@@ -421,9 +582,9 @@ impl Sink for CollectSink<'_> {
         Ok(true)
     }
 
-    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
+    fn matched(&mut self, s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
         self.max_count.matched();
-        let subs = match_submatches(self.matcher, self.invert, m);
+        let subs = match_submatches_in(s, self.matcher, self.invert, m);
         self.stats.matched_lines += m.lines().count() as u64;
         self.stats.matches += subs.len() as u64;
         self.items.push(Item::Match(Line {
@@ -439,14 +600,14 @@ impl Sink for CollectSink<'_> {
         Ok(self.max_count.go_on())
     }
 
-    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
+    fn context(&mut self, s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
         self.max_count.context(c);
         self.items.push(Item::Context(Line {
             line_number: c.line_number().unwrap_or(0),
             absolute_offset: c.absolute_byte_offset(),
             len: c.bytes().len(),
             owned: self.copy(c.bytes()),
-            submatches: context_submatches(self.matcher, self.invert, c.bytes()),
+            submatches: context_submatches_in(s, self.matcher, self.invert, c.bytes()),
         }));
         Ok(self.max_count.go_on())
     }
```

### Task 3.3: the decoding

**Files:** `src/source.rs`.

- [ ] **Step 1:** `Decoding` and the `_as` constructors (D10), and `Mapping`, which `open_as`
  takes: a file of `MMAP_THRESHOLD` bytes or more is mapped unless `--no-mmap` (D9):

```diff
diff --git a/src/source.rs b/src/source.rs
index a64fa04..a90077d 100644
--- a/src/source.rs
+++ b/src/source.rs
@@ -3,9 +3,29 @@ use std::path::{Path, PathBuf};
 
 /// Files at least this large are memory-mapped; smaller ones are read. (A memory-mapped file
 /// truncated by another process while we search it can crash the process; ripgrep accepts the
-/// same risk.)
+/// same risk, and `--no-mmap` avoids it: `Mapping::Never`.)
 pub const MMAP_THRESHOLD: u64 = 8 * 1024 * 1024;
 
+/// Whether `Source::open_as` maps a file of `MMAP_THRESHOLD` bytes or more (`Large`) or reads
+/// every file (`Never`, `--no-mmap`), as ripgrep reads every file then (review r2 of the M5a
+/// plan, Codex #1). `--mmap` changes nothing here: a smaller file is read either way.
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+pub enum Mapping {
+    #[default]
+    Large,
+    Never,
+}
+
+impl Mapping {
+    /// The mapping `--mmap`/`--no-mmap` ask for.
+    pub fn of(args: &crate::cli::Args) -> Mapping {
+        match args.mmap_mode {
+            crate::cli::MmapMode::Never => Mapping::Never,
+            _ => Mapping::Large,
+        }
+    }
+}
+
 /// What `Source::head` gives the Detector.
 const HEAD: usize = 512;
 
@@ -27,6 +47,29 @@ impl Data {
     }
 }
 
+/// How a file's bytes become the text every stage reads (M5a plan, D10), as grep-searcher
+/// decodes them for rg 14.1.1: by its BOM (`Auto`), with `-E`'s encoding unless a BOM says
+/// another, or as they are with `-E none`.
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+pub enum Decoding {
+    #[default]
+    Auto,
+    Label(&'static encoding_rs::Encoding),
+    Raw,
+}
+
+impl Decoding {
+    /// The decoding `-E` asks for.
+    pub fn of(args: &crate::cli::Args) -> Decoding {
+        match &args.encoding {
+            crate::cli::EncodingMode::Auto => Decoding::Auto,
+            crate::cli::EncodingMode::Disabled => Decoding::Raw,
+            crate::cli::EncodingMode::Label(l) => encoding_rs::Encoding::for_label(l.as_bytes())
+                .map_or(Decoding::Auto, Decoding::Label),
+        }
+    }
+}
+
 pub struct Source {
     /// The path as printed.
     pub path: PathBuf,
@@ -37,16 +80,27 @@ pub struct Source {
     skip: usize,
     /// The text of a UTF-16 file, transcoded to UTF-8.
     decoded: Option<Vec<u8>>,
-    /// The raw file started with a BOM (UTF-8 or UTF-16). grep-searcher then searches with a
-    /// reader even when memory maps are on (grep-searcher 0.1.14, searcher/mod.rs:762), so C9 does the same.
+    /// The raw file started with a BOM (UTF-8 or UTF-16), or `-E` named an encoding: grep-searcher
+    /// then transcodes, through a reader even when memory maps are on (grep-searcher 0.1.14,
+    /// searcher/mod.rs:762 and 804), so C9 does the same.
     pub had_bom: bool,
 }
 
 impl Source {
     pub fn open(path: &Path, explicit: bool) -> std::io::Result<Self> {
+        Self::open_as(path, explicit, Decoding::Auto, Mapping::Large)
+    }
+
+    /// `open`, decoded as `-E` asks (`Decoding`) and mapped as `--no-mmap` allows (`Mapping`).
+    pub fn open_as(
+        path: &Path,
+        explicit: bool,
+        decoding: Decoding,
+        mapping: Mapping,
+    ) -> std::io::Result<Self> {
         let file = std::fs::File::open(path)?;
         let len = file.metadata()?.len();
-        let data = if len >= MMAP_THRESHOLD {
+        let data = if len >= MMAP_THRESHOLD && mapping == Mapping::Large {
             // SAFETY: read-only map; see MMAP_THRESHOLD about concurrent truncation.
             Data::Mapped(unsafe { memmap2::Mmap::map(&file)? })
         } else {
@@ -54,16 +108,37 @@ impl Source {
             std::io::Read::read_to_end(&mut &file, &mut v)?;
             Data::Owned(v)
         };
-        Ok(Self::decode(path.to_path_buf(), explicit, data))
+        Ok(Self::decode(path.to_path_buf(), explicit, data, decoding))
     }
 
     /// For stdin and, from M4, Container Members.
     pub fn from_bytes(path: PathBuf, bytes: Vec<u8>, explicit: bool) -> Self {
-        Self::decode(path, explicit, Data::Owned(bytes))
+        Self::from_bytes_as(path, bytes, explicit, Decoding::Auto)
     }
 
-    fn decode(path: PathBuf, explicit: bool, data: Data) -> Self {
+    /// `from_bytes`, decoded as `-E` asks (`Decoding`).
+    pub fn from_bytes_as(
+        path: PathBuf,
+        bytes: Vec<u8>,
+        explicit: bool,
+        decoding: Decoding,
+    ) -> Self {
+        Self::decode(path, explicit, Data::Owned(bytes), decoding)
+    }
+
+    fn decode(path: PathBuf, explicit: bool, data: Data, decoding: Decoding) -> Self {
         let raw = data.raw();
+        // `-E none`: no BOM is sniffed, and the bytes are searched as they are.
+        if decoding == Decoding::Raw {
+            return Self {
+                path,
+                explicit,
+                data,
+                skip: 0,
+                decoded: None,
+                had_bom: false,
+            };
+        }
         if raw.starts_with(b"\xEF\xBB\xBF") {
             return Self {
                 path,
@@ -93,6 +168,24 @@ impl Source {
                 had_bom: true,
             };
         }
+        // `-E LABEL` without a BOM: the label's encoding, UTF-8 passed through as it is
+        // (encoding_rs_io's `utf8_passthru`), a sequence it cannot decode as U+FFFD.
+        if let Decoding::Label(enc) = decoding {
+            let decoded = (enc != encoding_rs::UTF_8).then(|| {
+                enc.decode_without_bom_handling(raw)
+                    .0
+                    .into_owned()
+                    .into_bytes()
+            });
+            return Self {
+                path,
+                explicit,
+                data,
+                skip: 0,
+                decoded,
+                had_bom: true,
+            };
+        }
         Self {
             path,
             explicit,
```

### Task 3.4: the callers in `app.rs`

**Files:** `src/app.rs`.

- [ ] **Step 1:** `Messages::ignore` and `--no-ignore-messages`, the `--ignore-file` errors
  printed where rg's walker prints them (after the patterns are known to be searched, before
  the matcher), an entry's ignore-file error printed as the walk yields it (`entry_error`), the
  walk sorted when the walker cannot sort (D8), `rg_in_memory` with the arguments, the decoding
  in every `Source` built from a file's bytes and the mapping in every one opened by name (D9),
  and the new signatures of `match_submatches_in`
  and `number_lines`:

```diff
diff --git a/src/app.rs b/src/app.rs
index 91485b0..7bdff4f 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -62,6 +62,8 @@ pub fn run(argv: Vec<OsString>) -> i32 {
 /// makes the exit code 2 (as ripgrep).
 struct Messages {
     silent: bool,
+    /// `--no-ignore-messages`: the errors of ignore files are not printed.
+    ignore_silent: bool,
     errored: AtomicBool,
 }
 
@@ -78,6 +80,15 @@ impl Messages {
             eprintln!("{msg}");
         }
     }
+
+    /// An error of an ignore file (a glob it cannot parse, an `--ignore-file` it cannot read):
+    /// printed unless `--no-messages` or `--no-ignore-messages`, and no error for the exit code,
+    /// as rg 14.1.1's `ignore_message!`.
+    fn ignore(&self, msg: impl std::fmt::Display) {
+        if !self.silent && !self.ignore_silent && !stopped() {
+            eprintln!("s4: {msg}");
+        }
+    }
 }
 
 /// A SIGINT or SIGTERM reached s4, which is about to die by it: from then on it prints
@@ -159,6 +170,7 @@ fn install_grammar(lang: &str) -> i32 {
 fn main(args: &Args) -> anyhow::Result<i32> {
     let msgs = Messages {
         silent: args.no_messages,
+        ignore_silent: args.no_ignore_messages,
         errored: AtomicBool::new(false),
     };
     // Actions (step 3).
@@ -220,7 +232,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
     // validated in this order before a pattern is compiled (code review r3, Codex #4). The
     // walker is built once, here, and walks the paths (code review r4, Fable #1).
     let cwd = std::env::current_dir()?;
-    let walker = search::walker(args, &paths, &cwd)?;
+    let (walker, ignore_errors) = search::walker_with(args, &paths, &cwd)?;
     let pre = Preprocess::new(args, &cwd)?;
     // Nothing can match: ripgrep 14.1.1 searches nothing and exits 1, in every mode and without
     // a JSON summary, once its file types and globs are valid (an unknown type is still an
@@ -232,6 +244,10 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         }
         return Ok(1);
     }
+    // rg 14.1.1 adds the --ignore-file files as it builds its walker, before the matcher.
+    for e in ignore_errors {
+        msgs.ignore(e);
+    }
     // The error carries rg 14.1.1's suggestions (`matcher::build`).
     let matcher = search::matcher(args, &pats.patterns)?;
     let one_file = paths.len() == 1 && (paths[0] == Path::new("-") || !paths[0].is_dir());
@@ -345,7 +361,9 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         linked: AtomicBool::new(false),
         msgs,
         pre,
-        in_memory: search::rg_in_memory(&paths, implicit_stdin),
+        decoding: crate::source::Decoding::of(args),
+        mapping: crate::source::Mapping::of(args),
+        in_memory: search::rg_in_memory(args, &paths, implicit_stdin),
         paths,
         walker,
         strip_dot: implicit_cwd,
@@ -652,6 +670,11 @@ struct Ctx<'a> {
     processors: processors::Processors,
     /// `--pre` and `-z`.
     pre: Preprocess,
+    /// How files are decoded: by their BOM, or as `-E` says (M5a plan, D10).
+    decoding: crate::source::Decoding,
+    /// Whether a large file is mapped: never with `--no-mmap` (review r2 of the M5a plan,
+    /// Codex #1).
+    mapping: crate::source::Mapping,
     /// The Format Definitions and their Detector, which with --docs tells documents by their
     /// name or magic bytes: loaded in `main`, or once needed (`Ctx::detector`).
     registry: &'a std::sync::OnceLock<Registry>,
@@ -683,7 +706,31 @@ impl<'a> Ctx<'a> {
         if self.sequential {
             let mut state = init();
             let mut above = Above::default();
-            for r in w.build() {
+            // `--sort` other than by path, and `--sortr`: every entry first, then in order, as
+            // rg 14.1.1 sorts its haystacks (M5a plan, D8).
+            let entries: Box<dyn Iterator<Item = Result<DirEntry, ignore::Error>>> =
+                match self.args.sort {
+                    Some(sort) if !search::walker_sorts(self.args) => {
+                        let mut all = Vec::new();
+                        for r in w.build() {
+                            match r {
+                                Ok(d) => {
+                                    self.entry_error(&d);
+                                    all.push(d);
+                                }
+                                Err(e) => self.msgs.error(e),
+                            }
+                        }
+                        search::sort_entries(&mut all, sort);
+                        Box::new(all.into_iter().map(Ok))
+                    }
+                    _ => Box::new(w.build().inspect(|r| {
+                        if let Ok(d) = r {
+                            self.entry_error(d);
+                        }
+                    })),
+                };
+            for r in entries {
                 if self.quit.load(Relaxed) {
                     break;
                 }
@@ -707,6 +754,7 @@ impl<'a> Ctx<'a> {
                     }
                     match r {
                         Ok(d) => {
+                            self.entry_error(&d);
                             if let Some(t) = self.target(&d, &mut above) {
                                 visit(&mut state, self.prepare(t));
                             }
@@ -724,6 +772,14 @@ impl<'a> Ctx<'a> {
         Ok(())
     }
 
+    /// The error of an ignore file the walker read for this entry (a glob it cannot parse),
+    /// printed as rg 14.1.1's `Haystack::build` prints it.
+    fn entry_error(&self, d: &DirEntry) {
+        if let Some(e) = d.error() {
+            self.msgs.ignore(e);
+        }
+    }
+
     /// ripgrep's choice of what to search: stdin, every file named on the command line, and
     /// regular files found by walking.
     fn target(&self, d: &DirEntry, above: &mut Above) -> Option<Target> {
@@ -1023,10 +1079,13 @@ impl<'a> Ctx<'a> {
     /// the whole file and no search read them yet, else read by its name.
     fn source(&self, t: &Target, p: &Path) -> std::io::Result<Source> {
         match t.held.take() {
-            Some(Held { head, rest: None }) => {
-                Ok(Source::from_bytes(p.to_path_buf(), head, t.explicit))
-            }
-            _ => Source::open(p, t.explicit),
+            Some(Held { head, rest: None }) => Ok(Source::from_bytes_as(
+                p.to_path_buf(),
+                head,
+                t.explicit,
+                self.decoding,
+            )),
+            _ => Source::open_as(p, t.explicit, self.decoding, self.mapping),
         }
     }
 
@@ -1452,7 +1511,7 @@ impl<'a> Ctx<'a> {
         if !self.in_memory && self.probe(st, t, path) != Some(true) {
             return true;
         }
-        let src = match Source::open(path, t.explicit) {
+        let src = match Source::open_as(path, t.explicit, self.decoding, self.mapping) {
             Ok(mut s) => {
                 s.path = t.path.clone();
                 s
@@ -1730,13 +1789,16 @@ impl<'a> Ctx<'a> {
             return Ok(None);
         }
         let src = match (kept, &t.fs) {
-            (Some(tee), _) => Source::from_bytes(t.path.clone(), tee.read, t.explicit),
+            (Some(tee), _) => {
+                Source::from_bytes_as(t.path.clone(), tee.read, t.explicit, self.decoding)
+            }
             (None, Some(p)) => open(p)?,
             (None, None) => unreachable!("stdin is always kept"),
         };
         let mut items = sink.items;
         if stats.binary_offset.is_none() {
-            search::number_lines(&mut items, src.bytes());
+            let term = search::line_terminator(self.args).as_byte();
+            search::number_lines(&mut items, src.bytes(), term);
         } else {
             // The searcher counts a NUL it converted as a line terminator: the bytes as the
             // first search read them, searched again counting lines.
@@ -1771,7 +1833,7 @@ impl<'a> Ctx<'a> {
             return false;
         }
         let head = read[..read.len().min(crate::source::READ_HEAD)].to_vec();
-        let partial = Source::from_bytes(t.path.clone(), head, t.explicit);
+        let partial = Source::from_bytes_as(t.path.clone(), head, t.explicit, self.decoding);
         detector
             .detect(&partial.path, partial.head())
             .is_some_and(|d| self.processors.describes(d))
@@ -2363,7 +2425,7 @@ impl<'a> Ctx<'a> {
             report(&e);
             return Err(());
         }
-        let src = Source::from_bytes(t.path.clone(), tee.read, t.explicit);
+        let src = Source::from_bytes_as(t.path.clone(), tee.read, t.explicit, self.decoding);
         let binary = probe.and_then(|p| BinaryProbe::read(p, quit, &src));
         Ok((src, binary))
     }
@@ -2969,11 +3031,11 @@ impl Sink for StatsSink<'_> {
         Ok(true)
     }
 
-    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> std::io::Result<bool> {
+    fn matched(&mut self, s: &Searcher, m: &SinkMatch<'_>) -> std::io::Result<bool> {
         self.max.matched();
         self.lines += 1;
         self.stats.matched_lines += m.lines().count() as u64;
-        let subs = search::match_submatches(self.matcher, self.invert, m);
+        let subs = search::match_submatches_in(s, self.matcher, self.invert, m);
         self.stats.matches += subs.len() as u64;
         Ok(self.max.go_on())
     }
```

- [ ] **Step 2:** `cargo test --lib -- search:: source::`: PASS; the whole suite still passes.
- [ ] **Step 3:** commit: `Walk and search with rg's walk, search and encoding flags`.

---
## Chunk 4: the output core

### Task 4.1: the tests

**Files:** `src/output/mod.rs`, `src/output/style.rs` (tests only).

- [ ] **Step 1:** the tests of the rows and of the line writer (D4, D6, D11, D12). Their
  expectations are rg's: `-o` rows at the Match's column and offset, `--vimgrep` rows at the
  Match's, a `-U` block's lines numbered 1, 2, 3 under the Match's Trail, a record's lines with
  its number and without its NUL, a record of one line too (review r4), rg's ` [... 1 more match]`, the replaced line with each row's replacement
  highlighted, an object file's string with `-o`, `--vimgrep` and `-r` (a row per Match at that
  Match's offset and under its Trail, the string replaced in one row; with `-r` each Match
  replaced where the search found it in the file, so `^needle` is not replaced after a NUL and a
  control byte inside a Match or a group is replaced with it: review r2, Codex #2; each Match
  printed with the replacement the search made, `needle <A|>` where a search from the string
  would give `<t|>`: review r4, Codex #2), `-o` rows as
  rg's printer of one line prints them (an empty Match and an empty replacement an empty row, a
  `\r` without `--crlf` printed: review r2, Codex #4), `--vimgrep` rows of a search of several
  lines without an empty Match at a line's start (`per_match_rows_of_a_multi_line_search`,
  review r3, Codex #4), `--trim`'s white space as
  grep-printer's (`\r` and `\x0B` included, the terminator's bytes never); and
  `a_match_of_a_records_line_break_stays_in_its_row`, the case C10's fuzz found (a `\n` Match
  inside a `--null-data` record panicked in the Fragments):

```diff
diff --git a/src/output/mod.rs b/src/output/mod.rs
index 98fd971..e266fcf 100644
--- a/src/output/mod.rs
+++ b/src/output/mod.rs
@@ -866,6 +1370,647 @@ pub(crate) mod tests {
         String::from_utf8(out).unwrap()
     }
 
+    /// (line number, text, highlighted ranges, column, byte offset, Trail names) of each row.
+    type Seen = (u64, String, Vec<(usize, usize)>, Option<u64>, u64, String);
+
+    fn seen(d: &Described, src: &[u8], o: &RenderOptions) -> Vec<Seen> {
+        rows(d, src, o)
+            .map(|r| {
+                let names: Vec<&str> = r.trail.iter().map(|s| s.name.as_str()).collect();
+                (
+                    r.line_number,
+                    r.line.text().to_string(),
+                    r.subs.clone(),
+                    r.column,
+                    r.byte_offset,
+                    names.join(" › "),
+                )
+            })
+            .collect()
+    }
+
+    fn with(look: Look) -> RenderOptions {
+        RenderOptions { look, ..opts() }
+    }
+
+    /// A line with two needles under two Trails, and a context line.
+    fn two_trails() -> (Vec<u8>, Described) {
+        use super::fixture::*;
+        let src = src(&["intro", "  a needle b needle"]);
+        let d = described(
+            &src,
+            vec![ctx(&src, 1), mat(&src, 2)],
+            vec![vec![
+                vec![seg("key", "A", 2, 2)],
+                vec![seg("key", "B", 2, 2)],
+            ]],
+        );
+        (src, d)
+    }
+
+    #[test]
+    fn rows_carry_column_and_byte_offset() {
+        let (src, d) = two_trails();
+        assert_eq!(
+            seen(&d, &src, &opts()),
+            [
+                (1, "intro".into(), vec![], None, 0, "A".into()),
+                (
+                    2,
+                    "  a needle b needle".into(),
+                    vec![(4, 10)],
+                    Some(5),
+                    6,
+                    "A".into()
+                ),
+                (
+                    2,
+                    "  a needle b needle".into(),
+                    vec![(13, 19)],
+                    Some(14),
+                    6,
+                    "B".into()
+                ),
+            ]
+        );
+    }
+
+    /// M5a plan, D4: `-o` makes a row of each Match, its text the matched bytes.
+    #[test]
+    fn only_matching_rows() {
+        let (src, d) = two_trails();
+        let o = with(Look {
+            only_matching: true,
+            ..Look::default()
+        });
+        assert_eq!(
+            seen(&d, &src, &o),
+            [
+                (1, "intro".into(), vec![], None, 0, "A".into()),
+                (2, "needle".into(), vec![(0, 6)], Some(5), 10, "A".into()),
+                (2, "needle".into(), vec![(0, 6)], Some(14), 19, "B".into()),
+            ]
+        );
+    }
+
+    /// `--vimgrep`: a row of each Match, its text the whole line.
+    #[test]
+    fn per_match_rows() {
+        let (src, d) = two_trails();
+        let o = with(Look {
+            per_match: true,
+            ..Look::default()
+        });
+        let got = seen(&d, &src, &o);
+        assert_eq!(
+            got[1],
+            (
+                2,
+                "  a needle b needle".into(),
+                vec![(4, 10)],
+                Some(5),
+                10,
+                "A".into()
+            )
+        );
+        assert_eq!(
+            got[2],
+            (
+                2,
+                "  a needle b needle".into(),
+                vec![(13, 19)],
+                Some(14),
+                19,
+                "B".into()
+            )
+        );
+    }
+
+    /// Review r3 of the M5a plan, Codex #4: `--vimgrep` in a search of several lines (`-U` with
+    /// a pattern that can match a line break) prints a row for the first line a Match touches,
+    /// so none for an empty Match at a line's start, as rg's printer of several lines; a search
+    /// of lines prints every Match.
+    #[test]
+    fn per_match_rows_of_a_multi_line_search() {
+        use super::fixture::*;
+        let src = src(&["needle", "next"]);
+        let columns = |n: u64, subs: &[(usize, usize)], multi_line: bool| -> Vec<(u64, u64)> {
+            let mut item = mat(&src, n);
+            if let Item::Match(l) = &mut item {
+                l.submatches = subs
+                    .iter()
+                    .map(|&(start, end)| Submatch { start, end })
+                    .collect();
+            }
+            let trails = vec![vec![seg("key", "A", 1, 2)]; subs.len()];
+            let d = described(&src, vec![item], vec![trails]);
+            let look = Look {
+                per_match: true,
+                multi_line,
+                ..Look::default()
+            };
+            seen(&d, &src, &with(look))
+                .into_iter()
+                .map(|r| (r.0, r.3.unwrap()))
+                .collect()
+        };
+        let subs = [(0, 0), (1, 2), (6, 6)];
+        assert_eq!(columns(1, &subs, true), [(1, 2), (1, 7)]);
+        assert_eq!(columns(1, &subs, false), [(1, 1), (1, 2), (1, 7)]);
+        assert_eq!(columns(2, &[(0, 0)], true), []);
+    }
+
+    /// M5a plan, D6: a `-U` Match prints a row per line, numbered as rg numbers them, under
+    /// its Trail; with `-o`, each line of the Match that is not empty.
+    #[test]
+    fn multi_line_blocks() {
+        use super::fixture::*;
+        let src = src(&["## Linux", "", "Run the needle"]);
+        let mut m = Line::owned(1, 0, &src[..]);
+        m.owned = None;
+        m.submatches = vec![Submatch { start: 3, end: 13 }];
+        let d = described(
+            &src,
+            vec![Item::Match(m)],
+            vec![vec![vec![seg("heading", "Linux", 1, 3)]]],
+        );
+        let o = with(Look {
+            multi_line: true,
+            ..Look::default()
+        });
+        assert_eq!(
+            seen(&d, &src, &o),
+            [
+                (
+                    1,
+                    "## Linux".into(),
+                    vec![(3, 8)],
+                    Some(4),
+                    0,
+                    "Linux".into()
+                ),
+                (2, "".into(), vec![(0, 0)], Some(4), 9, "Linux".into()),
+                (
+                    3,
+                    "Run the needle".into(),
+                    vec![(0, 3)],
+                    Some(4),
+                    10,
+                    "Linux".into()
+                ),
+            ]
+        );
+        let o = with(Look {
+            multi_line: true,
+            only_matching: true,
+            ..Look::default()
+        });
+        let got: Vec<(u64, String)> = seen(&d, &src, &o).into_iter().map(|r| (r.0, r.1)).collect();
+        assert_eq!(got, [(1, "Linux".into()), (3, "Run".into())]);
+    }
+
+    /// A `--null-data` record's lines are rows with the record's number, its NUL not printed.
+    #[test]
+    fn null_data_records() {
+        use super::fixture::*;
+        let src = b"one needle\ntwo\0".to_vec();
+        let mut m = Line::owned(1, 0, &src);
+        m.owned = None;
+        m.submatches = vec![Submatch { start: 4, end: 10 }];
+        let d = described(&src, vec![Item::Match(m)], vec![vec![vec![]]]);
+        let o = with(Look {
+            null_data: true,
+            ..Look::default()
+        });
+        let got: Vec<(u64, String)> = seen(&d, &src, &o).into_iter().map(|r| (r.0, r.1)).collect();
+        assert_eq!(got, [(1, "one needle".into()), (1, "two".into())]);
+        // A record of one line too (review r4 of the plan, found checking Fable #1).
+        let src = b"a needle\0".to_vec();
+        let mut m = Line::owned(1, 0, &src);
+        m.owned = None;
+        m.submatches = vec![Submatch { start: 2, end: 8 }];
+        let d = described(&src, vec![Item::Match(m)], vec![vec![vec![]]]);
+        let got: Vec<(u64, String)> = seen(&d, &src, &o).into_iter().map(|r| (r.0, r.1)).collect();
+        assert_eq!(got, [(1, "a needle".into())]);
+    }
+
+    /// A Match of the line break inside a `--null-data` record ends where its row's text ends
+    /// (found by tests/fuzz_m5.rs: its range past the text panicked in the Fragments).
+    #[test]
+    fn a_match_of_a_records_line_break_stays_in_its_row() {
+        use super::fixture::*;
+        let src = format!("# Long\n\n{} needle\n", "x".repeat(300)).into_bytes();
+        let mut m = Line::owned(1, 0, &src);
+        m.owned = None;
+        m.submatches = memchr::memchr_iter(b'\n', &src)
+            .map(|i| Submatch {
+                start: i,
+                end: i + 1,
+            })
+            .collect();
+        let d = described(&src, vec![Item::Match(m)], vec![vec![vec![]; 3]]);
+        let o = RenderOptions {
+            width: 40,
+            look: Look {
+                null_data: true,
+                ..Look::default()
+            },
+            ..opts()
+        };
+        let got = seen(&d, &src, &o);
+        assert_eq!(got.len(), 3);
+        assert!(
+            got.iter().all(|r| r.2.iter().all(|&(_, e)| e <= r.1.len())),
+            "{got:?}"
+        );
+        let mut w = NoColor::new(Vec::new());
+        super::flat::render(
+            &d,
+            &src,
+            &RenderOptions {
+                layout: Layout::Flat,
+                ..o
+            },
+            None,
+            &mut w,
+        )
+        .unwrap();
+    }
+
+    /// `-r`: each row's text is the line with its Matches replaced, and its highlight covers
+    /// its own replacements; the column is the replacement's, as rg's.
+    #[test]
+    fn replaced_rows() {
+        let (src, d) = two_trails();
+        let a = crate::cli::parse(["s4", "needle"].iter().map(Into::into).collect()).unwrap();
+        let m = crate::matcher::build(&a, &["needle".into()]).unwrap();
+        let r = replace::Replacer::new(
+            m,
+            b"N".to_vec(),
+            grep_matcher::LineTerminator::byte(b'\n'),
+            false,
+        );
+        let o = with(Look {
+            replace: Some(std::sync::Arc::new(r)),
+            ..Look::default()
+        });
+        let got = seen(&d, &src, &o);
+        assert_eq!(
+            got[1],
+            (2, "  a N b N".into(), vec![(4, 5)], Some(5), 6, "A".into())
+        );
+        assert_eq!(
+            got[2],
+            (2, "  a N b N".into(), vec![(8, 9)], Some(9), 6, "B".into())
+        );
+    }
+
+    /// A Replacer of `pattern` by `with`, as `-r` builds it.
+    fn replacer(pattern: &str, with: &[u8]) -> Look {
+        let a = crate::cli::parse(["s4", "x"].iter().map(Into::into).collect()).unwrap();
+        let m = crate::matcher::build(&a, &[pattern.into()]).unwrap();
+        let r = replace::Replacer::new(
+            m,
+            with.to_vec(),
+            grep_matcher::LineTerminator::byte(b'\n'),
+            false,
+        );
+        Look {
+            replace: Some(std::sync::Arc::new(r)),
+            ..Look::default()
+        }
+    }
+
+    /// Review r2 of the M5a plan, Codex #2: an object file's `-r` replaces each Match where the
+    /// search found it in the file's bytes, its anchors and groups read there, and only then
+    /// makes the string printable. `^needle` does not match after a NUL, and a control byte in a
+    /// Match is replaced with the Match.
+    #[test]
+    fn object_rows_replace_over_the_files_bytes() {
+        use super::fixture::*;
+        let src = b"\x7fELF\0\0needle target\0zz ab\x01cd yy\0".to_vec();
+        // As processors::object::rows makes them: each string at its offset, the control bytes
+        // of its Matches shown as `.`.
+        let row = |at: u64, text: &[u8], sub: (usize, usize)| {
+            let mut l = Line::owned(1, at, text);
+            l.submatches = vec![Submatch {
+                start: sub.0,
+                end: sub.1,
+            }];
+            l
+        };
+        let base = || vec![vec![seg("object", "ELF x86-64", 1, 1)]];
+        // The replacements as the search of the file makes them: one pass over each line.
+        let texts = |l: Line, look: Look| -> Vec<(String, Vec<(usize, usize)>)> {
+            let mut d = described(&src, vec![Item::Match(l)], vec![base()]);
+            d.offsets = true;
+            let r = look.replace.as_ref().unwrap();
+            d.replacements = r.replacements(&src, 0..src.len());
+            rows(&d, &src, &with(look))
+                .map(|r| (r.line.text().to_string(), r.subs.clone()))
+                .collect()
+        };
+        let target = || row(6, b"needle target", (7, 13));
+        assert_eq!(
+            texts(target(), replacer("^needle|target", b"X")),
+            [("needle X".to_string(), vec![(7, 8)])]
+        );
+        let control = || row(20, b"zz ab.cd yy", (3, 8));
+        assert_eq!(
+            texts(control(), replacer("ab\\x01cd", b"X")),
+            [("zz X yy".to_string(), vec![(3, 4)])]
+        );
+        // A group that holds the control byte: read from the file, then shown as `.`.
+        assert_eq!(
+            texts(control(), replacer("a(b\\x01c)d", b"<$1>")),
+            [("zz <b.c> yy".to_string(), vec![(3, 8)])]
+        );
+        let only = Look {
+            only_matching: true,
+            ..replacer("a(b\\x01c)d", b"<$1>")
+        };
+        assert_eq!(
+            texts(control(), only),
+            [("<b.c>".to_string(), vec![(0, 5)])]
+        );
+        // Review r4 of the plan, Codex #1 and #2: each Match prints the replacement the search
+        // made, with the groups it found (PCRE2's `(A)\x00\KB|(B)` gives `<A|>` where a search
+        // from the string's start finds the second alternative), and is not looked for again;
+        // a Match without one keeps its bytes.
+        let mut d = described(&src, vec![Item::Match(target())], vec![base()]);
+        d.offsets = true;
+        d.replacements = vec![((13, 19), b"<A|>".to_vec())];
+        let look = with(replacer("(t)arget|(target)", b"<$1|$2>"));
+        let got: Vec<String> = rows(&d, &src, &look)
+            .map(|r| r.line.text().to_string())
+            .collect();
+        assert_eq!(got, ["needle <A|>"]);
+        d.replacements.clear();
+        let got: Vec<String> = rows(&d, &src, &look)
+            .map(|r| r.line.text().to_string())
+            .collect();
+        assert_eq!(got, ["needle target"]);
+    }
+
+    /// Review r2 of the M5a plan, Codex #4: `-o` prints a row for every Match, as rg's printer
+    /// of one line does: an empty Match and an empty replacement are an empty row, and only the
+    /// active line terminator is left out (a `\r` Match without `--crlf` is printed).
+    #[test]
+    fn only_matching_rows_as_ripgrep() {
+        use super::fixture::*;
+        let texts = |src: &[u8], subs: &[(usize, usize)], look: Look| -> Vec<Seen> {
+            let mut item = mat(src, 1);
+            if let Item::Match(l) = &mut item {
+                l.submatches = subs
+                    .iter()
+                    .map(|&(start, end)| Submatch { start, end })
+                    .collect();
+            }
+            let trails = vec![vec![seg("key", "A", 1, 1)]; subs.len()];
+            let d = described(src, vec![item], vec![trails]);
+            seen(
+                &d,
+                src,
+                &with(Look {
+                    only_matching: true,
+                    ..look
+                }),
+            )
+        };
+        let row = |text: &str, subs: Vec<(usize, usize)>, column, at| {
+            (1, text.to_string(), subs, Some(column), at, "A".to_string())
+        };
+        assert_eq!(
+            texts(b"needle\n", &[(0, 6)], replacer("needle", b"")),
+            [row("", vec![], 1, 0)]
+        );
+        assert_eq!(
+            texts(b"needle\n", &[(0, 0), (3, 3)], Look::default()),
+            [row("", vec![], 1, 0), row("", vec![], 4, 3)]
+        );
+        assert_eq!(
+            texts(b"needle\r\n", &[(6, 7)], Look::default()),
+            [row("\r", vec![(0, 1)], 7, 6)]
+        );
+    }
+
+    /// An object file's rows (review r1 of the plan, Codex #4): `-o` and `--vimgrep` make a
+    /// row of each Match, at that Match's offset and under its Trail; `-r` replaces in the
+    /// string, which stays one row at its first Match.
+    #[test]
+    fn object_rows_take_o_vimgrep_and_r() {
+        let (src, d) = super::fixture::object_row_twice();
+        type ObjectRow = (Option<u64>, String, Vec<(usize, usize)>, String);
+        let at = |d: &Described, o: &RenderOptions| -> Vec<ObjectRow> {
+            rows(d, &src, o)
+                .map(|r| {
+                    let names: Vec<&str> = r.trail.iter().map(|s| s.name.as_str()).collect();
+                    (
+                        r.offset,
+                        r.line.text().to_string(),
+                        r.subs.clone(),
+                        names.join(" › "),
+                    )
+                })
+                .collect()
+        };
+        let only = with(Look {
+            only_matching: true,
+            ..Look::default()
+        });
+        assert_eq!(
+            at(&d, &only),
+            [
+                (
+                    Some(6),
+                    "needle".into(),
+                    vec![(0, 6)],
+                    "ELF x86-64 › .rodata › s".into()
+                ),
+                (
+                    Some(13),
+                    "needle".into(),
+                    vec![(0, 6)],
+                    "ELF x86-64 › .rodata › s+0x7".into()
+                ),
+            ]
+        );
+        let vim = with(Look {
+            per_match: true,
+            ..Look::default()
+        });
+        assert_eq!(
+            at(&d, &vim),
+            [
+                (
+                    Some(6),
+                    "needle needle".into(),
+                    vec![(0, 6)],
+                    "ELF x86-64 › .rodata › s".into()
+                ),
+                (
+                    Some(13),
+                    "needle needle".into(),
+                    vec![(7, 13)],
+                    "ELF x86-64 › .rodata › s+0x7".into()
+                ),
+            ]
+        );
+        let a = crate::cli::parse(["s4", "needle"].iter().map(Into::into).collect()).unwrap();
+        let m = crate::matcher::build(&a, &["needle".into()]).unwrap();
+        let r = replace::Replacer::new(
+            m,
+            b"N".to_vec(),
+            grep_matcher::LineTerminator::byte(b'\n'),
+            false,
+        );
+        // The replacements the search of the file makes, in one pass over its line.
+        let mut d = d.clone();
+        d.replacements = r.replacements(&src, 0..src.len());
+        let replaced = with(Look {
+            replace: Some(std::sync::Arc::new(r)),
+            ..Look::default()
+        });
+        assert_eq!(
+            at(&d, &replaced),
+            [(
+                Some(6),
+                "N N".into(),
+                vec![(0, 1), (2, 3)],
+                "ELF x86-64 › .rodata › s".into()
+            )]
+        );
+    }
+
+    /// `--trim` leaves out grep-printer's white space (`\t`, `\n`, `\x0B`, `\x0C`, `\r`, ` `)
+    /// but never a byte of the line terminator (review r1 of the plan, Codex #8).
+    #[test]
+    fn trim_leaves_out_greps_white_space_but_the_terminator() {
+        let o = |crlf: bool, null_data: bool| {
+            with(Look {
+                trim: true,
+                crlf,
+                null_data,
+                ..Look::default()
+            })
+        };
+        assert_eq!(trimmed(b"\r needle\n", &o(false, false)), 2);
+        assert_eq!(trimmed(b"\x0b needle\n", &o(false, false)), 2);
+        assert_eq!(trimmed(b"\x0c\t needle\n", &o(false, false)), 3);
+        assert_eq!(trimmed(b"\r needle\r\n", &o(true, false)), 0);
+        assert_eq!(trimmed(b"\n needle\n", &o(false, false)), 0);
+        assert_eq!(trimmed(b"\n\r needle\0", &o(false, true)), 3);
+        assert_eq!(trimmed(b"\0 needle", &o(false, true)), 0);
+    }
+
+    fn written(line: &[u8], subs: &[(usize, usize)], color: bool, o: &RenderOptions) -> String {
+        let pre = |w: &mut dyn WriteColor| w.write_all(b"P:");
+        let out = if color {
+            let mut w = Ansi::new(Vec::new());
+            write_line(&mut w, &pre, line, subs, true, 80, o).unwrap();
+            w.into_inner()
+        } else {
+            let mut w = NoColor::new(Vec::new());
+            write_line(&mut w, &pre, line, subs, true, 80, o).unwrap();
+            w.into_inner()
+        };
+        String::from_utf8(out).unwrap()
+    }
+
+    #[test]
+    fn trim_leaves_out_leading_ascii_space() {
+        let o = with(Look {
+            trim: true,
+            ..Look::default()
+        });
+        assert_eq!(
+            written(b" \t needle x\n", &[(3, 9)], false, &o),
+            "P:needle x\n"
+        );
+        assert_eq!(
+            written(b"  needle\n", &[(2, 8)], true, &o),
+            "P:\x1b[0m\x1b[1m\x1b[31mneedle\x1b[0m\n"
+        );
+    }
+
+    /// `--max-columns-preview`, as grep-printer's `write_exceeded_line`.
+    #[test]
+    fn previews_of_long_lines() {
+        let o = RenderOptions {
+            max_columns: Some(10),
+            look: Look {
+                max_columns_preview: true,
+                ..Look::default()
+            },
+            ..opts()
+        };
+        let line = b"Some intro needle here.\n";
+        assert_eq!(
+            written(line, &[(11, 17)], false, &o),
+            "P:Some intro [... omitted end of long line]\n"
+        );
+        assert_eq!(
+            written(line, &[(11, 17)], true, &o),
+            "P:Some intro [... 1 more match]\n"
+        );
+        assert_eq!(
+            written(b"Run the needle command:\n", &[(8, 14)], true, &o),
+            "P:Run the \x1b[0m\x1b[1m\x1b[31mne\x1b[0m [... 0 more matches]\n"
+        );
+    }
+
+    #[test]
+    fn the_palette_colours_matches_and_markers() {
+        let o = with(Look {
+            palette: style::Palette::new(&["match:fg:blue".into(), "match:style:nobold".into()]),
+            ..Look::default()
+        });
+        assert_eq!(
+            written(b"a needle\n", &[(2, 8)], true, &o),
+            "P:a \x1b[0m\x1b[34mneedle\x1b[0m\n"
+        );
+    }
+
+    #[test]
+    fn paths_with_a_separator_and_links() {
+        let look = Look {
+            path_separator: Some(b'\\'),
+            ..Look::default()
+        };
+        assert_eq!(
+            shown_path(std::path::Path::new("a/b/c.md"), &look),
+            b"a\\b\\c.md"
+        );
+        assert_eq!(
+            shown_path(std::path::Path::new("a/b"), &Look::default()),
+            b"a/b"
+        );
+        let d = tempfile::tempdir().unwrap();
+        let p = d.path().join("f.md");
+        std::fs::write(&p, "x").unwrap();
+        let look = Look {
+            hyperlinks: hyperlink::Hyperlinks::new("file://{path}:{line}", None)
+                .map(std::sync::Arc::new),
+            ..Look::default()
+        };
+        let mut w = Ansi::new(Vec::new());
+        let on = link_start(&mut w, &look, &p, Some(3), None).unwrap();
+        std::io::Write::write_all(&mut w, b"f.md").unwrap();
+        link_end(&mut w, on).unwrap();
+        let out = String::from_utf8(w.into_inner()).unwrap();
+        let canon = p.canonicalize().unwrap();
+        assert_eq!(
+            out,
+            format!(
+                "\x1b]8;;file://{}:3\x1b\\f.md\x1b]8;;\x1b\\",
+                canon.display()
+            )
+        );
+        // No colour, no link.
+        let mut w = NoColor::new(Vec::new());
+        assert!(!link_start(&mut w, &look, &p, Some(3), None).unwrap());
+    }
+
     #[test]
     fn submatches_group_by_trail_in_the_order_their_trails_come() {
         use super::fixture::seg;
```

```diff
diff --git a/src/output/style.rs b/src/output/style.rs
index 1daa7f9..7448c72 100644
--- a/src/output/style.rs
+++ b/src/output/style.rs
@@ -108,6 +183,24 @@ mod tests {
         String::from_utf8(w.into_inner()).unwrap()
     }
 
+    /// M5a plan, D11: `--colors` over rg's defaults, and the `trail` type of s4.
+    #[test]
+    fn palette_of_colors_specs() {
+        let d = Palette::default();
+        assert_eq!(d.spec("path").fg(), Some(&Color::Magenta));
+        assert_eq!(d.spec("trail").fg(), Some(&Color::Cyan));
+        assert!(d.spec("match").bold());
+        let p = Palette::new(&[
+            "path:none".into(),
+            "trail:fg:yellow".into(),
+            "column:fg:blue".into(),
+        ]);
+        assert_eq!(p.spec("path").fg(), None);
+        assert_eq!(p.spec("trail").fg(), Some(&Color::Yellow));
+        assert_eq!(p.spec("column").fg(), Some(&Color::Blue));
+        assert!(p.spec("dim").dimmed());
+    }
+
     #[test]
     fn no_colour_writes_the_text_unchanged() {
         assert_eq!(plain("a NEEDLE b", &[(2, 8)]), "a NEEDLE b");
```

- [ ] **Step 2:** `cargo test --lib output::` does not compile (`Row::column`, `shown_path`,
  `link_start`, `Palette::new`'s colours, `Replacer::replacements`… are missing): RED.

### Task 4.2: rows, line writing, paths and links

**Files:** `src/output/mod.rs`, `src/output/flat.rs` (one call), `src/output/stream.rs`.

- [ ] **Step 1:** `LineText` over owned or borrowed bytes with its terminator's length, `trimmed`,
  `write_line_text` with `--trim`, `-M`'s preview and the palette, and a highlight clamped to
  the text; `Row`'s `column` and `byte_offset` (read by C7 and C8, so `#[allow(dead_code)]`
  until then); `match_rows` for `-o`, `--vimgrep`, `-r` and multi-line items, an object file's
  string through the same paths as one line at hex offsets (spec §5.7), its `-r` over the file's
  bytes before the string is made printable (`sunk`), a row for every `-o` Match unless a `-U`
  search left it empty (D6), a `--vimgrep` row at the first line a Match touches, none for an
  empty Match at a line's start in a search of several lines (D6), each row's highlight
  clipped at its active terminator only
  (`clip_line`), `context_rows`,
  `last_number`; `shown_path`, `link_start`, `link_end`; `write_path` public until C9 drops it
  (design note 6):

```diff
diff --git a/src/output/mod.rs b/src/output/mod.rs
index 98fd971..e266fcf 100644
--- a/src/output/mod.rs
+++ b/src/output/mod.rs
@@ -79,6 +79,36 @@ pub struct Look {
     pub palette: style::Palette,
     /// `--hyperlink-format`: paths and line numbers as links on a terminal with colour.
     pub hyperlinks: Option<std::sync::Arc<hyperlink::Hyperlinks>>,
+    /// `--crlf`: a row's `\r\n` is its terminator.
+    pub crlf: bool,
+    /// `--null-data`: an item is a NUL-terminated record, whose `\n` lines are rows that keep
+    /// its number.
+    pub null_data: bool,
+    /// `-U`: a Match may span lines, each a row numbered as rg numbers it.
+    pub multi_line: bool,
+}
+
+impl Look {
+    /// Nothing but M1–M4's: no prelude field, separator or text change of M5's (the
+    /// streaming printer's fast path).
+    pub fn plain(&self) -> bool {
+        !self.column
+            && !self.byte_offset
+            && !self.only_matching
+            && !self.per_match
+            && !self.trim
+            && !self.max_columns_preview
+            && self.replace.is_none()
+            && self.sep_match == b":"
+            && self.sep_context == b"-"
+            && self.sep_group.as_deref() == Some(&b"--"[..])
+            && self.path_terminator.is_none()
+            && self.path_separator.is_none()
+            && self.hyperlinks.is_none()
+            && !self.crlf
+            && !self.null_data
+            && !self.multi_line
+    }
 }
 
 impl Default for Look {
@@ -98,6 +128,9 @@ impl Default for Look {
             path_separator: None,
             palette: style::Palette::default(),
             hyperlinks: None,
+            crlf: false,
+            null_data: false,
+            multi_line: false,
         }
     }
 }
@@ -125,47 +158,69 @@ pub(crate) fn omission_message(subs: usize, is_match: bool, color: bool) -> Stri
 /// A line's text as tree and flat print it, worked out once for all the rows of the line: a
 /// line with Matches under many Trails prints one row per Trail, and decoding the line and
 /// indexing its characters for each row made those rows cost rows × line (M3 code review r3,
-/// Fable #1). Each part is computed when a row first needs it.
+/// Fable #1). Each part is computed when a row first needs it. The bytes are the line's, or
+/// with `-r` the line with its Matches replaced (M5a plan, D4).
 pub(crate) struct LineText<'a> {
     /// The raw line, terminator included.
-    pub bytes: &'a [u8],
-    /// The text, and for a line that is not valid UTF-8 the map from its bytes to the text's.
-    decoded: OnceCell<(Cow<'a, str>, Option<Vec<usize>>)>,
+    bytes: Cow<'a, [u8]>,
+    /// The terminator's bytes at the end of `bytes`, which are not printed.
+    cut: usize,
+    /// For a line that is not valid UTF-8, its text with U+FFFD and the map from its bytes to
+    /// the text's; None for valid UTF-8, which is its own text.
+    lossy: OnceCell<Option<(String, Vec<usize>)>>,
     chars: OnceCell<usize>,
     index: OnceCell<Vec<usize>>,
 }
 
 impl<'a> LineText<'a> {
+    /// A line whose terminator, if any, is one `\n`.
     pub(crate) fn new(bytes: &'a [u8]) -> Self {
+        let cut = usize::from(bytes.ends_with(b"\n"));
+        Self::with_cut(Cow::Borrowed(bytes), cut)
+    }
+
+    /// A line ending in `cut` bytes of terminator (`\r\n` with `--crlf`, NUL with
+    /// `--null-data`), owned or borrowed.
+    pub(crate) fn with_cut(bytes: Cow<'a, [u8]>, cut: usize) -> Self {
         LineText {
+            cut: cut.min(bytes.len()),
             bytes,
-            decoded: OnceCell::new(),
+            lossy: OnceCell::new(),
             chars: OnceCell::new(),
             index: OnceCell::new(),
         }
     }
 
-    fn raw(&self) -> &'a [u8] {
-        crate::text::strip_terminator(self.bytes)
+    /// The line, terminator included.
+    pub(crate) fn bytes(&self) -> &[u8] {
+        &self.bytes
     }
 
-    /// Valid UTF-8 is its own text, without the offset map, which costs a word per byte.
-    fn decoded(&self) -> &(Cow<'a, str>, Option<Vec<usize>>) {
-        self.decoded.get_or_init(|| {
+    fn raw(&self) -> &[u8] {
+        &self.bytes[..self.bytes.len() - self.cut]
+    }
+
+    fn lossy(&self) -> &Option<(String, Vec<usize>)> {
+        self.lossy.get_or_init(|| {
             let raw = self.raw();
-            match std::str::from_utf8(raw) {
-                Ok(text) => (Cow::Borrowed(text), None),
-                Err(_) => {
-                    let (text, map) = crate::text::lossy_with_map(raw);
-                    (Cow::Owned(text), Some(map))
-                }
-            }
+            std::str::from_utf8(raw)
+                .is_err()
+                .then(|| crate::text::lossy_with_map(raw))
         })
     }
 
     /// The text as printed, without its terminator, invalid bytes as U+FFFD.
     pub(crate) fn text(&self) -> &str {
-        &self.decoded().0
+        match self.lossy() {
+            Some((text, _)) => text,
+            // Checked valid by `lossy`.
+            None => std::str::from_utf8(self.raw()).unwrap_or_default(),
+        }
+    }
+
+    /// The map from the line's bytes to the text's, for a line that is not valid UTF-8.
+    fn map(&self) -> Option<&[usize]> {
+        self.lossy().as_ref().map(|(_, m)| m.as_slice())
     }
 
     /// The characters of `text`.
@@ -182,9 +237,10 @@ impl<'a> LineText<'a> {
 }
 
 /// Writes the text of one line for tree and flat: whole, as Fragments (one per output line,
-/// each preceded by `prefix`), or ripgrep's omission message with -M.
-/// `bytes` is the raw line; `subs` are byte ranges in it; `avail` is the width left after the
-/// caller's prefix.
+/// each preceded by `prefix`), or ripgrep's omission message with -M (with
+/// `--max-columns-preview`, the line's first columns before it). `--trim` leaves out the
+/// line's leading ASCII white space first. `bytes` is the raw line; `subs` are byte ranges in
+/// it; `avail` is the width left after the caller's prefix.
 pub fn write_line(
     w: &mut dyn WriteColor,
     prefix: &dyn Fn(&mut dyn WriteColor) -> std::io::Result<()>,
@@ -197,6 +253,27 @@ pub fn write_line(
     write_line_text(w, prefix, &LineText::new(bytes), subs, is_match, avail, o)
 }
 
+/// The number of leading white-space bytes `--trim` leaves out of `raw`: grep-printer's
+/// `trim_ascii_prefix`, whose white space is `\t`, `\n`, `\x0B`, `\x0C`, `\r` and ` `, less
+/// the bytes of the line terminator (`\n`; `\r\n` with `--crlf`; NUL with `--null-data`).
+pub(crate) fn trimmed(raw: &[u8], o: &RenderOptions) -> usize {
+    if !o.look.trim {
+        return 0;
+    }
+    let term: &[u8] = if o.look.null_data {
+        b"\0"
+    } else if o.look.crlf {
+        b"\r\n"
+    } else {
+        b"\n"
+    };
+    raw.iter()
+        .take_while(|&&b| {
+            matches!(b, b'\t' | b'\n' | 0x0B | 0x0C | b'\r' | b' ') && !term.contains(&b)
+        })
+        .count()
+}
+
 /// `write_line` for a line whose text the rows before may have worked out already.
 pub(crate) fn write_line_text(
     w: &mut dyn WriteColor,
@@ -207,20 +284,56 @@ pub(crate) fn write_line_text(
     avail: usize,
     o: &RenderOptions,
 ) -> std::io::Result<()> {
-    if omitted(line.bytes, o) {
+    let p = &o.look.palette;
+    let skip = trimmed(line.raw(), o);
+    if skip > 0 {
+        // The trimmed line, its submatches moved, as its own text.
+        let rest = &line.bytes()[skip..];
+        let text = LineText::with_cut(Cow::Borrowed(rest), line.cut);
+        let subs: Vec<(usize, usize)> = subs
+            .iter()
+            .filter(|&&(_, e)| e > skip)
+            .map(|&(s, e)| (s.saturating_sub(skip), e - skip))
+            .collect();
+        let o = RenderOptions {
+            look: Look {
+                trim: false,
+                ..o.look.clone()
+            },
+            ..o.clone()
+        };
+        return write_line_text(w, prefix, &text, &subs, is_match, avail, &o);
+    }
+    if omitted(line.bytes(), o) {
         prefix(w)?;
-        let msg = omission_message(subs.len(), is_match, w.supports_color());
+        if o.look.max_columns_preview {
+            return write_preview(w, line, subs, o);
+        }
+        // `-o` rows say nothing of their count (grep-printer's `write_exceeded_line`).
+        let n = if o.look.only_matching { 0 } else { subs.len() };
+        let msg = omission_message(n, is_match, w.supports_color());
         w.write_all(msg.as_bytes())?;
         return w.write_all(b"\n");
     }
     let raw = line.raw();
-    let (text, map) = line.decoded();
-    let subs: Cow<[(usize, usize)]> = match map {
+    let text = line.text();
+    // Ranges past the text (a Match of the line's terminator) end where the text ends.
+    let clamped: Vec<(usize, usize)>;
+    let subs = if subs.iter().any(|&(_, e)| e > raw.len()) {
+        clamped = subs
+            .iter()
+            .map(|&(s, e)| (s.min(raw.len()), e.min(raw.len())))
+            .collect();
+        &clamped[..]
+    } else {
+        subs
+    };
+    let subs: Cow<[(usize, usize)]> = match line.map() {
         // The common case: it fits (its bytes are at least as many as its characters, and
         // Fragments start beyond `avail` characters).
         None if raw.len() <= avail => {
             prefix(w)?;
-            style::write_highlighted(w, text, subs)?;
+            style::write_highlighted_in(w, p, text, subs)?;
             return w.write_all(b"\n");
         }
         None => Cow::Borrowed(subs),
@@ -238,18 +351,20 @@ pub(crate) fn write_line_text(
     match frags {
         None => {
             prefix(w)?;
-            style::write_highlighted(w, text, &subs)?;
+            style::write_highlighted_in(w, p, text, &subs)?;
             w.write_all(b"\n")
         }
         Some(frags) => {
             for f in frags {
                 prefix(w)?;
                 if f.omitted_before > 0 {
-                    style::write_styled(w, "dim", &format!("…[+{}] ", f.omitted_before))?;
+                    let mark = format!("…[+{}] ", f.omitted_before);
+                    style::write_in(w, p, "dim", mark.as_bytes())?;
                 }
-                style::write_highlighted(w, &f.text, &f.submatches)?;
+                style::write_highlighted_in(w, p, &f.text, &f.submatches)?;
                 if f.omitted_after > 0 {
-                    style::write_styled(w, "dim", &format!(" […+{}]", f.omitted_after))?;
+                    let mark = format!(" […+{}]", f.omitted_after);
+                    style::write_in(w, p, "dim", mark.as_bytes())?;
                 }
                 w.write_all(b"\n")?;
             }
@@ -258,6 +373,46 @@ pub(crate) fn write_line_text(
     }
 }
 
+/// `--max-columns-preview` (grep-printer's `write_exceeded_line`): the first `-M` graphemes of
+/// the line, its Matches in them highlighted, then ` [... omitted end of long line]`, or with
+/// colour and Matches ` [... N more matches]`, N those that start past the preview.
+fn write_preview(
+    w: &mut dyn WriteColor,
+    line: &LineText,
+    subs: &[(usize, usize)],
+    o: &RenderOptions,
+) -> std::io::Result<()> {
+    use bstr::ByteSlice;
+    let raw = line.raw();
+    let max = o.max_columns.unwrap_or(0) as usize;
+    let end = raw
+        .grapheme_indices()
+        .map(|(_, end, _)| end)
+        .take(max)
+        .last()
+        .unwrap_or(0);
+    let shown = LineText::new(&raw[..end]);
+    let clipped: Vec<(usize, usize)> = subs
+        .iter()
+        .filter(|&&(s, _)| s < end)
+        .map(|&(s, e)| (s, e.min(end)))
+        .collect();
+    let map = shown.map();
+    let clipped: Vec<(usize, usize)> = match map {
+        None => clipped,
+        Some(m) => clipped.iter().map(|&(s, e)| (m[s], m[e])).collect(),
+    };
+    style::write_highlighted_in(w, &o.look.palette, shown.text(), &clipped)?;
+    if subs.is_empty() || !w.supports_color() {
+        w.write_all(b" [... omitted end of long line]")?;
+    } else {
+        let more = subs.iter().filter(|&&(s, _)| s >= end).count();
+        let tense = if more == 1 { "match" } else { "matches" };
+        write!(w, " [... {more} more {tense}]")?;
+    }
+    w.write_all(b"\n")
+}
+
 /// Byte ranges of submatches in a line.
 type Subs = Vec<(usize, usize)>;
 
@@ -274,6 +429,13 @@ pub(crate) struct Row<'a> {
     pub is_match: bool,
     /// Context is on and this line does not follow the previous one: print `--` first.
     pub gap_before: bool,
+    /// `--column`: the 1-based column of the row's first Match, as rg counts it; None on a
+    /// context row. (Printed by flat and tree from chunks 7 and 8 on.)
+    #[allow(dead_code)]
+    pub column: Option<u64>,
+    /// `-b`: the byte offset of the row in the file, as rg gives it.
+    #[allow(dead_code)]
+    pub byte_offset: u64,
 }
 
 /// What tree and flat print in a file's place: its rows, or with --expand its Sections.
@@ -362,9 +524,133 @@ where
     groups
 }
 
-/// The rows of Match `i`: one per distinct Trail of its submatches, each with only its own
-/// submatches, except when -M omits the line: then one row under its first Trail with every
-/// submatch (design note 5).
+/// The lines of an item's bytes (M5a plan, D6): a `-U` Match spans several, and a
+/// `--null-data` record holds `\n` lines; each line is a row of its own. Each is `(start, end)`
+/// in the bytes, its `\n` included, and the item's own terminator (a record's NUL) left out.
+fn split_lines(bytes: &[u8], o: &RenderOptions) -> Vec<(usize, usize)> {
+    let end = if o.look.null_data && bytes.last() == Some(&0) {
+        bytes.len() - 1
+    } else {
+        bytes.len()
+    };
+    let mut lines = Vec::new();
+    let mut start = 0;
+    for nl in memchr::memchr_iter(b'\n', &bytes[..end]) {
+        lines.push((start, nl + 1));
+        start = nl + 1;
+    }
+    if start < end || lines.is_empty() {
+        lines.push((start, end));
+    }
+    lines
+}
+
+/// The terminator bytes at the end of a row's text that are not printed: `\r\n` with
+/// `--crlf`, else a `\n`.
+fn cut_of(bytes: &[u8], o: &RenderOptions) -> usize {
+    if o.look.crlf && bytes.ends_with(b"\r\n") {
+        2
+    } else {
+        usize::from(bytes.ends_with(b"\n"))
+    }
+}
+
+/// The number of line `k` of an item at `line_number`: rg numbers each line of a `-U` block,
+/// and gives a `--null-data` record's lines its own number.
+fn number_of(line_number: u64, k: usize, o: &RenderOptions) -> u64 {
+    if o.look.null_data {
+        line_number
+    } else {
+        line_number + k as u64
+    }
+}
+
+/// An object file's replacements, each Match's range in the Source with its replacement
+/// (`Described::replacements`).
+type Replacements = [((usize, usize), Vec<u8>)];
+
+/// The text of a line with its Matches replaced (`-r`), or as it is; and its submatches in it.
+/// `object`: the line is an object file's string (spec §5.7), with the replacements its search
+/// made (`Described::replacements`).
+fn sunk<'a>(
+    m: &'a Line,
+    src: &'a [u8],
+    subs: Vec<(usize, usize)>,
+    o: &RenderOptions,
+    object: Option<&Replacements>,
+) -> (Cow<'a, [u8]>, Vec<(usize, usize)>) {
+    let bytes = m.bytes(src);
+    let Some(r) = o.look.replace.as_ref().filter(|_| !subs.is_empty()) else {
+        return (Cow::Borrowed(bytes), subs);
+    };
+    // An object file's string is printable text made from the file's bytes: each Match is
+    // replaced as the search's pass over its line replaced it in those bytes, its anchors and
+    // groups read there (review r2 of the M5a plan, Codex #2; review r4, Codex #1 and #2), and
+    // the string made printable after. A Match the search did not replace keeps its bytes.
+    if let Some(replacements) = object {
+        let start = (m.absolute_offset as usize).min(src.len());
+        let string = &src[start..(start + m.len).min(src.len())];
+        let mut text = Vec::with_capacity(string.len());
+        let mut spans = Vec::with_capacity(subs.len());
+        let mut last = 0;
+        for &(s, e) in &subs {
+            let (s, e) = (s.min(string.len()), e.min(string.len()));
+            let with = replacements
+                .binary_search_by_key(&(start + s, start + e), |(at, _)| *at)
+                .map_or(&string[s..e], |i| &replacements[i].1[..]);
+            text.extend_from_slice(&string[last.min(s)..s]);
+            let at = text.len();
+            text.extend(
+                with.iter()
+                    .copied()
+                    .map(crate::processors::object::rows::shown),
+            );
+            spans.push((at, text.len()));
+            last = last.max(e);
+        }
+        text.extend_from_slice(&string[last..]);
+        return (Cow::Owned(text), spans);
+    }
+    // Over the Source, as rg's printer replaces over its buffer; a line converted from binary
+    // data is its own haystack.
+    let replaced = match &m.owned {
+        None => {
+            let start = (m.absolute_offset as usize).min(src.len());
+            r.replace(src, start..start + bytes.len())
+        }
+        Some(own) => r.replace(own, 0..own.len()),
+    };
+    let mapped = replaced.map(&subs);
+    (Cow::Owned(replaced.bytes), mapped)
+}
+
+/// `subs` clipped to the text of the line `bytes[start..end]`, its active terminator left out
+/// (a Match of a `\n` inside a `--null-data` record ends where its row's text ends; a `\r`
+/// without `--crlf` is text), and made relative to `start`.
+fn clip_line(
+    bytes: &[u8],
+    subs: &[(usize, usize)],
+    start: usize,
+    end: usize,
+    o: &RenderOptions,
+) -> Vec<(usize, usize)> {
+    clip(subs, start, end - cut_of(&bytes[start..end], o))
+}
+
+/// `subs` clipped to `start..end` and made relative to `start`.
+fn clip(subs: &[(usize, usize)], start: usize, end: usize) -> Vec<(usize, usize)> {
+    subs.iter()
+        .filter(|&&(s, e)| s < end && (e > start || (s == e && s >= start)))
+        .map(|&(s, e)| (s.max(start) - start, e.min(end) - start))
+        .collect()
+}
+
+/// The rows of Match `i` (M5a plan, D4 and D6): one per distinct Trail of its submatches,
+/// each with only its own submatches, except when -M omits the line: then one row under its
+/// first Trail with every submatch (design note 5). With `-o`, one row per Match, its text
+/// the matched bytes; with `--vimgrep`, one row per Match, its text the line. A `-U` block or
+/// a `--null-data` record prints a row per line. An object file's string is one line whatever
+/// it holds, its rows at hex offsets.
 fn match_rows<'a>(
     d: &'a Described,
     i: usize,
@@ -372,38 +658,214 @@ fn match_rows<'a>(
     src: &'a [u8],
     o: &RenderOptions,
 ) -> Vec<Row<'a>> {
-    let bytes = m.bytes(src);
-    // An object file's row is one string: one row, with the Trail of its first Match (spec
-    // §5.7; code review of the M4 plan r1, Codex #9).
-    let one = d.offsets || m.submatches.is_empty() || omitted(bytes, o);
-    let groups: Vec<(&'a [Segment], Subs)> = if one {
-        vec![(trail_of(d, i, 0), subs_of(&m.submatches))]
-    } else {
-        group_by_trail(
-            m.submatches
-                .iter()
-                .enumerate()
-                .map(|(k, s)| (trail_of(d, i, k), (s.start, s.end))),
-        )
+    let original = m.bytes(src);
+    let off = m.absolute_offset;
+    let found = subs_of(&m.submatches);
+    let object = d.offsets.then_some(&d.replacements[..]);
+    let (bytes, subs) = sunk(m, src, found.clone(), o, object);
+    let trails: Vec<&'a [Segment]> = (0..subs.len().max(1)).map(|k| trail_of(d, i, k)).collect();
+    // An object file's row is one string at the hex offset of a Match (spec §5.7): one row at
+    // its first Match, with that Match's Trail (code review of the M4 plan r1, Codex #9); with
+    // `-o` and `--vimgrep`, a row per Match at that Match's offset (review r1 of the M5a plan,
+    // Codex #4). Match `k`'s offset is where it starts in the file, before `-r` replaces it.
+    let offset = |k: usize| {
+        d.offsets
+            .then(|| off + found.get(k).or(found.first()).map_or(0, |s| s.0) as u64)
     };
-    let line = Rc::new(LineText::new(bytes));
-    groups
-        .into_iter()
-        .map(|(trail, subs)| Row {
-            // At its first Match (spec §5.7).
-            offset: d
-                .offsets
-                .then(|| m.absolute_offset + m.submatches.first().map_or(0, |s| s.start) as u64),
+    let row = |k: usize, trail, number, text: Cow<'a, [u8]>, subs, column, at| {
+        let cut = cut_of(&text, o);
+        Row {
+            offset: offset(k),
             trail,
-            line_number: m.line_number,
-            line: line.clone(),
+            line_number: number,
+            line: Rc::new(LineText::with_cut(text, cut)),
             subs,
             is_match: true,
             gap_before: false,
+            column,
+            byte_offset: at,
+        }
+    };
+    // A string of an object file is one row whatever bytes it holds.
+    let lines = if d.offsets {
+        vec![(0, bytes.len())]
+    } else {
+        split_lines(&bytes, o)
+    };
+    let piece = |s: usize, e: usize| -> Cow<'a, [u8]> {
+        match &bytes {
+            Cow::Borrowed(b) => Cow::Borrowed(&b[s..e]),
+            Cow::Owned(b) => Cow::Owned(b[s..e].to_vec()),
+        }
+    };
+    if !subs.is_empty() && o.look.only_matching {
+        let mut rows = Vec::new();
+        for (k, &(s, e)) in subs.iter().enumerate() {
+            let before = rows.len();
+            for (n, &(ls, le)) in lines.iter().enumerate() {
+                let (ps, pe) = (s.max(ls), e.min(le));
+                let pe = pe.min(le - cut_of(&bytes[ls..le], o));
+                if ps >= pe {
+                    continue;
+                }
+                let text = piece(ps, pe);
+                let len = text.len();
+                rows.push(row(
+                    k,
+                    trails[k],
+                    number_of(m.line_number, n, o),
+                    text,
+                    vec![(0, len)],
+                    Some(s as u64 + 1),
+                    off + s as u64,
+                ));
+            }
+            // Nothing of the Match to print: it is empty, its replacement is, or it is a line
+            // terminator. rg's printer of one line prints it as an empty line, its printer of a
+            // `-U` search nothing (review r2 of the M5a plan, Codex #4).
+            if rows.len() == before && !o.look.multi_line {
+                let n = lines.iter().position(|&(_, le)| s < le);
+                let n = n.unwrap_or(lines.len() - 1);
+                rows.push(row(
+                    k,
+                    trails[k],
+                    number_of(m.line_number, n, o),
+                    piece(s, s),
+                    Vec::new(),
+                    Some(s as u64 + 1),
+                    off + s as u64,
+                ));
+            }
+        }
+        return rows;
+    }
+    if !subs.is_empty() && o.look.per_match {
+        let mut rows = Vec::new();
+        for (k, &(s, e)) in subs.iter().enumerate() {
+            // The first line the Match touches; rg's printer of several lines prints none for
+            // an empty Match at a line's start (review r3 of the M5a plan, Codex #4).
+            let Some((n, &(ls, le))) = lines
+                .iter()
+                .enumerate()
+                .find(|&(_, &(ls, le))| s < le && (e > ls || (s >= ls && !o.look.multi_line)))
+            else {
+                continue;
+            };
+            let (column, at) = if lines.len() == 1 {
+                (s as u64 + 1, off + s as u64)
+            } else {
+                ((s - ls) as u64 + 1, off + ls as u64)
+            };
+            rows.push(row(
+                k,
+                trails[k],
+                number_of(m.line_number, n, o),
+                piece(ls, le),
+                clip_line(&bytes, &[(s, e)], ls, le, o),
+                Some(column),
+                at,
+            ));
+        }
+        return rows;
+    }
+    let one = subs.is_empty() || omitted(original, o) || d.offsets;
+    let groups: Vec<(&'a [Segment], Subs)> = if one {
+        vec![(trails[0], subs.clone())]
+    } else {
+        group_by_trail(trails.iter().copied().zip(subs.iter().copied()))
+    };
+    // One line: its text without a `--null-data` record's NUL (review r4 of the M5a plan).
+    if let [(ls, le)] = lines[..] {
+        return groups
+            .into_iter()
+            .map(|(trail, subs)| {
+                let column = subs.first().map(|s| s.0 as u64 + 1);
+                row(0, trail, m.line_number, piece(ls, le), subs, column, off)
+            })
+            .collect();
+    }
+    // A block of lines: each line under the Trail of the first group with a Match in it, or of
+    // the line before it.
+    let mut rows = Vec::new();
+    let mut last = 0;
+    for (n, &(ls, le)) in lines.iter().enumerate() {
+        let g = groups
+            .iter()
+            .position(|(_, subs)| !clip_line(&bytes, subs, ls, le, o).is_empty())
+            .unwrap_or(last);
+        last = g;
+        let (trail, subs) = &groups[g];
+        rows.push(row(
+            0,
+            trail,
+            number_of(m.line_number, n, o),
+            piece(ls, le),
+            clip_line(&bytes, subs, ls, le, o),
+            subs.first().map(|s| s.0 as u64 + 1),
+            off + ls as u64,
+        ));
+    }
+    rows
+}
+
+/// The length of a line without its terminator: `\n`, `\r\n` or a record's NUL.
+fn content_len(line: &[u8]) -> usize {
+    let mut n = line.len();
+    if line.ends_with(b"\n") {
+        n -= 1;
+        if line[..n].ends_with(b"\r") {
+            n -= 1;
+        }
+    } else if line.ends_with(b"\0") {
+        n -= 1;
+    }
+    n
+}
+
+/// The rows of a context line: one, or with `--null-data` one per line of its record. With
+/// `-v` its submatches are the pattern's, which `-r` replaces.
+fn context_rows<'a>(c: &'a Line, src: &'a [u8], o: &RenderOptions) -> Vec<Row<'a>> {
+    let (bytes, subs) = sunk(c, src, subs_of(&c.submatches), o, None);
+    let lines = split_lines(&bytes, o);
+    lines
+        .iter()
+        .enumerate()
+        .map(|(n, &(ls, le))| {
+            let text: Cow<'a, [u8]> = match &bytes {
+                Cow::Borrowed(b) if lines.len() == 1 => Cow::Borrowed(*b),
+                Cow::Borrowed(b) => Cow::Borrowed(&b[ls..le]),
+                Cow::Owned(b) => Cow::Owned(b[ls..le].to_vec()),
+            };
+            let cut = cut_of(&text, o);
+            Row {
+                trail: &[],
+                line_number: number_of(c.line_number, n, o),
+                offset: None,
+                line: Rc::new(LineText::with_cut(text, cut)),
+                subs: if lines.len() == 1 {
+                    subs.clone()
+                } else {
+                    clip_line(&bytes, &subs, ls, le, o)
+                },
+                is_match: false,
+                gap_before: false,
+                column: None,
+                byte_offset: c.absolute_offset + ls as u64,
+            }
         })
         .collect()
 }
 
+/// The line number of the last line of an item: its own, or with `-U` the last of its block.
+fn last_number(l: &Line, src: &[u8], o: &RenderOptions) -> u64 {
+    if o.look.null_data || !o.look.multi_line {
+        return l.line_number;
+    }
+    let bytes = l.bytes(src);
+    let inner = &bytes[..content_len(bytes)];
+    l.line_number + memchr::memchr_iter(b'\n', inner).count() as u64
+}
+
 /// The rows of a file, in file order, produced as they are printed (only the context lines
 /// that open a group wait for the Match after them). A context line takes the Trail of the row
 /// before it in its group, or else of the row after.
@@ -448,29 +910,26 @@ impl<'a> Rows<'a> {
             self.ready.extend(self.pending.drain(..));
             self.last = None;
         }
-        self.prev = Some(line.line_number);
+        self.prev = Some(last_number(line, self.src, self.o));
         let gap_before = gap && self.o.context;
         match item {
             Item::Context(c) => {
-                let row = Row {
-                    trail: self.last.unwrap_or(&[]),
-                    line_number: c.line_number,
-                    offset: None,
-                    line: Rc::new(LineText::new(c.bytes(self.src))),
-                    subs: subs_of(&c.submatches),
-                    is_match: false,
-                    gap_before,
-                };
-                if self.last.is_some() {
-                    self.ready.push_back(row);
-                } else {
-                    self.pending.push(row);
+                for (n, mut row) in context_rows(c, self.src, self.o).into_iter().enumerate() {
+                    row.trail = self.last.unwrap_or(&[]);
+                    row.gap_before = gap_before && n == 0;
+                    if self.last.is_some() {
+                        self.ready.push_back(row);
+                    } else {
+                        self.pending.push(row);
+                    }
                 }
             }
             Item::Match(m) => {
                 let rows = match_rows(self.d, self.i, m, self.src, self.o);
                 self.i += 1;
-                let first = rows[0].trail;
+                let Some(first) = rows.first().map(|r| r.trail) else {
+                    return;
+                };
                 for mut r in self.pending.drain(..) {
                     r.trail = first;
                     self.ready.push_back(r);
@@ -656,7 +1115,52 @@ pub(crate) fn path_bytes(p: &std::path::Path) -> Vec<u8> {
     }
 }
 
-pub(crate) fn write_path(w: &mut dyn WriteColor, p: &std::path::Path) -> std::io::Result<()> {
+/// The path as the Trail layouts print it (M5a plan, D4): `path_bytes`, each `/` (and on
+/// Windows each `\\`) as `--path-separator`'s byte, as grep-printer's `PrinterPath`.
+pub fn shown_path(p: &std::path::Path, look: &Look) -> Vec<u8> {
+    let mut bytes = path_bytes(p);
+    if let Some(sep) = look.path_separator {
+        for b in bytes.iter_mut() {
+            if *b == b'/' || (cfg!(windows) && *b == b'\\') {
+                *b = sep;
+            }
+        }
+    }
+    bytes
+}
+
+/// Opens a link to `p` at `line` and `column` (M5a plan, D12) when `--hyperlink-format` is
+/// set and `w` writes colour and links, as grep-printer's `Interpolator::begin`; returns
+/// whether it did, for `link_end`.
+pub fn link_start(
+    w: &mut dyn WriteColor,
+    look: &Look,
+    p: &std::path::Path,
+    line: Option<u64>,
+    column: Option<u64>,
+) -> std::io::Result<bool> {
+    let Some(h) = &look.hyperlinks else {
+        return Ok(false);
+    };
+    if !w.supports_hyperlinks() || !w.supports_color() {
+        return Ok(false);
+    }
+    let Some(url) = h.url(p, line, column) else {
+        return Ok(false);
+    };
+    w.set_hyperlink(&termcolor::HyperlinkSpec::open(&url))?;
+    Ok(true)
+}
+
+/// Closes the link `link_start` opened.
+pub fn link_end(w: &mut dyn WriteColor, started: bool) -> std::io::Result<()> {
+    if started {
+        w.set_hyperlink(&termcolor::HyperlinkSpec::close())?;
+    }
+    Ok(())
+}
+
+pub fn write_path(w: &mut dyn WriteColor, p: &std::path::Path) -> std::io::Result<()> {
     w.set_color(&style::spec("path"))?;
     w.write_all(&path_bytes(p))?;
     w.reset()
```

- [ ] **Step 2:** `flat.rs` reads the line's bytes through the new accessor:

```diff
diff --git a/src/output/flat.rs b/src/output/flat.rs
index 6ab9d27..5197e17 100644
--- a/src/output/flat.rs
+++ b/src/output/flat.rs
@@ -195,7 +195,7 @@ pub(crate) fn write_row(
     let trail = if r.trail.is_empty() {
         None
     } else {
-        let text_width = if super::omitted(r.line.bytes, o) {
+        let text_width = if super::omitted(r.line.bytes(), o) {
             width(&super::omission_message(
                 r.subs.len(),
                 true,
```

- [ ] **Step 3:** the streaming printer sets the rows' new fields, and keeps its fast path to
  M1–M4's look (`Look::plain`):

```diff
diff --git a/src/output/stream.rs b/src/output/stream.rs
index aaeb898..415c070 100644
--- a/src/output/stream.rs
+++ b/src/output/stream.rs
@@ -85,7 +85,10 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
         jo: &'a JsonOptions,
         file: StreamFile<'a>,
     ) -> Self {
-        let fast = !w.supports_color() && o.max_columns.is_none() && file.outline.is_none();
+        let fast = !w.supports_color()
+            && o.max_columns.is_none()
+            && file.outline.is_none()
+            && o.look.plain();
         Self {
             w,
             matcher,
@@ -159,6 +162,8 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
                 subs,
                 is_match: false,
                 gap_before: p.gap_before,
+                column: None,
+                byte_offset: p.line.absolute_offset,
             };
             self.row(&row)?;
         }
@@ -254,9 +259,11 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
                 trail,
                 line_number: line.line_number,
                 line: text.clone(),
+                column: subs.first().map(|s| s.0 as u64 + 1),
                 subs: subs.clone(),
                 is_match: true,
                 gap_before: gap_before && n == 0,
+                byte_offset: line.absolute_offset,
             };
             self.row(&row)?;
             self.group = Some(trail.clone());
@@ -306,6 +313,8 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
                     subs,
                     is_match: false,
                     gap_before,
+                    column: None,
+                    byte_offset: line.absolute_offset,
                 };
                 self.row(&row)
             }
```

### Task 4.3: the palette, the hyperlinks and the replacements

**Files:** `src/output/style.rs`, `src/output/hyperlink.rs`, `src/output/replace.rs`,
`src/processors/object/rows.rs`.

- [ ] **Step 1:** `Palette` from rg's defaults and `--colors`, the `trail` type (D11), and the
  writers that take it:

```diff
diff --git a/src/output/style.rs b/src/output/style.rs
index 1daa7f9..7448c72 100644
--- a/src/output/style.rs
+++ b/src/output/style.rs
@@ -1,22 +1,69 @@
 //! Colours of the printers.
 use termcolor::{Color, ColorSpec, WriteColor};
 
-/// The colours of a run: ripgrep's defaults and `--colors` (M5a plan, D11; chunk 4).
-#[derive(Debug, Clone, Default)]
-pub struct Palette {}
+/// The colours of a run (M5a plan, D11): ripgrep's defaults with each `--colors` spec applied
+/// in order, as grep-printer's `ColorSpecs` applies them, and s4's: the Trail in cyan (8381),
+/// which `--colors 'trail:…'` changes with the same attributes, and its markers dimmed.
+#[derive(Debug, Clone)]
+pub struct Palette {
+    path: ColorSpec,
+    line: ColorSpec,
+    column: ColorSpec,
+    matched: ColorSpec,
+    trail: ColorSpec,
+}
+
+impl Default for Palette {
+    fn default() -> Self {
+        Palette::new(&[])
+    }
+}
 
 impl Palette {
-    /// The palette of the `--colors` specs, in order, over the defaults.
-    pub fn new(_specs: &[String]) -> Palette {
-        Palette {}
+    /// The palette of the `--colors` specs, each one valid (`cli::values::color_spec`).
+    pub fn new(specs: &[String]) -> Palette {
+        let mut rg = grep_printer::default_color_specs();
+        let mut trail: Vec<grep_printer::UserColorSpec> = vec![parse("path:fg:cyan")];
+        for s in specs {
+            match s.strip_prefix("trail:") {
+                Some(rest) => trail.push(parse(&format!("path:{rest}"))),
+                None => rg.push(parse(s)),
+            }
+        }
+        let rg = grep_printer::ColorSpecs::new(&rg);
+        Palette {
+            path: rg.path().clone(),
+            line: rg.line().clone(),
+            column: rg.column().clone(),
+            matched: rg.matched().clone(),
+            trail: grep_printer::ColorSpecs::new(&trail).path().clone(),
+        }
     }
 
-    /// The colour of `kind`: `path`, `line`, `column`, `match`, `trail`, or the markers' dim.
+    /// The colour of `kind`: `path`, `line`, `column`, `match`, `trail`; anything else is a
+    /// marker (`│`, `…[+N]`), dimmed.
     pub fn spec(&self, kind: &str) -> ColorSpec {
-        spec(kind)
+        match kind {
+            "path" => self.path.clone(),
+            "line" => self.line.clone(),
+            "column" => self.column.clone(),
+            "match" => self.matched.clone(),
+            "trail" => self.trail.clone(),
+            _ => {
+                let mut c = ColorSpec::new();
+                c.set_dimmed(true);
+                c
+            }
+        }
     }
 }
 
+/// A spec `cli::values::color_spec` accepted.
+fn parse(s: &str) -> grep_printer::UserColorSpec {
+    s.parse()
+        .unwrap_or_else(|_| "path:none".parse().expect("valid"))
+}
+
 /// ripgrep's default colours (path magenta, line number green, match bold red), plus cyan
 /// Trails and dim markers (grill decision 8381).
 pub fn spec(kind: &str) -> ColorSpec {
@@ -47,12 +94,38 @@ pub fn write_styled(w: &mut dyn WriteColor, kind: &str, s: &str) -> std::io::Res
     w.reset()
 }
 
+/// `write_styled` with the run's palette.
+pub fn write_in(w: &mut dyn WriteColor, p: &Palette, kind: &str, s: &[u8]) -> std::io::Result<()> {
+    w.set_color(&p.spec(kind))?;
+    w.write_all(s)?;
+    w.reset()
+}
+
+/// `write_highlighted` with the run's palette.
+pub fn write_highlighted_in(
+    w: &mut dyn WriteColor,
+    p: &Palette,
+    text: &str,
+    subs: &[(usize, usize)],
+) -> std::io::Result<()> {
+    highlighted(w, &p.spec("match"), text, subs)
+}
+
 /// Writes `text`, with each byte range of `subs` in the match colour. Ranges are clamped to
 /// `text` and moved to char boundaries.
 pub fn write_highlighted(
     w: &mut dyn WriteColor,
     text: &str,
     subs: &[(usize, usize)],
+) -> std::io::Result<()> {
+    highlighted(w, &spec("match"), text, subs)
+}
+
+fn highlighted(
+    w: &mut dyn WriteColor,
+    color: &ColorSpec,
+    text: &str,
+    subs: &[(usize, usize)],
 ) -> std::io::Result<()> {
     if subs.is_empty() || !w.supports_color() {
         return w.write_all(text.as_bytes());
@@ -85,7 +158,9 @@ pub fn write_highlighted(
             continue;
         }
         w.write_all(&text.as_bytes()[at..s])?;
-        write_styled(w, "match", &text[s..e])?;
+        w.set_color(color)?;
+        w.write_all(&text.as_bytes()[s..e])?;
+        w.reset()?;
         at = e;
     }
     w.write_all(&text.as_bytes()[at..])
```

- [ ] **Step 2:** `src/output/hyperlink.rs` in place of its stub (D12); its tests module first,
  then the rest:

```rust
//! `--hyperlink-format` in the Trail layouts (M5a plan, D12): a path, and with it the line and
//! column numbers of a row, as an OSC 8 link on a terminal with colour, the URL built as
//! grep-printer 0.2.2 builds it (hyperlink.rs): `{path}` the file's canonical path,
//! percent-encoded, `{host}` the hostname (`--hostname-bin`, else the system's), `{wslprefix}`
//! `wsl$/<distro>` under WSL, `{line}` and `{column}` 1 when the row has none.
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(Vec<u8>),
    Host,
    WslPrefix,
    Path,
    Line,
    Column,
}

/// The run's hyperlink format with the values it reads from the system.
#[derive(Debug)]
pub struct Hyperlinks {
    parts: Vec<Part>,
    host: Option<String>,
    wsl_prefix: Option<String>,
    /// The format names `{line}` or `{column}`: a row's numbers go inside its link.
    line_dependent: bool,
}

impl Hyperlinks {
    /// None when the format is empty (`none`, or no `--hyperlink-format`): no links. `format`
    /// is one `--hyperlink-format` accepted (`cli::values::hyperlink_format`).
    pub fn new(format: &str, hostname_bin: Option<&Path>) -> Option<Hyperlinks> {
        let parts = parse(&expand_alias(format)?)?;
        if parts.is_empty() {
            return None;
        }
        let line_dependent = parts.iter().any(|p| matches!(p, Part::Line | Part::Column));
        Some(Hyperlinks {
            host: parts
                .contains(&Part::Host)
                .then(|| hostname(hostname_bin))
                .flatten(),
            wsl_prefix: parts.contains(&Part::WslPrefix).then(wsl_prefix).flatten(),
            parts,
            line_dependent,
        })
    }

    /// Whether a row's line and column go in its link (grep-printer's `is_line_dependent`).
    pub fn line_dependent(&self) -> bool {
        self.line_dependent
    }

    /// The URL of `path` at `line` and `column`, or None when the path cannot be made absolute.
    pub fn url(&self, path: &Path, line: Option<u64>, column: Option<u64>) -> Option<Vec<u8>> {
        let encoded = encode_path(path)?;
        let mut url = Vec::new();
        for part in &self.parts {
            match part {
                Part::Text(t) => url.extend_from_slice(t),
                Part::Host => url.extend_from_slice(self.host.as_deref().unwrap_or("").as_bytes()),
                Part::WslPrefix => {
                    url.extend_from_slice(self.wsl_prefix.as_deref().unwrap_or("").as_bytes())
                }
                Part::Path => url.extend_from_slice(&encoded),
                Part::Line => url.extend_from_slice(line.unwrap_or(1).to_string().as_bytes()),
                Part::Column => url.extend_from_slice(column.unwrap_or(1).to_string().as_bytes()),
            }
        }
        Some(url)
    }
}

/// An alias (`default`, `vscode`, `none`, …) as the format grep-printer gives it; a format of
/// the user's as it is. An alias has no brace, and its format no literal brace, so grep-printer's
/// display of it reads back the same.
fn expand_alias(format: &str) -> Option<String> {
    let parsed: grep_printer::HyperlinkFormat = format.parse().ok()?;
    Some(if format.contains('{') {
        format.to_string()
    } else {
        parsed.to_string()
    })
}

/// grep-printer's `HyperlinkFormat::from_str`: `{name}` a variable, `{{` and `}}` a brace. A
/// format must have `{path}` (which grep-printer checked already: `values::hyperlink_format`).
fn parse(s: &str) -> Option<Vec<Part>> {
    let mut parts: Vec<Part> = Vec::new();
    let mut text = Vec::new();
    let mut chars = s.chars().peekable();
    let flush = |text: &mut Vec<u8>, parts: &mut Vec<Part>| {
        if !text.is_empty() {
            parts.push(Part::Text(std::mem::take(text)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push(b'{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push(b'}');
            }
            '{' => {
                let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                flush(&mut text, &mut parts);
                parts.push(match name.as_str() {
                    "host" => Part::Host,
                    "wslprefix" => Part::WslPrefix,
                    "path" => Part::Path,
                    "line" => Part::Line,
                    "column" => Part::Column,
                    _ => return None,
                });
            }
            c => {
                let mut b = [0; 4];
                text.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            }
        }
    }
    flush(&mut text, &mut parts);
    Some(parts)
}

/// grep-printer's `HyperlinkPath::from_path` on Unix: the canonical path, which must start with
/// `/`, percent-encoded but for RFC 3986's unreserved characters, `/`, `:` and non-ASCII bytes.
#[cfg(unix)]
fn encode_path(path: &Path) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    let path = path.canonicalize().ok()?;
    let bytes = path.as_os_str().as_bytes();
    if !bytes.starts_with(b"/") {
        return None;
    }
    Some(encode(bytes))
}

/// On Windows (grep-printer's `from_path`): `\\?\C:\dir` becomes `/C:/dir` and `\\?\UNC\server\x`
/// `//server/x`, separators as `/`.
#[cfg(not(unix))]
fn encode_path(path: &Path) -> Option<Vec<u8>> {
    let path = path.canonicalize().ok()?;
    let s = path.to_str()?;
    let s = if let Some(unc) = s.strip_prefix(r"\\?\UNC\") {
        format!("//{unc}")
    } else if let Some(local) = s.strip_prefix(r"\\?\") {
        format!("/{local}")
    } else {
        return None;
    };
    Some(encode(s.as_bytes()))
}

fn encode(input: &[u8]) -> Vec<u8> {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(input.len());
    for &b in input {
        match b {
            b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
            | b'/'
            | b':'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | 128.. => out.push(b),
            b'\\' if cfg!(windows) => out.push(b'/'),
            _ => out.extend_from_slice(&[b'%', HEX[(b >> 4) as usize], HEX[(b & 0xF) as usize]]),
        }
    }
    out
}

/// rg 14.1.1's `hostname`: the output of `--hostname-bin`, trimmed, when it runs and prints
/// something; else the system's.
fn hostname(bin: Option<&Path>) -> Option<String> {
    if let Some(bin) = bin
        && let Ok(bin) = grep_cli::resolve_binary(bin)
    {
        let mut cmd = std::process::Command::new(&bin);
        cmd.stdin(std::process::Stdio::null());
        if let Ok(rdr) = grep_cli::CommandReader::new(&mut cmd)
            && let Ok(out) = std::io::read_to_string(rdr)
            && !out.trim().is_empty()
        {
            return Some(out.trim().to_string());
        }
    }
    grep_cli::hostname().ok()?.to_str().map(str::to_string)
}

/// rg 14.1.1's `wsl_prefix`: `wsl$/<distro>` on Unix under WSL.
fn wsl_prefix() -> Option<String> {
    if !cfg!(unix) {
        return None;
    }
    let distro = std::env::var_os("WSL_DISTRO_NAME")?;
    Some(format!("wsl$/{}", distro.to_str()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_aliases_and_braces() {
        assert!(Hyperlinks::new("", None).is_none());
        assert!(Hyperlinks::new("none", None).is_none());
        let f = Hyperlinks::new("vscode", None).unwrap();
        assert!(f.line_dependent());
        let f = Hyperlinks::new("file://x{{{path}}}", None).unwrap();
        assert!(!f.line_dependent());
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a b.txt");
        std::fs::write(&p, "x").unwrap();
        let url = String::from_utf8(f.url(&p, None, None).unwrap()).unwrap();
        assert!(
            url.starts_with("file://x{/") && url.ends_with("/a%20b.txt}"),
            "{url}"
        );
    }

    #[test]
    fn line_and_column_default_to_one() {
        let f = Hyperlinks::new("file://{path}:{line}:{column}", None).unwrap();
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, "x").unwrap();
        let canon = p.canonicalize().unwrap();
        let url = String::from_utf8(f.url(&p, Some(7), None).unwrap()).unwrap();
        assert_eq!(url, format!("file://{}:7:1", canon.display()));
        assert!(f.url(&d.path().join("missing"), None, None).is_none());
    }

    #[test]
    fn the_hostname_comes_from_hostname_bin() {
        let f = Hyperlinks::new("file://{host}{path}", Some(Path::new("true"))).unwrap();
        // `true` prints nothing: the system's hostname.
        assert_eq!(
            f.host,
            grep_cli::hostname().ok().and_then(|h| h.into_string().ok())
        );
        assert!(
            Hyperlinks::new("file://{path}", None)
                .unwrap()
                .host
                .is_none()
        );
    }
}
```

- [ ] **Step 3:** `src/output/replace.rs` in place of its stub (D4), with `replacements`, the
  replacement of each Match of a line made in the one pass over it that `replace` makes, each
  Match with its range in the Source and the groups the search found, which an object file's
  search keeps for its strings (`Described::replacements`, filled by C9): a line of an object
  file holds many strings, and looking each one up again cost its line once per string, and
  found a Match after `\K` with the groups of another alternative (review r4, Codex #1 and #2;
  `\K` needs PCRE2, which C2 brings, so its cases are `m5_17`'s and `m5_22`'s); its tests
  module first:

```rust
//! `-r`/`--replace` in the Trail layouts (M5a plan, D4): each Match of a line replaced by the
//! replacement, its `$1`, `$name` and `${name}` read from the Match's capture groups, as
//! grep-printer 0.2.2's `Replacer::replace_all` replaces them (util.rs): over the Source up to
//! the line's end, its line terminator left out (on a `-U` block, 128 bytes past it, for a
//! look-ahead), from the line's start. A row's highlight then covers its replacements.
use crate::matcher::{Captures, PatternMatcher};
use grep_matcher::{Captures as _, LineTerminator, Match, Matcher as _};
use std::ops::Range;

/// The replacement of a run and the matcher whose groups it reads.
#[derive(Debug)]
pub struct Replacer {
    matcher: PatternMatcher,
    template: Vec<u8>,
    term: LineTerminator,
    multi_line: bool,
}

/// A line with its Matches replaced: the new bytes, and for each Match, in order, where it was
/// in the line and where its replacement is in `bytes`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replaced {
    pub bytes: Vec<u8>,
    pub spans: Vec<((usize, usize), (usize, usize))>,
}

impl Replaced {
    /// The ranges of `bytes` that the Matches at `subs` (ranges of the original line) became;
    /// a range that is no Match's is dropped.
    pub fn map(&self, subs: &[(usize, usize)]) -> Vec<(usize, usize)> {
        subs.iter()
            .filter_map(|s| self.spans.iter().find(|(o, _)| o == s).map(|(_, n)| *n))
            .collect()
    }
}

impl Replacer {
    /// `term` and `multi_line` are the search's (`search::line_terminator`, `-U`).
    pub fn new(
        matcher: PatternMatcher,
        template: Vec<u8>,
        term: LineTerminator,
        multi_line: bool,
    ) -> Self {
        Replacer {
            matcher,
            template,
            term,
            multi_line,
        }
    }

    /// The line at `range` of `haystack` with its Matches replaced; the line's terminator, when
    /// it has one, stays at the end.
    pub fn replace(&self, haystack: &[u8], range: Range<usize>) -> Replaced {
        let mut bytes = Vec::with_capacity(range.len());
        let mut spans = Vec::new();
        let mut last = range.start;
        self.pass(haystack, range.clone(), |m, caps, hay| {
            bytes.extend_from_slice(&hay[last..m.start()]);
            let at = bytes.len();
            self.interpolate(caps, hay, &mut bytes);
            spans.push((
                (m.start() - range.start, m.end() - range.start),
                (at, bytes.len()),
            ));
            last = m.end();
        });
        if last < range.end {
            bytes.extend_from_slice(&haystack[last..range.end]);
        }
        Replaced { bytes, spans }
    }

    /// The replacement of each Match of the line (with `-U`, the block) at `range` of
    /// `haystack`, with the Match's range in `haystack`, in file order: the one pass over the
    /// line that `replace` makes, so each Match has the groups the search found. An object
    /// file's search keeps them for its strings (`Described::replacements`): a line of an
    /// object file holds many strings, and each string looked for again from its own start or
    /// from its line's start either cost its line once per string or found a `\K` Match's
    /// range with another alternative's groups (review r4 of the M5a plan, Codex #1 and #2).
    pub fn replacements(
        &self,
        haystack: &[u8],
        range: Range<usize>,
    ) -> Vec<((usize, usize), Vec<u8>)> {
        let mut out = Vec::new();
        self.pass(haystack, range, |m, caps, hay| {
            let mut with = Vec::new();
            self.interpolate(caps, hay, &mut with);
            out.push(((m.start(), m.end()), with));
        });
        out
    }

    /// Each Match of the line at `range` of `haystack` with its groups, as grep-printer's
    /// `Replacer::replace_all` finds them: from the line's start, over the Source up to the
    /// line's end, its terminator left out (with `-U`, up to 128 bytes past the block, for a
    /// look-ahead), up to the first Match that starts past the line.
    fn pass(
        &self,
        haystack: &[u8],
        range: Range<usize>,
        mut found: impl FnMut(Match, &Captures, &[u8]),
    ) {
        let end = if self.multi_line {
            haystack.len().min(range.end + 128)
        } else {
            range.start + content_len(&haystack[range.clone()], self.term)
        };
        let hay = &haystack[..end];
        let Ok(mut caps) = self.matcher.new_captures() else {
            return;
        };
        let _ = self
            .matcher
            .captures_iter_at(hay, range.start, &mut caps, |caps| {
                let Some(m) = caps.get(0) else {
                    return false;
                };
                if m.start() >= range.end {
                    return false;
                }
                found(m, caps, hay);
                true
            });
    }

    /// The replacement of the Match of `caps`, appended to `dst`.
    fn interpolate(&self, caps: &Captures, hay: &[u8], dst: &mut Vec<u8>) {
        caps.interpolate(
            |name| self.matcher.capture_index(name),
            hay,
            &self.template,
            dst,
        );
    }
}

/// The length of `line` without its terminator (a CRLF's `\r` too).
fn content_len(line: &[u8], term: LineTerminator) -> usize {
    let mut end = line.len();
    if end > 0 && term.is_suffix(line) {
        end -= 1;
        if term.is_crlf() && end > 0 && line[end - 1] == b'\r' {
            end -= 1;
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacer(argv: &[&str], pat: &str, with: &str) -> Replacer {
        let mut v = vec!["s4"];
        v.extend_from_slice(argv);
        v.push(pat);
        let a = crate::cli::parse(v.iter().map(Into::into).collect()).unwrap();
        let m = crate::matcher::build(&a, &[pat.to_string()]).unwrap();
        let term = if a.crlf {
            LineTerminator::crlf()
        } else {
            LineTerminator::byte(b'\n')
        };
        Replacer::new(m, with.as_bytes().to_vec(), term, a.multiline)
    }

    #[test]
    fn groups_and_names_as_ripgrep() {
        let r = replacer(&[], r"(?P<w>\w+)@(\w+)", "$2 at ${w}");
        let hay = b"x a@b y c@d\n";
        let got = r.replace(hay, 0..hay.len());
        assert_eq!(got.bytes, b"x b at a y d at c\n");
        assert_eq!(got.spans, [((2, 5), (2, 8)), ((8, 11), (11, 17))]);
        assert_eq!(got.map(&[(8, 11)]), [(11, 17)]);
        assert_eq!(got.map(&[(0, 1)]), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn the_line_terminator_is_kept_and_not_matched() {
        let r = replacer(&["--crlf"], r"\s$", "!");
        let hay = b"a \r\nb\r\n";
        assert_eq!(r.replace(hay, 0..4).bytes, b"a!\r\n");
        // Only the line's Matches: the next line is not searched.
        let r = replacer(&[], "b", "B");
        assert_eq!(r.replace(hay, 0..4).bytes, b"a \r\n");
    }

    #[test]
    fn a_multi_line_match_is_replaced_whole() {
        let r = replacer(&["-U"], r"a\nb", "X");
        let hay = b"a\nb\nc\n";
        assert_eq!(r.replace(hay, 0..4).bytes, b"X\n");
    }

    /// Review r4 of the M5a plan, Codex #1 and #2: the replacements of a line are made in one
    /// pass from its start, each Match with its range in the haystack and its groups; the
    /// pass reads no byte of the next line, and stops at the line's end.
    #[test]
    fn the_replacements_of_a_line_in_one_pass() {
        let hay = b"\x7fELF\0abcd target\0x\nabcd\0";
        let r = replacer(&[], "(c)d|target|^a", "<$1>");
        assert_eq!(
            r.replacements(hay, 0..19),
            [((7, 9), b"<c>".to_vec()), ((10, 16), b"<>".to_vec())]
        );
        assert_eq!(
            r.replacements(hay, 19..24),
            [((19, 20), b"<>".to_vec()), ((21, 23), b"<c>".to_vec())]
        );
        let r = replacer(&[], "x$", "X");
        assert_eq!(r.replacements(hay, 0..19), [((17, 18), b"X".to_vec())]);
        assert_eq!(r.replacements(hay, 19..24), []);
    }
}
```

- [ ] **Step 3b:** `shown`, the byte of a Match as an object file's row shows it, shared by M4's
  rows and `sunk`:

```diff
diff --git a/src/processors/object/rows.rs b/src/processors/object/rows.rs
index 75b5381..d2eb571 100644
--- a/src/processors/object/rows.rs
+++ b/src/processors/object/rows.rs
@@ -77,9 +77,7 @@ pub fn rows(
             let mut owned = bytes[w.clone()].to_vec();
             for s in &subs {
                 for b in &mut owned[s.start - w.start..s.end - w.start] {
-                    if *b < 0x20 && *b != b'\t' || *b == 0x7f {
-                        *b = b'.';
-                    }
+                    *b = shown(*b);
                 }
             }
             Line {
@@ -99,6 +97,15 @@ pub fn rows(
         .collect()
 }
 
+/// A byte of a Match as its row shows it: a control byte (a tab aside) as `.`.
+pub fn shown(b: u8) -> u8 {
+    if b < 0x20 && b != b'\t' || b == 0x7f {
+        b'.'
+    } else {
+        b
+    }
+}
+
 /// Whether `bytes[from..to]` is printable characters only.
 fn printable(bytes: &[u8], from: usize, to: usize) -> bool {
     let mut i = from;
```

- [ ] **Step 4:** `cargo test --lib output::`: PASS; `cargo clippy --workspace --all-targets --
  -D warnings`: no finding.
- [ ] **Step 5:** commit: `Make the rows of rg's output flags, with a palette, links and
  replacements`.

---
## Chunk 5: `--generate` and the logger

### Task 5.1: `--generate`

**Files:** `src/cli/generate.rs`.

- [ ] **Step 1:** the tests module below, over C1's stub: `cargo test --lib cli::generate`
  FAILS (the stub writes nothing): RED.
- [ ] **Step 2:** the module (D16); the fish completions name a flag as `-l within`:

```rust
//! `--generate`: the man page and the shell completions of s4 (M5a plan, D16), made from s4's
//! own command line (`cli::command`) by clap_mangen and clap_complete, so they list s4's flags
//! with their help, ripgrep's and searchfor's alike. ripgrep writes its own from its flag
//! definitions; the kinds are its five.
use clap_complete::Shell;
use std::io::Write;

/// Writes what `kind` names (`man`, `complete-bash`, `complete-zsh`, `complete-fish`,
/// `complete-powershell`) to `w`.
pub fn generate(kind: &str, w: &mut dyn Write) -> std::io::Result<()> {
    let mut cmd = super::command();
    let shell = match kind {
        "man" => return clap_mangen::Man::new(cmd).render(w),
        "complete-bash" => Shell::Bash,
        "complete-zsh" => Shell::Zsh,
        "complete-fish" => Shell::Fish,
        "complete-powershell" => Shell::PowerShell,
        other => {
            return Err(std::io::Error::other(format!(
                "choice '{other}' is unrecognized"
            )));
        }
    };
    clap_complete::generate(shell, &mut cmd, "s4", w);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(kind: &str) -> String {
        let mut v = Vec::new();
        generate(kind, &mut v).unwrap();
        String::from_utf8(v).unwrap()
    }

    #[test]
    fn man_page_lists_rgs_and_s4s_flags() {
        let man = out("man");
        assert!(
            man.starts_with(".ie \\n(.g .ds Aq \\(aq") || man.contains(".TH s4"),
            "{man}"
        );
        assert!(man.contains(".TH s4"));
        assert!(man.contains("pcre2"));
        assert!(man.contains("within"));
    }

    #[test]
    fn completions_for_each_shell() {
        assert!(out("complete-bash").contains("_s4()"));
        assert!(out("complete-zsh").starts_with("#compdef s4"));
        assert!(out("complete-fish").contains("complete -c s4"));
        assert!(out("complete-powershell").contains("Register-ArgumentCompleter"));
        for kind in ["complete-bash", "complete-zsh"] {
            let text = out(kind);
            assert!(
                text.contains("--within") && text.contains("--pcre2"),
                "{kind}"
            );
        }
        let fish = out("complete-fish");
        assert!(fish.contains("-l within") && fish.contains("-l pcre2"));
    }

    #[test]
    fn an_unknown_kind_is_an_error() {
        assert!(generate("complete-tcsh", &mut Vec::new()).is_err());
    }
}
```

- [ ] **Step 3:** `cargo test --lib cli::generate`: PASS.

### Task 5.2: the logger of `--debug` and `--trace`

**Files:** `src/log.rs`.

- [ ] **Step 1:** the test module below over the stub: it does not compile (`format_record`):
  RED.
- [ ] **Step 2:** the module (D15), rg's `logger.rs` with `s4: ` before each line:

```rust
//! `--debug` and `--trace` (M5a plan, D15): messages on stderr in ripgrep 14.1.1's format
//! (crates/core/logger.rs), `s4: LEVEL|target|file:line: message`, from s4 and from the crates
//! it shares with ripgrep (ignore, globset, grep-regex, grep-searcher…), which log through the
//! `log` crate as they do in rg: why a file is skipped reads as rg says it. Without either flag
//! nothing is logged, and a message costs one comparison of its level.
use crate::cli::Logging;

struct Logger;

static LOGGER: Logger = Logger;

impl log::Log for Logger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let line = format_record(record);
        // One write per message, so the lines of several threads do not mix.
        let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), line.as_bytes());
    }

    fn flush(&self) {}
}

/// rg's format, prefixed `s4: ` as rg's messages are `rg: `.
fn format_record(record: &log::Record<'_>) -> String {
    match (record.file(), record.line()) {
        (Some(file), Some(line)) => format!(
            "s4: {}|{}|{}:{}: {}\n",
            record.level(),
            record.target(),
            file,
            line,
            record.args()
        ),
        (Some(file), None) => format!(
            "s4: {}|{}|{}: {}\n",
            record.level(),
            record.target(),
            file,
            record.args()
        ),
        _ => format!(
            "s4: {}|{}: {}\n",
            record.level(),
            record.target(),
            record.args()
        ),
    }
}

/// Starts the logger at the level asked for; nothing is logged without one. A second call (a
/// test, `--mcp`'s searches) changes the level only.
pub fn init(level: Option<Logging>) {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(match level {
        None => log::LevelFilter::Off,
        Some(Logging::Debug) => log::LevelFilter::Debug,
        Some(Logging::Trace) => log::LevelFilter::Trace,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_print_as_ripgreps() {
        let r = log::Record::builder()
            .level(log::Level::Debug)
            .target("ignore::walk")
            .file(Some("src/walk.rs"))
            .line(Some(7))
            .args(format_args!("ignoring ./x: Ignore(...)"))
            .build();
        assert_eq!(
            format_record(&r),
            "s4: DEBUG|ignore::walk|src/walk.rs:7: ignoring ./x: Ignore(...)\n"
        );
        let r = log::Record::builder()
            .level(log::Level::Trace)
            .target("searchfor")
            .args(format_args!("x"))
            .build();
        assert_eq!(format_record(&r), "s4: TRACE|searchfor: x\n");
    }
}
```

- [ ] **Step 3:** `cargo test --lib log::`: PASS.
- [ ] **Step 4:** commit: `Generate s4's man page and completions, and log as rg logs`.

---
## Chunk 6: the MCP server

### Task 6.1: the protocol and the tool

**Files:** `src/mcp.rs`.

- [ ] **Step 1:** the tests module of the file below, over C1's stub (whose `serve_io` fails):
  a session of `initialize`, `notifications/initialized`, `tools/list`, `tools/call` and `ping`
  through a `Searcher` that answers with canned output and a canned end (`Canned`); the
  protocol's errors (-32601, -32602, -32700) and a tool error (`isError`); every request
  JSON-RPC 2.0 does not allow answered with -32600, a line that is not UTF-8 with -32700, a
  request longer than `MAX_REQUEST_BYTES` refused, and the server answering the next request
  after each (`invalid_requests_are_answered`); the arguments as flags, each value joined to its
  flag after `--no-config`, every argument of the wrong type refused, `max_results` from 1 to
  `MAX_RESULTS` (`search_arguments_become_flags`); the child's environment without either
  configuration variable (`the_search_runs_without_configuration`); long texts, long names,
  deep Trails and repeated Sections (`long_texts_come_cut`); the answer's byte limit and a
  message too long to read, and the limit's scope beside a 300 000-character id
  (`the_answer_has_a_size_limit`); why a Match was left out: a line past `MAX_MESSAGE_BYTES`
  named in `errors`, a first Match cut until it fits, and a later one too long for any answer,
  `truncated_by` (`left_out_matches_say_why`); forty lines past `MAX_MESSAGE_BYTES` with names
  all escapes, read from a searcher that makes its output as it is read (`Repeated`), beside
  s4's longest stderr and Matches that fill the answer: the whole result within
  `MAX_RESPONSE_BYTES`, twenty files named and the rest counted
  (`many_long_lines_keep_the_answer_bounded`); a Match per Trail of a line, and a line of 240
  Trails read one Trail at a time (`a_match_per_trail_of_the_line`);
  Sections of two files whose names read alike (`sections_of_files_whose_names_read_alike`); a
  long request answered with its id (`invalid_requests_are_answered`); every way a search ends
  (`how_a_search_ends`); a line and a path that are not UTF-8 (`bytes_are_decoded`). `cargo test
  --lib mcp::` does not compile (`Searcher`, `Search`, `Ended`, `search_args`…): RED.
- [ ] **Step 2:** the server (D17). `serve` runs each search as a child `s4 --no-config --json`
  of the same binary (`Child`, `command`), its environment without `SEARCHFOR_CONFIG_PATH` and
  `RIPGREP_CONFIG_PATH`; `run` reads its stdout a line at a time without ever holding more than
  `MAX_MESSAGE_BYTES` (`read_line_capped`; a longer `match` named in `errors` by the path its
  first bytes hold, `path_of_prefix`, at most `MAX_LONG_LINES_NAMED` of them, each paid for in
  both copies of `errors` out of the Matches' share, `message_cost`, the rest counted), makes
  each `match` message a Match per Trail of its submatches, the Trails borrowed from it one at a
  time (`by_trail`), and each a compact Match (`compact`, a Section known by its file's exact
  path), cuts the first Match, and any that would not fit in an empty answer, until it fits
  (`shrink`), stops the child at the first Match past
  `max_results` or past the answer's bytes (`truncated`, `truncated_by`), answers a request past
  `MAX_REQUEST_BYTES` with the id its first bytes hold (`id_of_prefix`), and turns how the child
  ended into the answer: its Matches and its messages as
  `errors` when it exits 0, 1 or 2 with Matches, a tool error with its message otherwise:

```rust
//! `s4 --mcp`: an MCP server over stdin and stdout (spec §9; M5a plan, D17). It speaks
//! JSON-RPC 2.0, one message per line, as the Model Context Protocol's stdio transport, and
//! offers one tool, `search`, which runs `s4 --json` over the arguments it is given and answers
//! with its Matches: the path, the line, the Trail's names and the line's text, and with
//! `expand` the Section. Each search is a child process of the same binary, so a search that
//! fails or is slow never takes the server down. The child reads no configuration file and gets
//! only the flags the arguments ask for; what it prints is read and answered within fixed limits
//! of Matches and bytes (review r1 of the M5a plan: Codex #1, #2, #3, #6, #7; Fable #1, #2, #8),
//! saying why a Match was left out (review r2: Codex #5, #6; Fable #1, #2, #3), and its
//! messages paid for within the same limit (review r3: Codex #1, #2; Fable #1, #2).
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

/// The protocol versions this server speaks, newest first; it answers a client with the
/// client's version when it is one of them, else with the newest.
pub const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// The Matches `search` returns when the call does not say.
pub const DEFAULT_MAX_RESULTS: u64 = 100;
/// The most Matches a call may ask for.
pub const MAX_RESULTS: u64 = 1000;
/// The longest result of a `tools/call`, in bytes, both copies of the Matches included. Its
/// line adds the JSON-RPC envelope: `ENVELOPE_BYTES` and the id as the client sent it, at most
/// a request's `MAX_REQUEST_BYTES` (review r2 of the M5a plan, Codex #6).
pub const MAX_RESPONSE_BYTES: usize = 256 * 1024;
/// The bytes of a response's line besides its result and its id:
/// `{"jsonrpc":"2.0","id":` `,"result":` `}`.
pub const ENVELOPE_BYTES: usize = 33;
/// What the answer keeps out of the Matches' share: its fields besides the Matches, s4's
/// stderr (`MAX_STDERR_BYTES`, a control character 13 bytes in both copies) and the count of
/// the `match` messages too long to read that `errors` does not name. A message `errors` names
/// is paid for from the Matches' share.
const RESERVED_BYTES: usize = 64 * 1024;
/// The Matches' share of an answer: the most a Match may cost, cut until it fits.
const ROOM_BYTES: usize = MAX_RESPONSE_BYTES - RESERVED_BYTES;
/// The most bytes of a Match's `text`.
pub const MAX_TEXT_BYTES: usize = 2 * 1024;
/// The most bytes of a Section's `text`.
pub const MAX_SECTION_BYTES: usize = 16 * 1024;
/// The most bytes of a name of the Trail.
pub const MAX_NAME_BYTES: usize = 256;
/// The most names of a Trail a Match keeps: the innermost.
pub const MAX_TRAIL: usize = 32;
/// The longest message of `s4 --json` the server reads; a longer `match` is left out unread,
/// its file named in `errors`.
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
/// The most `match` messages too long to read whose file `errors` names; the rest are counted.
pub const MAX_LONG_LINES_NAMED: usize = 20;
/// The longest request the server reads.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;
/// The most bytes of the search's stderr kept.
pub const MAX_STDERR_BYTES: usize = 4 * 1024;

/// The arguments of `search`, as its schema names them.
const ARGUMENTS: &[&str] = &[
    "pattern",
    "paths",
    "within",
    "expand",
    "glob",
    "case_insensitive",
    "max_results",
];

/// Serves MCP requests from stdin until it closes; `s4` is the binary each search runs.
pub fn serve(s4: &Path) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve_io(stdin.lock(), stdout.lock(), &Child(s4.to_path_buf()))
}

/// What starts a search: `s4` in a child process, or a stand-in in the tests.
pub trait Searcher {
    /// Starts `s4 ARGS`, or says why it could not.
    fn start(&self, args: &[OsString]) -> Result<Box<dyn Search + '_>, String>;
}

/// A search started.
pub trait Search {
    /// What it prints on stdout.
    fn output(&mut self) -> &mut dyn BufRead;
    /// Waits for it to end, ending it first when `stop`: how it ended, and its stderr (at most
    /// `MAX_STDERR_BYTES`, lossy).
    fn end(self: Box<Self>, stop: bool) -> (Ended, String);
}

/// How a search ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Ended {
    /// It exited with this code.
    Code(i32),
    /// The server ended it: the answer was full.
    Stopped,
    /// Anything else (a signal, a wait that failed), said in words.
    Other(String),
}

/// `serve` over any reader, writer and searcher. A request longer than `MAX_REQUEST_BYTES` is
/// read to its end and refused, with its id when the part read holds it; a line that is not
/// JSON (or not UTF-8) gets -32700; the server goes on after either.
pub fn serve_io(
    mut input: impl BufRead,
    mut output: impl Write,
    s: &dyn Searcher,
) -> anyhow::Result<()> {
    let mut line = Vec::new();
    while let Some(whole) = read_line_capped(&mut input, MAX_REQUEST_BYTES, &mut line)? {
        let answer = if !whole {
            Some(error(
                id_of_prefix(&line),
                -32600,
                &format!("a request is at most {MAX_REQUEST_BYTES} bytes"),
            ))
        } else if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        } else {
            handle(&line, s)
        };
        if let Some(answer) = answer {
            writeln!(output, "{answer}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Reads one line of `r` into `buf`, its `\n` left out, keeping at most `cap` bytes: None at
/// the end of the input, else whether the line was whole (a longer one is read to its end and
/// the rest dropped, so it never takes more memory than `cap`).
fn read_line_capped(
    r: &mut dyn BufRead,
    cap: usize,
    buf: &mut Vec<u8>,
) -> std::io::Result<Option<bool>> {
    buf.clear();
    let mut whole = true;
    let mut any = false;
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            return Ok(any.then_some(whole));
        }
        any = true;
        let (take, done) = match memchr::memchr(b'\n', chunk) {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        let room = cap.saturating_sub(buf.len());
        whole &= take <= room;
        buf.extend_from_slice(&chunk[..take.min(room)]);
        r.consume(take + usize::from(done));
        if done {
            return Ok(Some(whole));
        }
    }
}

/// The JSON value that starts at `i` of `b` and where it ends, when `b` holds it whole.
fn value_at(b: &[u8], i: usize) -> Option<(Value, usize)> {
    let mut values = serde_json::Deserializer::from_slice(b.get(i..)?).into_iter::<Value>();
    let v = values.next()?.ok()?;
    let end = i + values.byte_offset();
    (end < b.len()).then_some((v, end))
}

/// The `id` of a request cut at `MAX_REQUEST_BYTES`, when the members of its object before the
/// cut hold it: read member by member, so an `"id"` inside `params` is never taken for it
/// (review r2 of the M5a plan, Fable #3). Null otherwise.
fn id_of_prefix(prefix: &[u8]) -> Value {
    let skip = |i: usize| {
        let rest = prefix.get(i..).unwrap_or_default();
        i + rest.iter().take_while(|c| c.is_ascii_whitespace()).count()
    };
    let mut i = skip(0);
    if prefix.get(i) != Some(&b'{') {
        return Value::Null;
    }
    i += 1;
    loop {
        let Some((Value::String(key), end)) = value_at(prefix, skip(i)) else {
            return Value::Null;
        };
        i = skip(end);
        if prefix.get(i) != Some(&b':') {
            return Value::Null;
        }
        let Some((v, end)) = value_at(prefix, i + 1) else {
            return Value::Null;
        };
        if key == "id" {
            return if v.is_string() || v.is_number() {
                v
            } else {
                Value::Null
            };
        }
        i = skip(end);
        if prefix.get(i) != Some(&b',') {
            return Value::Null;
        }
        i += 1;
    }
}

/// The answer to one message, None for a notification or a response. A message JSON-RPC 2.0
/// does not allow (not an object, a batch, no `"jsonrpc": "2.0"`, an `id` that is not a string
/// or a number, a `method` that is not a string, `params` that are not an object) gets -32600.
fn handle(line: &[u8], s: &dyn Searcher) -> Option<Value> {
    let msg: Value = match serde_json::from_slice(line) {
        Ok(v) => v,
        Err(e) => return Some(error(Value::Null, -32700, &format!("parse error: {e}"))),
    };
    let Value::Object(obj) = msg else {
        let why = if msg.is_array() {
            "batches are not supported: send one request per line"
        } else {
            "a request is a JSON object"
        };
        return Some(error(Value::Null, -32600, why));
    };
    // A response to a request of the server's (it sends none): nothing to answer.
    if !obj.contains_key("method") && (obj.contains_key("result") || obj.contains_key("error")) {
        return None;
    }
    let id = match obj.get("id") {
        None => None,
        Some(v @ (Value::String(_) | Value::Number(_))) => Some(v.clone()),
        Some(_) => {
            return Some(error(Value::Null, -32600, "`id` is a string or a number"));
        }
    };
    let invalid = |why: &str| Some(error(id.clone().unwrap_or(Value::Null), -32600, why));
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return invalid("`jsonrpc` is \"2.0\"");
    }
    let method = match obj.get("method") {
        Some(Value::String(m)) => m.as_str(),
        Some(_) => return invalid("`method` is a string"),
        None => return invalid("a request has a `method`"),
    };
    // A notification (`notifications/initialized`, `notifications/cancelled`): nothing to
    // answer.
    let id = id?;
    let params = match obj.get("params") {
        None => Value::Object(Map::new()),
        Some(p @ Value::Object(_)) => p.clone(),
        Some(_) => return Some(error(id, -32602, "`params` is an object")),
    };
    Some(match method {
        "initialize" => result(id, initialize(&params)),
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": [search_tool()] })),
        "tools/call" => match call(&params, s) {
            Ok(r) => result(id, r),
            Err(e) => error(id, -32602, &e),
        },
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

fn result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| VERSIONS.contains(v))
        .unwrap_or(VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "searchfor", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "searchfor (s4) searches like ripgrep and gives each Match its Trail: the \
            section, function, key, row, page or symbol it lies in. Use `search` to find text \
            and know where it sits without opening whole files; `within` keeps the Matches \
            under a Trail, `expand` returns the whole Section of each Match."
    })
}

/// The `search` tool (spec §9).
fn search_tool() -> Value {
    json!({
        "name": "search",
        "title": "Search with Trails",
        "description": "Search files like ripgrep (Rust regex syntax). Each Match comes with its \
            Trail: the headings, function, key path, table row, page or symbol it lies in. \
            Markdown, code, YAML/JSON/TOML, HTML/XML, CSV, subtitles, PDF and Office documents, \
            SQLite and object files. A line with Matches under two Trails is a Match per Trail. \
            Long texts come cut (`cut`), a Section repeated by a later Match comes once \
            (`same_as`), and `truncated` says the search found more than the answer holds, \
            `truncated_by` why: `max_results` (raise it, or narrow `paths`, `glob` or \
            `within`), `bytes` (the answer is full: narrow the search, or leave out `expand`), \
            or `long_line` (lines longer than 4 MiB, the files of the first 20 named in \
            `errors` and the rest counted: search them with s4 from a shell).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "A regular expression, as ripgrep reads it." },
                "paths": { "type": "array", "items": { "type": "string" },
                    "description": "Files or directories to search; the server's working directory when empty." },
                "within": { "type": "string",
                    "description": "Keep only the Matches whose Trail contains these Segment names, in order: \"Install › Linux\"." },
                "expand": { "type": "boolean", "description": "Return the whole Section of the last Segment of each Match." },
                "glob": { "type": "array", "items": { "type": "string" },
                    "description": "Include or exclude paths, as ripgrep's -g (\"*.md\", \"!vendor/**\")." },
                "case_insensitive": { "type": "boolean", "description": "Search case insensitively." },
                "max_results": { "type": "integer", "minimum": 1, "maximum": MAX_RESULTS,
                    "description": "The most Matches to return (100 when not given)." }
            },
            "required": ["pattern"],
            "additionalProperties": false
        },
        "outputSchema": {
            "type": "object",
            "properties": {
                "matches": { "type": "array", "items": { "type": "object" } },
                "truncated": { "type": "boolean" },
                "truncated_by": { "type": "string", "enum": ["max_results", "bytes", "long_line"] },
                "errors": { "type": "string" }
            },
            "required": ["matches", "truncated"]
        },
        "annotations": { "readOnlyHint": true, "openWorldHint": false }
    })
}

/// The command line of a `search` call and its `max_results`, every argument checked against
/// the schema: `--no-config` first (the configuration file's `--pre`, `-z` or paths never
/// reach a call), `--json`, the flags the arguments ask for with each value joined to its flag
/// (`--within=-x` is a value, `--within -x` would be a flag), and the paths after `--`.
pub fn search_args(arguments: &Value) -> Result<(Vec<OsString>, u64), String> {
    let obj = arguments.as_object().ok_or("`arguments` is an object")?;
    if let Some(key) = obj.keys().find(|k| !ARGUMENTS.contains(&k.as_str())) {
        return Err(format!("unknown argument `{key}`"));
    }
    let text = |key: &str, v: &Value| -> Result<String, String> {
        let s = v.as_str().ok_or(format!("`{key}` is a string"))?;
        if s.contains('\0') {
            return Err(format!(
                "`{key}` holds a NUL, which a command line cannot carry"
            ));
        }
        Ok(s.to_string())
    };
    let texts = |key: &str| -> Result<Vec<String>, String> {
        match obj.get(key) {
            None => Ok(Vec::new()),
            Some(Value::Array(v)) => v
                .iter()
                .map(|x| text(key, x).map_err(|_| format!("`{key}` is an array of strings")))
                .collect(),
            Some(_) => Err(format!("`{key}` is an array of strings")),
        }
    };
    let switch = |key: &str| -> Result<bool, String> {
        match obj.get(key) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(format!("`{key}` is a boolean")),
        }
    };
    let pattern = text(
        "pattern",
        obj.get("pattern").ok_or("`pattern` is required")?,
    )?;
    let mut args: Vec<OsString> = ["--no-config", "--json", "--color", "never"]
        .map(OsString::from)
        .to_vec();
    if switch("case_insensitive")? {
        args.push("-i".into());
    }
    if switch("expand")? {
        args.push("--expand".into());
    }
    if let Some(w) = obj.get("within") {
        args.push(format!("--within={}", text("within", w)?).into());
    }
    for g in texts("glob")? {
        args.push(format!("--glob={g}").into());
    }
    args.push(format!("--regexp={pattern}").into());
    args.push("--".into());
    args.extend(texts("paths")?.into_iter().map(OsString::from));
    let max = match obj.get("max_results") {
        None => DEFAULT_MAX_RESULTS,
        // JSON Schema's integer is a number without a fraction: 100.0 is 100.
        Some(v) => v
            .as_u64()
            .or_else(|| {
                v.as_f64()
                    .filter(|f| f.fract() == 0.0 && *f >= 1.0)
                    .map(|f| f as u64)
            })
            .filter(|n| (1..=MAX_RESULTS).contains(n))
            .ok_or(format!(
                "`max_results` is an integer from 1 to {MAX_RESULTS}"
            ))?,
    };
    Ok((args, max))
}

/// A Section already in the answer: its file, by the exact path s4 gave (its JSON, so two
/// paths that read the same with U+FFFD differ: review r2 of the M5a plan, Codex #5), and its
/// first and last line.
type SectionKey = (String, Value, Value);

/// The Matches of a `match` message's `data`, as the Trail and the Section of each: one per
/// distinct Trail of its submatches, in the order they first come, each with the Section of
/// the first submatch under it, as the Trail layouts print a row per Trail. s4 gives a
/// submatch its own `trail` and `section` only when they differ from the line's (spec §6.4;
/// review r2 of the M5a plan, Fable #2). They come one at a time and borrowed from the
/// message, so a line of many Trails costs only the Matches the answer takes (review r3,
/// Codex #1).
fn by_trail(d: &Value) -> impl Iterator<Item = (&Value, Option<&Value>)> {
    let subs = d["submatches"].as_array().map_or(&[][..], Vec::as_slice);
    let line = (&d["trail"], d.get("section"));
    let split = subs.iter().any(|s| s.get("trail").is_some());
    let mut trails: Vec<&Value> = Vec::new();
    let each = subs[..if split { subs.len() } else { 0 }]
        .iter()
        .filter_map(move |sub| {
            let trail = sub.get("trail").unwrap_or(line.0);
            if trails.contains(&trail) {
                return None;
            }
            trails.push(trail);
            Some((trail, sub.get("section").or(line.1)))
        });
    (!split).then_some(line).into_iter().chain(each)
}

/// A Match of `s4 --json` under `trail`, with `section` (`by_trail`), as the tool returns it
/// (D17): `path`, `line`, the Trail's names, the line's `text` without its terminator, the
/// Format, and with `expand` the Section.
/// Bytes that are not UTF-8 are decoded with U+FFFD (`lossy`), and a path that is not UTF-8
/// also comes exact, in base64 (`path_base64`). The text is s4's Fragment when the line is
/// longer than it (`omitted_before`, `omitted_after`); a text, a name or a Section longer than
/// its limit is cut (`cut`), the outermost names past `MAX_TRAIL` left out (`trail_omitted`),
/// and a Section `seen` before gives the Match that holds it (`same_as`). Returns the Section
/// this Match holds, to remember once the Match is in the answer.
fn compact(
    d: &Value,
    trail: &Value,
    section: Option<&Value>,
    seen: &HashMap<SectionKey, usize>,
) -> (Value, Option<SectionKey>) {
    let (path, path_lossy) = decoded(&d["path"]);
    let (mut line, line_lossy) = decoded(&d["lines"]);
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    let mut out = Map::new();
    out.insert("path".into(), Value::String(path));
    if path_lossy {
        out.insert("path_base64".into(), d["path"]["bytes"].clone());
    }
    out.insert("line".into(), d["line_number"].clone());
    let mut cut = false;
    let names: Vec<&Value> = trail
        .as_array()
        .map(|t| t.iter().map(|s| &s["name"]).collect())
        .unwrap_or_default();
    let omitted = names.len().saturating_sub(MAX_TRAIL);
    let trail: Vec<Value> = names[omitted..]
        .iter()
        .map(|n| {
            let (n, c) = cut_to(n.as_str().unwrap_or_default(), MAX_NAME_BYTES);
            cut |= c;
            Value::String(n)
        })
        .collect();
    out.insert("trail".into(), Value::Array(trail));
    if omitted > 0 {
        out.insert("trail_omitted".into(), omitted.into());
    }
    let fragment = d["fragment"].as_object();
    let whole = fragment
        .and_then(|f| f.get("text"))
        .and_then(Value::as_str)
        .unwrap_or(&line);
    let (text, c) = cut_to(whole, MAX_TEXT_BYTES);
    cut |= c;
    out.insert("text".into(), Value::String(text));
    if let Some(f) = fragment {
        out.insert("omitted_before".into(), f["omitted_before"].clone());
        out.insert("omitted_after".into(), f["omitted_after"].clone());
    }
    out.insert("format".into(), d["format"].clone());
    if path_lossy || line_lossy {
        out.insert("lossy".into(), Value::Bool(true));
    }
    let mut holds = None;
    if let Some(section) = section {
        let value = match section.as_object() {
            None => Value::Null,
            Some(s) => {
                let file = d["path"].to_string();
                let key = (file, s["start_line"].clone(), s["end_line"].clone());
                let mut v = json!({ "start_line": key.1.clone(), "end_line": key.2.clone() });
                match seen.get(&key) {
                    Some(&i) => v["same_as"] = i.into(),
                    None => {
                        let (t, c) =
                            cut_to(s["text"].as_str().unwrap_or_default(), MAX_SECTION_BYTES);
                        cut |= c;
                        v["text"] = Value::String(t);
                        holds = Some(key);
                    }
                }
                v
            }
        };
        out.insert("section".into(), value);
    }
    if cut {
        out.insert("cut".into(), Value::Bool(true));
    }
    (Value::Object(out), holds)
}

/// grep-printer's `{"text": …}` or `{"bytes": base64}` as text, and whether it was bytes.
fn decoded(v: &Value) -> (String, bool) {
    if let Some(t) = v.get("text").and_then(Value::as_str) {
        return (t.to_string(), false);
    }
    let bytes = v
        .get("bytes")
        .and_then(Value::as_str)
        .map(base64_decode)
        .unwrap_or_default();
    (String::from_utf8_lossy(&bytes).into_owned(), true)
}

/// Standard base64 with padding, as grep-printer writes it; what is not base64 is skipped.
fn base64_decode(s: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for v in s.bytes().filter_map(value) {
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// `s` in at most `max` bytes, cut at a character's boundary, and whether it was cut.
fn cut_to(s: &str, max: usize) -> (String, bool) {
    if s.len() <= max {
        return (s.to_string(), false);
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

/// Cuts a Match's Section and then its text by halves until the Match costs at most `room`:
/// a Match is never left out for its size alone, which counts its escapes (a control
/// character is six bytes in JSON and twelve in `content`; review r2 of the M5a plan, Fable
/// #1; review r3, Fable #1).
fn shrink(m: &mut Value, room: usize) {
    for section in [true, false] {
        while cost(m) > room {
            let text = if section {
                m.get_mut("section").and_then(|s| s.get_mut("text"))
            } else {
                m.get_mut("text")
            };
            let Some(Value::String(t)) = text else { break };
            if t.is_empty() {
                break;
            }
            *t = cut_to(t, t.len() / 2).0;
            m["cut"] = Value::Bool(true);
        }
    }
}

/// The file a `match` message longer than `MAX_MESSAGE_BYTES` is about, read from the part of
/// it read (s4 writes `path` first), cut to `MAX_NAME_BYTES`.
fn path_of_prefix(prefix: &[u8]) -> String {
    const HEAD: &[u8] = br#"{"type":"match","data":{"path":"#;
    prefix
        .strip_prefix(HEAD)
        .and_then(|_| value_at(prefix, HEAD.len()))
        .map_or_else(
            || "a file".to_string(),
            |(p, _)| format!("`{}`", cut_to(&decoded(&p).0, MAX_NAME_BYTES).0),
        )
}

/// The bytes a Match adds to the answer: its JSON in `structuredContent`, the same again
/// escaped in the text of `content`, and a comma in each.
fn cost(m: &Value) -> usize {
    let json = m.to_string();
    let escaped = Value::String(json.clone()).to_string().len() - 2;
    json.len() + escaped + 2
}

/// The bytes a message adds to `errors`: escaped in `structuredContent`, escaped again in the
/// text of `content`, and the line break that joins it to the next in each.
fn message_cost(message: &str) -> usize {
    let json = Value::String(format!("{message}\n")).to_string();
    json.len() + Value::String(json).to_string().len()
}

fn call(params: &Value, s: &dyn Searcher) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("`name` is a string")?;
    if name != "search" {
        return Err(format!("unknown tool `{name}`"));
    }
    let empty = json!({});
    let (args, max) = search_args(params.get("arguments").unwrap_or(&empty))?;
    Ok(run(s, &args, max))
}

/// Runs a search and answers with what it found (D17): at most `max` Matches and
/// `MAX_RESPONSE_BYTES` (the search is ended at the first Match past either: `truncated`, and
/// `truncated_by` says which), a `match` message longer than `MAX_MESSAGE_BYTES` left out and
/// its file named while `MAX_LONG_LINES_NAMED` and the answer's bytes allow, the rest counted
/// (`truncated_by: "long_line"` when nothing else ended the answer), and s4's messages, if
/// any, as `errors`. A search that exits 2 with no Match, ends with any other code, or
/// does not exit (a signal) is a tool error with what s4 printed, which the model can read.
fn run(s: &dyn Searcher, args: &[OsString], max: u64) -> Value {
    let mut search = match s.start(args) {
        Ok(search) => search,
        Err(e) => return tool_error(&e),
    };
    let mut matches = Vec::new();
    let mut sections = HashMap::new();
    let mut room = ROOM_BYTES;
    let mut truncated_by = None;
    let mut left_out = Vec::new();
    let mut unnamed = 0;
    let mut stop = false;
    let mut line = Vec::new();
    // A failed read ends the loop like the end of the output; how the search ended says why.
    'read: while let Ok(Some(whole)) =
        read_line_capped(search.output(), MAX_MESSAGE_BYTES, &mut line)
    {
        if !whole {
            if line.starts_with(br#"{"type":"match""#) {
                truncated_by = truncated_by.or(Some("long_line"));
                let message = format!(
                    "a Match in {} was left out: its line is longer than {} MiB; search that \
                     file with s4 from a shell",
                    path_of_prefix(&line),
                    MAX_MESSAGE_BYTES >> 20
                );
                let size = message_cost(&message);
                if left_out.len() < MAX_LONG_LINES_NAMED && size <= room {
                    room -= size;
                    left_out.push(message);
                } else {
                    unnamed += 1;
                }
            }
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if v["type"] != "match" {
            continue;
        }
        let d = &v["data"];
        for (trail, section) in by_trail(d) {
            let (mut m, holds) = compact(d, trail, section, &sections);
            let mut size = cost(&m);
            // A Match that fits in no answer is cut until it fits, the first in what is left.
            let fits = if matches.is_empty() { room } else { ROOM_BYTES };
            if size > fits {
                shrink(&mut m, fits);
                size = cost(&m);
            }
            if matches.len() as u64 == max || size > room {
                let full = matches.len() as u64 == max;
                truncated_by = Some(if full { "max_results" } else { "bytes" });
                stop = true;
                break 'read;
            }
            room -= size;
            if let Some(key) = holds {
                sections.insert(key, matches.len());
            }
            matches.push(m);
        }
    }
    let (ended, stderr) = search.end(stop);
    if unnamed > 0 {
        left_out.push(format!(
            "{unnamed} more Matches were left out: their lines are longer than {} MiB",
            MAX_MESSAGE_BYTES >> 20
        ));
    }
    left_out.push(stderr.trim_end().to_string());
    left_out.retain(|m| !m.is_empty());
    let messages = left_out.join("\n");
    let messages = messages.as_str();
    let says = |what: String| {
        if messages.is_empty() {
            what
        } else {
            format!("{what}: {messages}")
        }
    };
    let errors = match ended {
        Ended::Stopped | Ended::Code(0 | 1) => messages.to_string(),
        Ended::Code(2) if !matches.is_empty() => {
            if messages.is_empty() {
                "s4 exited with code 2".to_string()
            } else {
                messages.to_string()
            }
        }
        Ended::Code(2) if !messages.is_empty() => return tool_error(messages),
        Ended::Code(c) => return tool_error(&says(format!("s4 ended with exit code {c}"))),
        Ended::Other(why) => return tool_error(&says(format!("s4 {why}"))),
    };
    let mut structured = json!({ "matches": matches, "truncated": truncated_by.is_some() });
    if let Some(why) = truncated_by {
        structured["truncated_by"] = why.into();
    }
    if !errors.is_empty() {
        structured["errors"] = Value::String(errors);
    }
    json!({
        "content": [{ "type": "text", "text": structured.to_string() }],
        "structuredContent": structured,
        "isError": false
    })
}

/// A tool error the model can read (a pattern that does not parse, a missing path), as MCP
/// asks, not a protocol error.
fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

/// `s4` in a child process.
struct Child(PathBuf);

/// The command of a search: `s4 ARGS` with stdin closed, and neither `SEARCHFOR_CONFIG_PATH`
/// nor `RIPGREP_CONFIG_PATH` in its environment (D17), besides `--no-config` in `ARGS`.
pub fn command(s4: &Path, args: &[OsString]) -> std::process::Command {
    let mut c = std::process::Command::new(s4);
    c.args(args)
        .env_remove(crate::cli::config::ENV)
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    c
}

struct Running {
    child: std::process::Child,
    out: std::io::BufReader<std::process::ChildStdout>,
    errors: std::thread::JoinHandle<String>,
}

impl Searcher for Child {
    fn start(&self, args: &[OsString]) -> Result<Box<dyn Search + '_>, String> {
        let mut child = command(&self.0, args)
            .spawn()
            .map_err(|e| format!("s4 did not start: {e}"))?;
        let mut stderr = child.stderr.take().expect("piped");
        // Read to its end so the child never blocks on it, keeping the first bytes.
        let errors = std::thread::spawn(move || {
            let mut kept = Vec::new();
            let _ = (&mut stderr)
                .take(MAX_STDERR_BYTES as u64)
                .read_to_end(&mut kept);
            let more = std::io::copy(&mut stderr, &mut std::io::sink()).unwrap_or(0) > 0;
            let mut s = String::from_utf8_lossy(&kept).into_owned();
            if more {
                s.push_str(" […]");
            }
            s
        });
        let out = std::io::BufReader::new(child.stdout.take().expect("piped"));
        Ok(Box::new(Running { child, out, errors }))
    }
}

impl Search for Running {
    fn output(&mut self) -> &mut dyn BufRead {
        &mut self.out
    }

    fn end(self: Box<Self>, stop: bool) -> (Ended, String) {
        let Running {
            mut child,
            out,
            errors,
        } = *self;
        if stop {
            let _ = child.kill();
        }
        drop(out);
        let ended = match child.wait() {
            Err(e) => Ended::Other(format!("could not be waited for: {e}")),
            Ok(status) => match status.code() {
                Some(c) => Ended::Code(c),
                None if stop => Ended::Stopped,
                None => Ended::Other(format!("ended without an exit code ({status})")),
            },
        };
        (ended, errors.join().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// A searcher that answers every search with the same output and end, and keeps the
    /// arguments and whether the server stopped it.
    struct Canned {
        out: Vec<u8>,
        ended: Ended,
        stderr: String,
        seen: RefCell<Vec<Vec<OsString>>>,
        stopped: Cell<Option<bool>>,
    }

    struct CannedSearch<'a> {
        out: &'a [u8],
        c: &'a Canned,
    }

    impl Searcher for Canned {
        fn start(&self, args: &[OsString]) -> Result<Box<dyn Search + '_>, String> {
            self.seen.borrow_mut().push(args.to_vec());
            Ok(Box::new(CannedSearch {
                out: &self.out,
                c: self,
            }))
        }
    }

    impl Search for CannedSearch<'_> {
        fn output(&mut self) -> &mut dyn BufRead {
            &mut self.out
        }
        fn end(self: Box<Self>, stop: bool) -> (Ended, String) {
            self.c.stopped.set(Some(stop));
            let ended = if stop {
                Ended::Stopped
            } else {
                self.c.ended.clone()
            };
            (ended, self.c.stderr.clone())
        }
    }

    /// A searcher whose output is `line` `times` times and then `rest`, made as it is read:
    /// more output than a test could hold.
    struct Repeated {
        line: Vec<u8>,
        times: usize,
        rest: Vec<u8>,
        stderr: String,
    }

    struct RepeatedSearch<'a> {
        r: &'a Repeated,
        left: usize,
        at: usize,
    }

    impl Searcher for Repeated {
        fn start(&self, _: &[OsString]) -> Result<Box<dyn Search + '_>, String> {
            Ok(Box::new(RepeatedSearch {
                r: self,
                left: self.times,
                at: 0,
            }))
        }
    }

    impl Read for RepeatedSearch<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = {
                let b = self.fill_buf()?;
                let n = b.len().min(buf.len());
                buf[..n].copy_from_slice(&b[..n]);
                n
            };
            self.consume(n);
            Ok(n)
        }
    }

    impl BufRead for RepeatedSearch<'_> {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            let bytes = if self.left > 0 {
                &self.r.line
            } else {
                &self.r.rest
            };
            Ok(&bytes[self.at.min(bytes.len())..])
        }
        fn consume(&mut self, n: usize) {
            self.at += n;
            if self.left > 0 && self.at == self.r.line.len() {
                self.left -= 1;
                self.at = 0;
            }
        }
    }

    impl Search for RepeatedSearch<'_> {
        fn output(&mut self) -> &mut dyn BufRead {
            self
        }
        fn end(self: Box<Self>, stop: bool) -> (Ended, String) {
            let ended = if stop { Ended::Stopped } else { Ended::Code(0) };
            (ended, self.r.stderr.clone())
        }
    }

    fn guide() -> Value {
        json!({"type":"match","data":{"path":{"text":"md/guide.md"},
            "lines":{"text":"apt install needle-pkg\n"},"line_number":12,"absolute_offset":96,
            "submatches":[{"match":{"text":"needle"},"start":12,"end":18}],"format":"markdown",
            "trail":[{"kind":"heading","name":"Install","line":5,"end_line":34,"level":1},
                     {"kind":"heading","name":"Linux","line":9,"end_line":18,"level":2}],
            "fragment":null}})
    }

    fn with(messages: &[Value], ended: Ended, stderr: &str) -> Canned {
        let mut out = br#"{"type":"begin","data":{"path":{"text":"md/guide.md"}}}"#.to_vec();
        out.push(b'\n');
        for m in messages {
            out.extend_from_slice(m.to_string().as_bytes());
            out.push(b'\n');
        }
        Canned {
            out,
            ended,
            stderr: stderr.into(),
            seen: Default::default(),
            stopped: Cell::new(None),
        }
    }

    fn canned() -> Canned {
        with(&[guide(), guide()], Ended::Code(0), "")
    }

    fn session(lines: &[Value], s: &dyn Searcher) -> Vec<Value> {
        let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
        raw_session(input.as_bytes(), s)
    }

    fn raw_session(input: &[u8], s: &dyn Searcher) -> Vec<Value> {
        let mut out = Vec::new();
        serve_io(input, &mut out, s).unwrap();
        String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// One `tools/call` of `search` with these arguments, and its result.
    fn call_with(arguments: Value, s: &dyn Searcher) -> Value {
        let out = session(
            &[json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"search","arguments":arguments}})],
            s,
        );
        out[0]["result"].clone()
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn initialize_list_and_call() {
        let s = canned();
        let out = session(
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}),
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"pattern":"needle","within":"Install › Linux","max_results":1}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"ping"}),
            ],
            &s,
        );
        assert_eq!(out.len(), 4, "the notification gets no answer: {out:?}");
        assert_eq!(out[0]["id"], 1);
        assert_eq!(out[0]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(out[0]["result"]["serverInfo"]["name"], "searchfor");
        assert!(out[0]["result"]["capabilities"]["tools"].is_object());
        let tools = &out[1]["result"]["tools"];
        assert_eq!(tools[0]["name"], "search");
        assert_eq!(tools[0]["inputSchema"]["required"], json!(["pattern"]));
        let r = &out[2]["result"];
        assert_eq!(r["isError"], false);
        assert_eq!(r["structuredContent"]["truncated"], true);
        assert_eq!(
            s.stopped.get(),
            Some(true),
            "the search is ended at the limit"
        );
        assert_eq!(
            r["structuredContent"]["matches"],
            json!([{"path":"md/guide.md","line":12,"trail":["Install","Linux"],
                    "text":"apt install needle-pkg","format":"markdown"}])
        );
        let text: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, r["structuredContent"]);
        assert_eq!(out[3]["result"], json!({}));
        assert_eq!(
            strings(&s.seen.borrow()[0]),
            [
                "--no-config",
                "--json",
                "--color",
                "never",
                "--within=Install › Linux",
                "--regexp=needle",
                "--"
            ]
        );
    }

    #[test]
    fn an_unknown_version_gets_the_newest() {
        let out = session(
            &[
                json!({"jsonrpc":"2.0","id":"a","method":"initialize","params":{"protocolVersion":"1999-01-01"}}),
            ],
            &canned(),
        );
        assert_eq!(out[0]["id"], "a");
        assert_eq!(out[0]["result"]["protocolVersion"], VERSIONS[0]);
    }

    #[test]
    fn errors_of_the_protocol_and_of_the_tool() {
        let s = with(&[], Ended::Code(2), "s4: regex parse error\n");
        let out = session(
            &[
                json!({"jsonrpc":"2.0","id":1,"method":"resources/list"}),
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"grep","arguments":{}}}),
                json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"pattern":7}}}),
                json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search","arguments":{"pattern":"x","bogus":1}}}),
                json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"search","arguments":{"pattern":"("}}}),
            ],
            &s,
        );
        assert_eq!(out[0]["error"]["code"], -32601);
        assert_eq!(out[1]["error"]["code"], -32602);
        assert_eq!(out[2]["error"]["code"], -32602);
        assert!(
            out[3]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("bogus")
        );
        assert_eq!(out[4]["result"]["isError"], true);
        assert_eq!(
            out[4]["result"]["content"][0]["text"],
            "s4: regex parse error"
        );
        let out = raw_session(b"{not json}\n\n", &s);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["error"]["code"], -32700);
    }

    /// Codex r1 #6, Fable r1 #8: what JSON-RPC 2.0 does not allow gets -32600 (id null when
    /// the id cannot be read), a line that is not UTF-8 -32700, a request past
    /// `MAX_REQUEST_BYTES` -32600; the server answers the next request after each.
    #[test]
    fn invalid_requests_are_answered() {
        let s = canned();
        let mut input = Vec::new();
        for l in [
            json!({}),
            json!([{"jsonrpc":"2.0","id":1,"method":"ping"}]),
            json!(7),
            json!({"jsonrpc":"2.0","id":{},"method":"ping"}),
            json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":2,"method":7}),
            json!({"id":3,"method":"ping"}),
            json!({"jsonrpc":"1.0","id":4,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":5}),
            json!({"jsonrpc":"2.0","id":6,"method":"ping","params":[1]}),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"search","arguments":[]}}),
        ] {
            input.extend_from_slice(format!("{l}\n").as_bytes());
        }
        // Answered with nothing: a notification with params of any kind, and a response.
        input.extend_from_slice(
            b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/x\",\"params\":3}\n",
        );
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"result\":{}}\n");
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":\"\xff\",\"method\":\"ping\"}\n");
        input.extend_from_slice(&vec![b' '; MAX_REQUEST_BYTES + 1]);
        input.extend_from_slice(b"\n{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"ping\"}\n");
        // Longer than a request may be: answered with its id when the id comes before what
        // is cut (review r2 of the M5a plan, Fable #3), else with null.
        let long = "x".repeat(MAX_REQUEST_BYTES);
        let early = json!({"jsonrpc":"2.0","id":"r-11","method":"tools/call","params":{"name":"search","arguments":{"pattern":long}}});
        input.extend_from_slice(format!("{early}\n").as_bytes());
        input.extend_from_slice(
            format!("{{\"params\":{{\"note\":\"{long}\"}},\"id\":12,\"jsonrpc\":\"2.0\"}}\n")
                .as_bytes(),
        );
        let out = raw_session(&input, &s);
        let codes: Vec<(Value, Value)> = out
            .iter()
            .map(|a| (a["id"].clone(), a["error"]["code"].clone()))
            .collect();
        assert_eq!(
            codes,
            [
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (Value::Null, json!(-32600)),
                (json!(2), json!(-32600)),
                (json!(3), json!(-32600)),
                (json!(4), json!(-32600)),
                (json!(5), json!(-32600)),
                (json!(6), json!(-32602)),
                (json!(7), json!(-32602)),
                (Value::Null, json!(-32700)),
                (Value::Null, json!(-32600)),
                (json!(10), Value::Null),
                (json!("r-11"), json!(-32600)),
                (Value::Null, json!(-32600)),
            ]
        );
        assert!(
            out[1]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("batch")
        );
        assert_eq!(out[13]["result"], json!({}));
        assert!(s.seen.borrow().is_empty());
    }

    #[test]
    fn search_arguments_become_flags() {
        let (args, max) = search_args(&json!({
            "pattern": "-x",
            "paths": ["a", "-b"],
            "expand": true,
            "glob": ["*.md", "!vendor/**", "-g"],
            "within": "--files",
            "case_insensitive": true
        }))
        .unwrap();
        // Every value joined to its flag, the paths after `--`: nothing the client sends is
        // read as a flag of s4 (Codex r1 #1).
        assert_eq!(
            strings(&args),
            [
                "--no-config",
                "--json",
                "--color",
                "never",
                "-i",
                "--expand",
                "--within=--files",
                "--glob=*.md",
                "--glob=!vendor/**",
                "--glob=-g",
                "--regexp=-x",
                "--",
                "a",
                "-b"
            ]
        );
        assert_eq!(max, DEFAULT_MAX_RESULTS);
        let max = |v: Value| search_args(&json!({"pattern":"x","max_results":v})).map(|a| a.1);
        assert_eq!(max(json!(100.0)), Ok(100));
        assert_eq!(max(json!(MAX_RESULTS)), Ok(MAX_RESULTS));
        for bad in [
            json!(0),
            json!(-1),
            json!(2.5),
            json!(MAX_RESULTS + 1),
            json!("5"),
            Value::Null,
        ] {
            assert!(max(bad.clone()).is_err(), "{bad}");
        }
        // Each argument has its schema's type; null is not one of them.
        for bad in [
            json!({"pattern":"x","glob":"*.md"}),
            json!({"pattern":"x","paths":[1]}),
            json!({"pattern":"x","paths":null}),
            json!({"pattern":"x","expand":"true"}),
            json!({"pattern":"x","case_insensitive":1}),
            json!({"pattern":"x","within":7}),
            json!({"pattern":"x","within":null}),
            json!({"pattern":"a\u{0}b"}),
            json!({"pattern":"x","paths":["a\u{0}"]}),
            json!({"paths":["a"]}),
            json!(["x"]),
        ] {
            assert!(search_args(&bad).is_err(), "{bad}");
        }
    }

    /// Fable r1 #1, Codex r1 #1: the search reads no configuration file, so neither its
    /// `--pre`, `-z` nor paths reach a call.
    #[test]
    fn the_search_runs_without_configuration() {
        let (args, _) = search_args(&json!({"pattern":"x"})).unwrap();
        assert_eq!(args[0], "--no-config");
        assert!(!args.iter().any(|a| a == "--pre" || a == "-z"));
        let c = command(Path::new("s4"), &args);
        let envs: Vec<_> = c.get_envs().collect();
        for var in ["SEARCHFOR_CONFIG_PATH", "RIPGREP_CONFIG_PATH"] {
            assert!(
                envs.contains(&(std::ffi::OsStr::new(var), None)),
                "{var}: {envs:?}"
            );
        }
    }

    /// Codex r1 #2, Fable r1 #2: a long line comes as s4's Fragment, or cut; a long Section and
    /// a long name come cut; a Section repeated comes once; a Trail past `MAX_TRAIL` keeps
    /// its innermost names.
    #[test]
    fn long_texts_come_cut() {
        let mut long = guide();
        long["data"]["lines"]["text"] = json!(format!("{}\n", "é".repeat(MAX_TEXT_BYTES)));
        let mut fragment = guide();
        fragment["data"]["fragment"] =
            json!({"text":"install needle","start":4,"omitted_before":4,"omitted_after":4});
        let mut deep = guide();
        let names: Vec<Value> = (0..MAX_TRAIL + 2)
            .map(|i| json!({"kind":"key","name":format!("k{i}"),"line":1,"end_line":1}))
            .collect();
        deep["data"]["trail"] = Value::Array(names);
        deep["data"]["trail"][MAX_TRAIL + 1]["name"] = json!("n".repeat(MAX_NAME_BYTES + 1));
        let section =
            json!({"start_line":9,"end_line":18,"text":"s".repeat(MAX_SECTION_BYTES + 1)});
        let mut expanded = guide();
        expanded["data"]["section"] = section.clone();
        let s = with(
            &[long, fragment, deep, expanded.clone(), expanded],
            Ended::Code(0),
            "",
        );
        let r = call_with(json!({"pattern":"needle","expand":true}), &s);
        let m = &r["structuredContent"]["matches"];
        assert_eq!(m[0]["cut"], true);
        assert_eq!(m[0]["text"].as_str().unwrap().len(), MAX_TEXT_BYTES);
        assert_eq!(m[1]["text"], "install needle");
        assert_eq!(
            (&m[1]["omitted_before"], &m[1]["cut"]),
            (&json!(4), &Value::Null)
        );
        assert_eq!(m[2]["trail_omitted"], 2);
        let trail = m[2]["trail"].as_array().unwrap();
        assert_eq!((trail.len(), &trail[0]), (MAX_TRAIL, &json!("k2")));
        assert_eq!(trail[MAX_TRAIL - 1].as_str().unwrap().len(), MAX_NAME_BYTES);
        assert_eq!(m[2]["cut"], true);
        assert_eq!(
            m[3]["section"]["text"].as_str().unwrap().len(),
            MAX_SECTION_BYTES
        );
        assert_eq!(m[3]["cut"], true);
        assert_eq!(
            m[4]["section"],
            json!({"start_line":9,"end_line":18,"same_as":3})
        );
        assert_eq!(r["structuredContent"]["truncated"], false);
    }

    /// Codex r1 #2, Fable r1 #2: the answer stays within `MAX_RESPONSE_BYTES`, the search
    /// ended where it fills; a message past `MAX_MESSAGE_BYTES` is left out unread.
    #[test]
    fn the_answer_has_a_size_limit() {
        let mut big = guide();
        big["data"]["lines"]["text"] = json!(format!("{}\n", "\"x".repeat(MAX_TEXT_BYTES / 2)));
        let s = with(&vec![big.clone(); 400], Ended::Code(0), "");
        let out = session(
            &[json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":"search","arguments":{"pattern":"x","max_results":MAX_RESULTS}}})],
            &s,
        );
        let line = out[0].to_string();
        assert!(
            line.len() <= MAX_RESPONSE_BYTES + ENVELOPE_BYTES + 1,
            "{}",
            line.len()
        );
        let r = &out[0]["result"]["structuredContent"];
        assert_eq!(r["truncated"], true);
        assert_eq!(r["truncated_by"], "bytes");
        // The limit is the result's; the envelope adds `ENVELOPE_BYTES` and the client's id as
        // it sent it (review r2 of the M5a plan, Codex #6).
        let id = "i".repeat(300_000);
        let s = with(&vec![big; 400], Ended::Code(0), "");
        let out = session(
            &[json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
                "params":{"name":"search","arguments":{"pattern":"x","max_results":MAX_RESULTS}}})],
            &s,
        );
        let result = out[0]["result"].to_string().len();
        assert!(result <= MAX_RESPONSE_BYTES, "{result}");
        assert_eq!(
            out[0].to_string().len(),
            result + ENVELOPE_BYTES + json!(id).to_string().len()
        );
        assert!(!r["matches"].as_array().unwrap().is_empty());
        assert_eq!(s.stopped.get(), Some(true));
        let mut huge = br#"{"type":"match","data":{"lines":{"text":""#.to_vec();
        huge.extend(std::iter::repeat_n(b'x', MAX_MESSAGE_BYTES));
        huge.extend_from_slice(b"\"}}}\n");
        huge.extend_from_slice(format!("{}\n", guide()).as_bytes());
        let s = Canned {
            out: huge,
            ..with(&[], Ended::Code(0), "")
        };
        let r = call_with(json!({"pattern":"x"}), &s);
        let r = &r["structuredContent"];
        assert_eq!(
            (
                r["truncated"].clone(),
                r["matches"].as_array().unwrap().len()
            ),
            (json!(true), 1)
        );
    }

    /// Review r2 of the M5a plan, Fable #1: a Match left out says why. A `match` longer than
    /// `MAX_MESSAGE_BYTES` names its file in `errors`; the first Match is never left out for
    /// its size, its texts cut until it fits; and `truncated_by` says which limit ended the
    /// answer.
    #[test]
    fn left_out_matches_say_why() {
        let mut huge =
            br#"{"type":"match","data":{"path":{"text":"x/huge.txt"},"lines":{"text":""#.to_vec();
        huge.extend(std::iter::repeat_n(b'x', MAX_MESSAGE_BYTES));
        huge.extend_from_slice(b"\"}}}\n");
        huge.extend_from_slice(format!("{}\n", guide()).as_bytes());
        let s = Canned {
            out: huge,
            ..with(&[], Ended::Code(0), "")
        };
        let r = call_with(json!({"pattern":"x"}), &s);
        let r = &r["structuredContent"];
        assert_eq!(r["matches"].as_array().unwrap().len(), 1);
        assert_eq!(
            (&r["truncated"], &r["truncated_by"]),
            (&json!(true), &json!("long_line"))
        );
        let errors = r["errors"].as_str().unwrap();
        assert!(
            errors.contains("x/huge.txt") && errors.contains("4 MiB"),
            "{errors}"
        );
        // A Section of control characters, each six bytes escaped and twelve in `content`.
        let mut ctrl = guide();
        ctrl["data"]["section"] =
            json!({"start_line":9,"end_line":18,"text":"\u{1}".repeat(MAX_SECTION_BYTES)});
        let s = with(&[ctrl.clone()], Ended::Code(0), "");
        let r = call_with(json!({"pattern":"needle","expand":true}), &s);
        let r = &r["structuredContent"];
        let m = r["matches"].as_array().unwrap();
        assert_eq!((m.len(), &m[0]["cut"]), (1, &json!(true)));
        assert!(!m[0]["section"]["text"].as_str().unwrap().is_empty());
        assert_eq!(
            (&r["truncated"], &r["truncated_by"]),
            (&json!(false), &Value::Null)
        );
        // Review r3 of the plan, Fable #1: a Match that fits in no answer is cut wherever it
        // comes, so it does not end an answer that has room for it cut.
        let s = with(&[guide(), ctrl], Ended::Code(0), "");
        let r = call_with(json!({"pattern":"needle","expand":true}), &s);
        let r = &r["structuredContent"];
        let m = r["matches"].as_array().unwrap();
        assert_eq!((m.len(), &m[1]["cut"]), (2, &json!(true)));
        assert_eq!(
            (&r["truncated"], &r["truncated_by"]),
            (&json!(false), &Value::Null)
        );
        let s = with(&[guide(), guide()], Ended::Code(0), "");
        let r = call_with(json!({"pattern":"needle","max_results":1}), &s);
        assert_eq!(r["structuredContent"]["truncated_by"], "max_results");
    }

    /// Review r3 of the M5a plan, Codex #2, Fable #2: the `match` messages too long to read are
    /// named in `errors` up to `MAX_LONG_LINES_NAMED`, each paid for in both copies of it, and
    /// the rest counted. Many of them, with names that are all escapes, s4's longest stderr and
    /// Matches to fill the answer keep the whole result within `MAX_RESPONSE_BYTES`.
    #[test]
    fn many_long_lines_keep_the_answer_bounded() {
        let mut line = br#"{"type":"match","data":{"path":{"text":""#.to_vec();
        line.extend("\\u0001".repeat(MAX_NAME_BYTES).bytes());
        line.extend_from_slice(br#""},"lines":{"text":""#);
        line.extend(std::iter::repeat_n(b'x', MAX_MESSAGE_BYTES));
        line.extend_from_slice(b"\"}}}\n");
        let times = 2 * MAX_LONG_LINES_NAMED;
        let s = Repeated {
            line,
            times,
            rest: format!("{}\n", guide()).repeat(1000).into_bytes(),
            stderr: format!("{} […]", "\u{1}".repeat(MAX_STDERR_BYTES)),
        };
        let r = call_with(json!({"pattern":"x","max_results":MAX_RESULTS}), &s);
        let size = r.to_string().len();
        assert!(size <= MAX_RESPONSE_BYTES, "{size}");
        let r = &r["structuredContent"];
        assert!(!r["matches"].as_array().unwrap().is_empty());
        assert_eq!(r["truncated_by"], "bytes");
        let errors = r["errors"].as_str().unwrap();
        assert_eq!(
            errors.matches("a Match in `").count(),
            MAX_LONG_LINES_NAMED,
            "{errors}"
        );
        let more = format!(
            "{} more Matches were left out",
            times - MAX_LONG_LINES_NAMED
        );
        assert!(errors.contains(&more), "{errors}");
    }

    /// Review r2 of the M5a plan, Fable #2: a line whose submatches lie under different
    /// Trails is a Match per Trail, as the Trail layouts print a row per Trail; a submatch's
    /// own Section, when s4 gives one, is that Match's.
    #[test]
    fn a_match_per_trail_of_the_line() {
        let cell = |name: &str| {
            json!([{"kind":"heading","name":"Plans","line":19,"end_line":34,"level":2},
                   {"kind":"cell","name":name,"line":24,"end_line":24}])
        };
        let mut row = guide();
        row["data"]["line_number"] = json!(24);
        row["data"]["lines"] = json!({"text":"| needle-a | needle-b |\n"});
        row["data"]["trail"] = cell("Plan");
        row["data"]["submatches"] = json!([
            {"match":{"text":"needle-a"},"start":2,"end":10,"trail":cell("Plan")},
            {"match":{"text":"needle-b"},"start":13,"end":21,"trail":cell("Price")},
            {"match":{"text":"needle-a"},"start":22,"end":30,"trail":cell("Plan"),
             "section":{"start_line":20,"end_line":24,"text":"other"}}
        ]);
        row["data"]["section"] = json!({"start_line":21,"end_line":24,"text":"| Plan | Price |"});
        let s = with(&[row], Ended::Code(0), "");
        let r = call_with(json!({"pattern":"needle-[ab]","expand":true}), &s);
        let m = r["structuredContent"]["matches"]
            .as_array()
            .unwrap()
            .clone();
        let seen: Vec<(Value, Value, Value)> = m
            .iter()
            .map(|m| (m["line"].clone(), m["trail"].clone(), m["section"].clone()))
            .collect();
        assert_eq!(
            seen,
            [
                (
                    json!(24),
                    json!(["Plans", "Plan"]),
                    json!({"start_line":21,"end_line":24,"text":"| Plan | Price |"})
                ),
                (
                    json!(24),
                    json!(["Plans", "Price"]),
                    json!({"start_line":21,"end_line":24,"same_as":0})
                ),
            ]
        );
        assert_eq!(m[1]["text"], "| needle-a | needle-b |");
        // Review r3 of the plan, Codex #1: a line of many Trails costs only the Matches the
        // answer holds; each Trail is read from the message, never a copy of it.
        let mut many = guide();
        let subs: Vec<Value> = (0..240)
            .map(|i| {
                json!({"match":{"text":"needle"},"start":12,"end":18,
                    "trail":[{"kind":"element","name":format!("k{i}"),"line":1,"end_line":1}]})
            })
            .collect();
        many["data"]["submatches"] = Value::Array(subs);
        let d = &many["data"];
        let (trail, _) = by_trail(d).nth(239).unwrap();
        assert!(std::ptr::eq(trail, &d["submatches"][239]["trail"]));
        let s = with(&[many], Ended::Code(0), "");
        let r = call_with(json!({"pattern":"needle","max_results":1}), &s);
        let r = &r["structuredContent"];
        assert_eq!(r["matches"][0]["trail"], json!(["k0"]));
        assert_eq!(r["truncated_by"], "max_results");
    }

    /// Review r2 of the M5a plan, Codex #5: a Section is the same only in the same file, told
    /// by its exact path: two paths that read the same with U+FFFD are two files.
    #[test]
    fn sections_of_files_whose_names_read_alike() {
        let at = |b64: &str| {
            let mut m = guide();
            m["data"]["path"] = json!({"bytes": b64});
            m["data"]["section"] = json!({"start_line":9,"end_line":18,"text":b64});
            m
        };
        let s = with(&[at("Yf4ubWQ="), at("Yf8ubWQ=")], Ended::Code(0), "");
        let r = call_with(json!({"pattern":"needle","expand":true}), &s);
        let m = &r["structuredContent"]["matches"];
        assert_eq!(
            (&m[0]["path"], &m[1]["path"]),
            (&json!("a\u{FFFD}.md"), &json!("a\u{FFFD}.md"))
        );
        assert_eq!(m[1]["section"]["text"], "Yf8ubWQ=");
        assert_eq!(m[1]["section"]["same_as"], Value::Null);
    }

    /// Codex r1 #3: every way a search ends. Exit 2 with Matches gives them and s4's errors;
    /// exit 2 without Matches, another code or a signal is a tool error with s4's message.
    #[test]
    fn how_a_search_ends() {
        let found = |ended: Ended, stderr: &str| {
            let s = with(&[guide()], ended, stderr);
            call_with(json!({"pattern":"needle"}), &s)
        };
        let r = found(
            Ended::Code(2),
            "s4: nope: No such file or directory (os error 2)\n",
        );
        assert_eq!(r["isError"], false);
        assert_eq!(
            r["structuredContent"]["matches"].as_array().unwrap().len(),
            1
        );
        assert_eq!(
            r["structuredContent"]["errors"],
            "s4: nope: No such file or directory (os error 2)"
        );
        assert_eq!(
            found(Ended::Code(2), "")["structuredContent"]["errors"],
            "s4 exited with code 2"
        );
        let r = found(Ended::Code(0), "");
        assert_eq!(
            (&r["isError"], &r["structuredContent"]["errors"]),
            (&json!(false), &Value::Null)
        );
        let r = found(Ended::Code(101), "thread 'main' panicked\n");
        assert_eq!(r["isError"], true);
        assert_eq!(
            r["content"][0]["text"],
            "s4 ended with exit code 101: thread 'main' panicked"
        );
        let r = found(
            Ended::Other("ended without an exit code (signal: 9 (SIGKILL))".into()),
            "",
        );
        assert_eq!(r["isError"], true);
        assert_eq!(
            r["content"][0]["text"],
            "s4 ended without an exit code (signal: 9 (SIGKILL))"
        );
        let s = with(&[], Ended::Code(2), "s4: regex parse error\n");
        let r = call_with(json!({"pattern":"("}), &s);
        assert_eq!(
            (&r["isError"], &r["content"][0]["text"]),
            (&json!(true), &json!("s4: regex parse error"))
        );
        assert_eq!(s.stopped.get(), Some(false));
    }

    /// Codex r1 #7: a line or a path that is not UTF-8 comes decoded, with U+FFFD (`lossy`),
    /// the path exact in base64 too.
    #[test]
    fn bytes_are_decoded() {
        let mut m = guide();
        // `needle\xff\n` and `a\xff.md`, as grep-printer writes them.
        m["data"]["lines"] = json!({"bytes":"bmVlZGxl/wo="});
        m["data"]["path"] = json!({"bytes":"Yf8ubWQ="});
        let s = with(&[m], Ended::Code(0), "");
        let r = call_with(json!({"pattern":"needle"}), &s);
        let m = &r["structuredContent"]["matches"][0];
        assert_eq!(m["text"], "needle\u{FFFD}");
        assert_eq!(m["path"], "a\u{FFFD}.md");
        assert_eq!(m["path_base64"], "Yf8ubWQ=");
        assert_eq!(m["lossy"], true);
        assert_eq!(base64_decode("Yf8ubWQ="), b"a\xff.md");
    }
}
```

- [ ] **Step 3:** `cargo test --lib mcp::`: PASS. The end-to-end session over the real binary is
  `m5_11` of C9.
- [ ] **Step 4:** commit: `Serve the search over the Model Context Protocol`.

---
## Chunk 7: the one-line layout and the summaries

### Task 7.1: the tests

**Files:** `src/output/flat.rs`, `src/output/summary.rs` (tests only).

- [ ] **Step 1:** the one-line layout with rg's prelude flags, `-o`, colours and links (D4,
  D11, D12). The prelude's order is rg's: path, line, column, byte offset, each followed by the
  field separator (`rg --column -b -n` prints `3:12:11:`):

```diff
diff --git a/src/output/flat.rs b/src/output/flat.rs
index 5197e17..cd53ac1 100644
--- a/src/output/flat.rs
+++ b/src/output/flat.rs
@@ -329,6 +399,115 @@ mod tests {
         String::from_utf8(w.into_inner()).unwrap()
     }
 
+    /// The file of the M5 tests: a heading, a Match under it, a context line, a gap, a Match.
+    fn m5_file() -> (Vec<u8>, Described) {
+        let s = src(&["# Install", "  run needle", "after", "x", "y", "z needle"]);
+        let d = described(
+            &s,
+            vec![mat(&s, 2), ctx(&s, 3), mat(&s, 6)],
+            vec![
+                vec![vec![h(1, "Install", 1, 6)]],
+                vec![vec![h(1, "Install", 1, 6)]],
+            ],
+        );
+        (s, d)
+    }
+
+    fn with(look: super::super::Look) -> RenderOptions {
+        RenderOptions {
+            context: true,
+            look,
+            ..flat()
+        }
+    }
+
+    /// M5a plan, D4: `--column` and `-b` after the line number, rg's separators, `-0` after the
+    /// path, `--path-separator`, `--context-separator`.
+    #[test]
+    fn rows_with_ripgreps_prelude_flags() {
+        use super::super::Look;
+        let (s, d) = m5_file();
+        assert_eq!(
+            plain(&d, &s, &with(Look::default())),
+            "docs/a.md:2: Install │   run needle\ndocs/a.md-3-after\n--\ndocs/a.md:6: Install │ z needle\n"
+        );
+        let look = Look {
+            column: true,
+            byte_offset: true,
+            ..Look::default()
+        };
+        assert_eq!(
+            plain(&d, &s, &with(look)),
+            "docs/a.md:2:7:10: Install │   run needle\ndocs/a.md-3-23-after\n--\ndocs/a.md:6:3:33: Install │ z needle\n"
+        );
+        let look = Look {
+            sep_match: b"=".to_vec(),
+            sep_context: b"~".to_vec(),
+            sep_group: Some(b"##".to_vec()),
+            path_terminator: Some(0),
+            path_separator: Some(b'\\'),
+            trim: true,
+            ..Look::default()
+        };
+        assert_eq!(
+            plain(&d, &s, &with(look)),
+            "docs\\a.md\x002= Install │ run needle\ndocs\\a.md\x003~after\n##\ndocs\\a.md\x006= Install │ z needle\n"
+        );
+        let look = Look {
+            sep_group: None,
+            ..Look::default()
+        };
+        assert!(!plain(&d, &s, &with(look)).contains("--"));
+    }
+
+    /// `-o` and `--vimgrep` rows in the one-line layout, each Match with its Trail.
+    #[test]
+    fn only_matching_and_vimgrep_rows() {
+        use super::super::Look;
+        let (s, d) = m5_file();
+        let look = Look {
+            only_matching: true,
+            column: true,
+            ..Look::default()
+        };
+        assert_eq!(
+            plain(&d, &s, &with(look)),
+            "docs/a.md:2:7: Install │ needle\ndocs/a.md-3-after\n--\ndocs/a.md:6:3: Install │ needle\n"
+        );
+    }
+
+    /// `--colors` and `--hyperlink-format` in the prelude: the path's and the numbers' colours,
+    /// the Trail's, and one link over path, line and column.
+    #[test]
+    fn colours_and_links() {
+        use super::super::{Look, hyperlink::Hyperlinks, style::Palette};
+        let dir = tempfile::tempdir().unwrap();
+        let path = dir.path().join("a.md");
+        std::fs::write(&path, "x").unwrap();
+        let (s, mut d) = m5_file();
+        d.file.path = path.clone();
+        d.file.items.truncate(1);
+        let look = Look {
+            column: true,
+            palette: Palette::new(&["column:fg:blue".into(), "trail:fg:yellow".into()]),
+            hyperlinks: Hyperlinks::new("file://{path}:{line}:{column}", None)
+                .map(std::sync::Arc::new),
+            ..Look::default()
+        };
+        let mut w = Ansi::new(Vec::new());
+        render(&d, &s, &with(look), None, &mut w).unwrap();
+        let out = String::from_utf8(w.into_inner()).unwrap();
+        let link = format!(
+            "\x1b]8;;file://{}:2:7\x1b\\",
+            path.canonicalize().unwrap().display()
+        );
+        assert!(out.starts_with(&link), "{out:?}");
+        assert!(
+            out.contains("\x1b[34m7\x1b[0m:\x1b]8;;\x1b\\ \x1b[0m\x1b[33mInstall"),
+            "{out:?}"
+        );
+    }
+
     /// M4 plan, D6 and D8: an object file's row is at the hex offset of its first Match, and
     /// rows are strings, not lines: no `--` between them, even with context on.
     #[test]
```

- [ ] **Step 2:** the summaries with `-0`, `--path-separator` and `--include-zero` (rg's summary
  printer: `-c -0` prints `path\0N`, `-l -0` `path\0`):

```diff
diff --git a/src/output/summary.rs b/src/output/summary.rs
index 0490f2c..c3fc953 100644
--- a/src/output/summary.rs
+++ b/src/output/summary.rs
@@ -66,7 +78,16 @@ mod tests {
 
     fn out(mode: Mode, d: Option<&Described>, with_filename: bool) -> String {
         let mut w = NoColor::new(Vec::new());
-        let wrote = render(mode, Path::new("docs/a.md"), d, with_filename, &mut w).unwrap();
+        let wrote = render(
+            mode,
+            Path::new("docs/a.md"),
+            d,
+            with_filename,
+            &Look::default(),
+            false,
+            &mut w,
+        )
+        .unwrap();
         let s = String::from_utf8(w.into_inner()).unwrap();
         assert_eq!(wrote, !s.is_empty());
         s
@@ -123,6 +144,8 @@ mod tests {
             Path::new("a.md"),
             Some(&sample()),
             true,
+            &Look::default(),
+            false,
             &mut w,
         )
         .unwrap();
@@ -130,6 +153,32 @@ mod tests {
         assert_eq!(w.into_inner(), b"\x1b[0m\x1b[35ma.md\x1b[0m:2\n");
     }
 
+    /// M5a plan, D4: `-0`, `--path-separator` and `--include-zero`, as rg's summary printer.
+    #[test]
+    fn nul_separator_and_zero_counts() {
+        let look = Look {
+            path_terminator: Some(0),
+            path_separator: Some(b'\\'),
+            ..Look::default()
+        };
+        let run = |mode, d: Option<&Described>, zero| {
+            let mut w = NoColor::new(Vec::new());
+            render(mode, Path::new("docs/a.md"), d, true, &look, zero, &mut w).unwrap();
+            w.into_inner()
+        };
+        assert_eq!(
+            run(Mode::Count, Some(&sample()), false),
+            b"docs\\a.md\x002\n"
+        );
+        assert_eq!(
+            run(Mode::FilesWithMatches, Some(&sample()), false),
+            b"docs\\a.md\x00"
+        );
+        assert_eq!(run(Mode::Count, None, false), b"");
+        assert_eq!(run(Mode::Count, None, true), b"docs\\a.md\x000\n");
+        assert_eq!(run(Mode::FilesWithMatches, None, true), b"");
+    }
+
     #[test]
     fn count_matches_with_inverted_search_counts_lines() {
         // rg --count-matches -v counts the selected lines: inverted Matches have no submatches.
```

- [ ] **Step 3:** `cargo test --lib -- output::flat output::summary` does not compile (`render`
  takes the Look): RED.

### Task 7.2: the prelude, the separators and the summaries

**Files:** `src/output/flat.rs`, `src/output/summary.rs`, `src/output/mod.rs`, `src/app.rs`.

- [ ] **Step 1:** `FileHead` with the shown path and the path to link, `Head` with the column and
  the byte offset of the row, rg's separators, `-0`'s NUL, the palette and one link over the
  prelude; the group separator; the binary message's path and the Section's head with the Look:

```diff
diff --git a/src/output/flat.rs b/src/output/flat.rs
index 5197e17..cd53ac1 100644
--- a/src/output/flat.rs
+++ b/src/output/flat.rs
@@ -60,7 +60,10 @@ fn fit_trail(trail: &[Segment], room: usize) -> String {
 
 /// The file-name part of a row, computed once per file: present with file names on.
 pub(crate) struct FileHead {
+    /// The path as printed (`shown_path`), with file names on.
     path: Option<Vec<u8>>,
+    /// The path to link to with `--hyperlink-format`.
+    link: std::path::PathBuf,
     /// Width of `path` in characters, plus its separator.
     width: usize,
     /// The last Trail fitted, the room it had and the result: consecutive rows usually share
@@ -70,12 +73,13 @@ pub(crate) struct FileHead {
 
 impl FileHead {
     pub(crate) fn new(path: &std::path::Path, o: &RenderOptions) -> Self {
-        let path = o.with_filename.then(|| super::path_bytes(path));
-        let width = path
+        let shown = o.with_filename.then(|| super::shown_path(path, &o.look));
+        let width = shown
             .as_ref()
             .map_or(0, |p| width(&String::from_utf8_lossy(p)) + 1);
         FileHead {
-            path,
+            path: shown,
+            link: path.to_path_buf(),
             width,
             fitted: Default::default(),
         }
@@ -105,14 +109,20 @@ fn digits(mut n: u64) -> usize {
     d
 }
 
-/// `path:line:` (or `path-line-` for context), each part present when asked for; an object
-/// file's row has `path:0x1f3a:`, its hex offset in place of the line (spec §5.7).
+/// The prelude of a row, as ripgrep's `PreludeWriter` writes it (M5a plan, D4): the path, the
+/// line number, the column (`--column`) and the byte offset (`-b`), each present when asked
+/// for and followed by the field separator, `:` for a Match and `-` for a context line
+/// (`--field-match-separator`, `--field-context-separator`); `-0` puts a NUL after the path in
+/// place of its separator. An object file's row has `path:0x1f3a:`, its hex offset in place of
+/// the line (spec §5.7). With `--hyperlink-format`, the prelude is one link.
 struct Head<'a> {
     file: &'a FileHead,
     line: Option<u64>,
     /// `line` is an object row's offset, printed in hex.
     hex: bool,
-    sep: &'static str,
+    column: Option<u64>,
+    byte_offset: Option<u64>,
+    sep: &'a [u8],
 }
 
 impl<'a> Head<'a> {
@@ -120,16 +130,29 @@ impl<'a> Head<'a> {
         file: &'a FileHead,
         line_number: Option<u64>,
         is_match: bool,
-        o: &RenderOptions,
+        o: &'a RenderOptions,
     ) -> Self {
         Head {
             file,
             line: line_number.filter(|_| o.line_numbers),
             hex: false,
-            sep: if is_match { ":" } else { "-" },
+            column: None,
+            byte_offset: None,
+            sep: if is_match {
+                &o.look.sep_match
+            } else {
+                &o.look.sep_context
+            },
         }
     }
 
+    /// The column and byte offset of a row, where `--column` and `-b` ask for them.
+    fn of_row(mut self, r: &Row, o: &RenderOptions) -> Self {
+        self.column = r.column.filter(|_| o.look.column);
+        self.byte_offset = o.look.byte_offset.then_some(r.byte_offset);
+        self
+    }
+
     fn at_offset(mut self, offset: Option<u64>) -> Self {
         if let (Some(_), Some(off)) = (self.line, offset) {
             self.line = Some(off);
@@ -139,10 +162,14 @@ impl<'a> Head<'a> {
     }
 
     fn is_empty(&self) -> bool {
-        self.file.path.is_none() && self.line.is_none()
+        self.file.path.is_none()
+            && self.line.is_none()
+            && self.column.is_none()
+            && self.byte_offset.is_none()
     }
 
     fn width(&self) -> usize {
+        let sep = width(&String::from_utf8_lossy(self.sep));
         let digits = |l: u64| {
             if self.hex {
                 // `0x` and the hex digits.
@@ -151,31 +178,66 @@ impl<'a> Head<'a> {
                 digits(l)
             }
         };
-        self.file.width + self.line.map_or(0, |l| digits(l) + 1)
-    }
-
-    fn write(&self, w: &mut dyn WriteColor) -> std::io::Result<()> {
-        if let Some(p) = &self.file.path {
-            w.set_color(&style::spec("path"))?;
-            w.write_all(p)?;
-            w.reset()?;
-            w.write_all(self.sep.as_bytes())?;
+        let field = |n: Option<u64>, f: &dyn Fn(u64) -> usize| n.map_or(0, |n| f(n) + sep);
+        let path = if self.file.path.is_some() {
+            self.file.width - 1 + sep
+        } else {
+            0
+        };
+        path + field(self.line, &digits)
+            + field(self.column, &digits)
+            + field(self.byte_offset, &digits)
+    }
+
+    fn write(&self, w: &mut dyn WriteColor, o: &RenderOptions) -> std::io::Result<()> {
+        let p = &o.look.palette;
+        let link = if self.file.path.is_some() {
+            super::link_start(
+                w,
+                &o.look,
+                &self.file.link,
+                self.line.filter(|_| !self.hex),
+                self.column,
+            )?
+        } else {
+            false
+        };
+        if let Some(path) = &self.file.path {
+            style::write_in(w, p, "path", path)?;
+            match o.look.path_terminator {
+                Some(t) => w.write_all(&[t])?,
+                None => w.write_all(self.sep)?,
+            }
         }
         if let Some(l) = self.line {
-            w.set_color(&style::spec("line"))?;
-            if self.hex {
-                write!(w, "0x{l:x}")?;
+            let n = if self.hex {
+                format!("0x{l:x}")
             } else {
-                write!(w, "{l}")?;
-            }
-            w.reset()?;
-            w.write_all(self.sep.as_bytes())?;
+                l.to_string()
+            };
+            style::write_in(w, p, "line", n.as_bytes())?;
+            w.write_all(self.sep)?;
         }
-        Ok(())
+        for n in [self.column, self.byte_offset].into_iter().flatten() {
+            style::write_in(w, p, "column", n.to_string().as_bytes())?;
+            w.write_all(self.sep)?;
+        }
+        super::link_end(w, link)
     }
 }
 
-/// One row: `--` first when it opens a group after a gap, then `path:line: Trail │ text`.
+/// The separator between groups of context (`--context-separator`), none with
+/// `--no-context-separator`.
+fn write_group_separator(w: &mut dyn WriteColor, o: &RenderOptions) -> std::io::Result<()> {
+    if let Some(sep) = &o.look.sep_group {
+        w.write_all(sep)?;
+        w.write_all(b"\n")?;
+    }
+    Ok(())
+}
+
+/// One row: the group separator first when it opens a group after a gap, then
+/// `path:line: Trail │ text`.
 pub(crate) fn write_row(
     w: &mut dyn WriteColor,
     file: &FileHead,
@@ -183,11 +245,13 @@ pub(crate) fn write_row(
     o: &RenderOptions,
 ) -> std::io::Result<()> {
     if r.gap_before {
-        w.write_all(b"--\n")?;
+        write_group_separator(w, o)?;
     }
-    let head = Head::new(file, Some(r.line_number), r.is_match, o).at_offset(r.offset);
+    let head = Head::new(file, Some(r.line_number), r.is_match, o)
+        .at_offset(r.offset)
+        .of_row(r, o);
     if !r.is_match {
-        let prefix = |w: &mut dyn WriteColor| head.write(w);
+        let prefix = |w: &mut dyn WriteColor| head.write(w, o);
         let avail = o.width.saturating_sub(head.width());
         return super::write_line_text(w, &prefix, &r.line, &r.subs, false, avail, o);
     }
@@ -210,14 +274,15 @@ pub(crate) fn write_row(
         Some(file.fit(r.trail, room))
     };
     let used = head.width() + space + trail.as_ref().map_or(0, |t| width(t) + 3);
+    let p = &o.look.palette;
     let prefix = |w: &mut dyn WriteColor| -> std::io::Result<()> {
-        head.write(w)?;
+        head.write(w, o)?;
         if space == 1 {
             w.write_all(b" ")?;
         }
         if let Some(t) = &trail {
-            style::write_styled(w, "trail", t)?;
-            style::write_styled(w, "dim", " │ ")?;
+            style::write_in(w, p, "trail", t.as_bytes())?;
+            style::write_in(w, p, "dim", " │ ".as_bytes())?;
         }
         Ok(())
     };
@@ -233,7 +298,14 @@ pub(crate) fn write_binary_message(
     o: &RenderOptions,
 ) -> std::io::Result<()> {
     if o.with_filename {
-        super::write_path(w, path)?;
+        let link = super::link_start(w, &o.look, path, None, None)?;
+        style::write_in(
+            w,
+            &o.look.palette,
+            "path",
+            &super::shown_path(path, &o.look),
+        )?;
+        super::link_end(w, link)?;
         w.write_all(b": ")?;
     }
     w.write_all(format!("{msg}\n").as_bytes())
@@ -265,18 +337,16 @@ pub fn render(
             Piece::Row(r) => write_row(w, &file, &r, o)?,
             Piece::Section { trail, start, end } => {
                 let head = Head::new(&file, None, true, o);
-                head.write(w)?;
-                let range = format!("{start}-{end}:");
-                style::write_styled(w, "line", &range[..range.len() - 1])?;
-                w.write_all(b":")?;
+                head.write(w, o)?;
+                let range = format!("{start}-{end}");
+                let p = &o.look.palette;
+                style::write_in(w, p, "line", range.as_bytes())?;
+                w.write_all(&o.look.sep_match)?;
                 if !trail.is_empty() {
-                    let fixed = head.width() + range.len() + 1;
+                    let fixed = head.width() + range.len() + o.look.sep_match.len() + 1;
                     w.write_all(b" ")?;
-                    style::write_styled(
-                        w,
-                        "trail",
-                        &fit_trail(trail, o.width.saturating_sub(fixed)),
-                    )?;
+                    let fitted = fit_trail(trail, o.width.saturating_sub(fixed));
+                    style::write_in(w, p, "trail", fitted.as_bytes())?;
                 }
                 w.write_all(b"\n")?;
                 // Each line as any other: whole, as Fragments, or -M's message (spec §6.3).
```

- [ ] **Step 2:** `summary::render` with the Look and `--include-zero`:

```diff
diff --git a/src/output/summary.rs b/src/output/summary.rs
index 0490f2c..c3fc953 100644
--- a/src/output/summary.rs
+++ b/src/output/summary.rs
@@ -1,4 +1,5 @@
 //! -c, --count-matches, -l and --files-without-match computed after filters (design note 9).
+use super::Look;
 use crate::model::Described;
 use std::path::Path;
 use termcolor::WriteColor;
@@ -28,30 +29,41 @@ pub fn count(mode: Mode, d: Option<&Described>) -> u64 {
 
 /// Writes the file's line in ripgrep's format, if the mode prints one for it, and returns
 /// whether it did: `path:N` (`N` alone without file names) for the counts of files with a
-/// kept Match, `path` for -l, and for --files-without-match `path` when nothing was kept.
+/// kept Match (with `--include-zero`, of every file), `path` for -l, and for
+/// --files-without-match `path` when nothing was kept. The path is printed as the Trail
+/// layouts print it (`--path-separator`, `--colors`, `--hyperlink-format`), and `-0` ends it
+/// with a NUL, in place of the `:` of a count or the line break of a path alone, as rg's
+/// summary printer (M5a plan, D4).
 pub fn render(
     mode: Mode,
     path: &Path,
     d: Option<&Described>,
     with_filename: bool,
+    look: &Look,
+    include_zero: bool,
     w: &mut dyn WriteColor,
 ) -> std::io::Result<bool> {
     let n = count(mode, d);
+    let write_path = |w: &mut dyn WriteColor| -> std::io::Result<()> {
+        let link = super::link_start(w, look, path, None, None)?;
+        super::style::write_in(w, &look.palette, "path", &super::shown_path(path, look))?;
+        super::link_end(w, link)
+    };
     match mode {
-        Mode::Count | Mode::CountMatches if n > 0 => {
+        Mode::Count | Mode::CountMatches if n > 0 || include_zero => {
             if with_filename {
-                super::write_path(w, path)?;
-                w.write_all(b":")?;
+                write_path(w)?;
+                w.write_all(&[look.path_terminator.unwrap_or(b':')])?;
             }
             w.write_all(format!("{n}\n").as_bytes())?;
         }
         Mode::FilesWithMatches if n > 0 => {
-            super::write_path(w, path)?;
-            w.write_all(b"\n")?;
+            write_path(w)?;
+            w.write_all(&[look.path_terminator.unwrap_or(b'\n')])?;
         }
         Mode::FilesWithoutMatch if n == 0 => {
-            super::write_path(w, path)?;
-            w.write_all(b"\n")?;
+            write_path(w)?;
+            w.write_all(&[look.path_terminator.unwrap_or(b'\n')])?;
         }
         _ => return Ok(false),
     }
```

- [ ] **Step 3:** the rows' fields are read now:

```diff
diff --git a/src/output/mod.rs b/src/output/mod.rs
index e266fcf..f5402d8 100644
--- a/src/output/mod.rs
+++ b/src/output/mod.rs
@@ -430,11 +430,9 @@ pub(crate) struct Row<'a> {
     /// Context is on and this line does not follow the previous one: print `--` first.
     pub gap_before: bool,
     /// `--column`: the 1-based column of the row's first Match, as rg counts it; None on a
-    /// context row. (Printed by flat and tree from chunks 7 and 8 on.)
-    #[allow(dead_code)]
+    /// context row.
     pub column: Option<u64>,
     /// `-b`: the byte offset of the row in the file, as rg gives it.
-    #[allow(dead_code)]
     pub byte_offset: u64,
 }
```

- [ ] **Step 4:** the caller (`trail_file_printed`):

```diff
diff --git a/src/app.rs b/src/app.rs
index 7bdff4f..57cf61b 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -1377,7 +1377,8 @@ impl<'a> Ctx<'a> {
             if failed == Some(true) {
                 return;
             }
-            match summary::render(mode, &t.path, d, self.with_filename, buf) {
+            let (look, zero) = (&ro.look, self.args.include_zero);
+            match summary::render(mode, &t.path, d, self.with_filename, look, zero, buf) {
                 Ok(true) => matched(),
                 Ok(false) => {}
                 Err(e) => self.msgs.error(e),
```

- [ ] **Step 5:** `cargo test --lib output::`: PASS; clippy: no finding.
- [ ] **Step 6:** commit: `Print rg's output flags in the one-line layout and the summaries`.

---
## Chunk 8: the tree

### Task 8.1: the tests

**Files:** `src/output/tree.rs` (tests only).

- [ ] **Step 1:** the tree with rg's prelude flags, `-0` after the header's path, the separators
  and the links (D4, D12; rg's heading prints `path\0` in place of `path\n` with `-0`, and links
  the heading's path at line 1):

```diff
diff --git a/src/output/tree.rs b/src/output/tree.rs
index 52e0f20..f5fa823 100644
--- a/src/output/tree.rs
+++ b/src/output/tree.rs
@@ -299,6 +379,76 @@ mod tests {
         String::from_utf8(w.into_inner()).unwrap()
     }
 
+    /// M5a plan, D4: the tree's rows with `--column`, `-b`, rg's separators, `-0` after the
+    /// header's path, `--path-separator` and `--context-separator`.
+    #[test]
+    fn rows_with_ripgreps_prelude_flags() {
+        use crate::output::Look;
+        let s = src(&["# Install", "  run needle", "after", "x", "y", "z needle"]);
+        let d = described(
+            &s,
+            vec![mat(&s, 2), ctx(&s, 3), mat(&s, 6)],
+            vec![
+                vec![vec![h(1, "Install", 1, 6)]],
+                vec![vec![h(1, "Install", 1, 6)]],
+            ],
+        );
+        let o = |look| RenderOptions {
+            context: true,
+            look,
+            ..opts()
+        };
+        assert_eq!(
+            plain(&d, &s, &o(Look::default())),
+            "docs/a.md\n  # Install\n    2:  run needle\n    3-after\n    --\n    6:z needle\n"
+        );
+        let look = Look {
+            column: true,
+            byte_offset: true,
+            sep_match: b"=".to_vec(),
+            sep_group: Some(b"##".to_vec()),
+            path_terminator: Some(0),
+            path_separator: Some(b'\\'),
+            ..Look::default()
+        };
+        assert_eq!(
+            plain(&d, &s, &o(look)),
+            "docs\\a.md\x00  # Install\n    2=7=10=  run needle\n    3-23-after\n    ##\n    6=3=33=z needle\n"
+        );
+    }
+
+    /// The header's path and, with a format that names `{line}`, each row's numbers are links.
+    #[test]
+    fn links_in_the_tree() {
+        use crate::output::{Look, hyperlink::Hyperlinks};
+        let dir = tempfile::tempdir().unwrap();
+        let path = dir.path().join("a.md");
+        std::fs::write(&path, "x").unwrap();
+        let s = src(&["# Install", "needle"]);
+        let mut d = described(
+            &s,
+            vec![mat(&s, 2)],
+            vec![vec![vec![h(1, "Install", 1, 2)]]],
+        );
+        d.file.path = path.clone();
+        let o = RenderOptions {
+            look: Look {
+                hyperlinks: Hyperlinks::new("file://{path}:{line}", None).map(std::sync::Arc::new),
+                ..Look::default()
+            },
+            ..opts()
+        };
+        let mut w = Ansi::new(Vec::new());
+        render(&d, &s, &o, &mut w).unwrap();
+        let out = String::from_utf8(w.into_inner()).unwrap();
+        let url = format!("file://{}", path.canonicalize().unwrap().display());
+        assert!(
+            out.starts_with(&format!("\x1b]8;;{url}:1\x1b\\")),
+            "{out:?}"
+        );
+        assert!(out.contains(&format!("\x1b]8;;{url}:2\x1b\\")), "{out:?}");
+    }
+
     fn ansi(d: &Described, src: &[u8], o: &RenderOptions) -> String {
         let mut w = Ansi::new(Vec::new());
         render(d, src, o, &mut w).unwrap();
```

- [ ] **Step 2:** `cargo test --lib output::tree`: both FAIL (no column, `--` hard-coded, the
  header's line break): RED.

### Task 8.2: the tree's numbers, header and separators

**Files:** `src/output/tree.rs`, `src/output/stream.rs`.

- [ ] **Step 1:** `Numbers` (line or offset, column, byte offset, the path to link), the header
  with the Look (`TreeState::header` keeps the path for the rows' links), the palette in the
  Trail's labels, and the group separator:

```diff
diff --git a/src/output/tree.rs b/src/output/tree.rs
index 52e0f20..f5fa823 100644
--- a/src/output/tree.rs
+++ b/src/output/tree.rs
@@ -62,62 +62,109 @@ fn write_trail(
     printed: &[Segment],
     trail: &[Segment],
     seen: &Seen,
+    o: &RenderOptions,
 ) -> std::io::Result<()> {
     for (depth, s) in trail.iter().enumerate().skip(shared(printed, trail)) {
         if seen.iter().any(|&(_, line)| line == s.line) {
             continue;
         }
         w.write_all(indent(depth).as_bytes())?;
-        style::write_styled(w, "trail", &label(s))?;
+        style::write_in(w, &o.look.palette, "trail", label(s).as_bytes())?;
         w.write_all(b"\n")?;
     }
     Ok(())
 }
 
-/// One line at `depth`: indentation, line number (an object file's row: its hex offset, spec
-/// §5.7) with `:` or `-`, text.
-#[allow(clippy::too_many_arguments)]
+/// The numbers before a row's text: the line number (an object file's row: its hex offset,
+/// spec §5.7), the column (`--column`) and the byte offset (`-b`), each followed by the field
+/// separator, `:` for a Match and `-` for a context line (M5a plan, D4).
+#[derive(Default)]
+pub(crate) struct Numbers<'a> {
+    pub line: u64,
+    pub offset: Option<u64>,
+    pub column: Option<u64>,
+    pub byte_offset: Option<u64>,
+    /// The file, for a link over the numbers with a `--hyperlink-format` that names `{line}`.
+    pub path: Option<&'a std::path::Path>,
+}
+
+/// One line at `depth`: indentation, numbers, text.
 pub(crate) fn write_row(
     w: &mut dyn WriteColor,
     depth: usize,
-    line_number: u64,
-    offset: Option<u64>,
+    numbers: &Numbers,
     bytes: &[u8],
     subs: &[(usize, usize)],
     is_match: bool,
     o: &RenderOptions,
 ) -> std::io::Result<()> {
     let line = super::LineText::new(bytes);
-    write_row_text(w, depth, line_number, offset, &line, subs, is_match, o)
+    write_row_text(w, depth, numbers, &line, subs, is_match, o)
 }
 
 /// `write_row` for a line whose text the rows before may have worked out already.
-#[allow(clippy::too_many_arguments)]
 fn write_row_text(
     w: &mut dyn WriteColor,
     depth: usize,
-    line_number: u64,
-    offset: Option<u64>,
+    n: &Numbers,
     line: &super::LineText,
     subs: &[(usize, usize)],
     is_match: bool,
     o: &RenderOptions,
 ) -> std::io::Result<()> {
-    let at = match offset {
+    let at = match n.offset {
         Some(off) => format!("0x{off:x}"),
-        None => line_number.to_string(),
+        None => n.line.to_string(),
     };
-    let sep = if is_match { ":" } else { "-" };
-    let used = 2 * (depth + 1) + if o.line_numbers { at.len() + 1 } else { 0 };
+    let sep: &[u8] = if is_match {
+        &o.look.sep_match
+    } else {
+        &o.look.sep_context
+    };
+    let mut fields: Vec<(&str, String)> = Vec::new();
+    if o.line_numbers {
+        fields.push(("line", at));
+    }
+    if o.look.column
+        && let Some(c) = n.column
+    {
+        fields.push(("column", c.to_string()));
+    }
+    if o.look.byte_offset
+        && let Some(b) = n.byte_offset
+    {
+        fields.push(("column", b.to_string()));
+    }
+    let sep_width = String::from_utf8_lossy(sep).chars().count();
+    let used = 2 * (depth + 1)
+        + fields
+            .iter()
+            .map(|(_, f)| f.len() + sep_width)
+            .sum::<usize>();
+    // rg links a heading's row numbers when the format names `{line}` or `{column}`.
+    let link_path = n.path.filter(|_| {
+        o.look
+            .hyperlinks
+            .as_ref()
+            .is_some_and(|h| h.line_dependent())
+    });
     let prefix = |w: &mut dyn WriteColor| -> std::io::Result<()> {
         write_indent(w, depth)?;
-        if o.line_numbers {
-            w.set_color(&style::spec("line"))?;
-            w.write_all(at.as_bytes())?;
-            w.reset()?;
-            w.write_all(sep.as_bytes())?;
+        let link = match (link_path, fields.is_empty()) {
+            (Some(p), false) => super::link_start(
+                w,
+                &o.look,
+                p,
+                Some(n.line).filter(|_| n.offset.is_none()),
+                n.column,
+            )?,
+            _ => false,
+        };
+        for (kind, text) in &fields {
+            style::write_in(w, &o.look.palette, kind, text.as_bytes())?;
+            w.write_all(sep)?;
         }
-        Ok(())
+        super::link_end(w, link)
     };
     super::write_line_text(
         w,
@@ -130,10 +177,22 @@ fn write_row_text(
     )
 }
 
-/// The file's first line: its path.
-pub(crate) fn write_header(w: &mut dyn WriteColor, path: &std::path::Path) -> std::io::Result<()> {
-    super::write_path(w, path)?;
-    w.write_all(b"\n")
+/// The file's first line: its path (`--path-separator`, `--colors`, `--hyperlink-format`),
+/// ended by a line break or, with `-0`, by a NUL (grep-printer's `write_path_line`).
+pub(crate) fn write_header(
+    w: &mut dyn WriteColor,
+    path: &std::path::Path,
+    o: &RenderOptions,
+) -> std::io::Result<()> {
+    let link = super::link_start(w, &o.look, path, None, None)?;
+    style::write_in(
+        w,
+        &o.look.palette,
+        "path",
+        &super::shown_path(path, &o.look),
+    )?;
+    super::link_end(w, link)?;
+    w.write_all(&[o.look.path_terminator.unwrap_or(b'\n')])
 }
 
 /// ripgrep's binary message, after a file's rows.
@@ -149,10 +208,24 @@ pub(crate) struct TreeState {
     /// Rows printed under the Trail on screen: a row may be the label of a Segment printed
     /// later. Rows deeper than the part of the Trail that stays are out of view.
     seen: Seen,
+    /// The file, for the links of its rows (`header`).
+    path: Option<std::path::PathBuf>,
 }
 
 impl TreeState {
-    /// One row under its Trail: `--` first after a gap, the Segments not on screen yet, the line.
+    /// The file's header (`write_header`), and its path kept for the links of its rows.
+    pub(crate) fn header(
+        &mut self,
+        w: &mut dyn WriteColor,
+        path: &std::path::Path,
+        o: &RenderOptions,
+    ) -> std::io::Result<()> {
+        self.path = Some(path.to_path_buf());
+        write_header(w, path, o)
+    }
+
+    /// One row under its Trail: the group separator first after a gap, the Segments not on
+    /// screen yet, the line.
     pub(crate) fn row(
         &mut self,
         w: &mut dyn WriteColor,
@@ -161,10 +234,14 @@ impl TreeState {
     ) -> std::io::Result<()> {
         let depth = r.trail.len();
         let shown = shared(&self.printed, r.trail);
-        if r.gap_before {
+        if r.gap_before
+            && let Some(sep) = &o.look.sep_group
+        {
             // At the depth of what is printed next: the first new Segment, or the line.
             let at = depth.min(shown);
-            w.write_all(format!("{}--\n", indent(at)).as_bytes())?;
+            w.write_all(indent(at).as_bytes())?;
+            w.write_all(sep)?;
+            w.write_all(b"\n")?;
         }
         // Every row on screen is at most as deep as the Trail on screen: nothing to drop while
         // that Trail stays whole.
@@ -172,19 +249,17 @@ impl TreeState {
             self.seen.retain(|&(d, _)| d <= shown);
         }
         if shown != depth || self.printed.len() != depth {
-            write_trail(w, &self.printed, r.trail, &self.seen)?;
+            write_trail(w, &self.printed, r.trail, &self.seen, o)?;
             self.printed = r.trail.to_vec();
         }
-        write_row_text(
-            w,
-            depth,
-            r.line_number,
-            r.offset,
-            &r.line,
-            &r.subs,
-            r.is_match,
-            o,
-        )?;
+        let numbers = Numbers {
+            line: r.line_number,
+            offset: r.offset,
+            column: r.column,
+            byte_offset: Some(r.byte_offset),
+            path: self.path.as_deref(),
+        };
+        write_row_text(w, depth, &numbers, &r.line, &r.subs, r.is_match, o)?;
         if self.seen.len() == SEEN {
             self.seen.pop_front();
         }
@@ -202,13 +277,18 @@ impl TreeState {
         kept: &std::collections::HashMap<u64, Vec<(usize, usize)>>,
         o: &RenderOptions,
     ) -> std::io::Result<()> {
-        write_trail(w, &self.printed, trail, &Seen::new())?;
+        write_trail(w, &self.printed, trail, &Seen::new(), o)?;
         self.printed = trail.to_vec();
         self.seen.clear();
         for (n, line) in super::section_lines(src, start, end) {
             let subs = kept.get(&n);
             let s = subs.map_or(&[][..], |v| v.as_slice());
-            write_row(w, trail.len(), n, None, line, s, subs.is_some(), o)?;
+            let numbers = Numbers {
+                line: n,
+                path: self.path.as_deref(),
+                ..Numbers::default()
+            };
+            write_row(w, trail.len(), &numbers, line, s, subs.is_some(), o)?;
         }
         Ok(())
     }
@@ -220,13 +300,13 @@ pub fn render(
     o: &RenderOptions,
     w: &mut dyn WriteColor,
 ) -> std::io::Result<()> {
-    write_header(w, &d.file.path)?;
+    let mut state = TreeState::default();
+    state.header(w, &d.file.path, o)?;
     let kept = if o.expand {
         super::match_lines(d)
     } else {
         Default::default()
     };
-    let mut state = TreeState::default();
     for piece in super::pieces(d, src, o) {
         match piece {
             Piece::Row(r) => state.row(w, &r, o)?,
```

- [ ] **Step 2:** the streaming printer writes the header through the state:

```diff
diff --git a/src/output/stream.rs b/src/output/stream.rs
index 415c070..5b03f81 100644
--- a/src/output/stream.rs
+++ b/src/output/stream.rs
@@ -187,7 +187,7 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
         }
         if !self.header {
             self.header = true;
-            tree::write_header(self.w, self.file.path)?;
+            self.tree.header(self.w, self.file.path, self.o)?;
         }
         self.tree.row(self.w, r, self.o)
     }
@@ -369,7 +369,7 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
         }
         if !self.header && tree {
             self.header = true;
-            tree::write_header(self.w, self.file.path)?;
+            self.tree.header(self.w, self.file.path, self.o)?;
         }
         self.lead()?;
         if gap && self.o.context {
@@ -393,7 +393,7 @@ impl<'a, W: WriteColor> StreamSink<'a, W> {
         }
         self.flush(None)?;
         if self.o.layout == Layout::Tree && !self.header {
-            tree::write_header(self.w, self.file.path)?;
+            self.tree.header(self.w, self.file.path, self.o)?;
         }
         let msg = super::binary_message(self.stats.binary_offset, true, self.file.explicit, self.o);
         if let Some(msg) = msg {
```

- [ ] **Step 3:** `cargo test --lib output::`: PASS; clippy: no finding.
- [ ] **Step 4:** commit: `Print rg's output flags in the tree`.

---
## Chunk 9: the pipeline

### Task 9.1: the end-to-end tests

**Files:** `tests/rg_compat_m5.rs` (new), `tests/cli_m5.rs` (new).

- [ ] **Step 1:** `tests/rg_compat_m5.rs`: every M5a flag with `--no-trail` against rg 14.1.1,
  byte for byte, over the fixture copy and the files `write_m5_files` adds (spec §11 #29), each
  negation after its flag too (D2: `--no-ignore --ignore-dot`, `-b --no-byte-offset`…), and
  `--type-list` after `--type-add` and `--type-clear` (review r2, Codex #3). Each
  case runs with `--sort path` first; `--stats` and `--json` with their times masked:

```rust
//! M5a (M5a plan, Chunk 9): every flag of ripgrep 14.1.1 that M5a implements, `s4
//! --no-trail` against rg 14.1.1 byte for byte (stdout and exit code), over the fixture copy
//! and the files each case writes into it. Skipped with a note when rg 14.1.1 is not
//! installed; `SEARCHFOR_REQUIRE_REFERENCE=1` makes that a failure.
mod common;

use common::{Fixture, code, reference_rg, run, stdout};
use std::process::Output;

/// Bytes the cases search besides the fixtures: CRLF lines, NUL-separated records, Latin-1,
/// a UTF-16LE file without a BOM, a file with a UTF-8 BOM, runs of lines for
/// `--stop-on-nonmatch`, indented lines for `--trim`, and a long line for `-M`.
fn write_m5_files(fx: &Fixture) {
    let dir = fx.file("m5");
    std::fs::create_dir_all(&dir).unwrap();
    let files: &[(&str, Vec<u8>)] = &[
        ("crlf.txt", b"one needle\r\ntwo\r\nneedle$\r\n".to_vec()),
        (
            "records.dat",
            b"one needle\ntwo\0three\0needle four\n".to_vec(),
        ),
        ("latin1.txt", b"caf\xe9 needle\n".to_vec()),
        (
            "utf16.txt",
            "a needle\nb\n"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ),
        ("bom.txt", b"\xef\xbb\xbfneedle bom\n".to_vec()),
        ("stop.txt", b"a needle\nb needle\nc\nd needle\n".to_vec()),
        ("trim.txt", b"   needle  x\n\tneedle\n".to_vec()),
        (
            "long.txt",
            format!(
                "{} needle {}\nshort needle\n",
                "x".repeat(60),
                "y".repeat(60)
            )
            .into_bytes(),
        ),
        (
            "block.md",
            b"# Install\n\nSome intro needle here.\n\n## Linux\n\nRun the needle command:\n"
                .to_vec(),
        ),
    ];
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}

fn rg(fx: &Fixture, args: &[&str]) -> Output {
    run(&mut fx.rg(args))
}

fn s4(fx: &Fixture, args: &[&str]) -> Output {
    let mut a = vec!["--no-docs", "--no-trail"];
    a.extend_from_slice(args);
    run(&mut fx.s4(&a))
}

/// Every case, `--sort path` first so both walk in one order; all differences reported at once.
fn compare(fx: &Fixture, cases: &[Vec<&str>]) {
    let mut diffs = Vec::new();
    for case in cases {
        let mut a = vec!["--sort", "path"];
        a.extend_from_slice(case);
        let (want, got) = (rg(fx, &a), s4(fx, &a));
        if (code(&want), stdout(&want)) != (code(&got), stdout(&got)) {
            diffs.push(format!(
                "== {case:?}\n-- rg (exit {}):\n{}\n-- s4 (exit {}):\n{}",
                code(&want),
                stdout(&want),
                code(&got),
                stdout(&got)
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "{} case(s) differ:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

#[test]
fn m5_output_flags_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    let cases: Vec<Vec<&str>> = vec![
        vec!["--column", "needle"],
        vec!["--column", "-n", "needle", "md"],
        vec!["--no-column", "--vimgrep", "needle", "md"],
        vec!["-b", "needle", "md"],
        vec!["-b", "-o", "needle", "md"],
        // Each negation after its flag (review r1 of the plan, Fable #3; D2).
        vec!["-b", "--no-byte-offset", "needle", "md"],
        vec!["--heading", "needle", "md"],
        vec!["--no-heading", "-n", "needle", "md"],
        vec!["--pretty", "needle", "md"],
        vec!["-p", "--no-heading", "needle", "md"],
        vec!["--vimgrep", "needle"],
        vec!["-o", "needle-\\w+", "md"],
        vec!["-o", "-C1", "needle", "md/guide.md"],
        vec!["-r", "X", "needle", "md"],
        vec!["-r", "<$0>", "-o", "needle", "md"],
        vec!["--replace", "${1}!", "(needle)-(\\w+)", "md"],
        vec!["-0", "needle", "md"],
        vec!["-0", "--heading", "needle", "md"],
        vec!["-0", "-c", "needle", "md"],
        vec!["-0", "-l", "needle"],
        vec!["--path-separator", "\\", "needle", "md"],
        vec!["--path-separator", "", "-l", "needle", "md"],
        vec!["--trim", "needle", "m5/trim.txt"],
        vec!["--trim", "--no-trim", "needle", "m5/trim.txt"],
        vec!["--passthru", "needle", "md/guide.md"],
        vec!["--passthrough", "-n", "needle", "md/twice.md"],
        vec!["--passthru", "-A1", "needle", "md/guide.md"],
        vec!["-C1", "--context-separator", "##", "needle", "md"],
        vec!["-C1", "--no-context-separator", "needle", "md"],
        vec![
            "-C1",
            "--field-context-separator",
            "~",
            "--field-match-separator",
            "=",
            "-n",
            "needle",
            "md",
        ],
        vec!["-M", "20", "--max-columns-preview", "needle", "m5/long.txt"],
        vec![
            "-M",
            "20",
            "--max-columns-preview",
            "--no-max-columns-preview",
            "needle",
            "m5/long.txt",
        ],
        vec![
            "--color",
            "always",
            "-M",
            "20",
            "--max-columns-preview",
            "needle",
            "m5/long.txt",
        ],
        vec![
            "--color",
            "always",
            "--colors",
            "match:fg:blue",
            "--colors",
            "path:none",
            "needle",
            "md",
        ],
        vec![
            "--color",
            "always",
            "--colors",
            "line:style:bold",
            "--colors",
            "column:fg:green",
            "--column",
            "-n",
            "needle",
            "md",
        ],
        vec![
            "--color",
            "always",
            "--hyperlink-format",
            "default",
            "needle",
            "md",
        ],
        vec![
            "--color",
            "always",
            "--heading",
            "--hyperlink-format",
            "file://{path}:{line}:{column}",
            "--column",
            "needle",
            "md",
        ],
        vec!["-c", "--include-zero", "needle", "md", "misc"],
        vec![
            "-c",
            "--include-zero",
            "--no-include-zero",
            "needle",
            "md",
            "misc",
        ],
        vec!["-o", "-c", "needle", "md"],
        vec!["--line-buffered", "needle", "md"],
        vec!["--block-buffered", "needle", "md"],
        vec!["--no-line-buffered", "--no-block-buffered", "needle", "md"],
    ];
    compare(&fx, &cases);
}

#[test]
fn m5_search_flags_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    let cases: Vec<Vec<&str>> = vec![
        vec!["-P", "needle(?=-)", "md"],
        vec!["--pcre2", "(\\w)\\1", "md"],
        vec!["-P", "--no-pcre2", "needle", "md"],
        vec!["--engine", "pcre2", "-i", "-w", "NEEDLE", "md"],
        vec!["--engine", "auto", "needle-(?=pkg)", "md"],
        vec!["--auto-hybrid-regex", "needle-(?=pkg)", "md"],
        vec!["--engine", "default", "needle", "md"],
        vec!["(?=x)", "md"],
        vec!["-P", "("],
        vec!["--engine", "auto", "(?<=a)("],
        vec!["a\\x00b", "md"],
        vec!["-a", "a\\x00b", "md"],
        vec!["--no-unicode", "-o", "\\w+", "m5/latin1.txt"],
        vec!["--no-unicode", "--unicode", "-o", "\\w+", "m5/latin1.txt"],
        vec!["--no-pcre2-unicode", "-P", "-o", "\\w+", "m5/latin1.txt"],
        vec![
            "--no-pcre2-unicode",
            "--pcre2-unicode",
            "-P",
            "-o",
            "\\w+",
            "m5/latin1.txt",
        ],
        vec![
            "--auto-hybrid-regex",
            "--no-auto-hybrid-regex",
            "needle-(?=pkg)",
            "md",
        ],
        vec![
            "-U",
            "--multiline-dotall",
            "--no-multiline-dotall",
            "-n",
            "Linux.*Run",
            "m5/block.md",
        ],
        vec!["--crlf", "needle$", "m5/crlf.txt"],
        vec!["needle$", "m5/crlf.txt"],
        vec!["--crlf", "--no-crlf", "needle$", "m5/crlf.txt"],
        vec!["--null-data", "-n", "needle", "m5/records.dat"],
        vec!["--null-data", "--crlf", "needle$", "m5/crlf.txt"],
        vec!["-U", "-n", "intro needle here\\.\\n\\n##", "m5/block.md"],
        vec![
            "-U",
            "--multiline-dotall",
            "-n",
            "Linux.*Run",
            "m5/block.md",
        ],
        vec!["-U", "--no-multiline", "Linux\\n", "m5/block.md"],
        vec!["-U", "-o", "-n", "Linux\\n\\nRun", "m5/block.md"],
        vec!["-U", "--vimgrep", "Linux\\n\\nRun", "m5/block.md"],
        vec!["-U", "-c", "Linux\\n\\nRun", "m5/block.md"],
        vec!["--stop-on-nonmatch", "-n", "needle", "m5/stop.txt"],
        vec!["--dfa-size-limit", "1K", "needle", "md"],
        vec!["--regex-size-limit", "10", "a{1000}", "md"],
        vec!["-E", "latin1", "café", "m5/latin1.txt"],
        vec!["-E", "utf-16le", "needle", "m5/utf16.txt"],
        vec!["-E", "none", "needle", "m5/bom.txt"],
        vec!["-E", "utf-8", "needle", "m5/bom.txt"],
        vec!["-E", "latin1", "--no-encoding", "needle", "m5/latin1.txt"],
        vec!["--mmap", "-n", "needle", "md"],
        vec!["--no-mmap", "-n", "needle", "md/guide.md"],
    ];
    compare(&fx, &cases);
}

#[test]
fn m5_walk_flags_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    std::fs::write(fx.file("extra.ignore"), "md/twice.md\n").unwrap();
    let cases: Vec<Vec<&str>> = vec![
        vec!["-l", "needle"],
        vec!["--no-ignore-vcs", "-l", "needle"],
        vec!["--no-ignore", "--ignore-vcs", "-l", "needle"],
        // Each negation after its flag (review r1 of the plan, Fable #3; D2).
        vec!["--no-ignore", "--ignore-dot", "-l", "needle"],
        vec!["--no-ignore", "--ignore-exclude", "-l", "needle"],
        vec!["--no-ignore-global", "--ignore-global", "-l", "needle"],
        vec![
            "--ignore-file",
            "extra.ignore",
            "--no-ignore-files",
            "--ignore-files",
            "-l",
            "needle",
        ],
        vec![
            "--ignore-file",
            "missing.ignore",
            "--no-ignore-messages",
            "--ignore-messages",
            "-l",
            "needle",
        ],
        vec![
            "--ignore-file-case-insensitive",
            "--no-ignore-file-case-insensitive",
            "-l",
            "needle",
        ],
        vec!["--one-file-system", "--no-one-file-system", "-l", "needle"],
        vec!["--no-ignore-dot", "-l", "needle"],
        vec!["--no-ignore-exclude", "-l", "needle"],
        vec!["--no-ignore-global", "-l", "needle"],
        vec!["--no-ignore-parent", "-l", "needle"],
        vec!["--ignore-file", "extra.ignore", "-l", "needle"],
        vec![
            "--ignore-file",
            "extra.ignore",
            "--no-ignore-files",
            "-l",
            "needle",
        ],
        vec!["--ignore-file", "missing.ignore", "-l", "needle"],
        vec!["--ignore-file-case-insensitive", "-l", "needle"],
        vec!["--no-require-git", "-l", "needle"],
        vec!["--require-git", "-l", "needle"],
        vec!["--one-file-system", "-l", "needle"],
        vec!["--max-filesize", "100", "-l", "needle"],
        vec!["--max-filesize", "1K", "-l", "needle", "md/guide.md"],
        vec!["--glob-case-insensitive", "-g", "*.MD", "-l", "needle"],
        vec![
            "--glob-case-insensitive",
            "--no-glob-case-insensitive",
            "-g",
            "*.MD",
            "-l",
            "needle",
        ],
        vec!["--type-add", "notes:*.md", "-t", "notes", "-l", "needle"],
        vec![
            "--type-add",
            "notes:*.md",
            "--type-clear",
            "notes",
            "-t",
            "notes",
            "-l",
            "needle",
        ],
        vec!["--type-clear", "markdown", "-T", "markdown", "-l", "needle"],
        vec!["--type-add", "bad", "-l", "needle"],
        vec!["--sortr", "path", "-l", "needle"],
        vec!["--sort-files", "-l", "needle"],
        vec![
            "--sort-files",
            "--no-sort-files",
            "--sort",
            "path",
            "-l",
            "needle",
        ],
        vec!["--files"],
        vec!["--files", "md"],
        vec!["--files", "-0", "md"],
        vec!["--files", "--path-separator", "\\", "-g", "*.md"],
        vec!["--files", "-q"],
        vec!["--files", "-g", "*.zzz"],
        vec!["--files", "--type-list"],
        vec!["--type-list", "--files", "md"],
        // The types as the command line changes them (review r2 of the plan, Codex #3).
        vec!["--type-add", "review:*.review", "--type-list"],
        vec!["--type-clear", "rust", "--type-list"],
        vec![
            "--type-add",
            "rust:*.aa",
            "--type-add",
            "web:include:html,css",
            "--type-list",
        ],
        vec!["--maxdepth", "1", "-l", "needle"],
        vec!["-g", "*.zzz", "needle"],
    ];
    compare(&fx, &cases);
}

/// `--stats`: rg's block, with its times masked.
#[test]
fn m5_stats_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let mask = |o: &Output| {
        let re = regex_lite::Regex::new(r"[0-9]+\.[0-9]{6} seconds").unwrap();
        re.replace_all(&stdout(o), "T seconds").into_owned()
    };
    for case in [
        &["--stats", "needle", "md"][..],
        &["--stats", "-c", "needle", "md", "misc"],
        &["--stats", "-l", "needle", "md", "misc"],
        &["--stats", "-q", "needle", "md", "misc"],
        &["--stats", "--no-stats", "needle", "md"],
    ] {
        let mut a = vec!["--sort", "path"];
        a.extend_from_slice(case);
        let (want, got) = (rg(&fx, &a), s4(&fx, &a));
        assert_eq!(
            (code(&want), mask(&want)),
            (code(&got), mask(&got)),
            "{case:?}"
        );
    }
}

/// `--json` with the search flags of M5a: rg's messages, times masked.
#[test]
fn m5_json_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    for case in [
        &["-U", "Linux\\n\\nRun", "m5/block.md"][..],
        &["--null-data", "needle", "m5/records.dat"],
        &["--crlf", "needle$", "m5/crlf.txt"],
        &["-P", "needle(?=-)", "md"],
        &["-E", "latin1", "café", "m5/latin1.txt"],
        &["--passthru", "needle", "md/twice.md"],
        &[
            "-o",
            "-r",
            "X",
            "--vimgrep",
            "--column",
            "-b",
            "needle",
            "md",
        ],
    ] {
        let mut a = vec!["--sort", "path", "--json"];
        a.extend_from_slice(case);
        let (want, got) = (rg(&fx, &a), s4(&fx, &a));
        assert_eq!(
            (code(&want), common::mask_json_times(&stdout(&want))),
            (code(&got), common::mask_json_times(&stdout(&got))),
            "{case:?}"
        );
    }
}
```

- [ ] **Step 2:** `tests/cli_m5.rs`: the Trail layouts with rg's flags, the actions, the
  configuration file, `--debug`, the MCP server over the real binary (with a configuration file
  whose `--pre` and `--generate` it must not take, a value that starts with `-`, a missing path
  beside a Match, a long line, a line that is not UTF-8, a repeated Section, a line past 4 MiB
  and a line with Matches under two cells: D17), rg's message when nothing was searched,
  `--stats`, colours and links, the summaries, `--line-buffered`, the rows of an ELF, a PE and a
  Mach-O file with `-o`, `--vimgrep` and `-r` (`-r` with `^`, with a control byte in the
  Match, and with `-P`'s `\K` and a group before it, and before `\K` in one alternative of two
  against `rg -a`), `--trim` before `\r` and `\x0B`, `--no-mmap` seen in `/proc/PID/maps`
  (`m5_19`), `-o`'s rows, `-U --vimgrep`'s, and the rows of `--crlf` and `--null-data` against
  rg's, its terminator left out (`m5_20`), the MCP server's peak memory (`VmHWM`) on a line of
  240 Trails (`m5_21`), and an object file's `-r` over one line of 1 000 and of 4 000 strings in
  time that grows with them, not with their square (`m5_22`) (spec §11 #14, #30, #31). Every expected output is whole and was read row by row against the
  fixtures:

````rust
//! End-to-end tests of M5a (M5a plan, Chunk 9): ripgrep's flags in the Trail layouts, the
//! actions (`--files`, `--generate`, `--pcre2-version`, `--mcp`), the configuration file,
//! `--debug`, `--stats` and rg's message when nothing was searched.
mod common;

use common::{Fixture, code, run, stderr, stdout};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

fn ok(fx: &Fixture, args: &[&str]) -> String {
    let o = run(&mut fx.s4(args));
    assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
    assert_eq!(stderr(&o), "", "{args:?}");
    stdout(&o)
}

const PKG: &str = "Deploy guide › Install › Linux › bash";

/// M5a plan, D4: rg's output flags in the one-line layout.
#[test]
fn m5_01_output_flags_in_the_one_line_layout() {
    let fx = Fixture::new();
    let g = "md/guide.md";
    assert_eq!(
        ok(&fx, &["--flat", "--column", "-b", "needle-pkg", g]),
        format!("12:13:96: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(&fx, &["--flat", "-o", "needle-[0-9a-b]+", g]),
        "23: Deploy guide › Install › Plans › Price │ needle-10\n\
         24: Deploy guide › Install › Plans › Plan │ needle-a\n\
         24: Deploy guide › Install › Plans › Price │ needle-b\n\
         33: Deploy guide › Install › Plans │ needle-a\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "--vimgrep", "needle-1", g]),
        "md/guide.md:23:10: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "-r", "X", "needle-pkg", g]),
        format!("12: {PKG} │ apt install X\n")
    );
    assert_eq!(
        ok(&fx, &["--flat", "-0", "-H", "needle-pkg", g]),
        format!("md/guide.md\x0012: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &["--flat", "--path-separator", "\\", "-H", "needle-pkg", g]
        ),
        format!("md\\guide.md:12: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &["--flat", "--field-match-separator", "=", "needle-pkg", g]
        ),
        format!("12= {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "-C1",
                "--context-separator",
                "##",
                "needle-(pkg|10)",
                g
            ]
        ),
        format!(
            "11-```bash\n12: {PKG} │ apt install needle-pkg\n13-```\n##\n\
             22-|------|-------|\n23: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |\n\
             24-| needle-a | needle-b |\n"
        )
    );
}

/// The tree with rg's prelude flags, `-0` after the header's path, and the layout flags of rg:
/// `--heading` and `--pretty` are the tree, `--no-heading` and `--vimgrep` the one-line layout.
#[test]
fn m5_02_the_tree_and_the_layout_flags() {
    let fx = Fixture::new();
    let g = "md/guide.md";
    let tree = "md/guide.md\n  Deploy guide\n    # Install\n      ## Linux\n        ```bash\n";
    assert_eq!(
        ok(&fx, &["--tree", "--column", "-b", "needle-pkg", g]),
        format!("{tree}          12:13:96:apt install needle-pkg\n")
    );
    assert_eq!(
        ok(&fx, &["--tree", "-0", "needle-pkg", g]),
        format!(
            "{}\x00{}          12:apt install needle-pkg\n",
            "md/guide.md",
            &tree["md/guide.md\n".len()..]
        )
    );
    for args in [
        &["--heading"][..],
        &["-p", "--color", "never"],
        &["--flat", "--tree"],
    ] {
        let mut a = args.to_vec();
        a.extend(["needle-pkg", g]);
        assert_eq!(
            ok(&fx, &a),
            format!("{tree}          12:apt install needle-pkg\n"),
            "{args:?}"
        );
    }
    for args in [&["--no-heading"][..], &["--tree", "--no-heading"]] {
        let mut a = args.to_vec();
        a.extend(["needle-pkg", g]);
        assert_eq!(
            ok(&fx, &a),
            format!("12: {PKG} │ apt install needle-pkg\n"),
            "{args:?}"
        );
    }
    // --vimgrep prints one line per Match whatever layout came before it.
    assert_eq!(
        ok(&fx, &["--tree", "--vimgrep", "needle-pkg", g]),
        format!("md/guide.md:12:13: {PKG} │ apt install needle-pkg\n")
    );
}

/// M5a plan, D6: a `-U` Match prints a row per line, `-o` a row per line of its text, a
/// `--null-data` record a row per line with its number, and `--crlf` leaves the `\r` out.
#[test]
fn m5_03_multi_line_records_and_crlf() {
    let fx = Fixture::new();
    std::fs::write(
        fx.file("block.md"),
        "# Install\n\nSome intro needle here.\n\n## Linux\n\nRun the needle command:\n",
    )
    .unwrap();
    std::fs::write(
        fx.file("records.dat"),
        b"one needle\ntwo\0three\0needle four\n",
    )
    .unwrap();
    std::fs::write(fx.file("crlf.txt"), b"one needle\r\ntwo\r\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "-U", r"Linux\n\nRun", "block.md"]),
        "5: Install │ ## Linux\n6: Install │ \n7: Install │ Run the needle command:\n"
    );
    assert_eq!(
        ok(&fx, &["--tree", "-U", r"Linux\n\nRun", "block.md"]),
        "block.md\n  # Install\n    5:## Linux\n    6:\n    7:Run the needle command:\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "-U", "-o", r"Linux\n\nRun", "block.md"]),
        "5: Install │ Linux\n7: Install │ Run\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "--null-data", "needle", "records.dat"]),
        "1: one needle\n1: two\n3: needle four\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "--crlf", "needle$", "crlf.txt"]),
        "1: one needle\n"
    );
}

/// `--passthru` prints every line, a file without a Match too (whose exit code stays 1).
#[test]
fn m5_04_passthru() {
    let fx = Fixture::new();
    assert_eq!(
        ok(&fx, &["--flat", "--passthru", "Plan C", "md/twice.md"]),
        "1-# Plan of plans\n2-\n3-## Plan B\n4-\n5: Plan of plans │ ## Plan C\n"
    );
    let o = run(&mut fx.s4(["--flat", "--passthru", "absent", "md/twice.md"]));
    assert_eq!(code(&o), 1);
    assert_eq!(
        stdout(&o),
        "1-# Plan of plans\n2-\n3-## Plan B\n4-\n5-## Plan C\n"
    );
    let o = run(&mut fx.s4(["--json", "--passthru", "absent", "md/twice.md"]));
    let types: Vec<String> = stdout(&o)
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        types,
        [
            "begin", "context", "context", "context", "context", "context", "end", "summary"
        ]
    );
}

/// PCRE2 in the Trail layouts, and rg's errors and suggestions.
#[test]
fn m5_05_pcre2_and_the_engines() {
    let fx = Fixture::new();
    assert_eq!(
        ok(&fx, &["--flat", "-P", "needle-(?=pkg)", "md/guide.md"]),
        format!("12: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--engine",
                "auto",
                "needle-(?=pkg)",
                "md/guide.md"
            ]
        ),
        format!("12: {PKG} │ apt install needle-pkg\n")
    );
    let o = run(&mut fx.s4(["needle-(?=pkg)", "md/guide.md"]));
    assert_eq!(code(&o), 2);
    assert!(stderr(&o).ends_with(
        "Consider enabling PCRE2 with the --pcre2 flag, which can handle backreferences\nand look-around.\n"
    ));
    let o = run(&mut fx.s4(["-P", "(", "md/guide.md"]));
    assert_eq!(
        stderr(&o),
        "s4: PCRE2: error compiling pattern at offset 5: missing closing parenthesis\n"
    );
    let v = ok(&fx, &["--pcre2-version"]);
    assert!(
        v.starts_with("PCRE2 10.") && v.ends_with(" is available (JIT is available)\n"),
        "{v}"
    );
}

/// `-E`: the Trail is computed on the text as decoded, the line printed in UTF-8.
#[test]
fn m5_06_encodings() {
    let fx = Fixture::new();
    std::fs::write(fx.file("latin.md"), b"# Caf\xe9\n\nneedle in the caf\xe9\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "-E", "latin1", "needle", "latin.md"]),
        "3: Café │ needle in the café\n"
    );
    let le: Vec<u8> = "# Notes\nneedle\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(fx.file("utf16.md"), le).unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "-E", "utf-16le", "needle", "utf16.md"]),
        "2: Notes │ needle\n"
    );
}

/// `--files`, `--sort`/`--sortr`, `--type-add` and `--max-filesize` in a search.
#[test]
fn m5_07_files_sorting_and_types() {
    let fx = Fixture::new();
    assert_eq!(
        ok(&fx, &["--files", "--sort", "path", "md"]),
        "md/guide.md\nmd/twice.md\n"
    );
    assert_eq!(
        ok(&fx, &["--files", "--sortr", "path", "md"]),
        "md/twice.md\nmd/guide.md\n"
    );
    assert_eq!(
        ok(&fx, &["--files", "-0", "--sort", "path", "md"]),
        "md/guide.md\0md/twice.md\0"
    );
    let o = run(&mut fx.s4(["--files", "-g", "*.zzz"]));
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
    assert_eq!(
        ok(
            &fx,
            &["--type-add", "notes:twice.*", "-t", "notes", "-l", "Plan"]
        ),
        "md/twice.md\n"
    );
    let o = run(&mut fx.s4(["--max-filesize", "100", "-l", "needle-pkg", "md"]));
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
}

/// M5a plan, D3: `SEARCHFOR_CONFIG_PATH`'s arguments go before the command line's; `--no-config`
/// skips them; a file that cannot be read is a message.
#[test]
fn m5_08_the_configuration_file() {
    let fx = Fixture::new();
    let cfg = fx.file("s4rc");
    std::fs::write(&cfg, "# max one per file\n--max-count=1\n\n  -i  \n").unwrap();
    let with = |args: &[&str], path: &std::path::Path| {
        let mut c = fx.s4(args);
        c.env("SEARCHFOR_CONFIG_PATH", path);
        run(&mut c)
    };
    let o = with(&["--flat", "NEEDLE-[0-9a-b]+", "md/guide.md"], &cfg);
    assert_eq!(
        stdout(&o),
        "23: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |\n"
    );
    let o = with(&["--no-config", "--flat", "NEEDLE", "md/guide.md"], &cfg);
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
    // The command line wins over the file.
    let o = with(&["-s", "--flat", "NEEDLE", "md/guide.md"], &cfg);
    assert_eq!(code(&o), 1);
    let o = with(
        &["--flat", "needle-pkg", "md/guide.md"],
        &fx.file("missing"),
    );
    assert_eq!(code(&o), 0);
    assert!(
        stderr(&o).starts_with("s4: failed to read the file specified in SEARCHFOR_CONFIG_PATH: ")
    );
}

/// M5a plan, D16: `--generate`'s five kinds.
#[test]
fn m5_09_generate() {
    let fx = Fixture::new();
    assert!(ok(&fx, &["--generate", "man"]).contains(".TH s4"));
    assert!(ok(&fx, &["--generate", "complete-bash"]).contains("_s4()"));
    assert!(ok(&fx, &["--generate", "complete-zsh"]).starts_with("#compdef s4"));
    assert!(ok(&fx, &["--generate", "complete-fish"]).contains("complete -c s4"));
    assert!(ok(&fx, &["--generate", "complete-powershell"]).contains("Register-ArgumentCompleter"));
    let o = run(&mut fx.s4(["--generate", "complete-tcsh"]));
    assert_eq!(code(&o), 2);
}

/// M5a plan, D15: `--debug` and `--trace` in rg's format, the ignore crate's lines included.
#[test]
fn m5_10_debug_and_trace() {
    let fx = Fixture::new();
    let o = run(&mut fx.s4(["--debug", "-l", "needle-pkg", "md"]));
    assert_eq!(code(&o), 0);
    let err = stderr(&o);
    assert!(err.lines().all(|l| l.starts_with("s4: DEBUG|")), "{err}");
    assert!(err.contains("|globset|"), "{err}");
    let o = run(&mut fx.s4(["--trace", "-l", "needle-pkg", "md"]));
    assert!(stderr(&o).contains("s4: TRACE|"));
    let o = run(&mut fx.s4(["-l", "needle-pkg", "md"]));
    assert_eq!(stderr(&o), "");
}

/// M5a plan, D17; spec §11 #14: `s4 --mcp` over stdin and stdout. The server and its
/// searches read no configuration file, whose `--pre` would otherwise run on every call and
/// whose `--generate` would replace the server (review r1 of the plan, Fable #1, Codex #1); a
/// value that starts with `-` is a value; a search with an error keeps its Matches; long lines,
/// bytes that are not UTF-8 and a repeated Section come within limits (Codex #2, #3, #7;
/// Fable #2).
#[test]
fn m5_11_mcp_server() {
    let fx = Fixture::new();
    let pre = fx.file("pre.sh");
    std::fs::write(
        &pre,
        format!(
            "#!/bin/sh\ntouch '{}'\necho 'INJECTED needle'\n",
            fx.file("pre-ran").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&pre, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let cfg = fx.file("s4rc");
    std::fs::write(&cfg, format!("--pre={}\n", pre.display())).unwrap();
    let dir = fx.file("mcp");
    std::fs::create_dir_all(&dir).unwrap();
    let long = format!("{} needle {}\n", "x".repeat(3000), "y".repeat(3000));
    std::fs::write(dir.join("long.txt"), long).unwrap();
    // A line longer than the 4 MiB a message of s4 may be (review r2 of the plan, Fable #1).
    let huge = format!("{} needle\n", "x".repeat(5 << 20));
    std::fs::write(dir.join("huge.txt"), huge).unwrap();
    std::fs::write(dir.join("lossy.txt"), b"needle\xff\n").unwrap();
    std::fs::write(dir.join("two.md"), "# Part\n\nneedle one\n\nneedle two\n").unwrap();
    let mut child = fx
        .s4(["--mcp"])
        .env("SEARCHFOR_CONFIG_PATH", &cfg)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut ask = |msg: Value| -> Option<Value> {
        writeln!(input, "{msg}").unwrap();
        input.flush().unwrap();
        msg.get("id")?;
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        Some(serde_json::from_str(&line).unwrap())
    };
    let init = ask(json!({"jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}))
    .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(ask(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    let tools = ask(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
    assert_eq!(tools["result"]["tools"][0]["name"], "search");
    let r = ask(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"needle","paths":["md"],"within":"Install › Linux","glob":["guide.md"]}}}))
    .unwrap();
    let matches = &r["result"]["structuredContent"]["matches"];
    assert_eq!(r["result"]["isError"], false);
    assert_eq!(matches[0]["path"], "md/guide.md");
    assert_eq!(matches[0]["line"], 12);
    assert_eq!(
        matches[0]["trail"],
        json!(["Deploy guide", "Install", "Linux", "bash"])
    );
    assert_eq!(matches[0]["text"], "apt install needle-pkg");
    let r = ask(
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"needle","paths":["md/guide.md"],"max_results":2}}}),
    )
    .unwrap();
    assert_eq!(
        r["result"]["structuredContent"]["matches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(r["result"]["structuredContent"]["truncated"], true);
    assert_eq!(
        r["result"]["structuredContent"]["truncated_by"],
        "max_results"
    );
    let r = ask(
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"(","paths":["md"]}}}),
    )
    .unwrap();
    assert_eq!(r["result"]["isError"], true);
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("regex parse error")
    );
    let mut search = |arguments: Value| -> Value {
        let r = ask(json!({"jsonrpc":"2.0","id":6,"method":"tools/call",
            "params":{"name":"search","arguments":arguments}}))
        .unwrap();
        r["result"].clone()
    };
    let r = search(json!({"pattern":"needle","paths":["mcp/long.txt"]}));
    let m = &r["structuredContent"]["matches"][0];
    assert!(m["text"].as_str().unwrap().len() < 1000, "{m}");
    assert!(m["omitted_before"].as_u64().unwrap() > 2000, "{m}");
    let r = search(json!({"pattern":"needle","paths":["mcp/lossy.txt"]}));
    let m = &r["structuredContent"]["matches"][0];
    assert_eq!(
        (&m["text"], &m["lossy"]),
        (&json!("needle\u{FFFD}"), &json!(true))
    );
    let r = search(json!({"pattern":"needle","paths":["mcp/two.md"],"expand":true}));
    let m = &r["structuredContent"]["matches"];
    assert_eq!(
        m[0]["section"]["text"],
        "# Part\n\nneedle one\n\nneedle two\n"
    );
    assert_eq!(m[1]["section"]["same_as"], 0);
    // Review r2 of the plan, Fable #1: a Match on a line past 4 MiB is left out, and said.
    let r = search(json!({"pattern":"needle","paths":["mcp/huge.txt"]}));
    let r = &r["structuredContent"];
    assert_eq!(r["matches"], json!([]));
    assert_eq!(
        (&r["truncated"], &r["truncated_by"]),
        (&json!(true), &json!("long_line"))
    );
    assert!(
        r["errors"].as_str().unwrap().contains("`mcp/huge.txt`"),
        "{r}"
    );
    // Review r2 of the plan, Fable #2: line 24 holds Matches under two cells, two Matches.
    let r = search(json!({"pattern":"needle-[ab]","paths":["md/guide.md"]}));
    let trails: Vec<(Value, Value)> = r["structuredContent"]["matches"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["line"] == 24)
        .map(|m| (m["trail"][3].clone(), m["text"].clone()))
        .collect();
    let row = json!("| needle-a | needle-b |");
    assert_eq!(
        trails,
        [(json!("Plan"), row.clone()), (json!("Price"), row)]
    );
    let r = search(json!({"pattern":"needle","paths":["md"],"within":"-x"}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["matches"], json!([]));
    let r = search(json!({"pattern":"needle-pkg","paths":["md/guide.md","missing"]}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["matches"][0]["line"], 12);
    assert!(
        r["structuredContent"]["errors"]
            .as_str()
            .unwrap()
            .contains("missing"),
        "{r}"
    );
    drop(input);
    assert!(child.wait().unwrap().success());
    assert!(
        !fx.file("pre-ran").exists(),
        "the configuration's --pre ran"
    );
    // A `--generate` in the configuration file does not replace the server.
    std::fs::write(&cfg, "--generate=man\n").unwrap();
    let mut c = fx.s4(["--mcp"]);
    c.env("SEARCHFOR_CONFIG_PATH", &cfg)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut server = c.spawn().unwrap();
    let mut input = server.stdin.take().unwrap();
    writeln!(input, "{}", json!({"jsonrpc":"2.0","id":1,"method":"ping"})).unwrap();
    drop(input);
    let out = std::io::read_to_string(server.stdout.take().unwrap()).unwrap();
    assert_eq!(out, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n");
    assert!(server.wait().unwrap().success());
}

/// rg's message when the current directory, searched implicitly, gave no file.
#[test]
fn m5_12_nothing_searched() {
    let fx = Fixture::new();
    let o = run(&mut fx.s4(["-g", "*.zzz", "needle"]));
    assert_eq!(code(&o), 2);
    assert_eq!(
        stderr(&o),
        "s4: No files were searched, which means s4 probably applied a filter you didn't expect.\n\
         Running with --debug will show why files are being skipped.\n"
    );
    let o = run(&mut fx.s4(["-g", "*.zzz", "needle", "md"]));
    assert_eq!((code(&o), stderr(&o).as_str()), (1, ""));
}

/// M5a plan, D14: `--stats` after the Trail layouts' output, every file searched counted.
#[test]
fn m5_13_stats_in_the_trail_layouts() {
    let fx = Fixture::new();
    let out = ok(
        &fx,
        &["--flat", "--stats", "--sort", "path", "needle-pkg", "md"],
    );
    let re = regex_lite::Regex::new(r"[0-9]+\.[0-9]{6} seconds").unwrap();
    let out = re.replace_all(&out, "T seconds");
    let row = format!("md/guide.md:12: {PKG} │ apt install needle-pkg\n");
    assert_eq!(
        out,
        format!(
            "{row}\n1 matches\n1 matched lines\n1 files contained matches\n2 files searched\n\
             {} bytes printed\n{} bytes searched\nT seconds spent searching\nT seconds\n",
            row.len(),
            std::fs::metadata(fx.file("md/guide.md")).unwrap().len()
                + std::fs::metadata(fx.file("md/twice.md")).unwrap().len()
        )
    );
}

/// `--colors` with the `trail` type, and `--hyperlink-format`, in the one-line layout.
#[test]
fn m5_14_colours_and_links() {
    let fx = Fixture::new();
    let out = ok(
        &fx,
        &[
            "--flat",
            "--color",
            "always",
            "--colors",
            "trail:fg:yellow",
            "--colors",
            "line:fg:blue",
            "--hyperlink-format",
            "file://{path}:{line}",
            "-H",
            "needle-pkg",
            "md/guide.md",
        ],
    );
    let path = fx.file("md/guide.md").canonicalize().unwrap();
    assert!(
        out.starts_with(&format!("\x1b]8;;file://{}:12\x1b\\", path.display())),
        "{out:?}"
    );
    assert!(out.contains("\x1b[34m12\x1b[0m"), "{out:?}");
    assert!(out.contains("\x1b[33mDeploy guide"), "{out:?}");
}

/// `-c`, `-0` and `--include-zero` with a structural flag, through s4's own summaries.
#[test]
fn m5_15_summaries_with_trails() {
    let fx = Fixture::new();
    assert_eq!(
        ok(
            &fx,
            &[
                "-c", "-0", "--sort", "path", "--within", "Linux", "needle", "md"
            ]
        ),
        "md/guide.md\x002\n"
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "-c",
                "--include-zero",
                "--sort",
                "path",
                "--within",
                "Linux",
                "needle",
                "md"
            ]
        ),
        "md/guide.md:2\nmd/twice.md:0\n"
    );
}

/// M5a plan, D13: `--line-buffered` prints each line as it is found: the first line of a slow
/// stdin comes out before the input ends, as rg's.
#[test]
fn m5_16_line_buffered() {
    let fx = Fixture::new();
    let mut child = fx
        .s4(["--no-trail", "--line-buffered", "needle"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "a needle").unwrap();
    input.flush().unwrap();
    let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert_eq!(line, "a needle\n");
    drop(input);
    assert!(child.wait().unwrap().success());
}

/// Review r1 of the M5a plan, Codex #4: an object file's string takes `-o`, `--vimgrep` and
/// `-r` (spec §5.7, §8): a row per Match at its offset under its Trail, the string with its
/// Matches replaced; in ELF, PE and Mach-O.
#[test]
fn m5_17_object_rows_take_o_vimgrep_and_r() {
    use searchfor::processors::object::fixture as obj;
    let fx = Fixture::new();
    let strings = b"\0first needle|second needle\0";
    let elf = obj::elf(
        &obj::Spec::new()
            .section(".text", b"\xc3\0\0\0")
            .section(".rodata", strings)
            .sized("one", 1, 1, 13)
            .sized("two", 1, 14, 13),
    )
    .file;
    let pe = obj::pe(
        &obj::Spec::new()
            .section(".text", b"\xc3\0\0\0")
            .section(".rdata", strings)
            .symbol("first_string", 1, 1),
    )
    .file;
    let macho = obj::macho(
        &obj::Spec::new()
            .section("__TEXT,__text", b"\xc3\0\0\0")
            .section("__TEXT,__cstring", strings)
            .symbol("_first", 1, 1),
    )
    .file;
    let cases = [
        ("app", &elf, "ELF x86-64 › .rodata", ["one+0x6", "two+0x7"]),
        (
            "app.exe",
            &pe,
            "PE x86-64 › .rdata",
            ["first_string+0x6", "first_string+0x14"],
        ),
        (
            "app-mac",
            &macho,
            "Mach-O x86-64 › __TEXT,__cstring",
            ["_first+0x6", "_first+0x14"],
        ),
    ];
    for (name, bytes, trail, [sym1, sym2]) in cases {
        std::fs::write(fx.file(name), bytes).unwrap();
        // The string's first byte, and its two Matches at 6 and 20.
        let s = memchr::memmem::find(bytes, b"first needle").unwrap();
        let (a, b) = (format!("{:#x}", s + 6), format!("{:#x}", s + 20));
        assert_eq!(
            ok(&fx, &["--flat", "-o", "needle", name]),
            format!("{a}: {trail} › {sym1} │ needle\n{b}: {trail} › {sym2} │ needle\n")
        );
        assert_eq!(
            ok(&fx, &["--flat", "--vimgrep", "needle", name]),
            format!(
                "{name}:{a}:7: {trail} › {sym1} │ first needle|second needle\n\
                 {name}:{b}:21: {trail} › {sym2} │ first needle|second needle\n"
            )
        );
        assert_eq!(
            ok(&fx, &["--flat", "-r", "X", "needle", name]),
            format!("{a}: {trail} › {sym1} │ first X|second X\n")
        );
    }
    // Review r2 of the plan, Codex #2: `-r` replaces each Match where the search found it in
    // the file, so `^` does not match after a NUL and a group reads the file's bytes, and only
    // then is the string made printable.
    let strings = b"\0needle target\0zz ab\x01cd yy\0";
    let files = [
        (
            "anchors",
            obj::elf(
                &obj::Spec::new()
                    .section(".text", b"\xc3\0\0\0")
                    .section(".rodata", strings),
            )
            .file,
        ),
        (
            "anchors.exe",
            obj::pe(
                &obj::Spec::new()
                    .section(".text", b"\xc3\0\0\0")
                    .section(".rdata", strings),
            )
            .file,
        ),
        (
            "anchors-mac",
            obj::macho(
                &obj::Spec::new()
                    .section("__TEXT,__text", b"\xc3\0\0\0")
                    .section("__TEXT,__cstring", strings),
            )
            .file,
        ),
    ];
    let texts = |args: &[&str], name: &str| -> Vec<String> {
        let mut a = vec!["--flat"];
        a.extend_from_slice(args);
        a.push(name);
        ok(&fx, &a)
            .lines()
            .map(|l| l.split_once(" │ ").unwrap().1.to_string())
            .collect()
    };
    for (name, bytes) in &files {
        std::fs::write(fx.file(name), bytes).unwrap();
        assert_eq!(texts(&["-r", "X", "^needle|target"], name), ["needle X"]);
        assert_eq!(texts(&["-r", "X", "ab\\x01cd"], name), ["zz X yy"]);
        assert_eq!(texts(&["-r", "<$1>", "a(b\\x01c)d"], name), ["zz <b.c> yy"]);
        assert_eq!(
            texts(&["-o", "-r", "[$1]", "(tar)get|a(b\\x01c)d"], name),
            ["[tar]", "[]"]
        );
        // Review r3 of the plan, Codex #3: a `\K` Match, whose pattern reads the bytes before
        // it, is found again from its line's start, its groups before `\K` too, as `rg -a`.
        let k = ["-P", "-r", "X", "needle \\Ktarget"];
        assert_eq!(texts(&k, name), ["needle X"]);
        let k = ["-P", "-r", "<$1>", "(nee)dle \\Ktarget"];
        assert_eq!(texts(&k, name), ["needle <nee>"]);
        let k = ["-P", "-o", "-r", "X", "needle \\Ktarget"];
        assert_eq!(texts(&k, name), ["X"]);
    }
    // Review r4 of the plan, Codex #2: a Match after `\K` keeps the groups the search found.
    // From the line's start the first alternative matches, its group before `\K`; from the
    // string `target`, the second would match the same bytes with the other group.
    let strings = b"\0needle\0target\0";
    let alternatives = [
        obj::elf(
            &obj::Spec::new()
                .section(".text", b"\xc3\0\0\0")
                .section(".rodata", strings),
        )
        .file,
        obj::pe(
            &obj::Spec::new()
                .section(".text", b"\xc3\0\0\0")
                .section(".rdata", strings),
        )
        .file,
        obj::macho(
            &obj::Spec::new()
                .section("__TEXT,__text", b"\xc3\0\0\0")
                .section("__TEXT,__cstring", strings),
        )
        .file,
    ];
    let k = [
        "-a",
        "-P",
        "-o",
        "-r",
        "<$1|$2>",
        "(needle)\\x00\\Ktarget|(target)",
    ];
    for (name, bytes) in ["alt", "alt.exe", "alt-mac"].iter().zip(&alternatives) {
        std::fs::write(fx.file(name), bytes).unwrap();
        assert_eq!(texts(&k, name), ["<needle|>"]);
        if common::reference_rg() {
            let mut rg = k.to_vec();
            rg.push(name);
            assert_eq!(stdout(&run(&mut fx.rg(&rg))), "<needle|>\n");
        }
    }
}

/// Review r4 of the plan, Codex #1: an object file's `-r` reads each line once, however many
/// strings it holds. `.rodata` holds N strings `B`, each after a NUL and none with a `\n`, all
/// on one line; `\x00\KB` makes each Match start past the bytes its pattern reads. Looking
/// each string up again from its line's start made the time grow with N²: 4 000 strings took
/// ten times what 1 000 took. Here, four times the strings take less than four times as long.
#[test]
fn m5_22_object_replacements_take_linear_time() {
    use searchfor::processors::object::fixture as obj;
    let fx = Fixture::new();
    let time = |n: usize| -> std::time::Duration {
        let name = format!("b{n}");
        let mut strings = b"\0B".repeat(n);
        strings.push(0);
        let elf = obj::elf(
            &obj::Spec::new()
                .section(".text", b"\xc3\0\0\0")
                .section(".rodata", &strings),
        )
        .file;
        std::fs::write(fx.file(&name), elf).unwrap();
        let args = ["--flat", "-a", "-P", "-r", "X", "\\x00\\KB", &name];
        (0..3)
            .map(|_| {
                let started = std::time::Instant::now();
                let out = ok(&fx, &args);
                let elapsed = started.elapsed();
                let replaced = out.lines().filter(|l| l.ends_with(" │ X")).count();
                assert!(replaced >= n, "{replaced} rows replaced of {n}");
                elapsed
            })
            .min()
            .unwrap()
    };
    let (small, large) = (time(1_000), time(4_000));
    assert!(
        large < small * 4,
        "1 000 strings: {small:?}; 4 000: {large:?}"
    );
}

/// Review r1 of the M5a plan, Codex #8: `--trim` in the Trail layouts leaves out the white
/// space grep-printer leaves out (`\r` and `\x0B` too), never the line terminator's bytes.
#[test]
fn m5_18_trim_as_ripgrep() {
    let fx = Fixture::new();
    std::fs::write(fx.file("t.txt"), b"\r needle\n\x0b needle\n\x0c\tneedle\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "--trim", "needle", "t.txt"]),
        "1: needle\n2: needle\n3: needle\n"
    );
    std::fs::write(fx.file("crlf.txt"), b"\r\tneedle\r\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "--trim", "--crlf", "needle", "crlf.txt"]),
        "1: \r\tneedle\n"
    );
}

/// Review r2 of the plan, Codex #1: `--no-mmap` maps no file, however large, as rg; without it
/// s4 maps a file of 8 MiB or more. Seen in `/proc/PID/maps` while s4 prints: the test reads
/// the first line and no more, so s4 waits on a full pipe with the file still open.
#[cfg(target_os = "linux")]
#[test]
fn m5_19_no_mmap_maps_no_file() {
    let fx = Fixture::new();
    let line = format!("needle {}\n", "x".repeat(90));
    let lines = line.repeat(9 * 1024 * 1024 / line.len() + 1);
    std::fs::write(fx.file("big.md"), format!("# Big\n{lines}")).unwrap();
    let big = fx.file("big.md").canonicalize().unwrap();
    let mapped = |flags: &[&str]| -> bool {
        let mut args = flags.to_vec();
        args.extend(["--flat", "needle", "big.md"]);
        let mut child = fx
            .s4(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        out.read_line(&mut first).unwrap();
        assert!(first.contains("needle"), "{flags:?}: {first}");
        let maps = std::fs::read_to_string(format!("/proc/{}/maps", child.id())).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        maps.contains(big.to_str().unwrap())
    };
    assert!(mapped(&[]), "a file of 8 MiB is mapped by default");
    assert!(!mapped(&["--no-mmap"]));
    assert!(!mapped(&["--mmap", "--no-mmap"]));
}

/// Review r2 of the plan, Codex #4: `-o` in the Trail layouts prints the rows rg prints, an
/// empty Match or an empty replacement as an empty row, and a `\r` Match without `--crlf`.
/// A Plain file's row is `N: text`, rg's `N:text`, and ends with `\n`, with `--crlf` and
/// `--null-data` too.
#[test]
fn m5_20_only_matching_rows_as_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::write(fx.file("n.txt"), b"needle\nnext\n").unwrap();
    std::fs::write(fx.file("cr.txt"), b"needle\r\n").unwrap();
    std::fs::write(fx.file("nul.txt"), b"a needle\0b\0c needle\0").unwrap();
    let cases: [&[&str]; 14] = [
        &["-o", "-r", "", "needle", "n.txt"],
        &["-o", "-r", "", "e", "n.txt"],
        &["-o", "x*", "n.txt"],
        &["-o", "\\r", "cr.txt"],
        // With -U, a pattern that cannot match a line break prints as without it; one that
        // can, as rg's printer of several lines, which skips what is left empty.
        &["-U", "-o", "x*", "n.txt"],
        &["-U", "-o", "-r", "", "needle", "n.txt"],
        &["-U", "-o", "-r", "Z", "e\\nn", "n.txt"],
        // Review r3 of the plan, Codex #4: `--vimgrep` in a search of several lines prints no
        // row for an empty Match at a line's start, as rg's printer of several lines.
        &["-U", "--vimgrep", "^", "n.txt"],
        &["-U", "--vimgrep", "^|e", "n.txt"],
        &["-U", "--vimgrep", "-P", "e?", "n.txt"],
        // Review r4 of the plan, Fable #1: with `--crlf` rg ends each line it prints with
        // `\r\n`; the Trail layouts end every row with `\n` (rg-compat's deviations).
        &["--crlf", "needle", "cr.txt"],
        &["--crlf", "-o", "needle", "cr.txt"],
        &["-U", "--vimgrep", "--crlf", "$", "cr.txt"],
        // With `--null-data` rg ends each record with its NUL; a row leaves it out (D6).
        &["--null-data", "needle", "nul.txt"],
    ];
    for case in cases {
        let mut rg = vec!["-n"];
        rg.extend_from_slice(case);
        let mut want = stdout(&run(&mut fx.rg(&rg)));
        if case.contains(&"--crlf") {
            assert!(want.ends_with("\r\n"), "{case:?}: {want:?}");
            want = want.replace("\r\n", "\n");
        }
        if case.contains(&"--null-data") {
            assert!(want.ends_with('\0'), "{case:?}: {want:?}");
            want = want.replace('\0', "\n");
        }
        let mut s4 = vec!["--flat"];
        s4.extend_from_slice(case);
        let got: String = ok(&fx, &s4)
            .split_inclusive('\n')
            .map(|l| l.replacen(": ", ":", 1))
            .collect();
        assert_eq!(got, want, "{case:?}");
        assert!(!want.is_empty() || case[2] == "^", "{case:?}");
    }
}

/// Review r3 of the plan, Codex #1: the MCP server reads the Trails of a line from its message,
/// so a line under 240 Trails asked for one Match takes little memory (over 200 MiB when
/// each Trail copied the message). Seen in the server's `VmHWM` before it exits.
#[cfg(target_os = "linux")]
#[test]
fn m5_21_mcp_memory_of_a_line_with_many_trails() {
    let fx = Fixture::new();
    let elements: String = (0..240).map(|i| format!("<k{i}>needle</k{i}>")).collect();
    let xml = format!("<?xml version=\"1.0\"?><root>{elements}</root>\n");
    std::fs::write(fx.file("many.xml"), xml).unwrap();
    let mut server = fx
        .s4(["--mcp"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = server.stdin.take().unwrap();
    let call = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"needle","paths":["many.xml"],"max_results":1}}});
    writeln!(input, "{call}").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    std::io::BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let status = std::fs::read_to_string(format!("/proc/{}/status", server.id())).unwrap();
    drop(input);
    assert!(server.wait().unwrap().success());
    let r: Value = serde_json::from_str(&line).unwrap();
    let r = &r["result"]["structuredContent"];
    assert_eq!(r["matches"][0]["trail"], json!(["root", "k0"]), "{r}");
    assert_eq!(r["truncated_by"], "max_results");
    let peak: u64 = status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().strip_suffix(" kB"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(peak < 64 * 1024, "{peak} kB");
}
````

- [ ] **Step 3:** `SEARCHFOR_REQUIRE_REFERENCE=1 cargo test --test rg_compat_m5 --test cli_m5`:
  most cases FAIL (the actions do nothing, `--no-trail`'s printer ignores the output flags, the
  Look is M1–M4's): RED. Keep the list of failures for Step 3 of Task 9.4.

### Task 9.2: rg's printers and the Look from the command line

**Files:** `src/output/rg.rs` (new), `src/output/mod.rs`, `src/output/hyperlink.rs`,
`src/output/stream.rs`, `src/search.rs`.

- [ ] **Step 1:** `src/output/rg.rs` (design note 5; D4, D5, D14): rg's colours with the
  `--colors` specs, its hyperlink configuration, heading and column, the standard, summary and
  path printers as rg's `printer_standard`, `printer_summary` and `path_printer_builder`, the
  `Look` of the Trail layouts (a `-U` search is several lines only where grep-printer's
  `multi_line_with_matcher` says so: review r2, Codex #4), which files stream (`streams`) and
  the `--stats` block:

```rust
//! The run's output settings from the command line (M5a plan, D4): grep-printer's printers
//! configured as rg 14.1.1 configures them (hiargs.rs `printer_standard`, `printer_summary`,
//! `path_printer_builder`), for `--no-trail`, the summaries and `--files`; the `Look` of the
//! Trail layouts; and the `--stats` block.
use super::{Look, hyperlink::Hyperlinks, json::RunStats, replace::Replacer, style::Palette};
use crate::cli::Args;
use crate::matcher::PatternMatcher;
use grep_printer::{
    ColorSpecs, HyperlinkConfig, HyperlinkEnvironment, PathPrinter, PathPrinterBuilder, Standard,
    StandardBuilder, Summary, SummaryBuilder, SummaryKind, UserColorSpec,
};
use std::sync::Arc;
use termcolor::WriteColor;

/// rg's colours with the `--colors` specs of grep-printer's types (`trail:` is s4's own,
/// for the Trail layouts only).
pub fn color_specs(args: &Args) -> ColorSpecs {
    let mut specs = grep_printer::default_color_specs();
    specs.extend(
        args.colors
            .iter()
            .filter(|s| !s.starts_with("trail:"))
            .filter_map(|s| s.parse::<UserColorSpec>().ok()),
    );
    ColorSpecs::new(&specs)
}

/// rg's hyperlink configuration (hiargs.rs `take_hyperlink_config`): the format, the
/// hostname (`--hostname-bin`) and the WSL prefix.
pub fn hyperlink_config(args: &Args) -> HyperlinkConfig {
    let format = args
        .hyperlink_format
        .as_deref()
        .and_then(|f| f.parse().ok())
        .unwrap_or_default();
    let mut env = HyperlinkEnvironment::new();
    if args.hyperlink_format.is_some() {
        env.host(super::hyperlink::hostname(args.hostname_bin.as_deref()));
        env.wsl_prefix(super::hyperlink::wsl_prefix());
    }
    HyperlinkConfig::new(env, format)
}

/// rg's heading (hiargs.rs): asked for (`--heading`, `--tree`, `--pretty`) or on a terminal,
/// never with `--vimgrep`.
pub fn heading(args: &Args, tty: bool) -> bool {
    !args.vimgrep && (args.tree || (!args.flat && tty))
}

/// rg's `--column`: asked for, or with `--vimgrep` unless `--no-column` came last.
pub fn column(args: &Args) -> bool {
    if args.column {
        true
    } else if args.no_column {
        false
    } else {
        args.vimgrep
    }
}

/// rg's `-0`: a NUL after each path.
pub fn path_terminator(args: &Args) -> Option<u8> {
    args.null.then_some(0)
}

/// rg's standard printer for `--no-trail` (hiargs.rs `printer_standard`). `separator` is the
/// separator between files the printer writes itself, with one thread.
pub fn standard<W: WriteColor>(
    args: &Args,
    heading: bool,
    with_filename: bool,
    max_count: Option<u64>,
    separator: Option<Vec<u8>>,
    w: W,
) -> Standard<W> {
    StandardBuilder::new()
        .byte_offset(args.byte_offset)
        .color_specs(color_specs(args))
        .column(column(args))
        .heading(heading)
        .hyperlink(hyperlink_config(args))
        .max_columns_preview(args.max_columns_preview)
        .max_columns(args.max_columns)
        .max_matches(max_count)
        .only_matching(args.only_matching)
        .path(with_filename)
        .path_terminator(path_terminator(args))
        .per_match_one_line(true)
        .per_match(args.vimgrep)
        .replacement(args.replace.clone().map(String::into_bytes))
        .separator_context(args.context_separator.clone())
        .separator_field_context(args.field_context_separator.0.clone())
        .separator_field_match(args.field_match_separator.0.clone())
        .separator_path(args.path_separator.and_then(|s| s.0))
        .stats(args.stats)
        .trim_ascii(args.trim)
        .separator_search(separator)
        .build(w)
}

/// rg's summary printer (hiargs.rs `printer_summary`).
pub fn summary<W: WriteColor>(
    args: &Args,
    kind: SummaryKind,
    with_filename: bool,
    max_count: Option<u64>,
    w: W,
) -> Summary<W> {
    SummaryBuilder::new()
        .color_specs(color_specs(args))
        .exclude_zero(!args.include_zero)
        .hyperlink(hyperlink_config(args))
        .kind(kind)
        .max_matches(max_count)
        .path(with_filename)
        .path_terminator(path_terminator(args))
        .separator_field(b":".to_vec())
        .separator_path(args.path_separator.and_then(|s| s.0))
        .stats(args.stats)
        .build(w)
}

/// rg's printer of `--files` (hiargs.rs `path_printer_builder`).
pub fn path_printer<W: WriteColor>(args: &Args, w: W) -> PathPrinter<W> {
    PathPrinterBuilder::new()
        .color_specs(color_specs(args))
        .hyperlink(hyperlink_config(args))
        .separator(args.path_separator.and_then(|s| s.0))
        .terminator(path_terminator(args).unwrap_or(b'\n'))
        .build(w)
}

/// How the Trail layouts print rg's output flags (M5a plan, D4). A `-U` search whose pattern
/// cannot match a line terminator prints as a search of lines, as grep-printer's printers do
/// (`multi_line_with_matcher`): `-o` then prints an empty Match as an empty row (review r2 of
/// the M5a plan, Codex #4).
pub fn look(args: &Args, matcher: &PatternMatcher) -> Look {
    let multi_line = crate::search::searcher(args, false, true).multi_line_with_matcher(matcher);
    Look {
        column: column(args),
        byte_offset: args.byte_offset,
        only_matching: args.only_matching,
        per_match: args.vimgrep,
        trim: args.trim,
        max_columns_preview: args.max_columns_preview,
        replace: args.replace.as_ref().map(|r| {
            Arc::new(Replacer::new(
                matcher.clone(),
                r.clone().into_bytes(),
                crate::search::line_terminator(args),
                multi_line,
            ))
        }),
        sep_match: args.field_match_separator.0.clone(),
        sep_context: args.field_context_separator.0.clone(),
        sep_group: args.context_separator.clone(),
        path_terminator: path_terminator(args),
        path_separator: args.path_separator.and_then(|s| s.0),
        palette: Palette::new(&args.colors),
        hyperlinks: args
            .hyperlink_format
            .as_deref()
            .and_then(|f| Hyperlinks::new(f, args.hostname_bin.as_deref()))
            .map(Arc::new),
        crlf: args.crlf,
        null_data: args.null_data,
        multi_line,
    }
}

/// Whether the rows of a file can be printed as the searcher reports them (the streaming
/// printer, `output::stream`): not when a flag makes rows the searcher does not report (`-o`,
/// `--vimgrep`, `-r`, `-U`, `--null-data`, `--crlf`'s terminator), nor with `--passthru`,
/// which prints a file without a Match, nor with `--stats`, which counts every file's search in
/// the model's path.
pub fn streams(look: &Look, args: &Args) -> bool {
    !(look.only_matching
        || look.per_match
        || look.replace.is_some()
        || look.multi_line
        || look.null_data
        || look.crlf
        || args.passthru
        || args.stats)
}

/// rg 14.1.1's `--stats` block (main.rs `print_stats`), after the output.
pub fn stats_block(s: &RunStats) -> String {
    format!(
        "\n{} matches\n{} matched lines\n{} files contained matches\n{} files searched\n\
         {} bytes printed\n{} bytes searched\n{:0.6} seconds spent searching\n{:0.6} seconds\n",
        s.matches,
        s.matched_lines,
        s.searches_with_match,
        s.searches,
        s.bytes_printed,
        s.bytes_searched,
        s.elapsed.as_secs_f64(),
        s.elapsed_total.as_secs_f64(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Args {
        let mut argv = vec!["s4"];
        argv.extend_from_slice(v);
        argv.push("x");
        crate::cli::parse(argv.iter().map(Into::into).collect()).unwrap()
    }

    #[test]
    fn heading_and_column_as_ripgrep() {
        assert!(heading(&args(&[]), true));
        assert!(!heading(&args(&[]), false));
        assert!(heading(&args(&["--heading"]), false));
        assert!(!heading(&args(&["--no-heading"]), true));
        assert!(!heading(&args(&["--heading", "--vimgrep"]), true));
        assert!(heading(&args(&["-p"]), false));
        assert!(column(&args(&["--vimgrep"])));
        assert!(!column(&args(&["--vimgrep", "--no-column"])));
        assert!(column(&args(&["--column"])));
    }

    #[test]
    fn the_look_of_the_command_line() {
        let a = args(&[
            "-o",
            "--trim",
            "--field-match-separator",
            "=",
            "--no-context-separator",
            "-0",
            "--path-separator",
            r"\\",
            "-U",
        ]);
        // `-U` prints as several lines only when the pattern can match a line break (review r2
        // of the plan, Codex #4).
        let spans = crate::matcher::build(&a, &["x\\nx".into()]).unwrap();
        assert!(look(&a, &spans).multi_line);
        let m = crate::matcher::build(&a, &["x".into()]).unwrap();
        let l = look(&a, &m);
        assert!(l.only_matching && l.trim && !l.multi_line);
        assert_eq!(l.sep_match, b"=");
        assert_eq!(l.sep_group, None);
        assert_eq!(l.path_terminator, Some(0));
        assert_eq!(l.path_separator, Some(b'\\'));
        assert!(!streams(&l, &a));
        assert!(streams(
            &look(&args(&["--column"]), &m),
            &args(&["--column"])
        ));
    }

    #[test]
    fn stats_block_as_ripgrep_prints_it() {
        let s = RunStats {
            elapsed_total: std::time::Duration::from_micros(3961),
            elapsed: std::time::Duration::from_micros(57),
            searches: 2,
            searches_with_match: 2,
            bytes_searched: 152,
            bytes_printed: 106,
            matched_lines: 4,
            matches: 5,
        };
        assert_eq!(
            stats_block(&s),
            "\n5 matches\n4 matched lines\n2 files contained matches\n2 files searched\n\
             106 bytes printed\n152 bytes searched\n0.000057 seconds spent searching\n\
             0.003961 seconds\n"
        );
    }
}
```

- [ ] **Step 2:** the module, and `write_path` dropped (nothing calls it since C7 and C8):

```diff
diff --git a/src/output/mod.rs b/src/output/mod.rs
index f5402d8..0c3e81f 100644
--- a/src/output/mod.rs
+++ b/src/output/mod.rs
@@ -4,6 +4,7 @@ pub mod fragment;
 pub mod hyperlink;
 pub mod json;
 pub mod replace;
+pub mod rg;
 pub mod stream;
 pub mod style;
 pub mod summary;
@@ -1158,12 +1159,6 @@ pub fn link_end(w: &mut dyn WriteColor, started: bool) -> std::io::Result<()> {
     Ok(())
 }
 
-pub fn write_path(w: &mut dyn WriteColor, p: &std::path::Path) -> std::io::Result<()> {
-    w.set_color(&style::spec("path"))?;
-    w.write_all(&path_bytes(p))?;
-    w.reset()
-}
-
 /// Hand-built Described values for the printers' tests.
 #[cfg(test)]
 pub(crate) mod fixture {
```

- [ ] **Step 3:** the hostname and the WSL prefix for grep-printer's configuration too:

```diff
diff --git a/src/output/hyperlink.rs b/src/output/hyperlink.rs
index b432f57..0d329e2 100644
--- a/src/output/hyperlink.rs
+++ b/src/output/hyperlink.rs
@@ -178,7 +178,7 @@ fn encode(input: &[u8]) -> Vec<u8> {
 
 /// rg 14.1.1's `hostname`: the output of `--hostname-bin`, trimmed, when it runs and prints
 /// something; else the system's.
-fn hostname(bin: Option<&Path>) -> Option<String> {
+pub fn hostname(bin: Option<&Path>) -> Option<String> {
     if let Some(bin) = bin
         && let Ok(bin) = grep_cli::resolve_binary(bin)
     {
@@ -195,7 +195,7 @@ fn hostname(bin: Option<&Path>) -> Option<String> {
 }
 
 /// rg 14.1.1's `wsl_prefix`: `wsl$/<distro>` on Unix under WSL.
-fn wsl_prefix() -> Option<String> {
+pub fn wsl_prefix() -> Option<String> {
     if !cfg!(unix) {
         return None;
     }
```

- [ ] **Step 4:** the streaming printer with the searcher's submatches (design note 3), and M4's
  helpers dropped from `search.rs`; the walker sorts siblings by name, as rg's (D8, D20); and
  the look-ahead case of `submatches_follow_the_searchers_terminator`, which C3 could not test
  without C2's PCRE2; and `file_types`, the walker's types, which `--type-list` prints too:

```diff
diff --git a/src/output/stream.rs b/src/output/stream.rs
index 5b03f81..5d17054 100644
--- a/src/output/stream.rs
+++ b/src/output/stream.rs
@@ -7,7 +7,7 @@ use super::json::{JsonFile, JsonOptions, RunStats};
 use super::{Layout, LineText, RenderOptions, Row, flat, tree};
 use crate::matcher::PatternMatcher;
 use crate::model::{FileStats, Line, Segment, Submatch};
-use crate::search::{MaxCount, context_submatches, match_submatches};
+use crate::search::{MaxCount, context_submatches_in, match_submatches_in};
 use crate::trail::{Outline, TrailCursor};
 use grep_searcher::{Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
 use std::path::Path;
@@ -415,12 +415,12 @@ impl<W: WriteColor> Sink for StreamSink<'_, W> {
         Ok(true)
     }
 
-    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
+    fn matched(&mut self, s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
         self.max_count.matched();
         self.stats.matched_lines += m.lines().count() as u64;
         self.has_match = true;
         let (matcher, invert) = (self.matcher, self.invert);
-        let subs = || match_submatches(matcher, invert, m);
+        let subs = || match_submatches_in(s, matcher, invert, m);
         let line = Line {
             line_number: m.line_number().unwrap_or(0),
             absolute_offset: m.absolute_byte_offset(),
@@ -432,10 +432,10 @@ impl<W: WriteColor> Sink for StreamSink<'_, W> {
         Ok(self.max_count.go_on())
     }
 
-    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
+    fn context(&mut self, s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
         self.max_count.context(c);
         let (matcher, invert) = (self.matcher, self.invert);
-        let subs = || context_submatches(matcher, invert, c.bytes());
+        let subs = || context_submatches_in(s, matcher, invert, c.bytes());
         let line = Line {
             line_number: c.line_number().unwrap_or(0),
             absolute_offset: c.absolute_byte_offset(),
```

```diff
diff --git a/src/search.rs b/src/search.rs
index bdd3181..7a871ea 100644
--- a/src/search.rs
+++ b/src/search.rs
@@ -183,24 +183,7 @@ pub fn walker_with(
         w.add_custom_ignore_filename(".rgignore");
     }
     // The file types before the globs, as ripgrep 14.1.1 validates them (code review r3).
-    let mut tb = rg_types()?;
-    // In command-line order, as ripgrep (hiargs.rs `types`): the last selection of a type wins,
-    // and --type-add and --type-clear change the types the selections after them see.
-    for change in &args.type_changes {
-        match change {
-            TypeChange::Select(t) => {
-                tb.select(t);
-            }
-            TypeChange::Negate(t) => {
-                tb.negate(t);
-            }
-            TypeChange::Add(def) => tb.add_def(def)?,
-            TypeChange::Clear(t) => {
-                tb.clear(t);
-            }
-        }
-    }
-    w.types(tb.build()?);
+    w.types(file_types(args)?.build()?);
     // As ripgrep 14.1.1: every -g first, case-insensitive with --glob-case-insensitive, then
     // every --iglob (case-insensitive); the relative order of the two kinds on the command line
     // does not matter (measured in review r2).
@@ -218,8 +201,9 @@ pub fn walker_with(
         }
     }
     w.overrides(ob.build()?);
+    // As rg 14.1.1 (hiargs.rs `walk_builder`): siblings by name, which is the paths' order.
     if args.sort == Some(PATH) {
-        w.sort_by_file_path(|a, b| a.cmp(b));
+        w.sort_by_file_name(|a, b| a.cmp(b));
     }
     w.threads(walker_threads(args));
     Ok((w, errors))
@@ -288,6 +272,29 @@ pub fn walker_threads(args: &Args) -> usize {
 /// ripgrep 14.1.1's file types, as `rg --type-list` prints them: `name: glob, glob`.
 pub const RG_TYPES: &str = include_str!("rg_types.txt");
 
+/// The file types of a run: ripgrep 14.1.1's, changed in command-line order, as ripgrep
+/// (hiargs.rs `types`): the last selection of a type wins, and --type-add and --type-clear
+/// change the types the selections after them see, and the ones `--type-list` prints (review
+/// r2 of the M5a plan, Codex #3).
+pub fn file_types(args: &Args) -> anyhow::Result<TypesBuilder> {
+    let mut tb = rg_types()?;
+    for change in &args.type_changes {
+        match change {
+            TypeChange::Select(t) => {
+                tb.select(t);
+            }
+            TypeChange::Negate(t) => {
+                tb.negate(t);
+            }
+            TypeChange::Add(def) => tb.add_def(def)?,
+            TypeChange::Clear(t) => {
+                tb.clear(t);
+            }
+        }
+    }
+    Ok(tb)
+}
+
 /// A `TypesBuilder` with ripgrep 14.1.1's file types, which differ from the ignore crate's
 /// defaults.
 pub fn rg_types() -> anyhow::Result<TypesBuilder> {
@@ -545,35 +552,6 @@ pub fn context_submatches_in(
     }
 }
 
-/// `match_submatches_in` for a search with `\n` lines and no `-U`, M1–M4's: the streaming
-/// printer's until chunk 9 moves it to the searcher's terminator.
-pub fn match_submatches(
-    matcher: &PatternMatcher,
-    invert: bool,
-    m: &SinkMatch<'_>,
-) -> Vec<Submatch> {
-    if invert {
-        Vec::new()
-    } else {
-        lines_submatches(matcher, m.buffer(), m.bytes_range_in_buffer())
-    }
-}
-
-/// `context_submatches_in` as `match_submatches` is `match_submatches_in`.
-pub fn context_submatches(matcher: &PatternMatcher, invert: bool, bytes: &[u8]) -> Vec<Submatch> {
-    if invert {
-        lines_submatches(matcher, bytes, 0..bytes.len())
-    } else {
-        Vec::new()
-    }
-}
-
-/// `submatches_in` for `\n` lines without `-U`.
-fn lines_submatches(matcher: &PatternMatcher, buffer: &[u8], range: Range<usize>) -> Vec<Submatch> {
-    let searcher = SearcherBuilder::new().build();
-    submatches_in(&searcher, matcher, buffer, range)
-}
-
 impl Sink for CollectSink<'_> {
     type Error = std::io::Error;
 
@@ -810,7 +788,9 @@ fn search_with(
     }
     sink.stats.elapsed = started.elapsed();
     let stats = sink.stats.clone();
-    if !sink.items.iter().any(|i| matches!(i, Item::Match(_))) {
+    // `--passthru` prints a file without a Match too, every line a context line, as rg's.
+    let printed = searcher.passthru() && !sink.items.is_empty();
+    if !printed && !sink.items.iter().any(|i| matches!(i, Item::Match(_))) {
         return Ok((None, stats));
     }
     let fm = FileMatches {
@@ -1557,6 +1537,8 @@ mod tests {
         assert_eq!(subs(&["--crlf"], r"\s$", b"a \r\n"), [(1, 2)]);
         // Multi-line: one submatch across the lines of its block.
         assert_eq!(subs(&["-U"], r"a\nb", b"a\nb\n"), [(0, 3)]);
+        // A look-ahead past the block, as rg's printer sees it.
+        assert_eq!(subs(&["-U", "-P"], r"b(?=\nc)", b"a\nb\nc\n"), [(0, 1)]);
     }
 
     #[test]
```

### Task 9.3: the pipeline

**Files:** `src/app.rs`.

- [ ] **Step 1:** the logger started after the command line is read; the actions
  `--pcre2-version` (first, as rg's special mode), `--generate`, `--mcp` and `--files` (D16,
  D17, D20) and `--type-list` with the types the command line changed (exit 1 without any, as
  rg's `types`); `-o --count` as `--count-matches` (rg's hiargs.rs); `-q --stats` searching every
  file; `--vimgrep`'s layout, file names and line numbers; rg's heading for `--no-trail`; the
  context separator from `--context-separator`; `Out` (stdout with the bytes printed and the
  spill per line under line buffering, D13); the Look in the Trail options; the streaming paths
  only where `output::rg::streams` says; `--stats` counted in every path and printed after the
  output (D14); the JSON summary of `--no-trail` counting each file grep-printer printed a
  `begin` for; `--passthru` printing a file without a Match; rg's message when nothing was
  searched (D19); the hyperlink methods through `Spill` and `SpillLines`; and an object file's
  `-r`, its replacements made by its search in one pass over each line it reported
  (`Replacer::replacements`), kept in `Described::replacements` for `sunk` (D4; review r4, Codex
  #1 and #2):

```diff
diff --git a/src/app.rs b/src/app.rs
index 57cf61b..eed5db1 100644
--- a/src/app.rs
+++ b/src/app.rs
@@ -10,6 +10,7 @@ use crate::output::{
     self, Layout, RenderOptions,
     json::JsonOptions,
     json::RunStats,
+    replace::Replacer,
     stream::{StreamFile, StreamSink},
     summary,
 };
@@ -19,10 +20,7 @@ use crate::search::{self, CollectSink, MaxCount};
 use crate::source::Source;
 use crate::trail::{LineIndex, Outline};
 use grep_matcher::Matcher;
-use grep_printer::{
-    ColorSpecs, JSON, JSONBuilder, Standard, StandardBuilder, Stats, Summary, SummaryBuilder,
-    SummaryKind,
-};
+use grep_printer::{JSON, JSONBuilder, Standard, Stats, Summary, SummaryKind};
 use grep_searcher::{BinaryDetection, Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
 use ignore::{DirEntry, WalkBuilder, WalkState};
 use std::ffi::OsString;
@@ -30,7 +28,7 @@ use std::fs::File;
 use std::io::{Read, Write};
 use std::path::{Path, PathBuf};
 use std::sync::Mutex;
-use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
+use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
 use std::time::Instant;
 use termcolor::{Buffer, BufferWriter, ColorChoice, WriteColor};
 
@@ -46,6 +44,7 @@ pub fn run(argv: Vec<OsString>) -> i32 {
             return 2;
         }
     };
+    crate::log::init(args.logging);
     let code = match main(&args) {
         Ok(code) => code,
         Err(e) => {
@@ -173,7 +172,19 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         ignore_silent: args.no_ignore_messages,
         errored: AtomicBool::new(false),
     };
-    // Actions (step 3).
+    // Actions (step 3). `--pcre2-version` is rg's special mode: it wins over everything.
+    if args.pcre2_version {
+        println!("{}", crate::matcher::pcre2_version());
+        return Ok(0);
+    }
+    if let Some(kind) = &args.generate {
+        cli::generate::generate(kind, &mut std::io::stdout().lock())?;
+        return Ok(0);
+    }
+    if args.mcp {
+        crate::mcp::serve(&std::env::current_exe()?)?;
+        return Ok(0);
+    }
     if args.list_grammars {
         return Ok(list_grammars(&msgs));
     }
@@ -209,10 +220,17 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         let cwd = std::env::current_dir()?;
         search::walker(args, &[], &cwd)?;
         Preprocess::new(args, &cwd)?;
-        std::io::stdout()
-            .lock()
-            .write_all(search::RG_TYPES.as_bytes())?;
-        return Ok(0);
+        // The types as `--type-add` and `--type-clear` changed them, and exit 1 without any, as
+        // rg's `types` (review r2 of the M5a plan, Codex #3).
+        let defs = search::file_types(args)?.definitions();
+        let mut out = std::io::stdout().lock();
+        for d in &defs {
+            writeln!(out, "{}: {}", d.name(), d.globs().join(", "))?;
+        }
+        return Ok(if defs.is_empty() { 1 } else { 0 });
+    }
+    if args.files {
+        return files(args, &msgs);
     }
 
     // Patterns and paths (step 4).
@@ -256,7 +274,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
     } else if args.no_filename {
         false
     } else {
-        !one_file
+        args.vimgrep || !one_file
     };
     let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
     let color = match args.color {
@@ -266,8 +284,11 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         ColorWhen::Always => ColorChoice::Always,
         ColorWhen::Ansi => ColorChoice::AlwaysAnsi,
     };
-    // ripgrep 14.1.1 runs `-v --count-matches` as `-v --count` (hiargs.rs:129).
-    let summary_mode = if args.count || (args.count_matches && args.invert_match) {
+    // ripgrep 14.1.1 runs `-v --count-matches` as `-v --count`, and `-o --count` as
+    // `--count-matches` (hiargs.rs:129).
+    let summary_mode = if args.count && args.only_matching {
+        Some(summary::Mode::CountMatches)
+    } else if args.count || (args.count_matches && args.invert_match) {
         Some(summary::Mode::Count)
     } else if args.count_matches {
         Some(summary::Mode::CountMatches)
@@ -279,7 +300,8 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         None
     };
     let context = args.before() > 0 || args.after() > 0;
-    let mode = if args.quiet && args.json && !args.structural() {
+    // `-q --stats` searches every file for its statistics, as `--json -q` (hiargs.rs:163).
+    let mode = if args.quiet && (args.json || args.stats) && !args.structural() {
         Mode::QuietJson
     } else if args.quiet && !args.structural() {
         Mode::Quiet
@@ -290,6 +312,9 @@ fn main(args: &Args) -> anyhow::Result<i32> {
     };
     let layout = if args.json {
         Layout::Json
+    } else if args.vimgrep {
+        // rg's --vimgrep turns the heading off, whatever came before it.
+        Layout::Flat
     } else if args.tree {
         Layout::Tree
     } else if args.flat {
@@ -299,7 +324,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
     } else {
         Layout::Flat
     };
-    let heading = tty;
+    let heading = output::rg::heading(args, tty);
     // The separator between files, which BufferWriter prints between two non-empty file
     // outputs, as ripgrep's. One file needs none, so its output is printed as it grows
     // (`Ctx::spill`), and a closed pipe stops the search as rg's (code review r4 replay).
@@ -307,10 +332,10 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         _ if one_file => None,
         Mode::Passthrough if args.json || summary_mode.is_some() => None,
         Mode::Passthrough if heading => Some(Vec::new()),
-        Mode::Passthrough if context => Some(b"--".to_vec()),
+        Mode::Passthrough if context => args.context_separator.clone(),
         Mode::Trail if summary_mode.is_some() || args.quiet => None,
         Mode::Trail if layout == Layout::Tree => Some(Vec::new()),
-        Mode::Trail if layout == Layout::Flat && context => Some(b"--".to_vec()),
+        Mode::Trail if layout == Layout::Flat && context => args.context_separator.clone(),
         _ => None,
     };
     // With one thread, ripgrep 14.1.1's printer writes it instead, before a file's first line
@@ -326,6 +351,17 @@ fn main(args: &Args) -> anyhow::Result<i32> {
     let separator = separator.filter(|_| line_separator.is_none());
     let mut out = BufferWriter::stdout(color);
     out.separator(separator.clone());
+    // rg flushes each line on a terminal and with --line-buffered (hiargs.rs `stdout`).
+    let line_buffered = match args.buffer {
+        cli::BufferMode::Line => true,
+        cli::BufferMode::Block => false,
+        cli::BufferMode::Auto => tty,
+    };
+    let out = Out {
+        w: out,
+        bytes: AtomicU64::new(0),
+        spill_at: if line_buffered { 1 } else { SPILL },
+    };
     let width = if tty && layout != Layout::Json {
         terminal_size::terminal_size().map_or(args.fragment_width, |(w, _)| w.0 as usize)
     } else {
@@ -361,6 +397,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
         linked: AtomicBool::new(false),
         msgs,
         pre,
+        searched: AtomicBool::new(false),
         decoding: crate::source::Decoding::of(args),
         mapping: crate::source::Mapping::of(args),
         in_memory: search::rg_in_memory(args, &paths, implicit_stdin),
@@ -397,7 +434,9 @@ fn main(args: &Args) -> anyhow::Result<i32> {
                 || search::streaming_searcher(args, false, ctx.in_memory),
                 |s, t| ctx.quiet_json_file(s, &t),
             )?;
-            ctx.print_summary();
+            if args.json {
+                ctx.print_summary();
+            }
         }
         Mode::Passthrough => {
             let line_number =
@@ -425,7 +464,7 @@ fn main(args: &Args) -> anyhow::Result<i32> {
                 context,
                 expand: args.expand,
                 binary: args.binary_choice,
-                look: output::Look::default(),
+                look: output::rg::look(args, &ctx.matcher),
             };
             ctx.walk(
                 || TrailState {
@@ -443,6 +482,17 @@ fn main(args: &Args) -> anyhow::Result<i32> {
             }
         }
     }
+    // rg's `eprint_nothing_searched`: the current directory was searched implicitly and every
+    // file was left out.
+    if ctx.strip_dot && !ctx.searched.load(Relaxed) {
+        ctx.msgs.error(
+            "No files were searched, which means s4 probably applied a filter you didn't \
+             expect.\nRunning with --debug will show why files are being skipped.",
+        );
+    }
+    if args.stats && !args.json {
+        ctx.print_stats_block();
+    }
     let matched = ctx.matched.load(Relaxed);
     let errored = ctx.msgs.errored.load(Relaxed);
     Ok(if matched && (args.quiet || !errored) {
@@ -454,6 +504,152 @@ fn main(args: &Args) -> anyhow::Result<i32> {
     })
 }
 
+/// `--files`: every file the search would read, printed as rg 14.1.1's `files` and
+/// `files_parallel` print them (main.rs): the positional arguments are all paths, the current
+/// directory without one (even with a readable stdin), each file by the path printer (`-0`,
+/// `--path-separator`, `--colors`, `--hyperlink-format`), from the walker's threads through
+/// one printing thread, or in `--sort`'s order from one thread. stdout is flushed per line on a
+/// terminal or with `--line-buffered`, else per block, as rg's. Exit 0 when a file was
+/// found; with `-q`, the first one stops the walk and nothing is printed.
+fn files(args: &Args, msgs: &Messages) -> anyhow::Result<i32> {
+    let mut paths: Vec<PathBuf> = args.pattern.iter().map(PathBuf::from).collect();
+    paths.extend(args.paths.iter().cloned());
+    let implicit = paths.is_empty();
+    if implicit {
+        paths.push(PathBuf::from("./"));
+    }
+    let cwd = std::env::current_dir()?;
+    let (walker, ignore_errors) = search::walker_with(args, &paths, &cwd)?;
+    Preprocess::new(args, &cwd)?;
+    for e in ignore_errors {
+        msgs.ignore(e);
+    }
+    let tty = std::io::IsTerminal::is_terminal(&std::io::stdout());
+    let color = match args.color {
+        ColorWhen::Never => ColorChoice::Never,
+        ColorWhen::Auto if tty => ColorChoice::Auto,
+        ColorWhen::Auto => ColorChoice::Never,
+        ColorWhen::Always => ColorChoice::Always,
+        ColorWhen::Ansi => ColorChoice::AlwaysAnsi,
+    };
+    let line = match args.buffer {
+        cli::BufferMode::Line => true,
+        cli::BufferMode::Block => false,
+        cli::BufferMode::Auto => tty,
+    };
+    let stdout: Box<dyn WriteColor + Send> = if line {
+        Box::new(termcolor::StandardStream::stdout(color))
+    } else {
+        Box::new(termcolor::BufferedStandardStream::stdout(color))
+    };
+    let mut printer = output::rg::path_printer(args, stdout);
+    // A file to list: named on the command line, or a file found by the walk.
+    let listed = |d: &DirEntry| {
+        if let Some(e) = d.error() {
+            msgs.ignore(e);
+        }
+        let is_dir = d.file_type().is_some_and(|t| t.is_dir());
+        (d.depth() == 0 && !is_dir) || d.file_type().is_some_and(|t| t.is_file())
+    };
+    let shown = |p: &Path| -> PathBuf {
+        if implicit {
+            p.strip_prefix("./").unwrap_or(p).to_path_buf()
+        } else {
+            p.to_path_buf()
+        }
+    };
+    let matched = AtomicBool::new(false);
+    let printed: std::io::Result<()> = match args.sort {
+        // Another order than the walker's: every file first, then sorted (rg's `sort`).
+        Some(sort) if !search::walker_sorts(args) => {
+            let mut entries: Vec<DirEntry> = Vec::new();
+            for r in walker.build() {
+                match r {
+                    Ok(d) if listed(&d) => entries.push(d),
+                    Ok(_) => {}
+                    Err(e) => msgs.error(e),
+                }
+            }
+            search::sort_entries(&mut entries, sort);
+            matched.store(!entries.is_empty(), Relaxed);
+            if args.quiet {
+                Ok(())
+            } else {
+                entries
+                    .iter()
+                    .try_for_each(|d| printer.write(&shown(d.path())))
+            }
+        }
+        // The walker's order (`--sort path`, or one thread): printed as found.
+        sort if sort.is_some() || search::walker_threads(args) == 1 => {
+            let mut r = Ok(());
+            for e in walker.build() {
+                match e {
+                    Ok(d) if listed(&d) => {
+                        matched.store(true, Relaxed);
+                        if args.quiet {
+                            break;
+                        }
+                        r = printer.write(&shown(d.path()));
+                        if r.is_err() {
+                            break;
+                        }
+                    }
+                    Ok(_) => {}
+                    Err(e) => msgs.error(e),
+                }
+            }
+            r
+        }
+        // The walker's threads, and one thread that prints (rg's `files_parallel`).
+        _ => {
+            let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
+            std::thread::scope(|scope| {
+                let printing = scope.spawn(move || -> std::io::Result<()> {
+                    for p in rx {
+                        printer.write(&p)?;
+                    }
+                    Ok(())
+                });
+                walker.build_parallel().run(|| {
+                    let tx = tx.clone();
+                    let (listed, shown, matched) = (&listed, &shown, &matched);
+                    Box::new(move |r| match r {
+                        Ok(d) if listed(&d) => {
+                            matched.store(true, Relaxed);
+                            if args.quiet || tx.send(shown(d.path())).is_err() {
+                                WalkState::Quit
+                            } else {
+                                WalkState::Continue
+                            }
+                        }
+                        Ok(_) => WalkState::Continue,
+                        Err(e) => {
+                            msgs.error(e);
+                            WalkState::Continue
+                        }
+                    })
+                });
+                drop(tx);
+                printing.join().expect("the printing thread does not panic")
+            })
+        }
+    };
+    match printed {
+        Err(e) if e.kind() != std::io::ErrorKind::BrokenPipe => return Err(e.into()),
+        _ => {}
+    }
+    let matched = matched.load(Relaxed);
+    let errored = msgs.errored.load(Relaxed);
+    Ok(if matched && (args.quiet || !errored) {
+        0
+    } else if errored {
+        2
+    } else {
+        1
+    })
+}
+
 /// ripgrep 14.1.1's default for line numbers in passthrough (hiargs.rs:200-221): `-n` and `-N`
 /// first, always with `--json`, never with a summary, and else on a tty unless stdin, implicit
 /// or `-`, is all that is searched (hiargs.rs:214 and 1128), so a pipe reads as a pipe.
@@ -466,7 +662,7 @@ fn passthrough_line_number(args: &Args, summary: bool, tty: bool, paths: &[PathB
     } else if args.json {
         true
     } else {
-        !summary && tty && !only_stdin
+        !summary && ((tty && !only_stdin) || output::rg::column(args) || args.vimgrep)
     }
 }
 
@@ -647,7 +843,7 @@ struct Ctx<'a> {
     in_memory: bool,
     with_filename: bool,
     summary: Option<summary::Mode>,
-    out: BufferWriter,
+    out: Out,
     /// Set to stop every worker: `-q` found a Match, or stdout was closed.
     quit: AtomicBool,
     /// A file had a kept Match (or, for --files-without-match, was listed).
@@ -670,6 +866,9 @@ struct Ctx<'a> {
     processors: processors::Processors,
     /// `--pre` and `-z`.
     pre: Preprocess,
+    /// A file was searched: else, with the current directory searched implicitly, rg says
+    /// that no file was searched.
+    searched: AtomicBool,
     /// How files are decoded: by their BOM, or as `-E` says (M5a plan, D10).
     decoding: crate::source::Decoding,
     /// Whether a large file is mapped: never with `--no-mmap` (review r2 of the M5a plan,
@@ -737,6 +936,7 @@ impl<'a> Ctx<'a> {
                 match r {
                     Ok(d) => {
                         if let Some(t) = self.target(&d, &mut above) {
+                            self.searched.store(true, Relaxed);
                             visit(&mut state, self.prepare(t));
                         }
                     }
@@ -756,6 +956,7 @@ impl<'a> Ctx<'a> {
                         Ok(d) => {
                             self.entry_error(&d);
                             if let Some(t) = self.target(&d, &mut above) {
+                                self.searched.store(true, Relaxed);
                                 visit(&mut state, self.prepare(t));
                             }
                         }
@@ -1139,6 +1340,27 @@ impl<'a> Ctx<'a> {
         self.print(&buf);
     }
 
+    /// rg's `--stats` block after the output (main.rs `print_stats`): the statistics of every
+    /// file searched, the bytes printed being those s4 printed in the Trail layouts and grep-
+    /// printer's count with `--no-trail` (M5a plan, D14).
+    fn print_stats_block(&self) {
+        let mut stats = self.stats.lock().expect("stats").clone();
+        stats.elapsed_total = self.started.elapsed();
+        if self.counts_printed() {
+            stats.bytes_printed = self.out.bytes.load(Relaxed);
+        }
+        let mut buf = self.out.buffer();
+        let _ = buf.write_all(output::rg::stats_block(&stats).as_bytes());
+        self.print(&buf);
+    }
+
+    /// The Trail layouts' `--stats` count the bytes s4 prints; grep-printer counts its own.
+    fn counts_printed(&self) -> bool {
+        !self.args.quiet
+            && !self.args.no_trail
+            && (self.summary.is_none() || self.args.structural())
+    }
+
     fn quiet_file(&self, searcher: &mut Searcher, t: &Target) {
         let mut sink = CollectSink::new(&self.matcher, self.args.invert_match, true);
         match self.search_raw(searcher, t, &mut sink) {
@@ -1178,19 +1400,19 @@ impl<'a> Ctx<'a> {
         self.count_quiet_json(&sink.stats, sink.lines > 0);
     }
 
-    /// `--json -q`: adds a searched file to the summary.
+    /// `--json -q` and `--stats`: adds a searched file to the summary, whether it matched or
+    /// not, as grep-printer's sinks count every search.
     fn count_quiet_json(&self, s: &FileStats, matched: bool) {
-        if self.args.json && self.args.quiet {
+        if (self.args.json && self.args.quiet) || (self.args.stats && !self.args.json) {
             let mut stats = output::json::file_stats(s, 0);
             stats.searches_with_match = u64::from(matched);
             *self.stats.lock().expect("stats") += &stats;
         }
     }
 
-    /// grep-printer's printer for the mode, with ripgrep's defaults (design note 9) and its
-    /// `-m`, which grep-printer 0.2.2's printers count.
+    /// grep-printer's printer for the mode, configured as rg 14.1.1 configures it
+    /// (`output::rg`; design note 9), with its `-m`, which grep-printer 0.2.2's printers count.
     fn printer(&self, heading: bool) -> Printer<'_> {
-        let specs = ColorSpecs::new(&grep_printer::default_color_specs());
         let max = search::max_count(self.args);
         if let Some(mode) = self.summary {
             let kind = match mode {
@@ -1199,16 +1421,13 @@ impl<'a> Ctx<'a> {
                 summary::Mode::FilesWithMatches => SummaryKind::PathWithMatch,
                 summary::Mode::FilesWithoutMatch => SummaryKind::PathWithoutMatch,
             };
-            return Printer::Summary(
-                SummaryBuilder::new()
-                    .color_specs(specs)
-                    .kind(kind)
-                    .path(self.with_filename)
-                    .exclude_zero(true)
-                    .separator_field(b":".to_vec())
-                    .max_matches(max)
-                    .build(self.spill_lines()),
-            );
+            return Printer::Summary(output::rg::summary(
+                self.args,
+                kind,
+                self.with_filename,
+                max,
+                self.spill_lines(),
+            ));
         }
         if self.args.json {
             return Printer::Json(
@@ -1217,16 +1436,14 @@ impl<'a> Ctx<'a> {
                     .build(self.spill_lines()),
             );
         }
-        Printer::Standard(
-            StandardBuilder::new()
-                .color_specs(specs)
-                .heading(heading)
-                .path(self.with_filename)
-                .max_columns(self.args.max_columns)
-                .max_matches(max)
-                .separator_search(self.line_separator.clone())
-                .build(self.spill_lines()),
-        )
+        Printer::Standard(output::rg::standard(
+            self.args,
+            heading,
+            self.with_filename,
+            max,
+            self.line_separator.clone(),
+            self.spill_lines(),
+        ))
     }
 
     /// The writer of grep-printer's printers: one file's output, printed as it grows where
@@ -1248,6 +1465,7 @@ impl<'a> Ctx<'a> {
                 let mut sink = p.sink_with_path(&self.matcher, path);
                 let r = self.search_raw(searcher, t, &mut sink);
                 let m = sink.has_match();
+                self.count_stats(&r, sink.stats());
                 (r, m, &p.get_mut().buf)
             }
             Printer::Summary(p) => {
@@ -1255,6 +1473,7 @@ impl<'a> Ctx<'a> {
                 let mut sink = p.sink_with_path(&self.matcher, path);
                 let r = self.search_raw(searcher, t, &mut sink);
                 let m = sink.has_match();
+                self.count_stats(&r, sink.stats());
                 (r, m, &p.get_mut().buf)
             }
             Printer::Json(p) => {
@@ -1262,7 +1481,9 @@ impl<'a> Ctx<'a> {
                 let mut sink = p.sink_with_path(&self.matcher, path);
                 let r = self.search_raw(searcher, t, &mut sink);
                 let m = sink.has_match();
-                if m && r.is_ok() {
+                // grep-printer's JSON sink counts a search it printed a `begin` for: one with a
+                // Match, or with `--passthru` any (json.rs:802).
+                if sink.stats().searches() > 0 && r.is_ok() {
                     *self.stats.lock().expect("stats") += &run_stats(sink.stats());
                 }
                 (r, m, &p.get_mut().buf)
@@ -1293,6 +1514,14 @@ impl<'a> Ctx<'a> {
         self.print(buf);
     }
 
+    /// `--stats` with `--no-trail` or a summary: grep-printer's statistics of the file, for a
+    /// search that did not fail, as rg adds them (main.rs `search`).
+    fn count_stats(&self, r: &std::io::Result<()>, stats: Option<&Stats>) {
+        if let (Ok(()), Some(s)) = (r, stats) {
+            *self.stats.lock().expect("stats") += &run_stats(s);
+        }
+    }
+
     fn trail_file(&self, st: &mut TrailState, t: &Target, detector: &Detector, ro: &RenderOptions) {
         self.trail_file_printed(st, t, detector, ro);
         // A search that failed after its Matches: the error comes after them (`Failure`).
@@ -1317,6 +1546,7 @@ impl<'a> Ctx<'a> {
             && self.summary.is_none()
             && !self.args.quiet
             && t.through.is_none()
+            && output::rg::streams(&ro.look, self.args)
         {
             match detector.detect_by_path(&t.path) {
                 Some(None) if self.plain_file(st, t, ro, detector) => return,
@@ -1331,7 +1561,7 @@ impl<'a> Ctx<'a> {
                 _ => {}
             }
         }
-        let Ok(found) = self.describe(st, t, detector) else {
+        let Ok(found) = self.describe(st, t, detector, ro.look.replace.as_deref()) else {
             return;
         };
         let kept = found
@@ -1387,14 +1617,19 @@ impl<'a> Ctx<'a> {
             return;
         }
         let Some((src, d)) = found else { return };
-        if !kept {
+        // `--passthru` prints a file without a Match, its lines as context, as rg does; it
+        // does not count as matched.
+        let passthru = self.args.passthru && !d.file.items.is_empty();
+        if !kept && !passthru {
             // The summary counts the search's Matches, before the filters (spec §6.4).
             if ro.layout == Layout::Json && failed.is_none() {
                 *self.stats.lock().expect("stats") += &output::json::file_stats(&d.file.stats, 0);
             }
             return;
         }
-        matched();
+        if kept {
+            matched();
+        }
         let written = match ro.layout {
             Layout::Tree => output::tree::render(&d, src.bytes(), ro, buf),
             Layout::Flat => output::flat::render(&d, src.bytes(), ro, self.lead(), buf),
@@ -1843,12 +2078,13 @@ impl<'a> Ctx<'a> {
     /// Steps 6 to 8 for one file: its search (or with --in-headings its names), the Source of a
     /// file with Matches, Trails and filters. Err: an error, already reported. Ok(None):
     /// searched, nothing found. Ok(Some): the search found Matches; the filters may have left
-    /// none.
+    /// none. `replace`: `-r`'s, which an object file's search applies (`object`).
     fn describe(
         &self,
         st: &mut TrailState,
         t: &Target,
         detector: &Detector,
+        replace: Option<&Replacer>,
     ) -> Result<Option<(Source, Described)>, ()> {
         st.binary_quit = false;
         st.failed = None;
@@ -1876,7 +2112,7 @@ impl<'a> Ctx<'a> {
             // What a transform outputs is no object file nor document, whatever its bytes or
             // --format say: Plain (spec §5.9; code review of the M4 plan r1, Codex #11).
             let transformed = matches!(t.through, Some(Through::Pre | Through::Zip));
-            if !transformed && let Some(d) = self.object(st, detector, &src, &fm)? {
+            if !transformed && let Some(d) = self.object(st, detector, &src, &fm, replace)? {
                 return self.filter(st, t, src, d);
             }
             Some({
@@ -1968,13 +2204,14 @@ impl<'a> Ctx<'a> {
     /// Matches of a search of the whole file, each with the Trail of each Match. None: not an
     /// object file goblin reads, or `-v`, whose lines are the non-matching bytes. The search
     /// that found the file's first Match is rg's; this one reads on past the binary data, so it
-    /// costs only a file with a Match (8385).
+    /// costs only a file with a Match (8385). With `replace` (`-r`), its replacements too.
     fn object(
         &self,
         st: &mut TrailState,
         detector: &Detector,
         src: &Source,
         fm: &FileMatches,
+        replace: Option<&Replacer>,
     ) -> Result<Option<Described>, ()> {
         if self.args.invert_match {
             return Ok(None);
@@ -2000,13 +2237,25 @@ impl<'a> Ctx<'a> {
             self.msgs.error(format_args!("{}: {e}", fm.path.display()));
             return Err(());
         }
-        let hits: Vec<std::ops::Range<usize>> = sink
-            .items
-            .iter()
-            .filter_map(|i| match i {
+        let lines = || {
+            sink.items.iter().filter_map(|i| match i {
                 Item::Match(l) => Some(l),
                 Item::Context(_) => None,
             })
+        };
+        // `-r`: the replacement of each Match, made in one pass over the line (with `-U`, the
+        // block) the search reported, as rg's printer makes it: each Match with the groups
+        // the search found, and a line of many strings read once (review r4 of the M5a plan,
+        // Codex #1 and #2).
+        let replacements = replace.map_or_else(Vec::new, |r| {
+            lines()
+                .flat_map(|l| {
+                    let at = l.absolute_offset as usize;
+                    r.replacements(src.bytes(), at..at + l.len)
+                })
+                .collect()
+        });
+        let hits: Vec<std::ops::Range<usize>> = lines()
             .flat_map(|l| {
                 let base = l.absolute_offset as usize;
                 l.submatches
@@ -2050,7 +2299,7 @@ impl<'a> Ctx<'a> {
             trails,
             sections: Vec::new(),
             offsets: true,
-            replacements: Vec::new(),
+            replacements,
         }))
     }
 
@@ -2892,12 +3141,32 @@ fn rewindable(p: &Path) -> bool {
 /// What `Spill` holds before it prints: a file's output is printed in pieces of about this size.
 const SPILL: usize = 64 * 1024;
 
+/// stdout: the BufferWriter every output goes through, the bytes printed (for `--stats`), and
+/// how much `Spill` holds before it prints: `SPILL`, or with line buffering a byte, so each
+/// line goes out as it is written (M5a plan, D13).
+struct Out {
+    w: BufferWriter,
+    bytes: AtomicU64,
+    spill_at: usize,
+}
+
+impl Out {
+    fn buffer(&self) -> Buffer {
+        self.w.buffer()
+    }
+
+    fn print(&self, buf: &Buffer) -> std::io::Result<()> {
+        self.bytes.fetch_add(buf.as_slice().len() as u64, Relaxed);
+        self.w.print(buf)
+    }
+}
+
 /// A thread's output buffer that, with `out`, prints what it holds every `SPILL` bytes, so one
 /// file's output is not held in memory until the file ends (code review r2, Fable #3). Only
 /// where nothing can come between the pieces (`Ctx::spill`).
 struct Spill<'a> {
     buf: &'a mut Buffer,
-    out: Option<&'a BufferWriter>,
+    out: Option<&'a Out>,
     printed: &'a AtomicBool,
 }
 
@@ -2905,7 +3174,7 @@ impl Write for Spill<'_> {
     fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
         let n = self.buf.write(bytes)?;
         if let Some(out) = self.out
-            && self.buf.as_slice().len() >= SPILL
+            && self.buf.as_slice().len() >= out.spill_at
         {
             if stopped() {
                 self.buf.clear();
@@ -2928,6 +3197,14 @@ impl WriteColor for Spill<'_> {
         self.buf.supports_color()
     }
 
+    fn supports_hyperlinks(&self) -> bool {
+        self.buf.supports_hyperlinks()
+    }
+
+    fn set_hyperlink(&mut self, link: &termcolor::HyperlinkSpec) -> std::io::Result<()> {
+        self.buf.set_hyperlink(link)
+    }
+
     fn set_color(&mut self, spec: &termcolor::ColorSpec) -> std::io::Result<()> {
         self.buf.set_color(spec)
     }
@@ -2943,7 +3220,7 @@ impl WriteColor for Spill<'_> {
 /// (`Ctx::spill`).
 struct SpillLines<'a> {
     buf: Buffer,
-    out: Option<&'a BufferWriter>,
+    out: Option<&'a Out>,
     /// The end of the last complete line found in `buf[..scanned]`. The line breaks are looked
     /// for only once `buf` holds `SPILL` bytes: grep-printer's JSON printer writes a message in
     /// many pieces (code review r4, Fable #1).
@@ -2968,14 +3245,20 @@ impl Write for SpillLines<'_> {
         let Some(out) = self.out else {
             return Ok(n);
         };
-        if self.buf.as_slice().len() < SPILL {
+        if self.buf.as_slice().len() < out.spill_at {
             return Ok(n);
         }
-        // What was written before this piece: the line being written stays.
-        if let Some(j) = memchr::memrchr(b'\n', &self.buf.as_slice()[self.scanned..start]) {
+        // What was written before this piece: the line being written stays. Line by line
+        // (`spill_at` 1), this piece's complete lines go too.
+        let upto = if out.spill_at == 1 {
+            self.buf.as_slice().len()
+        } else {
+            start
+        };
+        if let Some(j) = memchr::memrchr(b'\n', &self.buf.as_slice()[self.scanned..upto]) {
             self.line_end = self.scanned + j + 1;
         }
-        self.scanned = start;
+        self.scanned = upto;
         let flushable = self.line_end;
         if flushable > 0 && stopped() {
             self.clear();
@@ -3003,6 +3286,14 @@ impl WriteColor for SpillLines<'_> {
         self.buf.supports_color()
     }
 
+    fn supports_hyperlinks(&self) -> bool {
+        self.buf.supports_hyperlinks()
+    }
+
+    fn set_hyperlink(&mut self, link: &termcolor::HyperlinkSpec) -> std::io::Result<()> {
+        self.buf.set_hyperlink(link)
+    }
+
     fn set_color(&mut self, spec: &termcolor::ColorSpec) -> std::io::Result<()> {
         self.buf.set_color(spec)
     }
```

### Task 9.4: green, the MCP check and the commit

- [ ] **Step 1:** `SEARCHFOR_REQUIRE_REFERENCE=1 cargo test --workspace`: PASS, the comparisons
  with rg included (none skipped).
- [ ] **Step 2:** `cargo build --release`; over a large tree (the probe used
  `~/.cargo/registry/src/index.crates.io-*`, 32 430 files) `hyperfine -N 'rg --files DIR' 's4
  --files DIR'` within 1.1 × (D20); and `diff <(rg --files --sort path DIR) <(s4 --files --sort
  path DIR)` empty.
- [ ] **Step 3:** every failure of Task 9.1, Step 3 now passes; any that does not is a
  difference with rg to be fixed here, or a deviation to write in C11, reported.
- [ ] **Step 4:** the MCP server with a real client (spec §11 #14), with a temporary
  configuration so the user's Claude Code settings are not touched, from the fixture copy
  (`tests/fixtures`):

```bash
cfg=$(mktemp --suffix=.json)
printf '{"mcpServers":{"searchfor":{"command":"%s","args":["--mcp"]}}}\n' "$PWD/target/release/s4" > "$cfg"
(cd tests/fixtures && claude -p "Use the search tool of the searchfor server to find needle-pkg under md. Reply with the path, the line and the trail it returns, nothing else." \
  --mcp-config "$cfg" --strict-mcp-config --allowedTools=mcp__searchfor__search)
rm "$cfg"
```

  The prompt goes before the options: `--allowedTools` takes several values and would take it
  as one. Expected: `md/guide.md`, line 12, `Deploy guide › Install › Linux › bash` (the words
  vary; the three facts must be there; the probe's run answered them). Paste the answer in the
  chunk report.
- [ ] **Step 5:** commit: `Run every ripgrep flag through the pipeline, and serve --mcp`.

---
## Chunk 10: perf ceilings, benchmark, fuzz and binary size

### Task 10.1: the ceilings of tests/perf.rs

**Files:** `tests/perf.rs`.

- [ ] **Step 1:** three ignored tests (D18): `-P` and `-U` with `--no-trail` over 10 MB within
  1.25 × rg; `--files` over 20 000 files within 1.25 × rg; each output flag in the Trail layouts
  over 50 MB of Markdown within 1.25 × the default's time and its RSS + 16 MiB. Their RED was
  the probe's first version: `-P` took 1.55 × rg before D7's per-thread clones, `--files` 6 ×
  before D20's parallel walk. And one test that runs in every suite, `benchmarks_run_isolated`
  (Task 10.3): every `scripts/bench*.sh` sets its own Definitions, Grammars and index and unsets
  both configuration variables before its first `hyperfine`; over M1–M4's scripts it fails
  (`bench.sh` isolates nothing, the others keep `SEARCHFOR_CONFIG_PATH`): RED until Task 10.3:

```diff
diff --git a/tests/perf.rs b/tests/perf.rs
index e6a8a85..818aa8f 100644
--- a/tests/perf.rs
+++ b/tests/perf.rs
@@ -2134,3 +2134,130 @@ fn an_installed_grammar_leaves_a_search_without_documents_unloaded() {
     let o = fx.s4(a).output().unwrap();
     assert_eq!(common::stdout(&o), "9\n", "{}", common::stderr(&o));
 }
+
+/// M5a plan, D7 and Task 10.1: PCRE2 and multi-line searches cost what rg's cost, with
+/// `--no-trail` over a 10 MB file (PCRE2's scratch space per thread: `matcher::Pcre2`).
+#[test]
+#[ignore]
+fn pcre2_and_multiline_without_trails_are_as_fast_as_ripgrep() {
+    let _alone = alone();
+    let Some(rg) = rg_bin() else {
+        return;
+    };
+    let fx = Fixture::new();
+    let line = "line 1234 of text here\n";
+    let file = repeated(&fx, "ten.txt", line, 10 * 1024 * 1024 / line.len());
+    for flags in [&["-P", r"of(?= text)"][..], &["-U", "here\\nline 1"]] {
+        let mut args = vec!["-c"];
+        args.extend(flags);
+        args.push(&file);
+        let Some((base, _)) = best_of_three_bin(&rg, &fx, &args, 0) else {
+            eprintln!("skipping: /usr/bin/time not found");
+            return;
+        };
+        args.splice(0..0, ["--no-docs", "--no-trail"]);
+        let (t, _) = best_of_three(&fx, &args, 0).unwrap();
+        eprintln!("10 MB {flags:?}: s4 {t:?}, rg {base:?}");
+        assert!(
+            t.as_secs_f64() <= base.as_secs_f64() * 1.25,
+            "{t:?} is more than 1.25 × the {base:?} of rg"
+        );
+    }
+}
+
+/// `--files` lists 20 000 files from the walker's threads, as rg does: within 1.25 × rg's
+/// time (the sequential walk of the probe's first version took 6 ×).
+#[test]
+#[ignore]
+fn files_lists_as_fast_as_ripgrep() {
+    let _alone = alone();
+    let Some(rg) = rg_bin() else {
+        return;
+    };
+    let fx = Fixture::new();
+    for a in 0..20 {
+        let d = fx.file(&format!("many/d{a}"));
+        std::fs::create_dir_all(&d).unwrap();
+        for b in 0..1000 {
+            std::fs::write(d.join(format!("f{b}.txt")), "x").unwrap();
+        }
+    }
+    let Some((base, _)) = best_of_three_drained(&rg, &fx, &["--files", "many"], 0) else {
+        eprintln!("skipping: /usr/bin/time not found");
+        return;
+    };
+    let (t, _) = best_of_three_drained(&sfor_bin(), &fx, &["--files", "many"], 0).unwrap();
+    eprintln!("--files over 20 000 files: s4 {t:?}, rg {base:?}");
+    assert!(t.as_secs_f64() <= base.as_secs_f64() * 1.25 + 0.01);
+}
+
+/// M5a plan, D4: the rows of rg's output flags in the Trail layouts cost about what the
+/// default rows cost: a 50 MB Markdown file with one Match in 1 000 lines, each of `--column
+/// -b`, `-o`, `--vimgrep`, `-r` and `--trim` within 1.25 × the default's time and its RSS
+/// plus 16 MiB (`-o`, `--vimgrep` and `-r` take the model's path, `output::rg::streams`).
+#[test]
+#[ignore]
+fn output_flags_in_the_trail_layouts_stay_under_the_ceilings() {
+    let _alone = alone();
+    let fx = Fixture::new();
+    let mut text = String::with_capacity(SIZE + 64);
+    let mut n = 0;
+    while text.len() < SIZE {
+        if n % 1000 == 0 {
+            text.push_str(&format!("# Section {n}\n\nthe needle of {n}\n"));
+        }
+        text.push_str("line of text here\n");
+        n += 1;
+    }
+    std::fs::write(fx.file("big.md"), text).unwrap();
+    let Some((base, base_rss)) = best_of_three(&fx, &["--flat", "needle", "big.md"], 0) else {
+        eprintln!("skipping: /usr/bin/time not found");
+        return;
+    };
+    for flags in [
+        &["--column", "-b"][..],
+        &["-o"],
+        &["--vimgrep"],
+        &["-r", "N"],
+        &["--trim"],
+    ] {
+        let mut args = vec!["--flat"];
+        args.extend(flags);
+        args.extend(["needle", "big.md"]);
+        let (t, rss) = best_of_three(&fx, &args, 0).unwrap();
+        eprintln!("{flags:?}: {t:?} {rss} B, default {base:?} {base_rss} B");
+        assert!(
+            t.as_secs_f64() <= base.as_secs_f64() * 1.25,
+            "{flags:?}: {t:?}"
+        );
+        assert!(rss <= base_rss + (16 << 20), "{flags:?}: {rss} B");
+    }
+}
+
+/// M5a plan, review r1, Codex #5: every benchmark runs isolated, hyperfine's runs too: its own
+/// Definitions, Grammars and index, and neither configuration file, so none reads `~/.config`
+/// or runs a `--pre` of the user's over the corpus.
+#[test]
+fn benchmarks_run_isolated() {
+    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");
+    for name in [
+        "bench.sh",
+        "bench-code.sh",
+        "bench-m3.sh",
+        "bench-m4.sh",
+        "bench-m5.sh",
+    ] {
+        let script = std::fs::read_to_string(dir.join(name)).unwrap();
+        let first_run = script.find("hyperfine --ignore-failure").unwrap();
+        for wanted in [
+            "unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH\n",
+            r#"export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json""#,
+        ] {
+            let at = script.find(wanted);
+            assert!(
+                at.is_some_and(|at| at < first_run),
+                "{name}: no `{wanted}` before its first run"
+            );
+        }
+    }
+}
```

- [ ] **Step 2:** `cargo test --release --test perf -- --ignored --test-threads=1`: PASS, the
  times printed with `--nocapture` go to the chunk report.

### Task 10.2: the fuzz

**Files:** `tests/fuzz_m5.rs` (new).

- [ ] **Step 1:** seeded combinations of rg's output and search flags with s4's own, over the
  fixtures and over files with CRLF lines, NUL records, Latin-1 and long lines; nothing panics,
  every run ends within 10 s, the exit code is 0, 1 or 2 with a message (spec §11 #33). 150 runs
  by default; `SEARCHFOR_FUZZ_SEED` and `SEARCHFOR_FUZZ_N` change the seed and the count:

```rust
//! M5a (M5a plan, Chunk 10): combinations of ripgrep's output and search flags with s4's own,
//! run over the fixtures and over files with CRLF lines, NUL records, Latin-1 and long lines.
//! Nothing may panic, every run ends within 10 s, and the exit code is 0, 1, or 2 with a
//! message. The generator is seeded: `SEARCHFOR_FUZZ_SEED` and `SEARCHFOR_FUZZ_N` (runs)
//! change the seed and the count; a failure prints its command line.
mod common;

use common::Fixture;
use std::time::{Duration, Instant};

/// xorshift64*, as tests/fuzz.rs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// The flags a run picks from, each a group of arguments.
#[rustfmt::skip]
const FLAGS: &[&[&str]] = &[
    &["--flat"], &["--tree"], &["--json"], &["--no-trail"], &["--heading"], &["--no-heading"],
    &["-p"], &["--vimgrep"], &["-o"], &["--column"], &["-b"], &["-n"], &["-N"], &["-H"], &["-I"],
    &["-0"], &["--path-separator", "\\"], &["--trim"], &["-r", "<$0>"], &["-r", ""],
    &["--passthru"], &["-C1"], &["-A2"], &["-B1"], &["--context-separator", "##"],
    &["--no-context-separator"], &["--field-match-separator", "="],
    &["--field-context-separator", "~"], &["-M", "12"], &["--max-columns-preview"], &["-m1"],
    &["-c"], &["--count-matches"], &["-l"], &["--files-without-match"], &["--include-zero"],
    &["-v"], &["-i"], &["-w"], &["-x"], &["-F"], &["-U"], &["--multiline-dotall"], &["-P"],
    &["--engine", "auto"], &["--crlf"], &["--null-data"], &["--no-unicode"],
    &["-E", "latin1"], &["-E", "utf-16le"], &["-E", "none"], &["--stop-on-nonmatch"],
    &["--stats"], &["--sort", "path"], &["--sortr", "modified"], &["--color", "always"],
    &["--colors", "match:fg:blue"], &["--colors", "trail:fg:yellow"],
    &["--hyperlink-format", "default"], &["--within", "Install"], &["--expand"],
    &["--in-headings"], &["--mmap"], &["--no-mmap"], &["--line-buffered"], &["-a"],
    &["--binary"], &["--no-docs"],
];

const PATTERNS: &[&str] = &[
    "needle",
    "needle-\\w+",
    "^",
    "$",
    "e",
    "(?i)NEEDLE",
    "\\n",
    "needle\\n",
    "a|b|c",
    "\\w+$",
    "ñ",
    "Install",
];

fn write_files(fx: &Fixture) {
    let dir = fx.file("m5");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("crlf.md"), b"# Install\r\n\r\na needle\r\nb\r\n").unwrap();
    std::fs::write(
        dir.join("records.dat"),
        b"one needle\ntwo\0three\0needle four\n",
    )
    .unwrap();
    std::fs::write(dir.join("latin.md"), b"# Caf\xe9\n\nneedle caf\xe9\n").unwrap();
    std::fs::write(
        dir.join("long.md"),
        format!(
            "# Long\n\n{} needle {} needle\n",
            "x".repeat(300),
            "ñ".repeat(200)
        ),
    )
    .unwrap();
}

/// The longest a run may take.
const LIMIT: Duration = Duration::from_secs(10);

fn count() -> usize {
    std::env::var("SEARCHFOR_FUZZ_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(150)
}

fn seed() -> u64 {
    std::env::var("SEARCHFOR_FUZZ_SEED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0x5eed_f00d_cafe_0005)
}

#[test]
fn m5_flag_combinations_never_panic() {
    let fx = Fixture::new();
    write_files(&fx);
    let mut rng = Rng(seed());
    let paths: &[&[&str]] = &[
        &["md"],
        &["m5"],
        &["md/guide.md"],
        &["m5/records.dat", "m5/crlf.md"],
        &[],
    ];
    let mut failures = Vec::new();
    for _ in 0..count() {
        let mut args: Vec<&str> = Vec::new();
        for _ in 0..1 + rng.below(5) {
            args.extend_from_slice(FLAGS[rng.below(FLAGS.len())]);
        }
        args.push("-e");
        args.push(PATTERNS[rng.below(PATTERNS.len())]);
        args.push("--");
        args.extend_from_slice(paths[rng.below(paths.len())]);
        let started = Instant::now();
        let mut child = fx
            .s4(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut stderr_pipe = child.stderr.take().unwrap();
        let reader = std::thread::spawn(move || {
            let mut e = String::new();
            let _ = std::io::Read::read_to_string(&mut stderr_pipe, &mut e);
            e
        });
        // A run that does not end within the limit is killed: a hang fails the case, not the
        // test run.
        let status = loop {
            if let Some(st) = child.try_wait().unwrap() {
                break Some(st);
            }
            if started.elapsed() > LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        let took = started.elapsed();
        let err = reader.join().unwrap();
        let code = status.and_then(|s| s.code()).unwrap_or(-1);
        let bad = err.contains("panicked")
            || status.is_none()
            || !matches!(code, 0..=2)
            || (code == 2 && err.is_empty());
        if bad {
            failures.push(format!(
                "s4 {} (exit {code}, {took:?}):\n{err}",
                args.join(" ")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} run(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
```

- [ ] **Step 2:** `cargo test --release --test fuzz_m5`: PASS. Then two long runs,
  `SEARCHFOR_FUZZ_N=2000 SEARCHFOR_FUZZ_SEED=12345` and `SEARCHFOR_FUZZ_N=2000
  SEARCHFOR_FUZZ_SEED=777` (about 200 s each in the probe): PASS. A failure is a bug of the
  chunk that owns the code: its regression test goes beside the code, as C4's
  `a_match_of_a_records_line_break_stays_in_its_row` did for the probe's one finding.

### Task 10.3: the benchmark

**Files:** `scripts/bench-m5.sh` (new), `scripts/bench.sh`, `scripts/bench-code.sh`,
`scripts/bench-m3.sh`, `scripts/bench-m4.sh`.

- [ ] **Step 1:** the script (D18), over M4's corpus generator (`scripts/gen-m4-corpus.py`),
  executable as the other benchmarks (`chmod +x scripts/bench-m5.sh`; the probe's first run
  failed with `Permission denied`):

```bash
#!/usr/bin/env bash
# M5a acceptance (M5a plan, D18), on the corpus of scripts/gen-m4-corpus.py (1 080 documents,
# 300 object files, 20 000 text files), against ripgrep 14.1.1:
#   - with --no-docs and --no-trail, a sparse pattern: `-P`, `-U`, `--column -b -o` and
#     `--passthru`-free output flags print rg's lines, and s4 ≤ 1.1 × rg;
#   - `--files` and `--files --sort path`: the same files as rg over the corpus, and s4 ≤ 1.1 ×
#     rg over a generated tree of 100 000 files, where rg takes tens of milliseconds and a
#     3 ms difference is not 10 % (M5a plan, review r1, Fable #5);
#   - the Trail layouts with rg's output flags (`--column -b`, `-o`, `--vimgrep`, `-r`,
#     `--trim`, `--stats`): ≤ 1.1 × s4 --flat with no flag;
#   - with --no-docs, M5a's s4 against M4's (SEARCHFOR_BASE_BIN, the s4 of 522bf96), the sparse
#     pattern over the whole corpus, `--flat` and `--no-trail`: ≤ 1.03 ×, the median of three
#     alternated series (what M5a adds to a search that uses none of its flags); skipped
#     without SEARCHFOR_BASE_BIN;
#   - the release binary at most 23 MB (8612), its size printed.
# Usage: scripts/bench-m5.sh [CORPUS_DIR]  (generated there when missing; a temporary one by
# default). Needs python3, hyperfine, jq, awk and GNU stat. SEARCHFOR_BIN replaces the s4 under
# test; SEARCHFOR_RG, else /usr/bin/rg, is the reference and must be ripgrep 14.1.1.
set -euo pipefail
export LC_ALL=C
cargo build --release --quiet
s4="${SEARCHFOR_BIN:-./target/release/s4}"
export SEARCHFOR_BIN_PATH="$s4"
rg="${SEARCHFOR_RG:-/usr/bin/rg}"
if ! "$rg" --version 2>/dev/null | head -1 | grep -qx 'ripgrep 14.1.1'; then
  echo "FAIL: the reference $rg is not ripgrep 14.1.1"; exit 1
fi
export SEARCHFOR_RG_PATH="$rg"
iso=$(mktemp -d)
trap 'rm -rf "$iso"' EXIT
mkdir -p "$iso/formats" "$iso/grammars"
export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH
dir="${1:-$iso/corpus}"
# Split in two, so no file holds it, this script included (M3 code review r1).
sparse='zqm4''sparse'
fail=0
[ -d "$dir" ] || python3 "$(dirname "$0")/gen-m4-corpus.py" "$dir" "$sparse"
export SEARCHFOR_BENCH_DIR="$dir" SEARCHFOR_BENCH_PATTERN="$sparse"

median() {
  hyperfine --ignore-failure --warmup 5 --runs 40 --export-json "$iso/$1.json" "$2" "$3" >/dev/null
  if ! jq -e '[.results[].exit_codes[]] | all(. == 0 or . == 1)' "$iso/$1.json" >/dev/null; then
    echo "  FAIL $1: a timed run did not exit 0 or 1"; fail=1
  fi
  A_S=$(jq '.results[0].median' "$iso/$1.json")
  B_S=$(jq '.results[1].median' "$iso/$1.json")
  awk -v a="$B_S" -v b="$A_S" -v n="$1" -v x="${4:-rg}" -v y="${5:-s4}" \
    'BEGIN { printf "%-16s %s %.3f s  %s %.3f s  ratio %.2f\n", n, x, b, y, a, a / b }'
}
within() {
  local f="${1:-1.1}"
  awk -v a="$B_S" -v b="$A_S" -v f="$f" 'BEGIN { exit !(a <= f * b) }' || { echo "  FAIL: over $f × the first"; fail=1; }
}
# same NAME ARGS...: `s4 --no-docs --no-trail ARGS` prints rg's lines (sorted: the threads).
same() {
  local name=$1; shift
  rc=0; "$s4" --color never --no-docs --no-trail "$@" >"$iso/s.out" 2>"$iso/s.err" || rc=$?
  rrc=0; "$rg" --color never "$@" >"$iso/r.out" 2>"$iso/r.err" || rrc=$?
  [ "$rc" = "$rrc" ] && cmp -s <(sort "$iso/s.out") <(sort "$iso/r.out") \
    || { echo "  FAIL $name: not rg's output"; fail=1; }
}

echo "== $dir"
r='"$SEARCHFOR_RG_PATH" --color never'
n='"$SEARCHFOR_BIN_PATH" --color never --no-docs --no-trail'
for flags in "-P" "-U" "--column -b -o" "--vimgrep" "-r X --trim"; do
  # shellcheck disable=SC2086
  same "$flags" $flags -e "$sparse" -- "$dir"
  median "no-trail $flags" "$r $flags -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\"" \
    "$n $flags -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\""; within
done

# --files, unsorted and sorted.
cmp -s <("$s4" --files -- "$dir" | sort) <("$rg" --files -- "$dir" | sort) || { echo "  FAIL --files: not rg's files"; fail=1; }
cmp -s <("$s4" --files --sort path -- "$dir") <("$rg" --files --sort path -- "$dir") || { echo "  FAIL --files --sort path"; fail=1; }
# Timed over a tree of 100 000 small files in 250 directories.
export SEARCHFOR_BENCH_FILES="$iso/files"
python3 - "$SEARCHFOR_BENCH_FILES" <<'PY'
import os, sys
for d in range(250):
    p = os.path.join(sys.argv[1], f"d{d:03}")
    os.makedirs(p)
    for f in range(400):
        with open(os.path.join(p, f"f{f:03}.txt"), "w") as out:
            out.write("x\n")
PY
median files '"$SEARCHFOR_RG_PATH" --files -- "$SEARCHFOR_BENCH_FILES"' '"$SEARCHFOR_BIN_PATH" --files -- "$SEARCHFOR_BENCH_FILES"'; within
median files-sorted '"$SEARCHFOR_RG_PATH" --files --sort path -- "$SEARCHFOR_BENCH_FILES"' \
  '"$SEARCHFOR_BIN_PATH" --files --sort path -- "$SEARCHFOR_BENCH_FILES"'; within

# The Trail layouts with rg's output flags, against the same layout without them.
f='"$SEARCHFOR_BIN_PATH" --color never --flat --no-docs'
for flags in "--column -b" "-o" "--vimgrep" "-r X" "--trim" "--stats"; do
  median "flat $flags" "$f -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\"" \
    "$f $flags -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\"" s4 "s4 $flags"; within
done

if [ -n "${SEARCHFOR_BASE_BIN:-}" ]; then
  # Three series in alternating order, and the median of their ratios (M4 plan r7).
  export SEARCHFOR_BASE_PATH="$SEARCHFOR_BASE_BIN"
  for layout in --flat --no-trail; do
    m4="\"\$SEARCHFOR_BASE_PATH\" --color never $layout --no-docs -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\""
    m5="\"\$SEARCHFOR_BIN_PATH\" --color never $layout --no-docs -e \"\$SEARCHFOR_BENCH_PATTERN\" -- \"\$SEARCHFOR_BENCH_DIR\""
    ratios=""
    for s in 1 2 3; do
      if [ "$s" = 2 ]; then
        median "m5-added-$s" "$m5" "$m4" 's4 M5a' 's4 M4' >/dev/null; x=$(awk -v a="$B_S" -v b="$A_S" 'BEGIN { print b / a }')
      else
        median "m5-added-$s" "$m4" "$m5" 's4 M4' 's4 M5a' >/dev/null; x=$(awk -v a="$B_S" -v b="$A_S" 'BEGIN { print a / b }')
      fi
      ratios="$ratios $x"
    done
    x=$(echo $ratios | tr ' ' '\n' | sort -g | sed -n 2p)
    printf "%-16s s4 M5a / s4 M4 %s, the median of%s: %.3f\n" m5-added "$layout" "$(printf ' %.3f' $ratios)" "$x"
    awk -v r="$x" 'BEGIN { exit !(r <= 1.03) }' || { echo "  FAIL: over 1.03 × M4"; fail=1; }
  done
else
  echo "m5-added         skipped: SEARCHFOR_BASE_BIN, the s4 of 522bf96, is not set"
fi

size=$(stat -c %s "$s4")
printf "binary           %d bytes (%.1f MB), ceiling 23 MB\n" "$size" "$(awk -v s="$size" 'BEGIN { print s / 1e6 }')"
[ "$size" -le 23000000 ] || { echo "  FAIL: over 23 MB"; fail=1; }
exit "$fail"
```

  `--files` is timed over a generated tree of 100 000 files, where rg takes tens of
  milliseconds: over the corpus (20 000 files, 31 ms) the probe measured 1.10 for 3 ms, a gate
  inside the noise (review r1, Fable #5). The corpus still checks that the files are rg's.
- [ ] **Step 1b:** M1–M4's benchmarks run isolated as `bench-m5.sh` does, hyperfine's runs
  included (review r1, Codex #5): `bench.sh` gains the block, the other three unset
  `SEARCHFOR_CONFIG_PATH` with `RIPGREP_CONFIG_PATH`; `cargo test --test perf
  benchmarks_run_isolated`: PASS:

```diff
diff --git a/scripts/bench.sh b/scripts/bench.sh
index 5c40d41..58c594f 100755
--- a/scripts/bench.sh
+++ b/scripts/bench.sh
@@ -12,6 +12,13 @@ notes_pattern="${2:-ExecStart}"
 binary_dir="${3:-/usr/lib/x86_64-linux-gnu}"
 binary_pattern="${4:-zzqq-searchfor-bench}"
 cargo build --release --quiet
+# Isolation (M5a plan, review r1, Codex #5): no user Definitions or Grammars, no network, no
+# configuration file of rg or of s4, for hyperfine's runs too.
+iso=$(mktemp -d)
+trap 'rm -rf "$iso"' EXIT
+mkdir -p "$iso/formats" "$iso/grammars"
+export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
+unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH
 
 # bench NAME DIR PATTERN: both ratios on one corpus; exits 1 when one is over its limit.
 bench() {
```

```diff
diff --git a/scripts/bench-code.sh b/scripts/bench-code.sh
index 6f8199b..3bffa2e 100755
--- a/scripts/bench-code.sh
+++ b/scripts/bench-code.sh
@@ -23,7 +23,7 @@ iso=$(mktemp -d)
 trap 'rm -rf "$iso"' EXIT
 mkdir -p "$iso/formats" "$iso/grammars"
 export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
-unset RIPGREP_CONFIG_PATH
+unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH
 # The code Formats of M2, for the parsing budget (Fable #7): rg's types for them.
 code_types=(-t php -t js -t ts -t sh -t go -t rust -t py -t json -t yaml -t toml)
 # ripgrep 14.1.1's default: the available parallelism, at most 12 (search.rs, walker_threads).
```

```diff
diff --git a/scripts/bench-m3.sh b/scripts/bench-m3.sh
index e10e83a..d1dd773 100755
--- a/scripts/bench-m3.sh
+++ b/scripts/bench-m3.sh
@@ -26,7 +26,7 @@ iso=$(mktemp -d)
 trap 'rm -rf "$iso"' EXIT
 mkdir -p "$iso/formats" "$iso/grammars"
 export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
-unset RIPGREP_CONFIG_PATH
+unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH
 threads=$(nproc); [ "$threads" -gt 12 ] && threads=12
 dir="${1:-$iso/corpus}"
 # `none` is split in two, so no file holds it, this script included (M3 code review r1).
```

```diff
diff --git a/scripts/bench-m4.sh b/scripts/bench-m4.sh
index 2e37d9a..2464145 100755
--- a/scripts/bench-m4.sh
+++ b/scripts/bench-m4.sh
@@ -49,7 +49,7 @@ mkdir -p "$iso/formats" "$iso/grammars"
 export SEARCHFOR_FORMATS_DIR="$iso/formats" SEARCHFOR_GRAMMARS_DIR="$iso/grammars" SEARCHFOR_GRAMMAR_INDEX="file://$iso/none.json"
 # M3's sfor (SEARCHFOR_BASE_BIN) reads the same overrides under their names of then.
 export SFOR_FORMATS_DIR="$iso/formats" SFOR_GRAMMARS_DIR="$iso/grammars" SFOR_GRAMMAR_INDEX="file://$iso/none.json"
-unset RIPGREP_CONFIG_PATH
+unset RIPGREP_CONFIG_PATH SEARCHFOR_CONFIG_PATH
 threads=$(nproc); [ "$threads" -gt 12 ] && threads=12
 dir="${1:-$iso/corpus}"
 # Split in two, so no file holds them, this script included (M3 code review r1).
```

- [ ] **Step 2:** M4's binary for the `base` case, built from `522bf96` in a worktree of its
  own:

```bash
git worktree add /tmp/s4-m4 522bf96
(cd /tmp/s4-m4 && CARGO_TARGET_DIR=/tmp/s4-m4-target cargo build --release --quiet)
SEARCHFOR_BASE_BIN=/tmp/s4-m4-target/release/s4 scripts/bench-m5.sh
```

  Expected: exit 0, every ratio within its ceiling, and the binary's size printed (D22: about
  21.0 MB). On a busy machine run it again before reading a ratio above its ceiling as a
  regression; three runs above it, `--files` and `--files --sort path` included, are a finding
  for the chunk report.
- [ ] **Step 3:** M1–M4's benchmarks with M5a's binary, each isolated by Step 1b (no
  configuration file and no Definition or Grammar of the user's reaches s4, rg or hyperfine's
  runs): `PATH=/usr/bin:$PATH scripts/bench.sh`, `scripts/bench-code.sh`,
  `scripts/bench-m3.sh`, `scripts/bench-m4.sh` (spec §11 #26), their ratios in the chunk report. Two failures the probe saw that are not
  M5a's, to tell apart from a regression by running M4's binary (`SEARCHFOR_BIN=/tmp/s4-m4-target/release/s4`)
  beside it:
  - `bench-code.sh`'s second corpus, `~/robotin/apps`, holds a `.docx` and a `.pdf` whose text
    matches `TODO`, so `--no-trail` prints two lines more than rg (M4's documents by default;
    M4's binary prints the same 669 lines). The check passes when that tree has no matching
    document; otherwise the report says so.
  - `bench-m4.sh`'s `noext-passthru` and `noext-L-passthru` gates (1.05 × `--no-docs`) gave
    1.05–1.06 with M5a's binary; 40 runs of each binary side by side gave M4 1.05 ± 0.05 and
    M5a 1.06 ± 0.06 for the same pair, so the cost is M4's and the gate sits at the noise (the
    M4 plan said so of its own gates). A ratio above M4's by more than its ± is a regression.
- [ ] **Step 4:** `git worktree remove /tmp/s4-m4`; commit: `Measure M5a: ceilings, fuzz and
  benchmark`.

---
## Chunk 11: rg-compat, README and skill

### Task 11.1: the deviations of M5a

**Files:** `src/cli/rg_flags.rs`, `docs/rg-compat.md`.

- [ ] **Step 1:** the test `m5_deviations_are_known` (the diff's `mod tests` part): it FAILS,
  the sections are missing: RED.
- [ ] **Step 2:** the sections (D3, D4, D6, D13–D16, D7's PCRE2; an object file's `-r`, `-o`'s
  rows and the document reader's map under `--no-mmap`, review r2; an object file's `\K` and
  `-U --vimgrep`'s rows, review r3; an object file's replacements in one pass per line, and the
  `\n` that ends every row with `--crlf` and `--null-data`, review r4) in `DEVIATIONS`, so
  `docs/rg-compat.md` says what each M5a flag prints in the Trail layouts and where s4's differ
  from rg's:

````diff
diff --git a/src/cli/rg_flags.rs b/src/cli/rg_flags.rs
index 6360a17..9de1a78 100644
--- a/src/cli/rg_flags.rs
+++ b/src/cli/rg_flags.rs
@@ -316,8 +316,13 @@ Mach-O file that it searches with a Match (a file named on the command line, or
 Matches instead, at the hex offset of its first Match and with its Trail (spec §5.7), and so
 searches the whole file: in `--json`, `bytes_searched` is the file's size, `matched_lines` the
 rows and `matches` the Matches, and `binary_offset` is null. `-c`, `-l`, `-q` and `--no-trail`
-print what rg prints. With `-v` the file is printed as rg prints it. A walked object file
-without `--binary` stops at its first NUL, as in rg, and prints nothing (M4 plan, D4):
+print what rg prints. With `-v` the file is printed as rg prints it. `-r` replaces each Match
+where the search found it in the file's bytes, so its anchors and groups read the bytes around
+it, and shows the control bytes of the replacement as `.`, as those of a Match (review r2 of
+the M5a plan, Codex #2); the replacements are made in one pass over each line of those bytes,
+as `rg -a` makes them, so a `-P` Match after `\\K` is replaced too, with the groups the search
+found (review r3, Codex #3; review r4, Codex #2). A walked object file without `--binary` stops at its first NUL, as in
+rg, and prints nothing (M4 plan, D4):
 
 ```sh
 s4 --flat version /usr/bin/ls      # rows; rg: binary file matches
@@ -333,7 +338,9 @@ it (spec §5.8), in a terminal and in a pipe alike (the Hilo, 18/10/2026, 8750).
 so where the search finds a document `--no-trail` prints more than rg, and `bytes_searched`
 counts the text. The cost differs too: every document of the search is extracted, a Match in
 it or not, in a child process with its own time and memory limits, so a tree with many
-documents takes longer than rg over it. The search finds a document by its name, and a file
+documents takes longer than rg over it. That child maps a document of 1 MiB or more even with
+`--no-mmap`, which keeps every other file unmapped: a document truncated while it is read can
+end the child, never s4. The search finds a document by its name, and a file
 without an extension by its first bytes.
 
 `--no-docs` turns that off: the documents are searched as rg searches them, at rg's cost, and
@@ -345,6 +352,90 @@ s4 --no-trail needle report.docx            # the lines of the document's text w
 s4 --no-docs --no-trail needle report.docx  # what rg prints for the bytes of the zip
 s4 --no-docs --docs needle report.docx      # the document's text again
 ```
+
+### ripgrep's output flags in the Trail layouts
+
+With `--no-trail` (and with `-c`, `--count-matches`, `-l`, `--files-without-match` and
+`--files`), ripgrep's own printers print, configured as rg 14.1.1 configures them, so every
+output flag prints what rg prints. In `--flat` and `--tree` the same flags change s4's rows
+(M5a plan, D4):
+
+- `--column` and `-b` add the column of the row's first Match and the row's byte offset after
+  its line number (`path:12:13:96: Trail │ text`, in the tree `12:13:96:text`); a row is the
+  part of a line under one Trail, so its column is its own first Match's, where rg's is the
+  line's.
+- `-o` prints a row per Match, its text the matched bytes, under that Match's Trail; `--vimgrep`
+  prints a row per Match with its line, `path:line:column:`, in the one-line layout whatever the
+  layout flags say (rg turns its heading off).
+- `-r` prints each line with its Matches replaced and the row's replacements highlighted;
+  `--trim` leaves out the line's leading white space; `--max-columns-preview` prints the start
+  of a line `-M` omits.
+- `--field-match-separator`, `--field-context-separator`, `--context-separator` and
+  `--no-context-separator` replace `:`, `-` and `--`; `-0` puts a NUL after each path (in the
+  tree, after the header's path in place of its line break); `--path-separator` replaces `/`.
+- `--colors` colours the path, the numbers and the Matches as in rg, and also takes the type
+  `trail`, the Trail's colour (`trail:fg:yellow`), which rg does not know.
+- `--hyperlink-format` makes the path and the numbers of a row a link on a terminal with colour,
+  as rg's.
+- `--heading` and `--pretty` choose the tree and `--no-heading` the one-line layout, as `--tree`
+  and `--flat` do; of the four, the last one given wins.
+
+`--json` prints rg's JSON Lines, which ignore these flags as rg's do.
+
+```sh
+s4 --flat --column -b needle-pkg md/guide.md   # 12:13:96: Deploy guide › Install › Linux › bash │ apt install needle-pkg
+s4 --flat -o 'needle-\\w+' md/guide.md          # one row per Match, its text the Match
+```
+
+### A `-U` Match and a `--null-data` record in the Trail layouts
+
+rg prints a Match that spans lines (`-U`) and a NUL-terminated record (`--null-data`) as it is,
+the record's line breaks inside the printed line. In `--flat` and `--tree` each line of them is a
+row: a `-U` Match's lines with their own numbers, as rg numbers them, a record's lines with the
+record's number (M5a plan, D6). `-o` prints each line of a Match's text that is not empty, as
+rg's `-o` does with `-U`. Without `-U`, or with a pattern that cannot match a line break, it
+prints a row for every Match, as rg's `-o` prints a line's: an empty Match or an empty
+replacement is an empty row, and a `\\r` without `--crlf` is part of the text (review r2 of the
+M5a plan, Codex #4). `--vimgrep` prints the first line of each Match; when the pattern can match
+a line break, an empty Match at the start of a line prints nothing, as rg's printer of several
+lines skips it (`s4 --flat -U --vimgrep '^'` prints nothing; review r3, Codex #4). `--flat` and
+`--tree` end every row with `\\n`, with `--crlf` and `--null-data` too, where rg ends each line
+with the search's terminator (`\\r\\n`, NUL); `--no-trail` prints rg's (review r4, Fable #1).
+
+```sh
+s4 --flat -U 'Linux\\n\\nRun' block.md    # 5: Install │ ## Linux / 6: Install │ / 7: Install │ Run…
+```
+
+### `--stats` in the Trail layouts
+
+After `--flat`, `--tree` or s4's summaries, `--stats` prints rg's block, with every file the
+search read: the bytes printed are those s4 printed, the times s4's. With it, every file goes
+through the Trail pipeline's model, which counts its search (M5a plan, D14).
+
+### stdin in the Trail layouts
+
+A file whose Format its name does not decide, stdin among them, is read whole before its first
+row is printed: the Format may be told by its first bytes. `tail -f log | s4 error` prints
+when the input ends; `--no-trail` prints each line as rg does, and with `--line-buffered`, or on
+a terminal, as soon as it is found.
+
+### `--debug`, `--trace` and `--generate`
+
+`--debug` and `--trace` print the messages of the crates s4 shares with rg (ignore, globset,
+grep-regex, grep-searcher) in rg's format, `s4: DEBUG|target|file:line: message`; s4's own
+messages differ from rg's. `--generate` writes s4's man page and completions, from s4's own
+flags, with clap_mangen and clap_complete (M5a plan, D15, D16).
+
+### The configuration file
+
+s4 reads `SEARCHFOR_CONFIG_PATH` where rg reads `RIPGREP_CONFIG_PATH`, in rg's format (one
+argument per line, `#` comments), before the command line; `--no-config` skips it. Without the
+variable nothing is read (M5a plan, D3).
+
+### PCRE2
+
+`-P` searches with the PCRE2 that s4 is built with (10.43, in its release binaries), which
+`--pcre2-version` prints; an rg built against the system's PCRE2 prints that one's.
 ";
 
 #[cfg(test)]
@@ -510,6 +601,27 @@ mod tests {
         assert!(!md.contains("Without `--docs`"));
     }
 
+    /// M5a plan, D3, D4, D6, D14–D16: what the M5 flags print in the Trail layouts and where
+    /// s4's differ from rg's.
+    #[test]
+    fn m5_deviations_are_known() {
+        let md = render_markdown();
+        for section in [
+            "\n### ripgrep's output flags in the Trail layouts\n",
+            "\n### A `-U` Match and a `--null-data` record in the Trail layouts\n",
+            "\n### `--stats` in the Trail layouts\n",
+            "\n### stdin in the Trail layouts\n",
+            "\n### `--debug`, `--trace` and `--generate`\n",
+            "\n### The configuration file\n",
+            "\n### PCRE2\n",
+        ] {
+            assert!(md.contains(section), "{section}");
+        }
+        assert!(md.contains("SEARCHFOR_CONFIG_PATH"));
+        // Review r4 of the plan, Fable #1: the rows' terminator with `--crlf`.
+        assert!(md.contains("end every row with `\\n`"));
+    }
+
     #[test]
     fn usage_errors_are_a_known_deviation() {
         // Code review m4-code-r3, Fable #3.
````

- [ ] **Step 3:** `cargo test --lib rg_compat_doc -- --ignored` regenerates
  `docs/rg-compat.md`; `cargo test --lib cli::rg_flags`: PASS (`rg_compat_doc_is_current`,
  `every_ripgrep_flag_is_implemented`, `m5_deviations_are_known`). The regenerated file's diff
  is the sections above:

````diff
diff --git a/docs/rg-compat.md b/docs/rg-compat.md
index f33e906..a2d2444 100644
--- a/docs/rg-compat.md
+++ b/docs/rg-compat.md
@@ -267,8 +267,13 @@ Mach-O file that it searches with a Match (a file named on the command line, or
 Matches instead, at the hex offset of its first Match and with its Trail (spec §5.7), and so
 searches the whole file: in `--json`, `bytes_searched` is the file's size, `matched_lines` the
 rows and `matches` the Matches, and `binary_offset` is null. `-c`, `-l`, `-q` and `--no-trail`
-print what rg prints. With `-v` the file is printed as rg prints it. A walked object file
-without `--binary` stops at its first NUL, as in rg, and prints nothing (M4 plan, D4):
+print what rg prints. With `-v` the file is printed as rg prints it. `-r` replaces each Match
+where the search found it in the file's bytes, so its anchors and groups read the bytes around
+it, and shows the control bytes of the replacement as `.`, as those of a Match (review r2 of
+the M5a plan, Codex #2); the replacements are made in one pass over each line of those bytes,
+as `rg -a` makes them, so a `-P` Match after `\K` is replaced too, with the groups the search
+found (review r3, Codex #3; review r4, Codex #2). A walked object file without `--binary` stops at its first NUL, as in
+rg, and prints nothing (M4 plan, D4):
 
 ```sh
 s4 --flat version /usr/bin/ls      # rows; rg: binary file matches
@@ -284,7 +289,9 @@ it (spec §5.8), in a terminal and in a pipe alike (the Hilo, 18/10/2026, 8750).
 so where the search finds a document `--no-trail` prints more than rg, and `bytes_searched`
 counts the text. The cost differs too: every document of the search is extracted, a Match in
 it or not, in a child process with its own time and memory limits, so a tree with many
-documents takes longer than rg over it. The search finds a document by its name, and a file
+documents takes longer than rg over it. That child maps a document of 1 MiB or more even with
+`--no-mmap`, which keeps every other file unmapped: a document truncated while it is read can
+end the child, never s4. The search finds a document by its name, and a file
 without an extension by its first bytes.
 
 `--no-docs` turns that off: the documents are searched as rg searches them, at rg's cost, and
@@ -296,3 +303,87 @@ s4 --no-trail needle report.docx            # the lines of the document's text w
 s4 --no-docs --no-trail needle report.docx  # what rg prints for the bytes of the zip
 s4 --no-docs --docs needle report.docx      # the document's text again
 ```
+
+### ripgrep's output flags in the Trail layouts
+
+With `--no-trail` (and with `-c`, `--count-matches`, `-l`, `--files-without-match` and
+`--files`), ripgrep's own printers print, configured as rg 14.1.1 configures them, so every
+output flag prints what rg prints. In `--flat` and `--tree` the same flags change s4's rows
+(M5a plan, D4):
+
+- `--column` and `-b` add the column of the row's first Match and the row's byte offset after
+  its line number (`path:12:13:96: Trail │ text`, in the tree `12:13:96:text`); a row is the
+  part of a line under one Trail, so its column is its own first Match's, where rg's is the
+  line's.
+- `-o` prints a row per Match, its text the matched bytes, under that Match's Trail; `--vimgrep`
+  prints a row per Match with its line, `path:line:column:`, in the one-line layout whatever the
+  layout flags say (rg turns its heading off).
+- `-r` prints each line with its Matches replaced and the row's replacements highlighted;
+  `--trim` leaves out the line's leading white space; `--max-columns-preview` prints the start
+  of a line `-M` omits.
+- `--field-match-separator`, `--field-context-separator`, `--context-separator` and
+  `--no-context-separator` replace `:`, `-` and `--`; `-0` puts a NUL after each path (in the
+  tree, after the header's path in place of its line break); `--path-separator` replaces `/`.
+- `--colors` colours the path, the numbers and the Matches as in rg, and also takes the type
+  `trail`, the Trail's colour (`trail:fg:yellow`), which rg does not know.
+- `--hyperlink-format` makes the path and the numbers of a row a link on a terminal with colour,
+  as rg's.
+- `--heading` and `--pretty` choose the tree and `--no-heading` the one-line layout, as `--tree`
+  and `--flat` do; of the four, the last one given wins.
+
+`--json` prints rg's JSON Lines, which ignore these flags as rg's do.
+
+```sh
+s4 --flat --column -b needle-pkg md/guide.md   # 12:13:96: Deploy guide › Install › Linux › bash │ apt install needle-pkg
+s4 --flat -o 'needle-\w+' md/guide.md          # one row per Match, its text the Match
+```
+
+### A `-U` Match and a `--null-data` record in the Trail layouts
+
+rg prints a Match that spans lines (`-U`) and a NUL-terminated record (`--null-data`) as it is,
+the record's line breaks inside the printed line. In `--flat` and `--tree` each line of them is a
+row: a `-U` Match's lines with their own numbers, as rg numbers them, a record's lines with the
+record's number (M5a plan, D6). `-o` prints each line of a Match's text that is not empty, as
+rg's `-o` does with `-U`. Without `-U`, or with a pattern that cannot match a line break, it
+prints a row for every Match, as rg's `-o` prints a line's: an empty Match or an empty
+replacement is an empty row, and a `\r` without `--crlf` is part of the text (review r2 of the
+M5a plan, Codex #4). `--vimgrep` prints the first line of each Match; when the pattern can match
+a line break, an empty Match at the start of a line prints nothing, as rg's printer of several
+lines skips it (`s4 --flat -U --vimgrep '^'` prints nothing; review r3, Codex #4). `--flat` and
+`--tree` end every row with `\n`, with `--crlf` and `--null-data` too, where rg ends each line
+with the search's terminator (`\r\n`, NUL); `--no-trail` prints rg's (review r4, Fable #1).
+
+```sh
+s4 --flat -U 'Linux\n\nRun' block.md    # 5: Install │ ## Linux / 6: Install │ / 7: Install │ Run…
+```
+
+### `--stats` in the Trail layouts
+
+After `--flat`, `--tree` or s4's summaries, `--stats` prints rg's block, with every file the
+search read: the bytes printed are those s4 printed, the times s4's. With it, every file goes
+through the Trail pipeline's model, which counts its search (M5a plan, D14).
+
+### stdin in the Trail layouts
+
+A file whose Format its name does not decide, stdin among them, is read whole before its first
+row is printed: the Format may be told by its first bytes. `tail -f log | s4 error` prints
+when the input ends; `--no-trail` prints each line as rg does, and with `--line-buffered`, or on
+a terminal, as soon as it is found.
+
+### `--debug`, `--trace` and `--generate`
+
+`--debug` and `--trace` print the messages of the crates s4 shares with rg (ignore, globset,
+grep-regex, grep-searcher) in rg's format, `s4: DEBUG|target|file:line: message`; s4's own
+messages differ from rg's. `--generate` writes s4's man page and completions, from s4's own
+flags, with clap_mangen and clap_complete (M5a plan, D15, D16).
+
+### The configuration file
+
+s4 reads `SEARCHFOR_CONFIG_PATH` where rg reads `RIPGREP_CONFIG_PATH`, in rg's format (one
+argument per line, `#` comments), before the command line; `--no-config` skips it. Without the
+variable nothing is read (M5a plan, D3).
+
+### PCRE2
+
+`-P` searches with the PCRE2 that s4 is built with (10.43, in its release binaries), which
+`--pcre2-version` prints; an rg built against the system's PCRE2 prints that one's.
````

### Task 11.2: README, README.es and the skill

**Files:** `README.md`, `README.es.md`, `skills/searchfor/SKILL.md`.

- [ ] **Step 1:** README: every rg flag implemented, the MCP server (a Match per Trail,
  `truncated_by`, the first 20 files of long lines named) and how to add it to an agent, PCRE2,
  the configuration file, `--generate`:

````diff
diff --git a/README.md b/README.md
index 2b46ce4..64d45d0 100644
--- a/README.md
+++ b/README.md
@@ -33,15 +33,16 @@ and `searchfor::docs::fixture`), with the same contents. The examples over `/bin
 
 ## Status
 
-**v0.4: Markdown, configuration files, code, markup, tables, subtitles, object files and
-documents.** The search engine is ripgrep's own crates, so the search behaves like
-`rg`; on top of it, Markdown, YAML, JSON, TOML, code in eight languages, HTML, XML, CSV, TSV,
+**v0.5: every ripgrep flag, and an MCP server.** The search engine is ripgrep's own crates, so
+the search behaves like `rg`, and every flag of ripgrep 14.1.1 works (see
+[ripgrep compatibility](#ripgrep-compatibility)); on top of it, Markdown, YAML, JSON, TOML, code in eight languages, HTML, XML, CSV, TSV,
 SubRip and WebVTT get Trails, and so do the matches ripgrep finds in ELF, PE and Mach-O binaries
 (see [Formats](#formats)). PDF, Word, Excel, PowerPoint, OpenDocument, EPUB and SQLite files
 are searched in their text, with its structure, by default; `--no-docs` turns that off (see
 [Documents](#documents)). Long lines are shown as Fragments, and
-`--within`, `--expand` and `--in-headings` filter and expand by structure. ripgrep's `--pre`,
-`--pre-glob` and `-z` work as in ripgrep. Other languages
+`--within`, `--expand` and `--in-headings` filter and expand by structure. `s4 --mcp` serves
+the search to AI agents over the Model Context Protocol (see [For AI agents](#for-ai-agents)).
+Other languages
 can be described by a [Helper Processor](#helper-processors), a separate program that s4
 talks to. Every other file is **Plain**: its matches print with an empty Trail. More Formats
 come by milestone (see [Roadmap](#roadmap)).
@@ -56,8 +57,8 @@ The glossary is in [CONTEXT.md](CONTEXT.md), the design in
 cargo install searchfor
 ```
 
-That installs the latest version published on crates.io. Until v0.4.0 is published there,
-install v0.4 from the repository:
+That installs the latest version published on crates.io. Until v0.5.0 is published there,
+install v0.5 from the repository:
 
 ```
 cargo install --git https://github.com/neitanod/searchfor
@@ -425,6 +426,7 @@ ripgrep's `-M/--max-columns` keeps its meaning: with it, long lines print ripgre
 | `--export-formats <DIR>` | write the built-in Format Definitions to `DIR` |
 | `--list-grammars` | list the compiled-in, installed and available Grammars (see [Grammars](#grammars)) |
 | `--install-grammar <LANG>` | install a Grammar Helper from the Grammar index |
+| `--mcp` | serve the search as an MCP server over stdin and stdout (see [For AI agents](#for-ai-agents)) |
 
 Format Definitions are TOML files. A file in `searchfor/formats/` under the user config
 directory (`~/.config` or `$XDG_CONFIG_HOME` on Linux) with the name of a built-in or
@@ -432,16 +434,36 @@ installed Format replaces it, and a new name adds a Format.
 
 ### ripgrep compatibility
 
-ripgrep 14.1.1 has 146 flag names (104 flags, 30 `--no-` negations and 12 other negations).
-v0.4 implements 56 of them. The other 90 are accepted by name and fail
-with `s4: --<flag> is not supported yet` and exit code 2, so nothing is silently ignored.
-The table is [docs/rg-compat.md](docs/rg-compat.md). Exit codes are ripgrep's: 0 when
-something matched, 1 when nothing did, 2 on error.
+ripgrep 14.1.1 has 146 flag names (104 flags, 30 `--no-` negations and 12 other negations),
+and s4 implements all of them; the table is [docs/rg-compat.md](docs/rg-compat.md). Exit codes
+are ripgrep's: 0 when something matched, 1 when nothing did, 2 on error. `SEARCHFOR_CONFIG_PATH`
+names a configuration file in the format of ripgrep's `RIPGREP_CONFIG_PATH`, and `--no-config`
+skips it.
 
 With `--no-trail`, the output is written by ripgrep's own printer, and `--json --no-trail`
 prints the same messages as `rg --json`, timings aside. Documents are the exception: add
 `--no-docs` for ripgrep's output over them too.
 
+ripgrep's output flags also change the Trail layouts: `--column` and `-b` add the column and the
+byte offset, `-o` prints each match alone under its own Trail, `--vimgrep` a line per match for
+an editor, `-r` replaces the matches, `-U` prints each line of a match that spans lines, and
+the separators, `-0`, `--path-separator`, `--colors` (with a `trail` type of its own) and
+`--hyperlink-format` work as in ripgrep. `--heading` is the tree and `--no-heading` the one-line
+layout:
+
+```
+$ s4 --flat --column -o 'needle-\w+' md/guide.md
+7:22: Deploy guide › Install │ needle-intro
+12:13: Deploy guide › Install › Linux › bash │ needle-pkg
+16:12: Deploy guide › Install › Linux › step one │ needle-item
+23:10: Deploy guide › Install › Plans › Price │ needle-10
+…
+$ s4 --vimgrep needle-pkg md
+md/guide.md:12:13: Deploy guide › Install › Linux › bash │ apt install needle-pkg
+$ s4 -P --flat 'needle-(?=pkg)' md
+md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
+```
+
 `--pre`, `--pre-glob`, `--no-pre`, `-z` and `--no-search-zip` work as in ripgrep 14.1.1, errors
 and exit codes included, and of `-z` and `--pre` the one given last wins. What they output is
 searched as stdin is, with the Trails of the Format that the file's path names. Here `pre.sh`,
@@ -462,6 +484,12 @@ Known differences with ripgrep 14.1.1:
 - Documents are searched in their text by default, also with `--no-trail`, `-c`, `-l`, `-q`
   and `--json`, and each one is extracted, which costs more than ripgrep's search of its bytes.
   `--no-docs` gives ripgrep's output and cost (see [Documents](#documents)).
+- In the Trail layouts, a file whose Format its name does not decide, stdin among them, is read
+  whole before its rows are printed; `--no-trail` prints each line as ripgrep does.
+- `--debug` prints the messages of ripgrep's crates as ripgrep does, and s4's own; `--generate`
+  writes s4's man page and completions; `-P` uses the PCRE2 s4 is built with.
+
+The full list is in [docs/rg-compat.md](docs/rg-compat.md#known-deviations).
 
 ## Helper Processors
 
@@ -547,6 +575,33 @@ An agent that finds a match with `rg` usually opens the whole file to learn whic
 line belongs to. `s4` prints that section in the Trail, and `--expand` prints the section
 itself, so the agent reads the part of the file that matters and nothing else.
 
+### MCP server
+
+`s4 --mcp` speaks the Model Context Protocol over stdin and stdout and offers one tool,
+`search`, with `pattern`, `paths`, `within`, `expand`, `glob`, `case_insensitive` and
+`max_results` (100 when not given, at most 1 000). It answers with each match's path, line,
+Trail and text, and with `expand` its Section, in at most 256 KiB: a line with matches under
+two Trails comes once per Trail, a long line as its Fragment, a long Section cut, a Section
+shared by several matches once, and `truncated` says the search found more than the answer
+holds, `truncated_by` why (`max_results`, `bytes`, or `long_line` for a line longer than 4 MiB,
+whose file `errors` names, the first 20 of them, counting the rest). Its searches read no
+configuration file, so a
+`--pre` in `SEARCHFOR_CONFIG_PATH` never runs for a client. To add it to Claude Code:
+
+```
+claude mcp add searchfor -- s4 --mcp
+```
+
+A call of the tool, and its answer:
+
+```
+{"name":"search","arguments":{"pattern":"needle","paths":["md"],"within":"Install › Linux"}}
+{"matches":[{"path":"md/guide.md","line":12,"trail":["Deploy guide","Install","Linux","bash"],
+  "text":"apt install needle-pkg","format":"markdown"}, …],"truncated":false}
+```
+
+### From the command line
+
 Which section is each match in:
 
 ```
@@ -663,7 +718,8 @@ copy that directory to `~/.claude/skills/` to install it.
 | M2 | v0.2 | YAML, JSON and TOML key paths (`server › ports[2] › host`); code (PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust, Python) as `Cart › addItem`, with tree-sitter Grammars compiled in; Helper Processors (`processor = "external"`); `--install-grammar` and `--list-grammars` |
 | M3 (done, 2026-10-17) | v0.3 | HTML and XML element paths (`section#pricing › table › tr[3] › td[2]`); CSV and TSV rows and columns (`row 2 › Plan`); SubRip and WebVTT cues (`cue 2 00:00:04,000`); control characters in printed names shown as escapes |
 | M4 (done, 2026-10-19) | v0.4 | object files (ELF, PE, Mach-O, fat Mach-O) as `ELF x86-64 › .rodata › main.version+0xe`; documents searched by default (`--no-docs` turns it off), with Extractors for PDF, Word, Excel, PowerPoint, OpenDocument text, spreadsheet and presentation, EPUB and SQLite, each document read in a worker process that counts its own memory; ripgrep's `--pre`, `--pre-glob`, `--no-pre`, `-z` and `--no-search-zip`; the M4 benchmark, performance ceilings and fuzz targets |
-| M5 | v1.0 | an MCP server, the rest of ripgrep's flags, prebuilt binaries (GitHub Releases), Homebrew and npm; Windows and macOS in CI, where the readers' memory count is run, and a Windows Job Object over each document reader as its second barrier; Grammar Helper binaries published in the Grammar index; signed releases; the guide for writing a Helper Processor; streaming the Trails of files where most lines match |
+| M5a | v0.5 | every ripgrep 14.1.1 flag, in `--no-trail` as ripgrep prints it and in the Trail layouts (PCRE2, multi-line search, encodings, the ignore and sort flags, `--files`, `-o`, `--column`, `--vimgrep`, `-r`, `--stats`, hyperlinks, a configuration file, `--generate`, `--debug`); an MCP server (`s4 --mcp`) |
+| M5b | v1.0 | prebuilt binaries (GitHub Releases), Homebrew and npm; Windows and macOS in CI, where the readers' memory count is run, and a Windows Job Object over each document reader as its second barrier; Grammar Helper binaries published in the Grammar index; signed releases; the guide for writing a Helper Processor; streaming the Trails of files where most lines match |
 | M6 | v1.x | more Formats: image and audio metadata, WebAssembly and Java `.class` (at the hex offset of the match, as object files); the binary Office formats (`.doc`, `.xls`, `.ppt`); the headers, footers, footnotes and comments of Office documents |
 
 Archive Formats (zip, tar, 7z, rar, deb, rpm, jar) stay out: searchfor searches what ripgrep
````

- [ ] **Step 2:** README.es, the same in Spanish (the project keeps both, M1 plan):

````diff
diff --git a/README.es.md b/README.es.md
index bb4b502..fcee6b3 100644
--- a/README.es.md
+++ b/README.es.md
@@ -35,15 +35,16 @@ Los ejemplos sobre `/bin/ls`, sobre el script de `--pre` de
 
 ## Estado
 
-**v0.4: Markdown, archivos de configuración, código, markup, tablas, subtítulos, archivos objeto
-y documentos.** El motor de búsqueda son los crates de ripgrep, así que la
-búsqueda se comporta como `rg`; encima de eso, Markdown, YAML, JSON, TOML, el código en ocho
+**v0.5: todos los flags de ripgrep, y un servidor MCP.** El motor de búsqueda son los crates de
+ripgrep, así que la búsqueda se comporta como `rg`, y funcionan todos los flags de ripgrep
+14.1.1 (ver [Compatibilidad con ripgrep](#compatibilidad-con-ripgrep)); encima de eso, Markdown, YAML, JSON, TOML, el código en ocho
 lenguajes, HTML, XML, CSV, TSV, SubRip y WebVTT tienen Trails, y también los resultados que
 ripgrep encuentra en binarios ELF, PE y Mach-O (ver [Formatos](#formatos)). Los archivos PDF,
 Word, Excel, PowerPoint, OpenDocument, EPUB y SQLite se buscan por defecto en su texto, con su
 estructura, y `--no-docs` lo apaga (ver [Documentos](#documentos)). Las líneas largas se muestran como
-Fragmentos, y `--within`, `--expand` e `--in-headings` filtran y expanden por estructura. Los
-flags `--pre`, `--pre-glob` y `-z` de ripgrep funcionan como en ripgrep. Otros lenguajes se
+Fragmentos, y `--within`, `--expand` e `--in-headings` filtran y expanden por estructura.
+`s4 --mcp` ofrece la búsqueda a los agentes de IA por el Model Context Protocol (ver
+[Para agentes de IA](#para-agentes-de-ia)). Otros lenguajes se
 pueden describir con un [Helper Processor](#helper-processors), un programa aparte con el que s4
 se comunica. Cualquier otro archivo es **Plain**: sus
 resultados se imprimen con el Trail vacío. Los demás Formatos llegan por hito (ver
@@ -59,8 +60,8 @@ El glosario está en [CONTEXT.md](CONTEXT.md), el diseño en
 cargo install searchfor
 ```
 
-Eso instala la última versión publicada en crates.io. Hasta que v0.4.0 esté publicada ahí,
-v0.4 se instala desde el repositorio:
+Eso instala la última versión publicada en crates.io. Hasta que v0.5.0 esté publicada ahí,
+v0.5 se instala desde el repositorio:
 
 ```
 cargo install --git https://github.com/neitanod/searchfor
@@ -441,6 +442,7 @@ de Fragmentos.
 | `--export-formats <DIR>` | escribe las Definiciones de Formato incluidas en `DIR` |
 | `--list-grammars` | lista las Grammars compiladas, las instaladas y las disponibles (ver [Grammars](#grammars)) |
 | `--install-grammar <LANG>` | instala un Grammar Helper desde el índice de Grammars |
+| `--mcp` | ofrece la búsqueda como servidor MCP por stdin y stdout (ver [Para agentes de IA](#para-agentes-de-ia)) |
 
 Las Definiciones de Formato son archivos TOML. Un archivo en `searchfor/formats/` dentro del
 directorio de configuración del usuario (`~/.config` o `$XDG_CONFIG_HOME` en Linux) con el
@@ -449,15 +451,35 @@ nombre de un Formato incluido o instalado lo reemplaza, y un nombre nuevo agrega
 ### Compatibilidad con ripgrep
 
 ripgrep 14.1.1 tiene 146 nombres de flag (104 flags, 30 negaciones `--no-` y 12 negaciones de
-otra forma). v0.4 implementa 56. Los otros 90 se aceptan por nombre y fallan con
-`s4: --<flag> is not supported yet` y código de salida 2, así que ninguno se ignora en
-silencio. La tabla está en [docs/rg-compat.md](docs/rg-compat.md). Los códigos de salida son
-los de ripgrep: 0 si algo coincidió, 1 si nada, 2 si hubo un error.
+otra forma), y s4 los implementa todos; la tabla está en [docs/rg-compat.md](docs/rg-compat.md).
+Los códigos de salida son los de ripgrep: 0 si algo coincidió, 1 si nada, 2 si hubo un error.
+`SEARCHFOR_CONFIG_PATH` nombra un archivo de configuración con el formato del
+`RIPGREP_CONFIG_PATH` de ripgrep, y `--no-config` lo saltea.
 
 Con `--no-trail`, la salida la escribe el propio printer de ripgrep, y `--json --no-trail`
 imprime los mismos mensajes que `rg --json`, salvo los tiempos. Los documentos son la
 excepción: para tener también sobre ellos la salida de ripgrep, sumá `--no-docs`.
 
+Los flags de salida de ripgrep también cambian las salidas con Trail: `--column` y `-b` agregan
+la columna y el offset en bytes, `-o` imprime cada resultado solo bajo su propio Trail,
+`--vimgrep` una línea por resultado para un editor, `-r` reemplaza los resultados, `-U` imprime
+cada línea de un resultado que ocupa varias, y los separadores, `-0`, `--path-separator`,
+`--colors` (con un tipo `trail` propio) y `--hyperlink-format` funcionan como en ripgrep.
+`--heading` es el árbol y `--no-heading` la salida de una línea:
+
+```
+$ s4 --flat --column -o 'needle-\w+' md/guide.md
+7:22: Deploy guide › Install │ needle-intro
+12:13: Deploy guide › Install › Linux › bash │ needle-pkg
+16:12: Deploy guide › Install › Linux › step one │ needle-item
+23:10: Deploy guide › Install › Plans › Price │ needle-10
+…
+$ s4 --vimgrep needle-pkg md
+md/guide.md:12:13: Deploy guide › Install › Linux › bash │ apt install needle-pkg
+$ s4 -P --flat 'needle-(?=pkg)' md
+md/guide.md:12: Deploy guide › Install › Linux › bash │ apt install needle-pkg
+```
+
 `--pre`, `--pre-glob`, `--no-pre`, `-z` y `--no-search-zip` funcionan como en ripgrep 14.1.1,
 errores y códigos de salida incluidos, y entre `-z` y `--pre` gana el último que se pasa. Lo que
 devuelven se busca como se busca stdin, con los Trails del Formato que dice la ruta del archivo.
@@ -479,6 +501,13 @@ Diferencias conocidas con ripgrep 14.1.1:
 - Los documentos se buscan por defecto en su texto, también con `--no-trail`, `-c`, `-l`, `-q`
   y `--json`, y cada uno se extrae, lo que cuesta más que la búsqueda de ripgrep en sus bytes.
   `--no-docs` da la salida y el costo de ripgrep (ver [Documentos](#documentos)).
+- En las salidas con Trail, un archivo cuyo Formato no lo decide su nombre, stdin entre ellos,
+  se lee entero antes de imprimir sus filas; `--no-trail` imprime cada línea como ripgrep.
+- `--debug` imprime los mensajes de los crates de ripgrep como ripgrep, y los propios de s4;
+  `--generate` escribe la página de manual y los completados de s4; `-P` usa el PCRE2 con el
+  que se compiló s4.
+
+La lista completa está en [docs/rg-compat.md](docs/rg-compat.md#known-deviations).
 
 ## Helper Processors
 
@@ -566,6 +595,34 @@ Un agente que encuentra un resultado con `rg` suele abrir el archivo entero para
 sección cae la línea. `s4` imprime esa sección en el Trail, y `--expand` imprime la sección
 misma, así que el agente lee la parte del archivo que importa y nada más.
 
+### Servidor MCP
+
+`s4 --mcp` habla el Model Context Protocol por stdin y stdout y ofrece una herramienta,
+`search`, con `pattern`, `paths`, `within`, `expand`, `glob`, `case_insensitive` y
+`max_results` (100 si no se pasa, 1 000 como máximo). Responde con la ruta, la línea, el Trail
+y el texto de cada resultado, y con `expand` su Sección, en 256 KiB como máximo: una línea con
+resultados bajo dos Trails llega una vez por Trail, una línea larga como su Fragmento, una
+Sección larga cortada, una Sección que comparten varios resultados una sola vez, y `truncated`
+avisa que la búsqueda encontró más de lo que entra en la respuesta, y `truncated_by` por qué
+(`max_results`, `bytes`, o `long_line` por una línea de más de 4 MiB, cuyo archivo nombra
+`errors`, las primeras 20, y cuenta las demás).
+Sus búsquedas no leen ningún archivo de configuración, así que un `--pre` en
+`SEARCHFOR_CONFIG_PATH` nunca corre para un cliente. Para sumarlo a Claude Code:
+
+```
+claude mcp add searchfor -- s4 --mcp
+```
+
+Una llamada a la herramienta, y su respuesta:
+
+```
+{"name":"search","arguments":{"pattern":"needle","paths":["md"],"within":"Install › Linux"}}
+{"matches":[{"path":"md/guide.md","line":12,"trail":["Deploy guide","Install","Linux","bash"],
+  "text":"apt install needle-pkg","format":"markdown"}, …],"truncated":false}
+```
+
+### Desde la línea de comandos
+
 En qué sección cae cada resultado:
 
 ```
@@ -684,7 +741,8 @@ recetas; para instalarlo, copiá ese directorio a `~/.claude/skills/`.
 | M2 | v0.2 | rutas de claves en YAML, JSON y TOML (`server › ports[2] › host`); código (PHP, JavaScript, TypeScript, TSX, Bash, Go, Rust, Python) como `Cart › addItem`, con las Grammars de tree-sitter compiladas adentro; Helper Processors (`processor = "external"`); `--install-grammar` y `--list-grammars` |
 | M3 (terminado, 2026-10-17) | v0.3 | rutas de elementos en HTML y XML (`section#pricing › table › tr[3] › td[2]`); filas y columnas en CSV y TSV (`row 2 › Plan`); cues de SubRip y WebVTT (`cue 2 00:00:04,000`); los caracteres de control de los nombres impresos como escapes |
 | M4 (terminado, 2026-10-19) | v0.4 | archivos objeto (ELF, PE, Mach-O, Mach-O fat) como `ELF x86-64 › .rodata › main.version+0xe`; documentos buscados por defecto (`--no-docs` lo apaga), con Extractores para PDF, Word, Excel, PowerPoint, texto, planilla y presentación de OpenDocument, EPUB y SQLite, cada documento leído en un proceso aparte que cuenta su propia memoria; los flags `--pre`, `--pre-glob`, `--no-pre`, `-z` y `--no-search-zip` de ripgrep; el benchmark de M4, los techos de rendimiento y los fuzz targets |
-| M5 | v1.0 | un servidor MCP, el resto de los flags de ripgrep, binarios precompilados (GitHub Releases), Homebrew y npm; Windows y macOS en CI, donde se corre la cuenta de memoria de los lectores, y un Job Object de Windows sobre cada lector de documentos como segunda barrera; los binarios de los Grammar Helpers publicados en el índice de Grammars; releases firmados; la guía para escribir un Helper Processor; los Trails en streaming para los archivos donde coinciden casi todas las líneas |
+| M5a | v0.5 | todos los flags de ripgrep 14.1.1, en `--no-trail` como los imprime ripgrep y en las salidas con Trail (PCRE2, búsqueda en varias líneas, codificaciones, los flags de ignorar y de ordenar, `--files`, `-o`, `--column`, `--vimgrep`, `-r`, `--stats`, hipervínculos, un archivo de configuración, `--generate`, `--debug`); un servidor MCP (`s4 --mcp`) |
+| M5b | v1.0 | binarios precompilados (GitHub Releases), Homebrew y npm; Windows y macOS en CI, donde se corre la cuenta de memoria de los lectores, y un Job Object de Windows sobre cada lector de documentos como segunda barrera; los binarios de los Grammar Helpers publicados en el índice de Grammars; releases firmados; la guía para escribir un Helper Processor; los Trails en streaming para los archivos donde coinciden casi todas las líneas |
 | M6 | v1.x | más Formatos: metadatos de imagen y audio, WebAssembly y `.class` de Java (en el offset hexadecimal del resultado, como los archivos objeto); los formatos binarios de Office (`.doc`, `.xls`, `.ppt`); los encabezados, pies de página, notas al pie y comentarios de los documentos de Office |
 
 Los Formatos de empaquetado (zip, tar, 7z, rar, deb, rpm, jar) quedan afuera: searchfor busca
````

- [ ] **Step 3:** the skill: the MCP tool (what to do for each `truncated_by`),
  `-o`/`--column`/`--vimgrep` with Trails, `-P` and `-U`; the line saying some rg flags fail
  goes:

````diff
diff --git a/skills/searchfor/SKILL.md b/skills/searchfor/SKILL.md
index e19a223..9032ff7 100644
--- a/skills/searchfor/SKILL.md
+++ b/skills/searchfor/SKILL.md
@@ -19,6 +19,16 @@ chapter and page, heading and table, sheet and cell, slide, or table, row and co
 `--expand` it prints the whole Section that holds the match, so you read that part of the
 file and nothing else.
 
+## As an MCP tool
+
+`s4 --mcp` is an MCP server (`claude mcp add searchfor -- s4 --mcp`). Its tool `search` takes
+`pattern`, `paths`, `within`, `expand`, `glob`, `case_insensitive` and `max_results` (100 by
+default, at most 1 000) and returns each match's path, line, Trail and text, in at most 256 KiB
+(`truncated: true` when the search found more, and `truncated_by` why: for `max_results` raise
+it or narrow `paths`, `glob` or `within`; for `bytes` narrow the search or leave out `expand`;
+for `long_line` search the files `errors` names, the first 20, with `s4` in a shell). Prefer it
+to running `s4` in a shell when the tool is there: the answer is structured and bounded.
+
 ## When to prefer `s4` over `rg`
 
 - You would otherwise open the whole file to learn which section, function or key a line
@@ -131,6 +141,28 @@ m4/bin/app-stripped:0x5a: ELF x86-64 › .rodata │ version=1.2.3-needle
 m4/bin/app.exe:0x402: PE x86-64 › .rdata › version_string │ version=1.2.3-needle
 ```
 
+Each match alone, with its column and its Trail (`-o` prints the matched text, `--column` its
+column; `--vimgrep` gives `path:line:column:` lines an editor reads):
+
+```bash
+s4 --flat --column -o 'needle-\w+' md/guide.md
+```
+
+```
+7:22: Deploy guide › Install │ needle-intro
+12:13: Deploy guide › Install › Linux › bash │ needle-pkg
+16:12: Deploy guide › Install › Linux › step one │ needle-item
+…
+```
+
+Look-around and backreferences: `-P` searches with PCRE2 (`--engine auto` falls back to it only
+when the default engine refuses the pattern); `-U` lets a match span lines, and each line of it
+prints as a row:
+
+```bash
+s4 -P --flat 'needle-(?=pkg)' md
+```
+
 Where something is defined, not used: `--in-headings` matches only Segment names (headings,
 keys, function and class names, elements, CSV header cells, cues):
 
@@ -162,8 +194,13 @@ or `--within addItem` is enough when the name is unique.
 - Page, sheet, slide, table, row, column, cell and chapter names are not in a document's text,
   so `--in-headings` finds only headings there; use `--within` to keep the matches under them
   (`--within 'slide 3'`, `--within 'Budget'`).
-- A ripgrep flag that s4 does not implement yet fails with `s4: --<flag> is not supported
-  yet` and exit code 2; drop the flag or use `rg` for that search.
+- Every ripgrep 14.1.1 flag works. With `--no-trail` they print what `rg` prints; in the Trail
+  layouts `--column`, `-b`, `-o`, `--vimgrep`, `-r`, `--trim`, `-0` and the separators change
+  s4's rows the same way, each row keeping its Trail (docs/rg-compat.md lists the details).
+- s4 reads a file named by `SEARCHFOR_CONFIG_PATH` (rg's format) before the command line; add
+  `--no-config` when a search must not depend on it.
+- In the Trail layouts, stdin is read whole before its rows print; for a stream that never ends
+  (`tail -f`), use `--no-trail`.
 - In `--json`, when two matches on one line have different Trails (two table cells), each
   object in `.data.submatches` carries its own `trail`. The `stats` of `end` and `summary`
   count matches before `--within` filters them.
````

- [ ] **Step 4:** every new example of the three files runs from `tests/fixtures` with M5a's
  binary and prints what the file shows, `…` standing for rows left out (the probe ran them by
  hand and found the skill's `-o` example cut without its `…`); `cargo test --workspace`: PASS.
- [ ] **Step 5:** commit: `Document M5a: ripgrep's flags with Trails, the MCP server, PCRE2`.

---
## M5b: what the next plan holds, and where it stops for Sebastián

This plan does not write M5b's code. M5b gets its own spec pass and plan, with a probe copy as
this one; the outline below fixes its scope and the points where it stops, so the cut of D1 is
complete.

| Part | Content | Stops for Sebastián |
|---|---|---|
| Windows and macOS | the `cfg(unix)` paths (isolated readers, signals, `RLIMIT_DATA`, paths); the Windows Job Object over each document reader (M4 plan, D31); the fixtures' paths and line endings | — |
| CI | GitHub Actions on Linux, macOS and Windows: build, clippy, fmt, the suite with `SEARCHFOR_REQUIRE_REFERENCE=1` against an rg 14.1.1 the workflow downloads (the workflow downloads, the tests stay offline) | the workflow is committed and **not pushed** until the orchestrator says so |
| Dense files | the streaming of the Trail pipeline (spec §3, item 5) with its own criterion; the per-file cost on small files (Hilo, 18/10) | — |
| Grammar Helper binaries | built per platform by CI, their URLs and SHA-256 in `grammars/index.json` (§7) | **Ask Sebastián** before uploading them anywhere |
| Releases with `dist` | `dist init` config, the release workflow, archives and checksums, built and checked locally and in CI artifacts | **Ask Sebastián** before the first tag and GitHub Release |
| Homebrew | the formula generated by `dist`, tested with `brew install --formula ./s4.rb` on macOS | **Ask Sebastián** before creating the tap repository and before each push to it |
| npm | the package `dist` generates, tested with `npm pack` and a local install | **Ask Sebastián** before `npm publish` (and the package name) |
| crates.io | `cargo package` and `cargo publish --dry-run` | **Ask Sebastián** before `cargo publish` (and the crate name) |
| Signing | the signing steps written and tested locally with a throwaway key | **Ask Sebastián** before using a real key or adding a secret to the repository |
| External Processor guide | spec §4.4, with a worked example over `--pre` | — |

Every «Ask Sebastián» step is its own checkbox in M5b's plan: the chunk reports what is ready,
what the publication will do and what cannot be undone, and waits. The limits at the top of
this plan hold for M5b as they are.

---
## Questions for Sebastián

The plan runs with the recommended answer of each question; a different answer changes the
decision named and the chunks under it. The MCP server's form is settled: the flag `s4 --mcp`
(Sebastián, 19/10, 8837; `docs/specs/grill-respuestas.md`).

1. **Cut M5 in two (D1)?** M5a: every rg flag and the MCP server, eleven chunks. M5b: Windows,
   macOS, CI, releases and their channels, Grammar Helper binaries, signing, the External
   Processor guide and dense streaming. *Recommended: yes.* The flags alone fill eleven chunks
   with their code tested; M5b's estimate (no code written) is about fifteen more.
2. **Configuration file: `SEARCHFOR_CONFIG_PATH` only, or also `RIPGREP_CONFIG_PATH` (D3)?**
   *Recommended: only `SEARCHFOR_CONFIG_PATH`.* A user's rg configuration may hold flags that
   change s4's output (`--no-heading`, `--colors`) without the user meaning it for s4; adding
   the second variable later is one line.
3. **The MCP tool's answer: 100 Matches by default, each as path, line, Trail and text (D17)?**
   *Recommended: yes.* An agent asks for more with `max_results` (up to 1 000); the answer stays
   within 256 KiB and says when it was cut (`truncated`).
4. **`--vimgrep` keeps the Trail (D4, D5)?** Its rows are `path:line:column: Trail │ text`.
   *Recommended: keep it*: an editor's quickfix list reads `path:line:column:` and shows the
   rest as text, so the Trail shows there too; `--vimgrep --no-trail` prints rg's lines.
5. **Usage errors stay clap's messages (D21)?** *Recommended: keep them*: the exit code (2) and
   the order of rg's checks match already, and rg's text for each of its 146 flags would be a
   parser of its own for stderr only.
