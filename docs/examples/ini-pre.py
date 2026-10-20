#!/usr/bin/env python3
"""ini-pre.py PATH: a `--pre` converter for INI files, the first half of the
worked example of docs/external-processors.md.

s4 (as rg) runs a `--pre` command with the file's path as its only argument
and the file on stdin, and searches what the command writes to stdout instead
of the file. This one writes only the values of the INI file, one per line,
as UTF-8 text: section names, keys and comments are dropped, so they can no
longer be matched, and the converted text has no structure left to give a
Trail.

On Unix it runs as it is (`#!/usr/bin/env python3` and the executable bit):
`s4 --pre docs/examples/ini-pre.py needle sample.ini`. Windows has no shebang
lines, so there `--pre` names the compiled `pre-launcher.rs`, which runs this
script with Python.
"""

import sys


def values(src):
    """The value of every `key = value` (or `key: value`) line."""
    for line in src.splitlines():
        body = line.strip()
        if not body or body[0] in ";#[":
            continue
        seps = [i for i in (body.find("="), body.find(":")) if i >= 0]
        if seps:
            yield body[min(seps) + 1:].strip()


def main():
    if len(sys.argv) != 2:
        print("usage: ini-pre.py PATH", file=sys.stderr)
        return 2
    with open(sys.argv[1], "rb") as f:
        # utf-8-sig drops a BOM; an invalid byte becomes U+FFFD.
        src = f.read().decode("utf-8-sig", errors="replace")
    out = sys.stdout.buffer  # bytes and "\n": no "\r\n" on Windows
    for value in values(src):
        out.write(value.encode("utf-8") + b"\n")
    out.flush()
    return 0


if __name__ == "__main__":
    sys.exit(main())
