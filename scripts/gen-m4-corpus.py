#!/usr/bin/env python3
"""The corpus of scripts/bench-m4.sh (M4 plan, D23), written with Python's standard library
only: documents of each document Format, object files copied from the system, and 20 000 small
text files. Every 40th document and object file holds SPARSE. `texts/` holds the text of each
document as the Extractors write it (a line per paragraph or row), for rg to search.
Usage: scripts/gen-m4-corpus.py DIR SPARSE"""
import os, shutil, sqlite3, sys, zipfile, zlib

out, sparse = sys.argv[1], sys.argv[2]
N = 120  # documents of each kind
PARAS = 200
for d in ["docs", "texts", "bin", "text"]:
    os.makedirs(os.path.join(out, d), exist_ok=True)

def para(i, k, word):
    w = word if k == PARAS // 2 else "hay"
    return f"Lorem ipsum dolor sit amet {i}, consectetur adipiscing elit, sed do {w} {k}."

def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")

def zipped(path, parts):
    with zipfile.ZipFile(path, "w") as z:
        for name, data in parts:
            method = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
            z.writestr(zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0)), data, compress_type=method)

CT = '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>'
def rels(items):
    body = "".join(f'<Relationship Id="{i}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{t}" Target="{g}"/>' for i, t, g in items)
    return f'<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>'

def docx(path, lines):
    body = "".join(f"<w:p><w:r><w:t>{esc(l)}</w:t></w:r></w:p>" for l in lines)
    doc = f'<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>'
    zipped(path, [("[Content_Types].xml", CT), ("_rels/.rels", rels([("rId1", "officeDocument", "word/document.xml")])), ("word/document.xml", doc)])

def xlsx(path, lines):
    rows = "".join(f'<row r="{r+1}"><c r="A{r+1}" t="inlineStr"><is><t>{esc(l)}</t></is></c></row>' for r, l in enumerate(lines))
    sheet = f'<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>'
    book = '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>'
    zipped(path, [("[Content_Types].xml", CT), ("_rels/.rels", rels([("rId1", "officeDocument", "xl/workbook.xml")])), ("xl/workbook.xml", book), ("xl/_rels/workbook.xml.rels", rels([("rId1", "worksheet", "worksheets/sheet1.xml")])), ("xl/worksheets/sheet1.xml", sheet)])

def pptx(path, lines):
    ns = 'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
    parts, ids, rl = [], "", []
    for s in range(0, len(lines), 20):
        n = s // 20 + 1
        ps = "".join(f"<a:p><a:r><a:t>{esc(l)}</a:t></a:r></a:p>" for l in lines[s:s + 20])
        parts.append((f"ppt/slides/slide{n}.xml", f'<?xml version="1.0"?><p:sld {ns}><p:cSld><p:spTree><p:sp><p:txBody>{ps}</p:txBody></p:sp></p:spTree></p:cSld></p:sld>'))
        ids += f'<p:sldId id="{255 + n}" r:id="rId{n}"/>'
        rl.append((f"rId{n}", "slide", f"slides/slide{n}.xml"))
    pres = f'<?xml version="1.0"?><p:presentation {ns}><p:sldIdLst>{ids}</p:sldIdLst></p:presentation>'
    zipped(path, [("[Content_Types].xml", CT), ("_rels/.rels", rels([("rId1", "officeDocument", "ppt/presentation.xml")])), ("ppt/presentation.xml", pres), ("ppt/_rels/presentation.xml.rels", rels(rl))] + parts)

ODF = 'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"'
def odf(path, mime, body):
    zipped(path, [("mimetype", mime), ("content.xml", f'<?xml version="1.0"?><office:document-content {ODF}><office:body>{body}</office:body></office:document-content>')])

def odt(path, lines):
    odf(path, "application/vnd.oasis.opendocument.text", "<office:text>" + "".join(f"<text:p>{esc(l)}</text:p>" for l in lines) + "</office:text>")

def ods(path, lines):
    rows = "".join(f'<table:table-row><table:table-cell office:value-type="string"><text:p>{esc(l)}</text:p></table:table-cell></table:table-row>' for l in lines)
    odf(path, "application/vnd.oasis.opendocument.spreadsheet", f'<office:spreadsheet><table:table table:name="S">{rows}</table:table></office:spreadsheet>')

