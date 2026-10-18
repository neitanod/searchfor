#!/usr/bin/env python3
"""Writes the SQLite fixtures of tests/fixtures/docs/ (M4 plan, D19). Python's own sqlite3
module, no third-party code; the data is ours (MIT OR Apache-2.0, as the repository).
Usage: scripts/gen-sqlite-fixtures.py [DIR]  (tests/fixtures/docs by default)."""
import os, sqlite3, sys

out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "tests", "fixtures", "docs")
os.makedirs(out, exist_ok=True)

def fresh(name, page_size=512, encoding=None):
    path = os.path.join(out, name)
    if os.path.exists(path):
        os.remove(path)
    db = sqlite3.connect(path)
    db.execute(f"PRAGMA page_size={page_size}")
    if encoding:
        db.execute(f"PRAGMA encoding='{encoding}'")
    db.execute("PRAGMA journal_mode=DELETE")
    return db

# shop.sqlite: the end-to-end fixture. `users` row 12 holds the needle in `email`.
db = fresh("shop.sqlite")
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT, score REAL)")
for i in range(1, 41):
    email = "ana.needle@example.com" if i == 12 else f"user{i}@example.com"
    db.execute("INSERT INTO users VALUES (?, ?, ?, ?)", (i, f"User {i}", email, i * 1.5))
db.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT, photo BLOB)")
db.execute("INSERT INTO notes VALUES (1, ?, ?)", ("short note", b"\x00\x01binary"))
# Longer than a 512-byte page: its payload continues on overflow pages.
db.execute("INSERT INTO notes VALUES (2, ?, NULL)", ("long " * 300 + "needle at the end",))
db.execute("CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT) WITHOUT ROWID")
for k in ["beta", "alpha", "gamma"]:
    db.execute("INSERT INTO kv VALUES (?, ?)", (k, f"value of {k}" + (" needle" if k == "gamma" else "")))
# A WITHOUT ROWID table stores its primary key first: `late` holds (k, payload) on disk, `comp`
# (c, a, b); the reader must name each value by its declared column (Codex r1 #4).
db.execute("CREATE TABLE late (payload TEXT, k TEXT PRIMARY KEY) WITHOUT ROWID")
db.execute("INSERT INTO late VALUES ('needle payload', 'k1')")
db.execute("CREATE TABLE comp (a TEXT, b REAL, c TEXT, PRIMARY KEY (c, a)) WITHOUT ROWID")
db.execute("INSERT INTO comp VALUES ('a1', 3.0, 'c1')")
# A NULL before a value: its field stays, empty (Codex r1 #12).
db.execute("CREATE TABLE gaps (x TEXT, y TEXT)")
db.execute("INSERT INTO gaps VALUES (NULL, 'needle gap')")
# The schema as SQLite keeps it, its spaces and comments included: `WITHOUT` and `ROWID` on two
# lines, a comment between them, and a key whose quoted name holds a comma (M4 plan r2, Codex #4).
db.execute('CREATE TABLE odd (payload TEXT, "k,key" REAL /* the key */, PRIMARY KEY ("k,key")) WITHOUT\n  -- no rowid\n  ROWID')
db.execute("INSERT INTO odd VALUES ('needle odd', 2)")
# A VIRTUAL generated column is not in the record; a STORED one is (M4 plan r3, Codex #6).
db.execute("CREATE TABLE gen (a TEXT, calc TEXT GENERATED ALWAYS AS (a) VIRTUAL, payload TEXT, st TEXT AS (upper(a)) STORED)")
db.execute("INSERT INTO gen (a, payload) VALUES ('first', 'needle gen')")
# A column added later has its DEFAULT in the rows written before; a NULL stays NULL (M4 plan
# r5, Codex #2).
db.execute("CREATE TABLE added (a TEXT)")
db.execute("INSERT INTO added VALUES ('old')")
db.commit()
db.execute("ALTER TABLE added ADD COLUMN payload TEXT DEFAULT 'needle added'")
db.execute("INSERT INTO added VALUES ('new', 'needle later')")
db.execute("INSERT INTO added VALUES ('null', NULL)")
# Every DEFAULT an older row gets is a constant as SQLite reads it: parentheses nested, a number
# with an exponent or in hexadecimal, and the column's affinity applied, which a TEXT column
# keeps as written (M4 plan r6, Codex #1, Fable #2).
db.execute("CREATE TABLE defaults (a TEXT)")
db.execute("INSERT INTO defaults VALUES ('old')")
db.commit()
for column in ["deep TEXT DEFAULT (('deep default'))", "e REAL DEFAULT 1e-3", "h INTEGER DEFAULT 0x2a",
               "i INTEGER DEFAULT 7.0", "n NUMERIC DEFAULT '7.0'", "m DEFAULT (-(1))", "t TEXT DEFAULT 1E+3"]:
    db.execute(f"ALTER TABLE defaults ADD COLUMN {column}")
db.execute("INSERT INTO defaults (a) VALUES ('new')")
db.execute("INSERT INTO defaults VALUES ('null', NULL, NULL, NULL, NULL, NULL, NULL, NULL)")
# A sign, a bare word, a quoted type and a real as SQLite reads them (M4 plan r7, Codex #1,
# Fable #1 and #3).
db.execute("CREATE TABLE signed (a TEXT)")
db.execute("INSERT INTO signed VALUES ('old')")
db.commit()
for column in ["p TEXT DEFAULT +'plus default'", "w TEXT DEFAULT current_word", 'q "INTEGER" DEFAULT 7.0',
               "z TEXT DEFAULT -'abc'", "s TEXT DEFAULT -'7.0'", "r REAL DEFAULT 1e-5"]:
    db.execute(f"ALTER TABLE signed ADD COLUMN {column}")
db.execute("INSERT INTO signed (a) VALUES ('new')")
db.execute("CREATE INDEX users_email ON users(email)")
db.execute("CREATE VIEW v AS SELECT name FROM users")
db.commit()
db.close()

# many.sqlite: 2 000 rows, enough for interior pages of a table and of a WITHOUT ROWID table.
db = fresh("many.sqlite")
db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, word TEXT)")
db.execute("CREATE TABLE w (k INTEGER PRIMARY KEY, word TEXT) WITHOUT ROWID")
for i in range(1, 2001):
    db.execute("INSERT INTO t VALUES (?, ?)", (i, f"word{i}"))
    db.execute("INSERT INTO w VALUES (?, ?)", (i, f"word{i}"))
db.commit()
db.close()

# utf16.sqlite: text stored as UTF-16le.
db = fresh("utf16.sqlite", encoding="UTF-16le")
db.execute("CREATE TABLE t (name TEXT)")
db.execute("INSERT INTO t VALUES ('año needle')")
db.commit()
db.close()
