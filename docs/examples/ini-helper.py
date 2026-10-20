#!/usr/bin/env python3
"""ini-helper.py: a Helper Processor for INI files, the worked example of
docs/external-processors.md.

It speaks protocol v1 in `oneshot` mode: it reads its whole stdin (JSON lines,
`hello` first), and only at EOF writes one answer per request, each with the
`id` of its request. EOF completes the batch.

- `hello` -> `{"id":0,"protocol":1,...}`.
- `trails` -> one Trail per Match, in the order of `matches`.
- `names` -> every Segment name, for `--in-headings`.
- anything else -> `{"id":N,"error":"..."}`, which fails that request only.

An INI file has `[section]` lines, which open a `section` up to the next one,
and `key = value` (or `key: value`) lines, each a `key`. Lines starting with
`;` or `#` are comments.

Every position is a byte offset into the file at `path`: the file is read as
bytes and never decoded before the offsets are computed, so a multi-byte
character or a BOM moves no offset. Diagnostics go to stderr, which s4 shows
as `s4: <format>: <line>` warnings. Nothing is kept from one request for the
next: each request reads its file, answers, and drops it.

Run it as `[PYTHON, "ini-helper.py"]` in the Definition's `command`; it needs
no shebang or executable bit there.
"""

import json
import sys

PROTOCOL = 1


def split_lines(src):
    """Yields (start, end, next) for each line: `end` excludes the newline,
    `next` is where the following line starts."""
    start = 0
    while start < len(src):
        end = src.find(b"\n", start)
        if end < 0:
            end = len(src)
        yield start, end, min(end + 1, len(src))
        start = end + 1


def line_starts(src):
    """The byte offset where each line starts; line N (1-based) is index N-1."""
    starts = [0]
    at = src.find(b"\n")
    while at >= 0:
        starts.append(at + 1)
        at = src.find(b"\n", at + 1)
    return starts


def line_of(starts, byte):
    """The 1-based line holding `byte` (binary search over `starts`)."""
    lo, hi = 0, len(starts)
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if starts[mid] <= byte:
            lo = mid
        else:
            hi = mid
    return lo + 1


def segments(src):
    """The Segments of an INI file, in source order. Each is a dict with
    `kind`, the `name` bytes, `at` (where the name starts) and the byte range
    [`start`, `end`) the Segment covers."""
    segs = []
    section = None
    for start, end, nxt in split_lines(src):
        line = src[start:end]
        lead = len(line) - len(line.lstrip())
        body = line.strip()
        if len(body) >= 2 and body.startswith(b"[") and body.endswith(b"]"):
            if section is not None:
                section["end"] = start
            inner = body[1:-1]
            skip = len(inner) - len(inner.lstrip())
            name = inner.strip()
            section = {"kind": "section", "name": name,
                       "at": start + lead + 1 + skip,
                       "start": start, "end": len(src)}
            segs.append(section)
        elif body and body[:1] not in (b";", b"#"):
            seps = [i for i in (body.find(b"="), body.find(b":")) if i >= 0]
            if seps:
                key = body[:min(seps)].rstrip()
                if key:
                    segs.append({"kind": "key", "name": key,
                                 "at": start + lead,
                                 "start": start, "end": nxt})
    return segs


def text(name):
    """The name as shown: UTF-8, with invalid bytes replaced."""
    return name.decode("utf-8", errors="replace")


def wire(seg, starts):
    """A Segment as protocol v1 sends it (the object of `--json`)."""
    last = max(seg["end"] - 1, seg["start"])
    return {"kind": seg["kind"], "name": text(seg["name"]),
            "line": line_of(starts, seg["start"]),
            "end_line": line_of(starts, last)}


def trail_at(segs, starts, byte):
    """The Trail of a byte: every Segment that contains it, outermost first."""
    return [wire(s, starts) for s in segs if s["start"] <= byte < s["end"]]


def trails(request):
    with open(request["path"], "rb") as f:
        src = f.read()
    segs, starts = segments(src), line_starts(src)
    return {"trails": [trail_at(segs, starts, m["byte"])
                       for m in request["matches"]]}


def names(request):
    with open(request["path"], "rb") as f:
        src = f.read()
    segs, starts = segments(src), line_starts(src)
    out = []
    for seg in segs:
        shown = text(seg["name"])
        # One piece: [text_start, text_end, src_start, src_end], in bytes.
        # Equal lengths when the name is verbatim; the replacement of an
        # invalid byte makes the text longer, and the piece maps all of it.
        piece = [0, len(shown.encode("utf-8")),
                 seg["at"], seg["at"] + len(seg["name"])]
        out.append({"text": shown, "pieces": [piece], "anchor": seg["at"],
                    "trail": trail_at(segs, starts, seg["start"])})
    return {"names": out}


def hello(request):
    if request.get("protocol") != PROTOCOL:
        raise ValueError(f"protocol {request.get('protocol')} is not {PROTOCOL}")
    # `params` is the Definition's [params] table; this helper needs none.
    return {"protocol": PROTOCOL, "name": "ini-helper.py", "version": "1.0.0"}


OPS = {"hello": hello, "trails": trails, "names": names}


def answer(line):
    """The answer to one request line, or None when it has no `id` (s4 never
    sends one; a reply without `id` would make s4 kill this process)."""
    try:
        request = json.loads(line)
    except ValueError as e:
        print(f"ini-helper.py: not JSON: {e}", file=sys.stderr)
        return None
    if not isinstance(request, dict) or "id" not in request:
        print("ini-helper.py: a request without id", file=sys.stderr)
        return None
    op = OPS.get(request.get("op"))
    try:
        if op is None:
            raise ValueError(f"unknown op {request.get('op')!r}")
        reply = op(request)
    except Exception as e:  # any failure fails this request only
        reply = {"error": f"{type(e).__name__}: {e}"}
    return {"id": request["id"], **reply}


def main():
    # oneshot: the whole batch first, then the answers.
    batch = sys.stdin.buffer.read().splitlines()
    out = sys.stdout.buffer
    for line in batch:
        if not line.strip():
            continue
        reply = answer(line)
        if reply is not None:
            # Bytes and "\n", so Windows writes no "\r\n"; non-ASCII names
            # stay UTF-8.
            data = json.dumps(reply, ensure_ascii=False, separators=(",", ":"))
            out.write(data.encode("utf-8") + b"\n")
    out.flush()


if __name__ == "__main__":
    main()