def odp(path, lines):
    pages = "".join('<draw:page><draw:frame><draw:text-box>' + "".join(f"<text:p>{esc(l)}</text:p>" for l in lines[s:s + 20]) + '</draw:text-box></draw:frame></draw:page>' for s in range(0, len(lines), 20))
    odf(path, "application/vnd.oasis.opendocument.presentation", f"<office:presentation>{pages}</office:presentation>")

def epub(path, lines):
    body = "".join(f"<p>{esc(l)}</p>" for l in lines)
    x = f'<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>'
    opf = '<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf"><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>'
    cont = '<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>'
    zipped(path, [("mimetype", "application/epub+zip"), ("META-INF/container.xml", cont), ("OEBPS/content.opf", opf), ("OEBPS/c.xhtml", x)])

def pdf(path, lines):
    objs = ["<< /Type /Catalog /Pages 2 0 R >>", None, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"]
    kids = []
    for s in range(0, len(lines), 50):
        ops = "BT /F1 10 Tf 72 760 Td " + " ".join(f"({l}) Tj 0 -14 Td" for l in lines[s:s + 50]) + " ET"
        data = zlib.compress(ops.encode("latin-1"))
        objs.append(f"<< /Length {len(data)} /Filter /FlateDecode >>stream\n".encode() + data + b"\nendstream")
        objs.append(f"<< /Type /Page /Parent 2 0 R /Contents {len(objs)} 0 R >>")
        kids.append(len(objs))
    objs[1] = f"<< /Type /Pages /Kids [{' '.join(f'{k} 0 R' for k in kids)}] /Count {len(kids)} /Resources << /Font << /F1 3 0 R >> >> /MediaBox [0 0 612 792] >>"
    b, offs = bytearray(b"%PDF-1.5\n"), []
    for i, o in enumerate(objs, 1):
        offs.append(len(b))
        b += f"{i} 0 obj\n".encode() + (o if isinstance(o, bytes) else o.encode()) + b"\nendobj\n"
    x = len(b)
    b += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode() + "".join(f"{o:010d} 00000 n \n" for o in offs).encode()
    b += f"trailer << /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n".encode()
    open(path, "wb").write(b)

def sqlite(path, lines):
    if os.path.exists(path):
        os.remove(path)
    db = sqlite3.connect(path)
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, body TEXT)")
    db.executemany("INSERT INTO t (body) VALUES (?)", [(l,) for l in lines])
    db.commit()
    db.close()

kinds = [("docx", docx), ("xlsx", xlsx), ("pptx", pptx), ("odt", odt), ("ods", ods), ("odp", odp), ("epub", epub), ("pdf", pdf), ("sqlite", sqlite)]
for ext, write in kinds:
    for i in range(N):
        word = sparse if i % 40 == 0 else "hay"
        lines = [para(i, k, word) for k in range(PARAS)]
        write(os.path.join(out, "docs", f"d{i:03d}.{ext}"), lines)
        with open(os.path.join(out, "texts", f"d{i:03d}.{ext}.txt"), "w") as f:
            f.write("".join(l + "\n" for l in lines))

# Object files: the system's ELF executables under 4 MB, every 40th with SPARSE appended
# after its sections (the file stays a valid ELF).
n = 0
for d in ["/usr/bin", "/usr/sbin", "/usr/lib/x86_64-linux-gnu"]:
    if not os.path.isdir(d):
        continue
    for name in sorted(os.listdir(d)):
        p = os.path.join(d, name)
        try:
            if not os.path.isfile(p) or os.path.islink(p) or os.path.getsize(p) > 4 << 20:
                continue
            with open(p, "rb") as f:
                if f.read(4) != b"\x7fELF":
                    continue
        except OSError:
            continue
        dst = os.path.join(out, "bin", f"o{n:04d}")
        shutil.copyfile(p, dst)
        if n % 40 == 0:
            with open(dst, "ab") as f:
                f.write(b"\0" + sparse.encode() + b" marker\0")
        n += 1
        if n >= 300:
            break
    if n >= 300:
        break

for i in range(20000):
    with open(os.path.join(out, "text", f"t{i:04d}.txt"), "w") as f:
        f.write("".join(f"plain line {k} of file {i}\n" for k in range(40)))
