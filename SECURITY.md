# Security policy

## Supported versions

searchfor has not published a release yet. Security fixes go to the `main` branch, and to the
latest released version once there is one.

## Reporting a vulnerability

Report it privately through GitHub: open the repository's **Security** tab and choose
**Report a vulnerability**. Please do not open a public issue, a pull request or a discussion
for a security problem before it is fixed.

Include, when you can:

- the `s4 --version` output and your operating system;
- the command line, and the file or input that triggers the problem (a minimal one is best);
- what happens, and what you expected.

You will get an answer within seven days. Once the problem is confirmed, the fix and an advisory
are published together, and you are credited in the advisory unless you prefer otherwise.

## What counts as a security problem

`s4` reads files it did not write, and some of them may be hostile. These are security problems:

- a file that makes `s4` run code, write files, or read outside the paths it was asked to search;
- a file that crashes `s4` with a memory error, or makes it use memory or time far beyond the
  documented limits (a document reader stops at its memory and time limits with a warning; going
  past them is a bug);
- anything that makes `s4 --mcp` run a `--pre` command, read the user's configuration file, or
  search outside the paths of the request;
- a child process (a document reader, a Helper Processor or an MCP search) that outlives `s4`.

These are not, on their own:

- a `--pre` command doing what it was written to do: `--pre` runs the program the user names, as
  in ripgrep;
- slow searches over very large inputs, when the time grows with the input as ripgrep's does;
- output that differs from ripgrep's: that is a compatibility bug, and it goes to the public issue
  tracker. `docs/rg-compat.md` lists the differences that are intended.

## How s4 limits the damage

- Documents (PDF, Office, OpenDocument, EPUB, SQLite) are read in isolated worker processes, each
  with its own memory limit and timeout, and a reader that fails only costs its own file.
- Object files and documents are parsed by Rust code, without calling external programs.
- The MCP server talks only over stdio. It never opens a network port, and each search runs as a
  child `s4 --json` without the user's configuration file and without `--pre`.
- Fuzz tests feed `s4` random flags and malformed files, and check that it never panics or hangs.
