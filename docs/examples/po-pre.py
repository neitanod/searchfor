#!/usr/bin/env python3
"""po-pre.py PATH: a `--pre` converter for gettext `.po` files, the first half of
the worked example of docs/external-processors.md. It writes only the
translations, one per line, so contexts and source strings can no longer be
matched. It reads the one-line subset of PO that sample.po uses
(`msgstr "..."` on one line); a production converter must also read PO's
multi-line strings and plural forms.

On Unix it runs as it is (`#!/usr/bin/env python3` and the executable bit);
Windows has no shebang lines, so there `--pre` names the compiled
pre-launcher.rs, which runs this script with Python.
"""
import json, pathlib, sys
for line in pathlib.Path(sys.argv[1]).read_text(encoding='utf-8').splitlines():
    if line.startswith('msgstr '): print(json.loads(line[7:]))
