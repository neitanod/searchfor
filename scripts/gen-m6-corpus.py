#!/usr/bin/env python3
"""The corpus of scripts/bench-m6.sh (M6 plan, Task 11.3), with Python's standard library only:
`mixed/`, 20 000 copies of the M6 text fixtures, one in a hundred holding M6MARK once, and
`unclaimed/`, 100 000 files whose names no M6 signal claims (spec #51), among them `.json`,
`.html` and `.txt` names that Telegram's and WhatsApp's globs consult.
Usage: scripts/gen-m6-corpus.py DIR"""
import json, pathlib, sys
root = pathlib.Path(sys.argv[1]); fixtures = pathlib.Path(__file__).resolve().parents[1] / 'tests/fixtures'
files = [p for group in ('docs-a', 'config', 'code-c', 'mail', 'chat')
         for p in (fixtures / group).rglob('*') if p.is_file() and 'broken-' not in p.name]
files = [p for p in files if b'needle' in p.read_bytes()]
assert files, 'M6 fixtures missing'
meta = {'matching_bytes': 0, 'largest_matching_bytes': 0, 'files': 0, 'matching_files': 0}
for i in range(20000):
    matched = i % 100 == 0
    # The matched files cycle through every fixture, as the others do (100 and the fixture
    # count share factors, so `i % len(files)` alone would match a few Formats only).
    source = files[(i // 100 if matched else i) % len(files)]; raw = source.read_bytes()
    if matched:
        raw = raw.replace(b'needle', b'M6MARK', 1).replace(b'needle', b'normal')
        meta['matching_files'] += 1; meta['matching_bytes'] += len(raw)
        meta['largest_matching_bytes'] = max(meta['largest_matching_bytes'], len(raw))
    else:
        raw = raw.replace(b'needle', b'normal')
    path = root / 'mixed' / str(i) / source.relative_to(fixtures)
    path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(raw); meta['files'] += 1
for i in range(100000):
    # Spec #51: unclaimed names, plus the .json/.html/.txt ones that signal 4
    # consults (Telegram's globs and WhatsApp's `*WhatsApp*.txt`).
    d = root / 'unclaimed' / str(i // 1000)
    if i % 100 == 0:
        p = d / f'r{i}' / ('result.json', 'messages.html', 'notes.txt', 'chat.txt')[(i // 100) % 4]
    else:
        p = d / f'file-{i}.{("unclaimed", "json", "html", "conf", "txt")[i % 5]}'
    p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(b'ordinary content\n' * 40)
(root / 'metadata.json').write_text(json.dumps(meta))
