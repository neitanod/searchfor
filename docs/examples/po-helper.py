#!/usr/bin/env python3
"""po-helper.py: a Helper Processor for gettext `.po` files, the second half of
the worked example of docs/external-processors.md (protocol v1, no third-party
library).

It reads the one-line subset of PO that sample.po uses (`msgctxt "..."`,
`msgid "..."`, `msgstr "..."`, each on one line) and gives a Match in a
translation the Trail `context › entry`, each Segment named by its source
value. A production helper must also read PO's multi-line strings, plural
forms and comments.
"""
import json, pathlib, sys

def value(start, line, number):
    # The quoted value of a one-line `keyword "..."`: its decoded text, and the
    # Source bytes between the quotes, which an escape makes longer than the text.
    body = line.rstrip(b'\r\n')
    first, last = body.index(b'"'), len(body) - 1
    return (json.loads(body[first:].decode('utf-8', 'replace')),
            start + first + 1, start + last, number)

def read(path):
    raw = pathlib.Path(path).read_bytes()
    offset = 0; context = None; msgid = None; records = []
    for number, line in enumerate(raw.splitlines(keepends=True), 1):
        if line.startswith(b'msgctxt "'): context = value(offset, line, number)
        elif line.startswith(b'msgid "'): msgid = value(offset, line, number)
        elif line.startswith(b'msgstr ') and msgid:
            records.append((offset, offset + len(line), number, context, msgid))
            context = msgid = None
        offset += len(line)
    return records

def segments(record):
    # Two Segments, so the Trail separator stays out of any name: the context
    # (when there is one) and the entry, each named by its source value.
    start, end, number, context, msgid = record
    named = [('context', context)] if context else []
    named.append(('entry', msgid))
    return [(kind, v, {'kind':kind, 'name':v[0], 'line':v[3], 'end_line':number})
            for kind, v in named]

def trail(record):
    return [segment for _, _, segment in segments(record)]

for line in sys.stdin:
    request = json.loads(line); ident = request.get('id'); op = request.get('op', 'trails')
    try:
        if op == 'hello': response = {'id':ident, 'protocol':1}
        elif op == 'names':
            result = []
            for record in read(request['path']):
                found = segments(record)
                for depth, (kind, (text, src_start, src_end, number), _) in enumerate(found):
                    # One piece over the value between the quotes, in bytes.
                    result.append({'text':text,
                                   'pieces':[[0, len(text.encode('utf-8')), src_start, src_end]],
                                   'anchor':src_start,
                                   'trail':[s for _, _, s in found[:depth + 1]]})
            response = {'id':ident, 'names':result}
        elif op == 'trails':
            records = read(request['path']); result = []
            for match in request['matches']:
                anchor = match['byte']
                result.append(next((trail(r) for r in records if r[0] <= anchor < r[1]), []))
            response = {'id':ident, 'trails':result}
        else: response = {'id':ident, 'error':'unknown operation'}
    except Exception as error: response = {'id':ident, 'error':str(error)}
    sys.stdout.buffer.write((json.dumps(response, ensure_ascii=False)+'\n').encode('utf-8'))
    sys.stdout.buffer.flush()
