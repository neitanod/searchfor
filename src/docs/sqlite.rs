//! SQLite (spec §5.8): `users › row 12 › email`. A reader of the file format of our own (M4
//! plan, D15): the schema table, then every table's b-tree, in page order of its keys. Each row
//! is one line of tab-separated values, named by its rowid; each value a `column` Segment named
//! by the column's name in the table's `CREATE TABLE`. Indexes, views and triggers are not read;
//! neither is a `-wal` file next to the database, so changes not yet checkpointed into the file
//! are not found (D16).
use super::{Builder, DocError};

/// The b-trees of one database are at most this deep (SQLite's own limit is 20).
const MAX_DEPTH: usize = 40;

struct Db<'a> {
    bytes: &'a [u8],
    page_size: usize,
    /// The usable bytes of each page: the page size less the reserved bytes at its end.
    usable: usize,
    pages: usize,
    /// 1 UTF-8, 2 UTF-16le, 3 UTF-16be.
    encoding: u32,
}

/// A value of a record.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
    Blob(usize),
}

impl Value {
    fn shown(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => real(*f),
            Value::Text(t) => t.clone(),
            Value::Blob(n) => format!("[blob {n} bytes]"),
        }
    }
}

/// A real as the `sqlite3` shell prints it, and `CAST(x AS TEXT)`: SQLite's `%!.15g`, 15
/// significant digits, rounded half away from zero as SQLite rounds them, the zeros at the end
/// dropped but one after the point, and an exponent of at least two digits below 1e-4 and from
/// 1e15 on (M4 plan r7, Fable #1: `0.1 + 0.2` is `0.3`, where Rust's shortest form is
/// `0.30000000000000004`).
fn real(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f < 0.0 { "-Inf" } else { "Inf" }.into();
    }
    if f == 0.0 {
        return "0.0".into();
    }
    // 41 digits, exact as Rust prints them, of which the 16th rounds the first 15: Rust's own
    // rounding at 15 is half to even, and an exact half (`8979436893026.125`) goes up in SQLite.
    let e = format!("{:.40e}", f.abs());
    let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let mut exp: i32 = exp.parse().unwrap_or(0);
    let mut d: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let up = d.get(15).is_some_and(|&c| c >= b'5');
    d.truncate(15);
    if up {
        match d.iter().rposition(|&c| c != b'9') {
            Some(i) => {
                d[i] += 1;
                d[i + 1..].fill(b'0');
            }
            None => {
                d.fill(b'0');
                d[0] = b'1';
                exp += 1;
            }
        }
    }
    let digits = String::from_utf8(d).unwrap_or_default();
    let digits = digits.trim_end_matches('0');
    let sign = if f < 0.0 { "-" } else { "" };
    if !(-4..15).contains(&exp) {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        let es = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{first}.{rest}e{es}{:02}", exp.unsigned_abs());
    }
    if exp < 0 {
        let zeros = "0".repeat(exp.unsigned_abs() as usize - 1);
        return format!("{sign}0.{zeros}{digits}");
    }
    let whole = exp as usize + 1;
    if digits.len() <= whole {
        format!("{sign}{digits}{}.0", "0".repeat(whole - digits.len()))
    } else {
        format!("{sign}{}.{}", &digits[..whole], &digits[whole..])
    }
}

fn be(b: &[u8], at: usize, n: usize) -> Option<u64> {
    let s = b.get(at..at + n)?;
    Some(s.iter().fold(0u64, |a, &x| a << 8 | x as u64))
}

/// SQLite's varint: up to 9 bytes, big-endian, 7 bits per byte but the ninth's 8.
fn varint(b: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut v = 0u64;
    for i in 0..9 {
        let x = *b.get(at + i)?;
        if i == 8 {
            return Some((v << 8 | x as u64, 9));
        }
        v = v << 7 | (x & 0x7f) as u64;
        if x & 0x80 == 0 {
            return Some((v, i + 1));
        }
    }
    None
}

impl<'a> Db<'a> {
    fn open(bytes: &'a [u8]) -> Result<Db<'a>, DocError> {
        if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
            return Err(DocError::broken("not an SQLite 3 database"));
        }
        let raw = be(bytes, 16, 2).unwrap() as usize;
        let page_size = if raw == 1 { 65536 } else { raw };
        if !(512..=65536).contains(&page_size) || !page_size.is_power_of_two() {
            return Err(DocError::broken(format!("page size {raw} is not valid")));
        }
        let reserved = bytes[20] as usize;
        let usable = page_size.saturating_sub(reserved);
        if usable < 480 {
            return Err(DocError::broken(
                "the reserved space leaves pages too small",
            ));
        }
        let encoding = be(bytes, 56, 4).unwrap() as u32;
        Ok(Db {
            bytes,
            page_size,
            usable,
            pages: bytes.len() / page_size,
            encoding: if (1..=3).contains(&encoding) {
                encoding
            } else {
                1
            },
        })
    }

    fn page(&self, n: usize) -> Option<&'a [u8]> {
        if n == 0 || n > self.pages {
            return None;
        }
        self.bytes.get((n - 1) * self.page_size..n * self.page_size)
    }

    /// The payload of a cell: its local bytes and, past them, its overflow pages.
    fn payload(&self, page: &[u8], at: usize, size: usize, index: bool) -> Option<Vec<u8>> {
        let u = self.usable;
        let max_local = if index {
            (u - 12) * 64 / 255 - 23
        } else {
            u - 35
        };
        if size <= max_local {
            return page.get(at..at + size).map(<[u8]>::to_vec);
        }
        let min_local = (u - 12) * 32 / 255 - 23;
        let k = min_local + (size - min_local) % (u - 4);
        let local = if k <= max_local { k } else { min_local };
        let mut out = page.get(at..at + local)?.to_vec();
        let mut next = be(page, at + local, 4)? as usize;
        let mut hops = 0;
        while out.len() < size {
            hops += 1;
            if hops > self.pages {
                return None;
            }
            let p = self.page(next)?;
            next = be(p, 0, 4)? as usize;
            let take = (size - out.len()).min(u - 4);
            out.extend(p.get(4..4 + take)?);
        }
        Some(out)
    }

    fn text(&self, b: &[u8]) -> String {
        match self.encoding {
            2 => encoding_rs::UTF_16LE
                .decode_without_bom_handling(b)
                .0
                .into_owned(),
            3 => encoding_rs::UTF_16BE
                .decode_without_bom_handling(b)
                .0
                .into_owned(),
            _ => String::from_utf8_lossy(b).into_owned(),
        }
    }

    /// The values of record `p`, read as strictly as SQLite reads them (Codex M4 code r6 #2):
    /// None for a header shorter than its own length's varint or longer than the payload, a
    /// serial type whose varint ends past the header, a reserved serial type, or values that do
    /// not take exactly the payload's bytes after the header.
    fn record(&self, p: &[u8]) -> Option<Vec<Value>> {
        let (hlen, mut at) = varint(p, 0)?;
        let hlen = usize::try_from(hlen).ok()?;
        if hlen < at || hlen > p.len() {
            return None;
        }
        let mut body = hlen;
        let mut out = Vec::new();
        while at < hlen {
            let (t, n) = varint(&p[..hlen], at)?;
            at += n;
            let (v, len) = match t {
                0 => (Value::Null, 0),
                1..=6 => {
                    let len = [0, 1, 2, 3, 4, 6, 8][t as usize];
                    let raw = be(p, body, len)?;
                    let shift = 64 - 8 * len as u32;
                    (Value::Int(((raw << shift) as i64) >> shift), len)
                }
                7 => (Value::Float(f64::from_bits(be(p, body, 8)?)), 8),
                8 => (Value::Int(0), 0),
                9 => (Value::Int(1), 0),
                10 | 11 => return None,
                t if t % 2 == 0 => (
                    Value::Blob(((t - 12) / 2) as usize),
                    ((t - 12) / 2) as usize,
                ),
                t => {
                    let len = ((t - 13) / 2) as usize;
                    (Value::Text(self.text(p.get(body..body + len)?)), len)
                }
            };
            if body + len > p.len() {
                return None;
            }
            body += len;
            out.push(v);
        }
        (body == p.len()).then_some(out)
    }

    /// Calls `row` with (rowid, values) for every row of the table b-tree rooted at `root`, in
    /// rowid order; for a `WITHOUT ROWID` table (an index b-tree), with the 1-based position
    /// as its id. Stops when `row` returns false, which is no error. Err: a page that is not
    /// what its parent says, or a cell that cannot be read (Codex M4 code r5 #3).
    fn walk(
        &self,
        root: usize,
        row: &mut dyn FnMut(i64, Vec<Value>) -> bool,
    ) -> Result<(), DocError> {
        // A page to read, or the row an interior index cell holds between its children.
        enum Next {
            Page(usize, usize),
            Cell(usize, usize),
        }
        let mut stack = vec![Next::Page(root, 0)];
        let mut seen = std::collections::HashSet::new();
        let mut ordinal = 0i64;
        // Interior pages push their children right to left, so rows come out in key order.
        while let Some(next) = stack.pop() {
            let (n, depth) = match next {
                Next::Page(n, d) => (n, d),
                Next::Cell(n, c) => {
                    ordinal += 1;
                    let page = self.page(n).unwrap_or_default();
                    if !self.index_cell(n, page, c, ordinal, row)? {
                        return Ok(());
                    }
                    continue;
                }
            };
            if depth > MAX_DEPTH || !seen.insert(n) {
                return Err(DocError::broken("a b-tree that loops"));
            }
            let page = self
                .page(n)
                .ok_or_else(|| DocError::broken(format!("page {n} is past the end")))?;
            let h = if n == 1 { 100 } else { 0 };
            let kind = *page
                .get(h)
                .ok_or_else(|| DocError::broken("an empty page"))?;
            let cells = be(page, h + 3, 2).unwrap_or(0) as usize;
            let interior = matches!(kind, 0x02 | 0x05);
            let ptrs = h + if interior { 12 } else { 8 };
            let cell = |i: usize| be(page, ptrs + 2 * i, 2).map(|c| c as usize);
            match kind {
                0x05 | 0x02 => {
                    let right = be(page, h + 8, 4).unwrap_or(0) as usize;
                    stack.push(Next::Page(right, depth + 1));
                    for i in (0..cells).rev() {
                        let c = cell(i).ok_or_else(|| DocError::broken("a cell past its page"))?;
                        // An interior index cell holds a row of its own, after its left child.
                        if kind == 0x02 {
                            stack.push(Next::Cell(n, c + 4));
                        }
                        let child = be(page, c, 4).unwrap_or(0) as usize;
                        stack.push(Next::Page(child, depth + 1));
                    }
                }
                0x0d => {
                    for i in 0..cells {
                        let c = cell(i).ok_or_else(|| DocError::broken("a cell past its page"))?;
                        let (size, a) = varint(page, c).ok_or_else(|| past(n))?;
                        let (rowid, b) = varint(page, c + a).ok_or_else(|| unreadable(n))?;
                        let v = self
                            .payload(page, c + a + b, size as usize, false)
                            .and_then(|p| self.record(&p))
                            .ok_or_else(|| unreadable(n))?;
                        if !row(rowid as i64, v) {
                            return Ok(());
                        }
                    }
                }
                0x0a => {
                    for i in 0..cells {
                        let c = cell(i).ok_or_else(|| DocError::broken("a cell past its page"))?;
                        ordinal += 1;
                        if !self.index_cell(n, page, c, ordinal, row)? {
                            return Ok(());
                        }
                    }
                }
                _ => return Err(DocError::broken(format!("page {n} is not a b-tree page"))),
            }
        }
        Ok(())
    }

    /// The row of index cell `c` of page `n`, given to `row`: Ok(false) when `row` stops the
    /// walk.
    fn index_cell(
        &self,
        n: usize,
        page: &[u8],
        c: usize,
        id: i64,
        row: &mut dyn FnMut(i64, Vec<Value>) -> bool,
    ) -> Result<bool, DocError> {
        let (size, a) = varint(page, c).ok_or_else(|| past(n))?;
        let v = self
            .payload(page, c + a, size as usize, true)
            .and_then(|p| self.record(&p))
            .ok_or_else(|| unreadable(n))?;
        Ok(row(id, v))
    }
}

fn past(page: usize) -> DocError {
    DocError::broken(format!("page {page} has a cell past its end"))
}

fn unreadable(page: usize) -> DocError {
    DocError::broken(format!("page {page} has a cell that cannot be read"))
}

/// What a `CREATE TABLE` says of its columns.
#[derive(Debug, Default, PartialEq)]
struct Columns {
    names: Vec<String>,
    /// Columns of REAL affinity, whose integral values SQLite stores as integers and shows
    /// as floats (`3.0`).
    real: Vec<bool>,
    /// The `INTEGER PRIMARY KEY`, which a row stores as NULL: its value is the rowid.
    ipk: Option<usize>,
    without_rowid: bool,
    /// The declared column of each value as a record stores it: in order, except in a
    /// `WITHOUT ROWID` table, which stores its primary key's columns first, in the key's order,
    /// then the others (Codex r1 #4). A column the key names twice with one collation is stored
    /// once, and twice with two (Codex M4 code r5 #2): the record then holds its value twice.
    stored: Vec<usize>,
    /// Each column's `DEFAULT`, when it is a constant: a column added by `ALTER TABLE ADD
    /// COLUMN` has it in the rows written before, whose records end earlier (M4 plan r5,
    /// Codex #2).
    defaults: Vec<Option<Value>>,
    /// Whether `ALTER TABLE ADD COLUMN` could have added each column, so that a record may
    /// end before it: not one of a key or of a UNIQUE constraint, not a STORED generated
    /// column, and with a constant DEFAULT if it has a DEFAULT or is NOT NULL (non-NULL then).
    addable: Vec<bool>,
    /// How many of `stored`'s first values are a `WITHOUT ROWID` table's key, which SQLite
    /// keeps NOT NULL.
    key_len: usize,
}

/// A token of SQL (M4 plan r2, Codex #4): a word, a quoted name (`"…"`, `` `…` ``, `[…]`) or
/// string (`'…'`), or one other character. White space and comments separate tokens.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// A bare word: a keyword or a name, as written.
    Word(String),
    /// A quoted name or a string, unquoted: never a keyword.
    Quoted(String),
    Punct(char),
}

impl Tok {
    /// Whether this is the keyword `kw` (any case).
    fn is(&self, kw: &str) -> bool {
        matches!(self, Tok::Word(w) if w.eq_ignore_ascii_case(kw))
    }

    fn name(&self) -> Option<String> {
        match self {
            Tok::Word(w) | Tok::Quoted(w) => Some(w.clone()),
            Tok::Punct(_) => None,
        }
    }
}

fn tokens(sql: &str) -> Vec<Tok> {
    let c: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '-' && c.get(i + 1) == Some(&'-') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if matches!(ch, '"' | '`' | '\'' | '[') {
            let close = if ch == '[' { ']' } else { ch };
            let mut t = String::new();
            i += 1;
            while i < c.len() {
                if c[i] == close {
                    // A doubled quote is the quote itself; `]` always closes.
                    if close != ']' && c.get(i + 1) == Some(&close) {
                        t.push(close);
                        i += 2;
                        continue;
                    }
                    break;
                }
                t.push(c[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Quoted(t));
        } else if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '.' {
            let start = i;
            // A decimal number's exponent may have a sign: `1e-3` is one token, where `0x1e`
            // ends before a `-` (M4 plan r6, Codex #1).
            let decimal = (ch.is_ascii_digit() || ch == '.')
                && !(ch == '0' && matches!(c.get(i + 1), Some('x' | 'X')));
            let signed_exponent = |i: usize| {
                decimal
                    && matches!(c[i], '+' | '-')
                    && matches!(c[i - 1], 'e' | 'E')
                    && c.get(i + 1).is_some_and(char::is_ascii_digit)
            };
            while i < c.len()
                && (c[i].is_alphanumeric() || matches!(c[i], '_' | '$' | '.') || signed_exponent(i))
            {
                i += 1;
            }
            out.push(Tok::Word(c[start..i].iter().collect()));
        } else {
            out.push(Tok::Punct(ch));
            i += 1;
        }
    }
    out
}

/// `toks` split at the commas outside parentheses.
fn split_commas(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => depth = depth.saturating_sub(1),
            Tok::Punct(',') if depth == 0 => {
                parts.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&toks[start..]);
    parts
}

/// The tokens of a column definition or a table constraint at its own level, with their
/// index: a parenthesis that opens or closes there is one, what it holds is not. A
/// constraint is a word at this level, never one of a CHECK's, a DEFAULT's or a generated
/// column's expression, which `CHECK(a IS NOT NULL)` holds (Codex M4 code r7 #1); a string or
/// a quoted name is one token, never a keyword.
fn top(part: &[Tok]) -> Vec<(usize, &Tok)> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for (i, t) in part.iter().enumerate() {
        match t {
            Tok::Punct('(') => {
                if depth == 0 {
                    out.push((i, t));
                }
                depth += 1;
            }
            Tok::Punct(')') => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    out.push((i, t));
                }
            }
            t if depth == 0 => out.push((i, t)),
            _ => {}
        }
    }
    out
}

/// The index in `part` of the first of two consecutive words `a b` at its own level.
fn top_pair(part: &[Tok], a: &str, b: &str) -> Option<usize> {
    top(part)
        .windows(2)
        .find(|w| w[0].1.is(a) && w[1].1.is(b))
        .map(|w| w[0].0)
}

/// The index of the `(` that opens a generated column's expression, after `AS` at the
/// definition's level.
fn generated_open(part: &[Tok]) -> Option<usize> {
    top(part)
        .windows(2)
        .find(|w| w[0].1.is("AS") && *w[1].1 == Tok::Punct('('))
        .map(|w| w[1].0)
}

/// Whether a column definition is `[GENERATED ALWAYS] AS (…)` and not `STORED`: the word
/// right after the expression's closing parenthesis says, never one inside it (a column may
/// be named `stored`; M4 plan r4, Codex #6).
fn generated_virtual(part: &[Tok]) -> bool {
    let Some(open) = generated_open(part) else {
        return false;
    };
    let mut depth = 0usize;
    for (i, t) in part.iter().enumerate().skip(open) {
        match t {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    return !part.get(i + 1).is_some_and(|t| t.is("STORED"));
                }
            }
            _ => {}
        }
    }
    true
}

/// The collation a column definition or a key's term names (`COLLATE nocase`), in lower case,
/// at its own level: of several clauses, the last, which SQLite applies over the ones before
/// (Codex M4 code r6 #1).
fn collation(part: &[Tok]) -> Option<String> {
    top(part)
        .iter()
        .rev()
        .filter(|(_, t)| t.is("COLLATE"))
        .find_map(|(i, _)| part.get(i + 1).and_then(Tok::name))
        .map(|c| c.to_ascii_lowercase())
}

/// The index of the token after `PRIMARY KEY` in `toks`, if it holds one at its own level.
fn primary_key(toks: &[Tok]) -> Option<usize> {
    top_pair(toks, "PRIMARY", "KEY").map(|i| i + 2)
}

/// Whether `part` holds the word `kw` at its own level.
fn top_word(part: &[Tok], kw: &str) -> bool {
    top(part).iter().any(|(_, t)| t.is(kw))
}

/// The index of a column definition's DEFAULT clause: of several, the last, which SQLite
/// keeps; never the `SET DEFAULT` of a foreign key's `ON DELETE` or `ON UPDATE` (Codex M4
/// code r7 #1).
fn default_clause(part: &[Tok]) -> Option<usize> {
    top(part)
        .windows(2)
        .rev()
        .find(|w| w[1].1.is("DEFAULT") && !w[0].1.is("SET"))
        .map(|w| w[1].0)
}

/// The columns a `CREATE TABLE` declares. `wide` is whether the database is UTF-16, where a
/// DEFAULT's `CAST(… AS BLOB)` is the UTF-16 bytes of a text.
fn columns(sql: &str, wide: bool) -> Columns {
    let toks = tokens(sql);
    let Some(open) = toks.iter().position(|t| *t == Tok::Punct('(')) else {
        return Columns::default();
    };
    let mut depth = 0usize;
    let mut close = toks.len();
    for (i, t) in toks.iter().enumerate().skip(open) {
        match t {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => {
                depth -= 1;
                if depth == 0 {
                    close = i;
                    break;
                }
            }
            _ => {}
        }
    }
    let options = &toks[(close + 1).min(toks.len())..];
    let without_rowid = options
        .windows(2)
        .any(|w| w[0].is("WITHOUT") && w[1].is("ROWID"));
    let mut names = Vec::new();
    let mut types = Vec::new();
    let mut integers: Vec<bool> = Vec::new();
    let mut ipk = None;
    // The primary key's columns, by name and with the collation the key gives them, from a
    // column's `PRIMARY KEY` or the table's.
    let mut pk: Vec<(String, Option<String>)> = Vec::new();
    // Each column's own collation.
    let mut collations: Vec<Option<String>> = Vec::new();
    // Whether `pk` is the table's constraint, `PRIMARY KEY(…)`, rather than a column's.
    let mut table_pk = false;
    // Columns a table's UNIQUE constraint names, and each column's own constraints that no
    // added column may have.
    let mut unique: Vec<String> = Vec::new();
    let mut fixed: Vec<bool> = Vec::new();
    let mut virtuals: Vec<usize> = Vec::new();
    let mut defaults: Vec<Option<Value>> = Vec::new();
    for part in split_commas(&toks[open + 1..close.max(open + 1)]) {
        let Some(first) = part.first() else { continue };
        if ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"]
            .iter()
            .any(|k| first.is(k))
        {
            if let Some(at) = top(part)
                .iter()
                .find(|(_, t)| t.is("UNIQUE"))
                .map(|(i, _)| *i)
                && part.get(at + 1) == Some(&Tok::Punct('('))
            {
                let end = part
                    .iter()
                    .rposition(|t| *t == Tok::Punct(')'))
                    .unwrap_or(part.len());
                unique.extend(
                    split_commas(&part[at + 2..end.max(at + 2)])
                        .iter()
                        .filter_map(|c| c.first().and_then(Tok::name)),
                );
            }
            if let Some(at) = primary_key(part)
                && part.get(at) == Some(&Tok::Punct('('))
            {
                let end = part
                    .iter()
                    .rposition(|t| *t == Tok::Punct(')'))
                    .unwrap_or(part.len());
                pk = split_commas(&part[at + 1..end.max(at + 1)])
                    .iter()
                    .filter_map(|c| Some((c.first().and_then(Tok::name)?, collation(c))))
                    .collect();
                table_pk = true;
            }
            continue;
        }
        let Some(name) = first.name() else { continue };
        // The type: the words after the name, up to a constraint or a parenthesis; a quoted
        // one is a type too (`"INTEGER"`, M4 plan r7, Codex #1), never a keyword.
        let ty: Vec<String> = part[1..]
            .iter()
            .take_while(|t| {
                matches!(t, Tok::Word(_) | Tok::Quoted(_))
                    && ![
                        "CONSTRAINT",
                        "PRIMARY",
                        "NOT",
                        "NULL",
                        "UNIQUE",
                        "CHECK",
                        "DEFAULT",
                        "COLLATE",
                        "REFERENCES",
                        "GENERATED",
                        "AS",
                    ]
                    .iter()
                    .any(|k| t.is(k))
            })
            .filter_map(Tok::name)
            .map(|w| w.to_ascii_uppercase())
            .collect();
        // Only a declared type of exactly `INTEGER` makes the primary key the rowid: SQLite
        // compares the whole type, so `INTEGER(8)` is not one (Codex r3 #2).
        let integer = ty == ["INTEGER"] && part.get(2) != Some(&Tok::Punct('('));
        let ty = ty.join(" ");
        // `GENERATED ALWAYS AS (…)` or `AS (…)`, VIRTUAL unless it says STORED: a virtual
        // column takes no place in the record (M4 plan r3, Codex #6, Fable #5).
        if generated_virtual(part) {
            virtuals.push(names.len());
        }
        let generated = generated_open(part).is_some();
        if let Some(at) = primary_key(part) {
            pk = vec![(name.clone(), None)];
            table_pk = false;
            if integer && !part.get(at).is_some_and(|t| t.is("DESC")) {
                ipk = Some(names.len());
            }
        }
        let default = default_clause(part);
        let value = default
            .and_then(|d| constant(&part[d + 1..], wide))
            .map(|(lit, _)| lit.with_affinity(affinity(&ty)));
        let not_null = top_pair(part, "NOT", "NULL").is_some();
        fixed.push(
            primary_key(part).is_some()
                || top_word(part, "UNIQUE")
                || (generated && !generated_virtual(part))
                || (default.is_some() && value.is_none())
                || (not_null && value.as_ref().is_none_or(|v| *v == Value::Null)),
        );
        defaults.push(value);
        collations.push(collation(part));
        names.push(name);
        types.push(ty);
        integers.push(integer);
    }
    // A column the key names again with the same collation is left out of it, as SQLite's
    // `convertToWithoutRowidTable` does: its collation is the term's, else the column's, else
    // BINARY.
    let mut key: Vec<usize> = Vec::new();
    let mut terms: Vec<(usize, String)> = Vec::new();
    for (k, coll) in &pk {
        let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(k)) else {
            continue;
        };
        let coll = coll
            .clone()
            .or_else(|| collations[i].clone())
            .unwrap_or_else(|| "binary".into());
        if !terms.contains(&(i, coll.clone())) {
            terms.push((i, coll));
            key.push(i);
        }
    }
    // `PRIMARY KEY(id, id)` has two terms, and is no rowid.
    if ipk.is_none()
        && table_pk
        && pk.len() == 1
        && let [only] = key.as_slice()
        && integers[*only]
    {
        // A table constraint `PRIMARY KEY(id)` over an INTEGER column makes it the rowid too,
        // also with DESC; a column's `INTEGER PRIMARY KEY DESC` does not (Codex r1 #6).
        ipk = Some(*only);
    }
    let stored = if without_rowid {
        let mut s = key.clone();
        s.extend((0..names.len()).filter(|i| !key.contains(i)));
        s
    } else {
        (0..names.len()).collect()
    };
    let stored: Vec<usize> = stored
        .into_iter()
        .filter(|i| !virtuals.contains(i))
        .collect();
    let real = types
        .iter()
        .map(|t| affinity(t) == Affinity::Real)
        .collect();
    let addable = (0..names.len())
        .map(|i| {
            !fixed[i]
                && !pk.iter().any(|(k, _)| k.eq_ignore_ascii_case(&names[i]))
                && !unique.iter().any(|u| u.eq_ignore_ascii_case(&names[i]))
        })
        .collect();
    Columns {
        names,
        real,
        ipk: if without_rowid { None } else { ipk },
        without_rowid,
        key_len: if without_rowid { key.len() } else { 0 },
        stored,
        defaults,
        addable,
    }
}

/// A row's values in their declared order, from the values its record holds in `stored`
/// order. A record may hold fewer, when the columns it lacks were added since, which then have
/// their DEFAULT; a NULL the record holds stays NULL. A column the key stores twice (two
/// collations) holds one value twice.
///
/// Err when the row does not fit what the `CREATE TABLE` says, which would else lose or move
/// a value silently (Codex M4 code r5 #2, r6 #1): more values than the table stores, one
/// missing that no `ALTER TABLE ADD COLUMN` could have left out, two copies of a key's column
/// that differ, a value in the rowid's alias, which SQLite stores as NULL, or a NULL in a
/// `WITHOUT ROWID` key.
fn rebuild(cols: &Columns, stored: Vec<Value>) -> Result<Vec<Value>, String> {
    let held = stored.len();
    let name = |at: usize| &cols.names[at];
    if held > cols.stored.len() {
        return Err(format!(
            "a row holds {held} values, where the table stores {}",
            cols.stored.len()
        ));
    }
    if let Some(&at) = cols.stored[held..].iter().find(|&&at| !cols.addable[at]) {
        return Err(format!("a row has no value of column {}", name(at)));
    }
    let mut values = vec![Value::Null; cols.names.len()];
    let mut set = vec![false; values.len()];
    for (k, v) in stored.into_iter().enumerate() {
        let at = cols.stored[k];
        if k < cols.key_len && v == Value::Null {
            return Err(format!("a row's key holds NULL in column {}", name(at)));
        }
        if Some(at) == cols.ipk && v != Value::Null {
            return Err(format!(
                "a row holds a value in the rowid's column {}",
                name(at)
            ));
        }
        if std::mem::replace(&mut set[at], true) {
            if values[at] != v {
                return Err(format!(
                    "a row's key holds two values of column {}",
                    name(at)
                ));
            }
        } else {
            values[at] = v;
        }
    }
    for &at in &cols.stored[held..] {
        if let Some(Some(d)) = cols.defaults.get(at) {
            values[at] = d.clone();
        }
    }
    Ok(values)
}

/// A column's affinity, from its declared type by SQLite's rules, in their order: `INT`, then
/// `CHAR`, `CLOB` or `TEXT`, then `BLOB` or no type, then `REAL`, `FLOA` or `DOUB`, else NUMERIC.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Affinity {
    Integer,
    Text,
    Blob,
    Real,
    Numeric,
}

fn affinity(ty: &str) -> Affinity {
    let t = ty.to_ascii_uppercase();
    if t.contains("INT") {
        Affinity::Integer
    } else if ["CHAR", "CLOB", "TEXT"].iter().any(|k| t.contains(k)) {
        Affinity::Text
    } else if t.contains("BLOB") || t.trim().is_empty() {
        Affinity::Blob
    } else if ["REAL", "FLOA", "DOUB"].iter().any(|k| t.contains(k)) {
        Affinity::Real
    } else {
        Affinity::Numeric
    }
}

/// A `DEFAULT` constant as written, before the column's affinity applies to it (M4 plan r6,
/// Codex #1, Fable #2).
#[derive(Debug, Clone, PartialEq)]
enum Lit {
    /// A string, or a bare word, which SQLite takes for one.
    Text(String),
    /// A number, and the text a TEXT column keeps of it: as written, a hexadecimal one in
    /// decimal.
    Num(Value, String),
    /// NULL, which no affinity changes.
    Other(Value),
    /// A blob's bytes, which no affinity changes either, and which made text are UTF-8.
    Blob(Vec<u8>),
    /// A text made a blob in a UTF-16 database: the text's UTF-16 bytes, which made text again
    /// are the same text.
    Wide(String),
    /// A blob made text in a UTF-8 database, whose bytes are not all UTF-8: SQLite keeps them,
    /// and a CAST back to a blob gives them back (Codex M4 code r9 #3); they are U+FFFD only
    /// when shown.
    Bytes(Vec<u8>),
}

impl Lit {
    #[cfg(test)]
    fn shown(&self) -> String {
        match self {
            Lit::Text(t) => t.clone(),
            Lit::Num(v, _) | Lit::Other(v) => v.shown(),
            Lit::Blob(b) => Value::Blob(b.len()).shown(),
            Lit::Wide(t) => Value::Blob(2 * t.encode_utf16().count()).shown(),
            Lit::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        }
    }

    /// The value SQLite gives a row whose record lacks the column: the constant with the
    /// column's affinity, as `sqlite3ValueFromExpr` applies it.
    fn with_affinity(self, a: Affinity) -> Value {
        match self.affined(a) {
            Lit::Text(t) => Value::Text(t),
            Lit::Num(v, _) | Lit::Other(v) => v,
            Lit::Blob(b) => Value::Blob(b.len()),
            Lit::Wide(t) => Value::Blob(2 * t.encode_utf16().count()),
            Lit::Bytes(b) => Value::Text(String::from_utf8_lossy(&b).into_owned()),
        }
    }

    /// The constant with affinity `a`: a number made text keeps its digits as written, and a
    /// text or a number made a number is then shown as SQLite shows that number.
    fn affined(self, a: Affinity) -> Lit {
        let num = |v: Value| {
            let v = numeric(v, a);
            let t = v.shown();
            Lit::Num(v, t)
        };
        match (self, a) {
            (Lit::Num(_, t), Affinity::Text) => Lit::Text(t),
            // Bytes that are not UTF-8 are no number either.
            (lit @ (Lit::Other(_) | Lit::Blob(_) | Lit::Wide(_) | Lit::Bytes(_)), _)
            | (lit @ Lit::Num(..), Affinity::Blob)
            | (lit @ Lit::Text(_), Affinity::Text | Affinity::Blob) => lit,
            (Lit::Num(v, _), _) => num(v),
            (Lit::Text(t), _) => match decimal(&t) {
                Some(v) => num(v),
                None => Lit::Text(t),
            },
        }
    }

    /// `CAST(<self> AS <type of affinity a>)`, as `sqlite3ValueFromExpr` evaluates it: the
    /// constant with the type's affinity, then `sqlite3VdbeMemCast` (Fable M4 code r8 #1). A
    /// text or a blob made a number reads the number at its start, 0 when none; a real made an
    /// integer drops its fraction, and one past 64 bits is the largest or the smallest integer.
    /// In a UTF-16 database (`wide`) a text made a blob is its UTF-16 bytes, and a blob written
    /// as such made text loses an odd last byte (each case checked against Python's sqlite3).
    fn cast(self, a: Affinity, wide: bool) -> Lit {
        let lit = self.affined(a);
        let num = |v: Value| {
            let t = v.shown();
            Lit::Num(v, t)
        };
        // The text a number is read from: a byte that is not UTF-8 ends it, as U+FFFD does.
        let text = |lit: Lit| match lit {
            Lit::Text(t) | Lit::Wide(t) => Some(t),
            Lit::Blob(b) | Lit::Bytes(b) => Some(String::from_utf8_lossy(&b).into_owned()),
            _ => None,
        };
        let blob = |t: String| {
            if wide {
                Lit::Wide(t)
            } else {
                Lit::Blob(t.into_bytes())
            }
        };
        match (lit, a) {
            (lit @ Lit::Other(_), _) => lit,
            (Lit::Num(v, _), Affinity::Blob) => blob(v.shown()),
            (Lit::Text(t), Affinity::Blob) => blob(t),
            (Lit::Num(_, t), Affinity::Text) => Lit::Text(t),
            (lit @ (Lit::Blob(_) | Lit::Wide(_)), Affinity::Blob) => lit,
            (Lit::Bytes(b), Affinity::Blob) => Lit::Blob(b),
            (lit @ Lit::Bytes(_), Affinity::Text) => lit,
            (Lit::Blob(mut b), Affinity::Text) if wide => {
                b.truncate(b.len() & !1);
                Lit::Text(String::from_utf8_lossy(&b).into_owned())
            }
            (Lit::Blob(b), Affinity::Text) => match String::from_utf8(b) {
                Ok(t) => Lit::Text(t),
                Err(e) => Lit::Bytes(e.into_bytes()),
            },
            (lit, Affinity::Text) => Lit::Text(text(lit).unwrap_or_default()),
            (Lit::Num(Value::Float(f), _), Affinity::Integer) => num(Value::Int(f as i64)),
            (Lit::Num(Value::Int(n), _), Affinity::Real) => num(Value::Float(n as f64)),
            (lit @ Lit::Num(..), _) => lit,
            (lit, Affinity::Integer) => {
                num(Value::Int(integer_prefix(&text(lit).unwrap_or_default())))
            }
            (lit, Affinity::Real) => num(match numerify(&text(lit).unwrap_or_default()) {
                Value::Int(n) => Value::Float(n as f64),
                v => v,
            }),
            (lit, Affinity::Numeric) => num(numerify(&text(lit).unwrap_or_default())),
        }
    }
}

/// White space before and after a number, as SQLite's `sqlite3Isspace` reads it: ASCII's, and
/// the vertical tab, which Rust's leaves out (Fable M4 code r9 #3).
fn space(c: char) -> bool {
    c == '\u{0b}' || c.is_ascii_whitespace()
}

/// The integer at the start of a text, as SQLite's `sqlite3Atoi64` reads one for a cast: after
/// white space, a sign and digits, the largest or the smallest integer past 64 bits, and 0 for
/// a text that starts with none.
fn integer_prefix(t: &str) -> i64 {
    let t = t.trim_start_matches(space);
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut n: i128 = 0;
    for d in digits.bytes().take_while(u8::is_ascii_digit) {
        n = (n * 10 + i128::from(d - b'0')).min(i128::from(u64::MAX));
    }
    let n = if neg { -n } else { n };
    n.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

/// A number as a column of INTEGER, NUMERIC or REAL affinity keeps it: in the first two a real
/// without a fraction that fits in 64 bits is an integer, in the third every number is a real.
fn numeric(v: Value, a: Affinity) -> Value {
    const I64: std::ops::Range<f64> = i64::MIN as f64..i64::MAX as f64;
    match (v, a) {
        (Value::Int(n), Affinity::Real) => Value::Float(n as f64),
        (Value::Float(f), Affinity::Integer | Affinity::Numeric)
            if f.fract() == 0.0 && I64.contains(&f) =>
        {
            Value::Int(f as i64)
        }
        (v, _) => v,
    }
}

/// A text that reads as a decimal number, as SQLite converts one for a column of numeric
/// affinity: white space around it, a sign, digits with a point or an exponent; never
/// hexadecimal.
fn decimal(s: &str) -> Option<Value> {
    let t = s.trim_matches(space);
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    let ok = digits.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && digits
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'));
    if !ok {
        return None;
    }
    t.parse::<i64>()
        .map(Value::Int)
        .ok()
        .or_else(|| t.parse::<f64>().ok().map(Value::Float))
}

/// A numeric literal as SQLite reads it: decimal, with a point or an exponent, or hexadecimal
/// (`0x2a`, 64 bits in two's complement); `_` may separate digits, and SQLite drops it. An
/// integer that fits in 64 bits is its value, also to a TEXT column (`007` is `7`, `1_000` is
/// `1000`); any other number keeps its digits as written (M4 plan r7, Fable #3).
fn number(w: &str) -> Option<Lit> {
    let digits = w.replace('_', "");
    if let Some(h) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        let n = u64::from_str_radix(h, 16).ok()? as i64;
        return Some(Lit::Num(Value::Int(n), n.to_string()));
    }
    let v = decimal(&digits)?;
    let t = match v {
        Value::Int(n) => n.to_string(),
        _ => digits,
    };
    Some(Lit::Num(v, t))
}

/// A value made a number, as SQLite's `sqlite3VdbeMemNumerify` makes one before a sign: the
/// number at the start of a text (after white space), an integer when it has no fraction, and
/// 0 for a text that starts with none.
fn numerify(t: &str) -> Value {
    let b = t.trim_start_matches(space).as_bytes();
    let digits = |from: usize| from + b[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut end = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let start = end;
    end = digits(end);
    let mut whole = end > start;
    if b.get(end) == Some(&b'.') {
        let after = digits(end + 1);
        whole |= after > end + 1;
        end = after;
    }
    if !whole {
        return Value::Int(0);
    }
    if matches!(b.get(end), Some(b'e' | b'E')) {
        let mut e = end + 1;
        e += usize::from(matches!(b.get(e), Some(b'+' | b'-')));
        if digits(e) > e {
            end = digits(e);
        }
    }
    let n = std::str::from_utf8(&b[..end]).unwrap_or("0");
    match n.parse::<i64>() {
        Ok(i) => Value::Int(i),
        Err(_) => numeric(Value::Float(n.parse().unwrap_or(0.0)), Affinity::Numeric),
    }
}

/// `-` before a constant, as `sqlite3ValueFromExpr` reads it: before a number as written, the
/// number with the sign (`-1.50` stays as written to a TEXT column); before anything else, the
/// value made a number and negated (`-'abc'` and `-X'01'` are `0`, `-'7.0'` is `-7`: M4 plan r7,
/// Fable #3). `written` is whether the constant is a numeric literal.
fn negative(lit: Lit, written: bool) -> Option<Lit> {
    let v = match lit {
        Lit::Num(Value::Int(n), _) if written => {
            let n = n.wrapping_neg();
            return Some(Lit::Num(Value::Int(n), n.to_string()));
        }
        Lit::Num(_, t) if written => {
            let t = format!("-{t}");
            return Some(Lit::Num(decimal(&t)?, t));
        }
        Lit::Num(v, _) => v,
        Lit::Text(t) => numerify(&t),
        Lit::Blob(b) | Lit::Bytes(b) => numerify(&String::from_utf8_lossy(&b)),
        Lit::Wide(t) => numerify(&t),
        Lit::Other(v) => return Some(Lit::Other(v)),
    };
    let v = match v {
        Value::Int(i64::MIN) => Value::Float(-(i64::MIN as f64)),
        Value::Int(n) => Value::Int(-n),
        Value::Float(f) => Value::Float(-f),
        _ => return None,
    };
    let t = v.shown();
    Some(Lit::Num(v, t))
}

/// The constant a `DEFAULT` names at the start of `toks`, and how many tokens it takes: a
/// string, a number, `NULL`, `TRUE` or `FALSE`, a blob, a bare word (which SQLite takes for a
/// string), a sign before one of these, a `CAST` of one of these, or one of these in
/// parentheses, nested or not. None for an expression, and for `CURRENT_DATE`, `CURRENT_TIME`
/// and `CURRENT_TIMESTAMP`.
fn constant(toks: &[Tok], wide: bool) -> Option<(Lit, usize)> {
    // Where the parenthesis open before `rest` closes.
    let closing = |rest: &[Tok]| {
        let mut depth = 1usize;
        rest.iter().position(|t| {
            match t {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => depth -= 1,
                _ => {}
            }
            depth == 0
        })
    };
    match toks {
        [Tok::Punct('('), rest @ ..] => {
            let close = closing(rest)?;
            let (lit, n) = constant(&rest[..close], wide)?;
            (n == close).then_some((lit, close + 2))
        }
        // `CAST(<constant> AS <type>)`, which SQLite evaluates (Fable M4 code r8 #1).
        [Tok::Word(w), Tok::Punct('('), rest @ ..] if w.eq_ignore_ascii_case("CAST") => {
            let close = closing(rest)?;
            let (lit, n) = constant(&rest[..close], wide)?;
            let ty = rest[..close].get(n + 1..)?;
            if !rest[n].is("AS") {
                return None;
            }
            let ty: Vec<String> = ty.iter().filter_map(Tok::name).collect();
            let ty = ty.join(" ");
            // A CAST to no type, `AS` alone or `AS ""`, is one to NUMERIC, as SQLite's
            // `sqlite3AffinityType` reads it; no type is BLOB only in a column (Fable M4 code
            // r9 #2).
            let a = if ty.trim().is_empty() {
                Affinity::Numeric
            } else {
                affinity(&ty)
            };
            Some((lit.cast(a, wide), close + 3))
        }
        [Tok::Quoted(s), ..] => Some((Lit::Text(s.clone()), 1)),
        [Tok::Word(x), Tok::Quoted(h), ..] if x.eq_ignore_ascii_case("X") => {
            let bytes = h.as_bytes().chunks(2).filter_map(|c| {
                std::str::from_utf8(c)
                    .ok()
                    .and_then(|c| u8::from_str_radix(c, 16).ok())
            });
            Some((Lit::Blob(bytes.collect()), 2))
        }
        // A unary plus is no operator to SQLite: `+'needle'` is `'needle'` (M4 plan r7, Codex
        // #1).
        [Tok::Punct('+'), rest @ ..] => constant(rest, wide).map(|(lit, n)| (lit, n + 1)),
        [Tok::Punct('-'), rest @ ..] => {
            let (lit, n) = constant(rest, wide)?;
            // A numeric literal, in parentheses or not.
            let written = rest
                .iter()
                .find(|t| **t != Tok::Punct('('))
                .is_some_and(|t| {
                    matches!(t, Tok::Word(w) if w.starts_with(|c: char| c.is_ascii_digit() || c == '.'))
                });
            Some((negative(lit, written)?, n + 1))
        }
        [Tok::Word(w), ..] if w.starts_with(|c: char| c.is_ascii_digit() || c == '.') => {
            Some((number(w)?, 1))
        }
        [Tok::Word(w), ..] if w.eq_ignore_ascii_case("NULL") => Some((Lit::Other(Value::Null), 1)),
        [Tok::Word(w), ..] if w.eq_ignore_ascii_case("TRUE") => {
            Some((Lit::Num(Value::Int(1), "1".into()), 1))
        }
        [Tok::Word(w), ..] if w.eq_ignore_ascii_case("FALSE") => {
            Some((Lit::Num(Value::Int(0), "0".into()), 1))
        }
        // A function, or the time of the insertion: not constants.
        [Tok::Word(_), Tok::Punct('('), ..] => None,
        // Exactly these three: `current_needle` is a bare word, a string (M4 plan r7, Codex #1).
        [Tok::Word(w), ..]
            if ["CURRENT_DATE", "CURRENT_TIME", "CURRENT_TIMESTAMP"]
                .iter()
                .any(|k| w.eq_ignore_ascii_case(k)) =>
        {
            None
        }
        [Tok::Word(w), ..] => Some((Lit::Text(w.clone()), 1)),
        _ => None,
    }
}

pub fn extract(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let db = Db::open(bytes)?;
    // sqlite_schema: type, name, tbl_name, rootpage, sql.
    let mut tables = Vec::new();
    db.walk(1, &mut |_, v| {
        if let [
            Value::Text(ty),
            Value::Text(name),
            _,
            Value::Int(root),
            sql,
            ..,
        ] = v.as_slice()
            && ty == "table"
            && !name.starts_with("sqlite_")
            && *root > 0
        {
            let sql = match sql {
                Value::Text(s) => s.clone(),
                _ => String::new(),
            };
            tables.push((name.clone(), *root as usize, sql));
        }
        true
    })?;
    for (name, root, sql) in tables {
        let cols = columns(&sql, matches!(db.encoding, 2 | 3));
        let t = b.open("table", &name);
        let mut misfit = None;
        let r = db.walk(root, &mut |rowid, stored| {
            let values = match rebuild(&cols, stored) {
                Ok(v) => v,
                Err(e) => {
                    misfit = Some(DocError::broken(format!("table {name}: {e}")));
                    return false;
                }
            };
            b.start_row(Some(&format!("row {rowid}")));
            for (i, (v, col)) in values.iter().zip(&cols.names).enumerate() {
                let shown = match v {
                    Value::Null if Some(i) == cols.ipk => rowid.to_string(),
                    Value::Int(n) if cols.real.get(i) == Some(&true) => {
                        Value::Float(*n as f64).shown()
                    }
                    v => v.shown(),
                };
                b.cell("column", col, &shown);
            }
            b.end_row();
            !b.full()
        });
        b.close(t);
        r?;
        if let Some(e) = misfit {
            return Err(e);
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The columns of a table in a UTF-8 database.
    fn columns(sql: &str) -> Columns {
        super::columns(sql, false)
    }

    /// The constant at the start of `toks`, in a UTF-8 database.
    fn constant(toks: &[Tok]) -> Option<(Lit, usize)> {
        super::constant(toks, false)
    }

    #[test]
    fn varints_are_sqlites() {
        assert_eq!(varint(&[0x05], 0), Some((5, 1)));
        assert_eq!(varint(&[0x81, 0x00], 0), Some((128, 2)));
        assert_eq!(varint(&[0xff; 9], 0), Some((u64::MAX, 9)));
        assert_eq!(varint(&[0x81], 0), None);
    }

    #[test]
    fn create_table_gives_the_column_names() {
        let c = columns(
            "CREATE TABLE \"users\" (id INTEGER PRIMARY KEY, \"full name\" TEXT NOT NULL, [e,mail] TEXT, `n` INT DEFAULT (1+2), s REAL, CONSTRAINT u UNIQUE(email), CHECK (n > 0))",
        );
        assert_eq!(c.names, ["id", "full name", "e,mail", "n", "s"]);
        assert_eq!(c.real, [false, false, false, false, true]);
        assert_eq!((c.ipk, c.without_rowid), (Some(0), false));
        let c = columns("CREATE TABLE kv(k TEXT PRIMARY KEY, v) WITHOUT ROWID");
        assert_eq!(c.names, ["k", "v"]);
        assert_eq!((c.ipk, c.without_rowid), (None, true));
        // A `[` inside a bracketed name does not close it (found in the probe's review).
        assert_eq!(columns("CREATE TABLE t([a[b] TEXT, c)").names, ["a[b", "c"]);
        let c = columns("CREATE TABLE t(a INTEGER, b, PRIMARY KEY(a))");
        assert_eq!(c.names, ["a", "b"]);
        assert_eq!(c.ipk, Some(0));
    }

    /// Codex M4 code r5 #2: a `WITHOUT ROWID` key that names a column twice stores it once
    /// per collation it has in the key (its own, BINARY when it declares none, unless the key
    /// says another), as SQLite 3.46.1 stores `keys.sqlite`; each declared column prints once,
    /// with its value, as `SELECT *` gives it.
    #[test]
    fn a_key_naming_a_column_twice_stores_it_once_per_collation() {
        let e = extracted("keys.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        assert_eq!(
            text,
            "one1\tneedle1\none2\tneedle2\none3\tneedle3\tthree3\n\
             one4\tneedle4\tthree4\none5\tneedle5\none6\tneedle6\n\
             one7\tneedle7\none8\tneedle8\n"
        );
        assert_eq!(trail(&e, "needle1"), ["k1", "row 1", "b"]);
        assert_eq!(trail(&e, "three3"), ["k3", "row 1", "c"]);
        let stored = |sql: &str| columns(sql).stored;
        assert_eq!(
            stored("CREATE TABLE t(a, b, PRIMARY KEY(b, b)) WITHOUT ROWID"),
            [1, 0]
        );
        assert_eq!(
            stored("CREATE TABLE t(a, b, PRIMARY KEY(b, b COLLATE NOCASE)) WITHOUT ROWID"),
            [1, 1, 0]
        );
        assert_eq!(
            stored(
                "CREATE TABLE t(a, b COLLATE NOCASE, c, PRIMARY KEY(b, a, b COLLATE nocase, c, a)) \
                 WITHOUT ROWID"
            ),
            [1, 0, 2]
        );
        assert_eq!(
            stored(
                "CREATE TABLE t(a, b COLLATE \"NoCase\", PRIMARY KEY(b COLLATE nocase, b)) WITHOUT ROWID"
            ),
            [1, 0]
        );
        // Two terms over one INTEGER column: no alias of the rowid, as SQLite 3.46.1 reads it.
        assert_eq!(
            columns("CREATE TABLE t(id INTEGER, x, PRIMARY KEY(id, id))").ipk,
            None
        );
    }

    /// Codex M4 code r6 #1: of several COLLATE clauses, in a column's definition or in a
    /// term of the key, the last is the collation, as SQLite 3.46.1 reads them (`keys.sqlite`'s
    /// k7 and k8 above).
    #[test]
    fn the_last_collate_clause_is_the_collation() {
        let stored = |sql: &str| columns(sql).stored;
        assert_eq!(
            stored(
                "CREATE TABLE t(a TEXT, b TEXT COLLATE NOCASE COLLATE RTRIM, \
                 PRIMARY KEY(b, b COLLATE RTRIM)) WITHOUT ROWID"
            ),
            [1, 0]
        );
        assert_eq!(
            stored(
                "CREATE TABLE t(a TEXT, b TEXT, PRIMARY KEY(b COLLATE NOCASE COLLATE BINARY, b)) \
                 WITHOUT ROWID"
            ),
            [1, 0]
        );
        assert_eq!(
            stored(
                "CREATE TABLE t(a, b COLLATE RTRIM COLLATE NOCASE, \
                 PRIMARY KEY(b COLLATE BINARY COLLATE NOCASE, b)) WITHOUT ROWID"
            ),
            [1, 0]
        );
        assert_eq!(
            stored(
                "CREATE TABLE t(a, b COLLATE NOCASE COLLATE BINARY, \
                 PRIMARY KEY(b, b COLLATE NOCASE)) WITHOUT ROWID"
            ),
            [1, 1, 0]
        );
    }

    /// Codex M4 code r6 #2: a record is read strictly, as SQLite reads it: its header's
    /// length within the payload and at least its own varint, every serial type's varint
    /// ending within the header, and the values taking exactly the rest of the payload.
    #[test]
    fn a_record_is_read_strictly() {
        let db = Db {
            bytes: &[],
            page_size: 512,
            usable: 512,
            pages: 0,
            encoding: 1,
        };
        assert_eq!(
            db.record(&[2, 15, b'x']),
            Some(vec![Value::Text("x".into())])
        );
        // Bytes past the last value.
        assert_eq!(db.record(&[2, 15, b'x', b'y']), None);
        assert_eq!(db.record(&[1, b'x']), None);
        // A header shorter than its own length's varint, or longer than the payload.
        assert_eq!(db.record(&[0]), None);
        assert_eq!(db.record(&[5, 15, b'x']), None);
        // A serial type whose varint goes on past the header: 0x81 0x00 is 128, a blob of 58
        // bytes, which the rest of the payload holds.
        let mut p = vec![2, 0x81, 0x00];
        p.resize(60, 0);
        assert_eq!(db.record(&p), None);
        // A value past the payload.
        assert_eq!(db.record(&[2, 17, b'x']), None);
    }

    /// Codex M4 code r5 and r6: a row that does not fit what the table's `CREATE TABLE` says
    /// is not rebuilt with a value lost or moved: more values than the table stores, a value
    /// missing that no `ALTER TABLE ADD COLUMN` could have left out, two copies of a key's
    /// column that differ, a value where the rowid's alias holds NULL, or a NULL in a WITHOUT
    /// ROWID key are an error, and the database is searched as rg searches it.
    #[test]
    fn a_row_that_does_not_fit_its_table_is_an_error() {
        let r = |sql: &str, v: Vec<Value>| rebuild(&columns(sql), v);
        let t = |s: &str| Value::Text(s.into());
        assert_eq!(
            r("CREATE TABLE t(a, b DEFAULT 'x')", vec![t("1")]),
            Ok(vec![t("1"), t("x")])
        );
        assert_eq!(
            r("CREATE TABLE t(a, b)", vec![t("1")]),
            Ok(vec![t("1"), Value::Null])
        );
        assert_eq!(
            r(
                "CREATE TABLE t(a, b NOT NULL DEFAULT 'x' COLLATE NOCASE)",
                vec![t("1")]
            ),
            Ok(vec![t("1"), t("x")])
        );
        assert_eq!(
            r(
                "CREATE TABLE t(a, b, PRIMARY KEY(b, b COLLATE NOCASE)) WITHOUT ROWID",
                vec![t("k"), t("k"), t("v")]
            ),
            Ok(vec![t("v"), t("k")])
        );
        assert_eq!(
            r(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, a)",
                vec![Value::Null, t("a")]
            ),
            Ok(vec![Value::Null, t("a")])
        );
        assert!(r("CREATE TABLE t(a)", vec![t("1"), t("2")]).is_err());
        for sql in [
            "CREATE TABLE t(a, b NOT NULL)",
            "CREATE TABLE t(a, b UNIQUE)",
            "CREATE TABLE t(a, b, UNIQUE(b))",
            "CREATE TABLE t(a, b, CONSTRAINT u UNIQUE (b COLLATE NOCASE))",
            "CREATE TABLE t(a, b PRIMARY KEY)",
            "CREATE TABLE t(a, b, PRIMARY KEY(a, b))",
            "CREATE TABLE t(a, b AS (a) STORED)",
            "CREATE TABLE t(a, b DEFAULT (1 + 2))",
            "CREATE TABLE t(a, b, PRIMARY KEY(a, b)) WITHOUT ROWID",
        ] {
            assert!(r(sql, vec![t("1")]).is_err(), "{sql}");
        }
        assert!(
            r(
                "CREATE TABLE t(a, b, PRIMARY KEY(b, b COLLATE NOCASE)) WITHOUT ROWID",
                vec![t("k"), t("v"), t("a")]
            )
            .is_err()
        );
        assert!(
            r(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, a)",
                vec![Value::Int(3), t("a")]
            )
            .is_err()
        );
        assert!(
            r(
                "CREATE TABLE t(a, b PRIMARY KEY) WITHOUT ROWID",
                vec![Value::Null, t("a")]
            )
            .is_err()
        );
        // keys.sqlite's k7 with its two COLLATE clauses swapped, which keeps the schema's
        // length: b is then NOCASE, and its key two copies of it, which the rows do not hold.
        let mut f = fixture("keys.sqlite");
        let (from, to) = (
            b"b TEXT COLLATE NOCASE COLLATE RTRIM",
            b"b TEXT COLLATE RTRIM COLLATE NOCASE",
        );
        let at = memchr::memmem::find(&f, from).unwrap();
        f[at..at + from.len()].copy_from_slice(to);
        match super::super::extract(super::super::Kind::Sqlite, &f) {
            Err(DocError::Broken(m)) => assert!(m.starts_with("table k7: "), "{m}"),
            other => panic!("{:?}", other.map(|e| e.text)),
        }
    }

    /// Codex M4 code r7 #1: a column's constraints are read at its definition's level, never
    /// inside a CHECK's or a DEFAULT's expression nor inside a string, so a column added with
    /// `CHECK(a IS NOT NULL)` is no NOT NULL column. The DEFAULT of a foreign key's
    /// `ON DELETE SET DEFAULT` is no DEFAULT clause, and of several DEFAULT clauses the last
    /// counts, as in SQLite. Each case was checked against Python's sqlite3.
    #[test]
    fn constraints_are_read_at_the_definitions_level() {
        let t = |s: &str| Value::Text(s.into());
        let added = |def: &str| {
            rebuild(
                &columns(&format!("CREATE TABLE t(a TEXT, {def})")),
                vec![t("1")],
            )
        };
        for (def, want) in [
            ("b CHECK(a IS NOT NULL)", Value::Null),
            ("b CHECK(a NOT NULL)", Value::Null),
            ("b CHECK((a) IS NOT NULL) COLLATE nocase", Value::Null),
            ("b CHECK (b NOT IN ('PRIMARY KEY', 'UNIQUE'))", Value::Null),
            ("b TEXT CHECK(b <> 'x' COLLATE rtrim)", Value::Null),
            ("b TEXT CHECK(CAST(a AS TEXT) <> '') DEFAULT 'q'", t("q")),
            ("b CHECK(a GLOB '*') NOT NULL DEFAULT 'z'", t("z")),
            ("b CHECK(a NOTNULL) DEFAULT 3", Value::Int(3)),
            ("b DEFAULT 'NOT NULL'", t("NOT NULL")),
            ("b REFERENCES p(id) ON DELETE SET DEFAULT", Value::Null),
            ("b REFERENCES p MATCH full NOT DEFERRABLE", Value::Null),
            ("b REFERENCES p ON UPDATE SET DEFAULT DEFAULT 'x'", t("x")),
            ("b DEFAULT 1 DEFAULT 2", Value::Int(2)),
            ("b DEFAULT 'x' DEFAULT NULL", Value::Null),
            ("b CONSTRAINT desc CHECK(1)", Value::Null),
        ] {
            assert_eq!(added(def), Ok(vec![t("1"), want]), "{def}");
        }
        for def in [
            "b CHECK(a IS NOT NULL) NOT NULL",
            "b REFERENCES p ON DELETE SET DEFAULT NOT NULL",
            "b DEFAULT 'x' DEFAULT (1 + 2)",
        ] {
            assert!(added(def).is_err(), "{def}");
        }
        // The other rules read the definition's level too.
        let c =
            columns("CREATE TABLE t(id INTEGER CONSTRAINT desc PRIMARY KEY, b CHECK(b <> 'x'))");
        assert_eq!(c.ipk, Some(0));
        let c = columns("CREATE TABLE t(a TEXT CHECK(CAST(a AS TEXT) <> ''), b)");
        assert_eq!(c.stored, [0, 1]);
        let c = columns(
            "CREATE TABLE t(a, b TEXT CHECK(b <> 'x' COLLATE rtrim), PRIMARY KEY(b, b COLLATE rtrim)) \
             WITHOUT ROWID",
        );
        assert_eq!(c.stored, [1, 1, 0]);
        let e = extracted("constraints.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        assert!(
            text.starts_with("needle\t\t\tneedle last default\tneedle check\n"),
            "{text}"
        );
        assert_eq!(trail(&e, "needle\t"), ["checked", "row 1", "a"]);
        assert_eq!(trail(&e, "needle last"), ["checked", "row 1", "d"]);
        assert_eq!(trail(&e, "needle check"), ["checked", "row 1", "e"]);
        assert!(text.contains("\nlater\tb\t\t2\tlater check\n"), "{text}");
    }

    /// Fable M4 code r8 #1: `CAST(<constant> AS <type>)` is a constant to SQLite, whose
    /// `sqlite3ValueFromExpr` gives the constant the type's affinity, casts it to the type and
    /// then gives it the column's affinity, so a column added with such a DEFAULT may be missing
    /// from a row. Each case is what SQLite 3.46.1, through Python's sqlite3, gives a row written
    /// before `ALTER TABLE t ADD COLUMN <def>`; the same for the other DEFAULTs common in a
    /// schema.
    #[test]
    fn a_cast_default_is_evaluated_as_sqlite_does() {
        let t = |s: &str| Value::Text(s.into());
        let (i, f, blob) = (Value::Int, Value::Float, Value::Blob);
        let added = |def: &str| {
            rebuild(
                &columns(&format!("CREATE TABLE t(a TEXT, {def})")),
                vec![t("1")],
            )
        };
        for (def, want) in [
            ("b DEFAULT (CAST(1 AS TEXT))", t("1")),
            ("b DEFAULT (CAST('12abc' AS INTEGER))", i(12)),
            ("b REAL DEFAULT (CAST(1 AS INT))", f(1.0)),
            ("b DEFAULT (CAST('A' AS BLOB))", blob(1)),
            ("b DEFAULT (CAST(CAST(1.5 AS INT) AS TEXT))", t("1")),
            ("b INTEGER DEFAULT (CAST('7.0' AS TEXT))", i(7)),
            ("b NOT NULL DEFAULT (CAST(0 AS TEXT))", t("0")),
            ("b DEFAULT (CAST(1 AS TEXT)) CHECK(b IS NOT NULL)", t("1")),
            ("b DEFAULT (CAST(7.0 AS BLOB))", blob(3)),
            ("b DEFAULT (CAST(1.5 AS TEXT))", t("1.5")),
            ("b DEFAULT (CAST(100.0 AS TEXT))", t("100.0")),
            ("b DEFAULT (CAST(1e2 AS TEXT))", t("1e2")),
            ("b DEFAULT (CAST(1.0e0 AS TEXT))", t("1.0e0")),
            ("b DEFAULT (CAST(1.50 AS TEXT))", t("1.50")),
            ("b DEFAULT (CAST(-1.50 AS TEXT))", t("-1.50")),
            ("b DEFAULT (CAST(-0.0 AS TEXT))", t("-0.0")),
            ("b DEFAULT (CAST(0x10 AS TEXT))", t("16")),
            ("b DEFAULT (CAST(1_000 AS TEXT))", t("1000")),
            ("b DEFAULT (CAST(TRUE AS TEXT))", t("1")),
            ("b DEFAULT (CAST(-1 AS TEXT))", t("-1")),
            ("b DEFAULT (CAST(-'5' AS TEXT))", t("-5")),
            ("b DEFAULT (CAST(+(-1) AS TEXT))", t("-1")),
            ("b DEFAULT (CAST(- -1 AS TEXT))", t("1")),
            ("b DEFAULT (CAST((1) AS TEXT))", t("1")),
            ("b DEFAULT (CAST(x'C3A9' AS TEXT))", t("é")),
            ("b DEFAULT (CAST(x'41' AS TEXT))", t("A")),
            ("b DEFAULT (CAST(CAST('abc' AS BLOB) AS TEXT))", t("abc")),
            ("b DEFAULT (CAST(1e2 AS BLOB))", blob(5)),
            ("b DEFAULT (CAST(0x10 AS BLOB))", blob(2)),
            ("b DEFAULT (CAST(1.50 AS BLOB))", blob(3)),
            ("b DEFAULT (CAST(-1.50 AS BLOB))", blob(4)),
            ("b DEFAULT (CAST(TRUE AS BLOB))", blob(1)),
            ("b DEFAULT (CAST(-'5' AS BLOB))", blob(2)),
            ("b DEFAULT (CAST('1e2' AS INTEGER))", i(100)),
            ("b DEFAULT (CAST('1.9' AS INTEGER))", i(1)),
            ("b DEFAULT (CAST('-1.9' AS INTEGER))", i(-1)),
            ("b DEFAULT (CAST('  42  ' AS INTEGER))", i(42)),
            ("b DEFAULT (CAST(' -12abc' AS INTEGER))", i(-12)),
            ("b DEFAULT (CAST('+5x' AS INTEGER))", i(5)),
            ("b DEFAULT (CAST('1e5x' AS INTEGER))", i(1)),
            ("b DEFAULT (CAST('abc' AS INTEGER))", i(0)),
            ("b DEFAULT (CAST('0x10' AS INTEGER))", i(0)),
            ("b DEFAULT (CAST('' AS INTEGER))", i(0)),
            ("b DEFAULT (CAST(x'' AS INTEGER))", i(0)),
            ("b DEFAULT (CAST(x'3132' AS INTEGER))", i(12)),
            (
                "b DEFAULT (CAST('99999999999999999999' AS INTEGER))",
                i(i64::MAX),
            ),
            (
                "b DEFAULT (CAST('99999999999999999999x' AS INTEGER))",
                i(i64::MAX),
            ),
            (
                "b DEFAULT (CAST('-99999999999999999999x' AS INTEGER))",
                i(i64::MIN),
            ),
            ("b DEFAULT (CAST(1e30 AS INTEGER))", i(i64::MAX)),
            ("b DEFAULT (CAST(-1e30 AS INTEGER))", i(i64::MIN)),
            ("b DEFAULT (CAST(1e400 AS INTEGER))", i(i64::MAX)),
            (
                "b DEFAULT (CAST(9223372036854775808 AS INTEGER))",
                i(i64::MAX),
            ),
            ("b DEFAULT (CAST(1e2 AS INTEGER))", i(100)),
            ("b DEFAULT (CAST(1.9 AS INTEGER))", i(1)),
            ("b DEFAULT (CAST(-1.9 AS INTEGER))", i(-1)),
            ("b DEFAULT (CAST('12abc' AS REAL))", f(12.0)),
            ("b DEFAULT (CAST('1.5e1x' AS REAL))", f(15.0)),
            ("b DEFAULT (CAST('.5' AS REAL))", f(0.5)),
            ("b DEFAULT (CAST('abc' AS REAL))", f(0.0)),
            ("b DEFAULT (CAST('' AS REAL))", f(0.0)),
            ("b DEFAULT (CAST('-0.0' AS REAL))", f(0.0)),
            ("b DEFAULT (CAST(3 AS REAL))", f(3.0)),
            ("b DEFAULT (CAST('3' AS REAL))", f(3.0)),
            (
                "b DEFAULT (CAST(9223372036854775807 AS REAL))",
                f(9223372036854775807.0),
            ),
            ("b DEFAULT (CAST('3.0' AS NUMERIC))", i(3)),
            ("b DEFAULT (CAST('3.5' AS NUMERIC))", f(3.5)),
            ("b DEFAULT (CAST('12abc' AS NUMERIC))", i(12)),
            ("b DEFAULT (CAST('1.5e1x' AS NUMERIC))", i(15)),
            ("b DEFAULT (CAST('1e2' AS NUMERIC))", i(100)),
            ("b DEFAULT (CAST('abc' AS NUMERIC))", i(0)),
            ("b DEFAULT (CAST('0x10' AS NUMERIC))", i(0)),
            ("b DEFAULT (CAST('' AS NUMERIC))", i(0)),
            ("b DEFAULT (CAST(x'41' AS NUMERIC))", i(0)),
            ("b DEFAULT (CAST(3.0 AS NUMERIC))", i(3)),
            ("b DEFAULT (CAST(1.5 AS NUMERIC))", f(1.5)),
            (
                "b DEFAULT (CAST('9223372036854775807' AS NUMERIC))",
                i(i64::MAX),
            ),
            (
                "b DEFAULT (CAST('9223372036854775808' AS NUMERIC))",
                f(9223372036854775808.0),
            ),
            ("b DEFAULT (CAST(NULL AS TEXT))", Value::Null),
            ("b DEFAULT (CAST(NULL AS INTEGER))", Value::Null),
            ("b DEFAULT (CAST(NULL AS BLOB))", Value::Null),
            // The type's affinity by SQLite's rules, from all its words.
            ("b DEFAULT (CAST(5 AS VARCHAR(10)))", t("5")),
            ("b DEFAULT (CAST(5 AS \"TEXT\"))", t("5")),
            ("b DEFAULT (CAST(5 AS 'TEXT'))", t("5")),
            ("b DEFAULT (CAST('5' AS NONE))", i(5)),
            ("b DEFAULT (CAST('5' AS FLOAT))", f(5.0)),
            ("b DEFAULT (CAST('5' AS DOUBLE PRECISION))", f(5.0)),
            ("b DEFAULT (CAST('5' AS UNSIGNED BIG INT))", i(5)),
            ("b DEFAULT (CAST(('5') AS INTEGER))", i(5)),
            ("b DEFAULT (CAST(+'5' AS INTEGER))", i(5)),
            ("b DEFAULT ((CAST('5' AS INTEGER)))", i(5)),
            ("b DEFAULT (cast('5' as integer))", i(5)),
            ("b DEFAULT (CAST (1 AS TEXT))", t("1")),
            ("b DEFAULT (CAST(1 AS TEXT) )", t("1")),
            // A CAST in a CAST: the outer type's affinity applies to the inner one's value.
            ("b DEFAULT (CAST(CAST('7' AS INTEGER) AS REAL))", f(7.0)),
            ("b DEFAULT (CAST(CAST('7.5' AS REAL) AS INTEGER))", i(7)),
            ("b DEFAULT (CAST(CAST(3 AS REAL) AS NUMERIC))", i(3)),
            ("b TEXT DEFAULT (CAST(CAST(3 AS REAL) AS NUMERIC))", t("3")),
            // A sign before a CAST.
            ("b DEFAULT (-CAST('5' AS INTEGER))", i(-5)),
            ("b DEFAULT (-CAST('5' AS TEXT))", i(-5)),
            ("b TEXT DEFAULT (-CAST('5' AS TEXT))", t("-5")),
            // And then the column's affinity.
            ("b TEXT DEFAULT (CAST(1.5 AS REAL))", t("1.5")),
            ("b TEXT DEFAULT (CAST(2 AS REAL))", t("2.0")),
            ("b TEXT DEFAULT (CAST('5' AS INTEGER) )", t("5")),
            ("b INTEGER DEFAULT (CAST(2.0 AS TEXT))", i(2)),
            ("b INTEGER DEFAULT (CAST('2.5' AS TEXT))", f(2.5)),
            ("b REAL DEFAULT (CAST('2' AS TEXT))", f(2.0)),
            ("b NUMERIC DEFAULT (CAST(2.0 AS REAL))", i(2)),
            ("b DEFAULT (CAST(2 AS REAL))", f(2.0)),
            ("b BLOB DEFAULT (CAST(2 AS REAL))", f(2.0)),
            ("b TEXT DEFAULT (CAST(x'41' AS BLOB))", blob(1)),
            ("b INTEGER DEFAULT (CAST('7' AS BLOB))", blob(1)),
            // The other DEFAULTs common in a schema.
            ("b DEFAULT 7.0", f(7.0)),
            ("b DEFAULT (7.0)", f(7.0)),
            ("b DEFAULT -7.0", f(-7.0)),
            ("b DEFAULT (-(7.0))", f(-7.0)),
            ("b DEFAULT 1e2", f(100.0)),
            ("b DEFAULT 9223372036854775808.0", f(9223372036854775808.0)),
            ("b DEFAULT '7.0'", t("7.0")),
            ("b TEXT DEFAULT 7.0", t("7.0")),
            ("b DEFAULT 'abc' COLLATE nocase", t("abc")),
            ("b DEFAULT ''", t("")),
            ("b DEFAULT x''", blob(0)),
            ("b DEFAULT 0", i(0)),
            ("b BOOLEAN DEFAULT FALSE", i(0)),
            ("b BOOLEAN DEFAULT 'false'", t("false")),
            ("b DATETIME DEFAULT '1970-01-01'", t("1970-01-01")),
            (
                "b DATE DEFAULT '2020-01-01 00:00:00'",
                t("2020-01-01 00:00:00"),
            ),
            ("b VARCHAR(10) DEFAULT 0", t("0")),
            ("b DECIMAL(10,2) DEFAULT 0.00", i(0)),
            ("b DECIMAL(10,2) DEFAULT '0.00'", i(0)),
            ("b NUMERIC DEFAULT 1.50", f(1.5)),
            ("b NUMERIC DEFAULT -0.0", i(0)),
            ("b REAL DEFAULT '-0.0'", f(0.0)),
            ("b JSON DEFAULT '{}'", t("{}")),
            ("b TEXT DEFAULT 'null'", t("null")),
            // Codex M4 code r9 #3: a blob made text keeps its bytes, and is shown with U+FFFD
            // only at the end.
            ("b DEFAULT (CAST(CAST(x'FFFE' AS TEXT) AS BLOB))", blob(2)),
            ("b DEFAULT (CAST(CAST(x'C3A9FF' AS TEXT) AS BLOB))", blob(3)),
            ("b DEFAULT (CAST(x'41FF42' AS TEXT))", t("A\u{fffd}B")),
            (
                "b DEFAULT (CAST(CAST(CAST(x'FF41' AS TEXT) AS BLOB) AS TEXT))",
                t("\u{fffd}A"),
            ),
            ("b DEFAULT (CAST(CAST(x'FF31' AS TEXT) AS INTEGER))", i(0)),
            (
                "b DEFAULT (CAST(CAST(x'3231FF' AS TEXT) AS INTEGER))",
                i(21),
            ),
            ("b DEFAULT (-CAST(x'FF' AS TEXT))", i(0)),
            ("b INTEGER DEFAULT (CAST(x'35FF' AS TEXT))", t("5\u{fffd}")),
            (
                "b BLOB DEFAULT (CAST(x'FFFE' AS TEXT))",
                t("\u{fffd}\u{fffd}"),
            ),
            // Fable M4 code r9 #2: a CAST to no type is one to NUMERIC.
            ("b DEFAULT (CAST(1 AS))", i(1)),
            ("b DEFAULT (CAST('5' AS \"\"))", i(5)),
            ("b DEFAULT (CAST('5' AS ''))", i(5)),
            ("b DEFAULT (CAST('5x' AS))", i(5)),
            ("b DEFAULT (CAST(x'35' AS))", i(5)),
            ("b DEFAULT (CAST(1.5 AS))", f(1.5)),
            ("b TEXT DEFAULT (CAST(1 AS))", t("1")),
            ("b DEFAULT (CAST('abc' AS))", i(0)),
            // Fable M4 code r9 #3: a vertical tab is white space around a number.
            ("b DEFAULT (CAST('\x0b42' AS INTEGER))", i(42)),
            ("b DEFAULT (CAST('\x0b42' AS REAL))", f(42.0)),
            ("b DEFAULT (CAST('\x0b42' AS NUMERIC))", i(42)),
            ("b DEFAULT (CAST('\x0b4.5\x0b' AS NUMERIC))", f(4.5)),
            ("b INTEGER DEFAULT '\x0b42'", i(42)),
            ("b INTEGER DEFAULT '\x0b42\x0b'", i(42)),
            ("b REAL DEFAULT '42\x0b'", f(42.0)),
            ("b DEFAULT (-'\x0b5')", i(-5)),
        ] {
            assert_eq!(added(def), Ok(vec![t("1"), want.clone()]), "{def}");
            // The value as the Extractor prints it, which a -0.0 would else hide from the
            // comparison above.
            let got = added(def).unwrap().pop().unwrap();
            assert_eq!(got.shown(), want.shown(), "{def}");
        }
        // In a UTF-16 database a text made a blob is its UTF-16 bytes, and a blob written as such
        // is read as UTF-8, without an odd last byte when made text: the same in UTF-16le and
        // UTF-16be.
        for (def, want) in [
            ("b DEFAULT (CAST('A' AS BLOB))", blob(2)),
            ("b DEFAULT (CAST('é' AS BLOB))", blob(2)),
            ("b DEFAULT (CAST(7.0 AS BLOB))", blob(6)),
            ("b DEFAULT (CAST(-'5' AS BLOB))", blob(4)),
            ("b DEFAULT (CAST(x'4100' AS TEXT))", t("A\0")),
            ("b DEFAULT (CAST(x'0041' AS TEXT))", t("\0A")),
            ("b DEFAULT (CAST(x'410042' AS TEXT))", t("A\0")),
            ("b DEFAULT (CAST(x'31003200' AS INTEGER))", i(1)),
            ("b DEFAULT (CAST(x'00310032' AS INTEGER))", i(0)),
            ("b DEFAULT (CAST(x'3132' AS INTEGER))", i(12)),
            ("b DEFAULT (CAST(x'31003200' AS REAL))", f(1.0)),
            ("b DEFAULT (CAST(x'31003200' AS NUMERIC))", i(1)),
            ("b DEFAULT (CAST(CAST('abc' AS BLOB) AS TEXT))", t("abc")),
            ("b DEFAULT (-CAST('5' AS BLOB))", i(-5)),
            ("b DEFAULT x'0041'", blob(2)),
            // Codex M4 code r9 #3: read as UTF-8, a blob's bytes that are not are U+FFFD.
            ("b DEFAULT (CAST(CAST(x'FFFE' AS TEXT) AS BLOB))", blob(4)),
            ("b DEFAULT (CAST(CAST(x'FF' AS TEXT) AS BLOB))", blob(0)),
            ("b DEFAULT (CAST(CAST(x'C3A9FF' AS TEXT) AS BLOB))", blob(2)),
            ("b DEFAULT (CAST(CAST(x'FFFE41' AS TEXT) AS BLOB))", blob(4)),
            ("b DEFAULT (CAST(x'41FF42' AS TEXT))", t("A\u{fffd}")),
            ("b INTEGER DEFAULT (CAST(x'35' AS TEXT))", t("")),
            (
                "b DEFAULT (CAST(CAST(x'3231FF' AS TEXT) AS INTEGER))",
                i(21),
            ),
            ("b DEFAULT (CAST(1 AS))", i(1)),
            ("b DEFAULT (CAST('\x0b42' AS INTEGER))", i(42)),
        ] {
            let got = rebuild(
                &super::columns(&format!("CREATE TABLE t(a TEXT, {def})"), true),
                vec![t("1")],
            );
            assert_eq!(got, Ok(vec![t("1"), want]), "UTF-16: {def}");
        }
        // And in UTF-8.
        assert_eq!(
            added("b DEFAULT (CAST('é' AS BLOB))"),
            Ok(vec![t("1"), blob(2)])
        );
        assert_eq!(
            added("b DEFAULT (CAST(x'410042' AS TEXT))"),
            Ok(vec![t("1"), t("A\0B")])
        );
        // SQLite refuses these with «Cannot add a column with non-constant default», so a row
        // holds the column.
        for def in [
            "b DEFAULT (CAST(1 + 1 AS TEXT))",
            "b DEFAULT (CAST(1 AS TEXT) || 'x')",
            "b DEFAULT (CAST(1 AS TEXT) COLLATE nocase)",
            "b DEFAULT ('abc' COLLATE nocase)",
            "b DEFAULT CURRENT_TIMESTAMP",
            "b DEFAULT (datetime('now'))",
            "b DEFAULT (lower('A'))",
        ] {
            assert!(added(def).is_err(), "{def}");
        }
    }

    /// Codex r1 #6: SQLite's quirk: a column's `INTEGER PRIMARY KEY DESC` is no alias of the
    /// rowid, and the row stores its value (NULL when the insert gave none); a table's
    /// `PRIMARY KEY(id DESC)` over an INTEGER column is one.
    #[test]
    fn integer_primary_key_desc_is_not_the_rowid() {
        let c = columns("CREATE TABLE t(id INTEGER PRIMARY KEY DESC, note TEXT)");
        assert_eq!(c.ipk, None);
        let c = columns("CREATE TABLE t(id INTEGER PRIMARY KEY ASC, note TEXT)");
        assert_eq!(c.ipk, Some(0));
        let c = columns("CREATE TABLE t(id INTEGER, note TEXT, PRIMARY KEY(id DESC))");
        assert_eq!(c.ipk, Some(0));
    }

    /// Codex r3 #2: only a declared type of exactly `INTEGER` makes the primary key the rowid;
    /// `INTEGER(8)` and `INTEGER UNSIGNED` are stored, NULL when the insert gave none. Each case
    /// is what SQLite 3.46.1 does with `INSERT INTO t(note) VALUES ('needle')`.
    #[test]
    fn only_the_exact_type_integer_is_the_rowid() {
        for (alias, body) in [
            (true, "id INTEGER PRIMARY KEY, note TEXT"),
            (true, "id integer primary key, note TEXT"),
            (false, "id INTEGER(8) PRIMARY KEY, note TEXT"),
            (false, "id INTEGER (8) PRIMARY KEY, note TEXT"),
            (false, "id INTEGER(8, 2) PRIMARY KEY, note TEXT"),
            (true, "id \"INTEGER\" PRIMARY KEY, note TEXT"),
            (true, "id [integer] PRIMARY KEY, note TEXT"),
            (true, "id INTEGER /* c */ PRIMARY KEY, note TEXT"),
            (false, "id UNSIGNED INTEGER PRIMARY KEY, note TEXT"),
            (false, "id INTEGER UNSIGNED PRIMARY KEY, note TEXT"),
            (true, "id INTEGER CONSTRAINT c PRIMARY KEY, note TEXT"),
            (true, "id INTEGER NOT NULL PRIMARY KEY, note TEXT"),
            (false, "id INTEGER(8) PRIMARY KEY DESC, note TEXT"),
            (false, "id INTEGER(8), note TEXT, PRIMARY KEY(id)"),
            (false, "id INTEGER(8), note TEXT, PRIMARY KEY(id DESC)"),
            (true, "id \"INTEGER\", note TEXT, PRIMARY KEY(id)"),
            (false, "id INTEGER UNSIGNED, note TEXT, PRIMARY KEY(id)"),
            (true, "id INTEGER, note TEXT, CONSTRAINT k PRIMARY KEY(id)"),
        ] {
            let c = columns(&format!("CREATE TABLE t({body})"));
            assert_eq!(c.ipk, alias.then_some(0), "{body}");
        }
        let c = columns("CREATE TABLE t(id INTEGER(8) PRIMARY KEY, note TEXT) WITHOUT ROWID");
        assert_eq!(c.ipk, None);
    }

    /// Fable r7 #1: a real prints as the `sqlite3` shell prints it, with `%!.15g`; each case is
    /// SQLite 3.46.1's `CAST(x AS TEXT)` of the same value.
    #[test]
    fn values_print_as_sqlite_shows_them() {
        assert_eq!(Value::Float(100.0).shown(), "100.0");
        assert_eq!(Value::Float(1.5).shown(), "1.5");
        for (f, shell) in [
            (0.1 + 0.2, "0.3"),
            (1.1 * 3.0, "3.3"),
            (1.0 / 3.0, "0.333333333333333"),
            (1e-5, "1.0e-05"),
            (-2.5e-7, "-2.5e-07"),
            (1e16, "1.0e+16"),
            (1e300, "1.0e+300"),
            (123456789012345.6, "123456789012346.0"),
            (999999999999999.9, "1.0e+15"),
            (8979436893026.125, "8979436893026.13"),
            (-95185635704743.25, "-95185635704743.3"),
            (0.0001, "0.0001"),
            (40.7128, "40.7128"),
            (-0.0, "0.0"),
            (f64::INFINITY, "Inf"),
            (f64::NEG_INFINITY, "-Inf"),
        ] {
            assert_eq!(Value::Float(f).shown(), shell, "{f:e}");
        }
        assert_eq!(Value::Int(-3).shown(), "-3");
        assert_eq!(Value::Null.shown(), "");
        assert_eq!(Value::Blob(12).shown(), "[blob 12 bytes]");
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/docs/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    fn extracted(name: &str) -> super::super::Extracted {
        super::super::extract(super::super::Kind::Sqlite, &fixture(name)).unwrap()
    }

    fn trail(e: &super::super::Extracted, needle: &str) -> Vec<String> {
        let at = memchr::memmem::find(&e.text, needle.as_bytes()).expect(needle);
        crate::trail::trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    #[test]
    fn tables_rows_and_columns() {
        let e = extracted("shop.sqlite");
        assert_eq!(trail(&e, "ana.needle"), ["users", "row 12", "email"]);
        // The INTEGER PRIMARY KEY is the rowid, which the row stores as NULL.
        let text = String::from_utf8_lossy(&e.text);
        assert!(
            text.contains("12\tUser 12\tana.needle@example.com\t18.0\n"),
            "{text}"
        );
        assert!(text.contains("1\tshort note\t[blob 8 bytes]\n"));
        // A value longer than its page continues on overflow pages.
        assert_eq!(trail(&e, "needle at the end"), ["notes", "row 2", "body"]);
        // WITHOUT ROWID: rows in key order, numbered by position.
        assert_eq!(trail(&e, "value of gamma"), ["kv", "row 3", "v"]);
        assert_eq!(trail(&e, "value of alpha"), ["kv", "row 1", "v"]);
        // Indexes and views are not tables.
        assert!(!text.contains("CREATE"));
    }

    /// Codex r1 #4 and #12: a WITHOUT ROWID table stores its primary key first; each value is
    /// named by its declared column and written in the declared order, with its affinity; a NULL
    /// keeps its empty field.
    #[test]
    fn values_follow_their_declared_columns() {
        let e = extracted("shop.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        assert_eq!(trail(&e, "needle payload"), ["late", "row 1", "payload"]);
        assert!(text.contains("\nneedle payload\tk1\n"), "{text}");
        assert!(text.contains("\na1\t3.0\tc1\n"), "{text}");
        assert!(text.contains("\n\tneedle gap\n"), "{text}");
        assert_eq!(trail(&e, "needle gap"), ["gaps", "row 1", "y"]);
        let c = columns("CREATE TABLE t(a, b, c, PRIMARY KEY (c DESC, a)) WITHOUT ROWID");
        assert_eq!(c.stored, [2, 0, 1]);
        assert_eq!(
            columns("CREATE TABLE t(a, b PRIMARY KEY) WITHOUT ROWID").stored,
            [1, 0]
        );
        assert_eq!(columns("CREATE TABLE t(a, b)").stored, [0, 1]);
    }

    /// Codex r2 #4: the schema is read by its tokens, so spaces, line breaks and comments
    /// between keywords, quoted names holding commas or keywords, and parentheses inside
    /// strings change nothing.
    #[test]
    fn the_schema_is_read_by_its_tokens() {
        for sql in [
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY KEY) WITHOUT\nROWID",
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY KEY) WITHOUT  ROWID",
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY KEY) without /* c */ rowid",
            "CREATE TABLE t(payload TEXT, k TEXT PRIMARY  KEY) WITHOUT -- c\n ROWID",
        ] {
            let c = columns(sql);
            assert_eq!(
                (c.without_rowid, c.stored.as_slice()),
                (true, &[1, 0][..]),
                "{sql}"
            );
        }
        let c = columns(
            "CREATE TABLE t(payload TEXT, \"k,key\" REAL, PRIMARY KEY(\"k,key\")) WITHOUT ROWID",
        );
        assert_eq!(c.names, ["payload", "k,key"]);
        assert_eq!((c.stored, c.real), (vec![1, 0], vec![false, true]));
        let c = columns("CREATE TABLE t(a /* , x */ TEXT, -- b,\n b INT, \"PRIMARY KEY\" TEXT)");
        assert_eq!(c.names, ["a", "b", "PRIMARY KEY"]);
        assert_eq!((c.ipk, c.stored), (None, vec![0, 1, 2]));
        let c = columns("CREATE TABLE t(a VARCHAR(10, 2) DEFAULT ')', b INTEGER PRIMARY KEY)");
        assert_eq!(
            (c.names, c.ipk),
            (vec!["a".to_string(), "b".to_string()], Some(1))
        );
        let e = extracted("shop.sqlite");
        assert_eq!(trail(&e, "needle odd"), ["odd", "row 1", "payload"]);
        assert!(String::from_utf8_lossy(&e.text).contains("\nneedle odd\t2.0\n"));
    }

    /// Codex r3 #6, Fable r3 #5: a VIRTUAL generated column takes no place in the record; it
    /// prints empty, and the next value keeps its own column. A STORED one is stored.
    #[test]
    fn virtual_columns_take_no_place() {
        let c = columns(
            "CREATE TABLE t(a TEXT, calc TEXT GENERATED ALWAYS AS (a) VIRTUAL, payload TEXT, \
             st TEXT AS (upper(a)) STORED, v AS (a || 'x'))",
        );
        assert_eq!(c.names, ["a", "calc", "payload", "st", "v"]);
        assert_eq!(c.stored, [0, 2, 3]);
        // Codex r4 #6: `stored` inside the expression is a column's name, not the modifier.
        let c = columns("CREATE TABLE t(a TEXT, stored TEXT, calc TEXT AS (stored), payload TEXT)");
        assert_eq!(c.stored, [0, 1, 3]);
        let c = columns("CREATE TABLE t(a, b AS ((a) || 'stored') STORED, c)");
        assert_eq!(c.stored, [0, 1, 2]);
        let e = extracted("shop.sqlite");
        assert_eq!(trail(&e, "needle gen"), ["gen", "row 1", "payload"]);
        assert!(String::from_utf8_lossy(&e.text).contains("\nfirst\t\tneedle gen\tFIRST\n"));
    }

    /// Codex r5 #2: a column added with a DEFAULT has it in the rows written before; a row
    /// written after keeps its value, and a NULL it holds stays empty.
    #[test]
    fn added_columns_have_their_default_in_older_rows() {
        let e = extracted("shop.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        assert!(text.contains("\nold\tneedle added\n"), "{text}");
        assert!(text.contains("\nnew\tneedle later\n"), "{text}");
        assert!(text.contains("\nnull\t\n"), "{text}");
        assert_eq!(trail(&e, "needle added"), ["added", "row 1", "payload"]);
        assert_eq!(trail(&e, "needle later"), ["added", "row 2", "payload"]);
        let c = |sql: &str| constant(&tokens(sql)).map(|(v, _)| v.shown());
        assert_eq!(c("-3.5"), Some("-3.5".into()));
        assert_eq!(c("('x')"), Some("x".into()));
        assert_eq!(c("(1 + 2)"), None);
    }

    /// Codex r6 #1, Fable r6 #2: the DEFAULT an older row gets is the constant SQLite reads,
    /// parentheses nested and numbers whole, with the column's affinity: an INTEGER or NUMERIC
    /// column turns `7.0` and `'7.0'` into `7`, and a TEXT column keeps a number as written.
    #[test]
    fn defaults_are_read_as_sqlite_reads_them() {
        let e = extracted("shop.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        assert!(
            text.contains("\nold\tdeep default\t0.001\t42\t7\t7\t-1\t1E+3\n"),
            "{text}"
        );
        assert!(
            text.contains("\nnew\tdeep default\t0.001\t42\t7\t7\t-1\t1000.0\n"),
            "{text}"
        );
        assert!(text.contains("\nnull\t\t\t\t\t\t\t\n"), "{text}");
        assert_eq!(text.matches("deep default").count(), 2);
        assert_eq!(trail(&e, "deep default"), ["defaults", "row 1", "deep"]);
        let c = |sql: &str| constant(&tokens(sql)).map(|(v, n)| (v.shown(), n));
        assert_eq!(c("((('x'))) NOT NULL"), Some(("x".into(), 7)));
        assert_eq!(c("1e-3"), Some(("0.001".into(), 1)));
        assert_eq!(c("-0x10"), Some(("-16".into(), 2)));
        assert_eq!(c("0xffffffffffffffff"), Some(("-1".into(), 1)));
        assert_eq!(
            c("-9223372036854775808"),
            Some(("-9223372036854775808".into(), 2))
        );
        assert_eq!(c("(1) + 2"), Some(("1".into(), 3)));
        assert_eq!(c("((1) + 2)"), None);
        assert_eq!(c("(abs(1))"), None);
        assert_eq!(c("CURRENT_TIMESTAMP"), None);
        let d = |ty: &str, sql: &str| {
            let (v, _) = constant(&tokens(sql)).unwrap();
            Lit::with_affinity(v, affinity(ty)).shown()
        };
        assert_eq!(d("INTEGER", "7.0"), "7");
        assert_eq!(d("NUMERIC", "' 7.0 '"), "7");
        assert_eq!(d("INT", "1.5"), "1.5");
        assert_eq!(d("NUMERIC", "'0x10'"), "0x10");
        assert_eq!(d("NUMERIC", "-'5'"), "-5");
        assert_eq!(d("REAL", "'3'"), "3.0");
        assert_eq!(d("TEXT", "-1e3"), "-1e3");
        assert_eq!(d("VARCHAR(5)", "+12"), "12");
        assert_eq!(d("TEXT", "0x10"), "16");
        assert_eq!(d("", "1.50"), "1.5");
        assert_eq!(d("BLOB", "'7'"), "7");
        assert_eq!(d("", "abc"), "abc");
        // Codex r7 #1: a unary plus is nothing to SQLite; only the three CURRENT_ keywords are
        // the time of the insertion; a quoted type is a type.
        assert_eq!(d("TEXT", "+'needle'"), "needle");
        assert_eq!(d("TEXT", "((+'x'))"), "x");
        assert_eq!(d("TEXT", "current_needle"), "current_needle");
        assert_eq!(c("current_time"), None);
        let q =
            columns("CREATE TABLE t(a \"INTEGER\" DEFAULT 7.0, b [REAL], c 'INTEGER' PRIMARY KEY)");
        assert_eq!((q.real, q.ipk), (vec![false, true, false], Some(2)));
        assert_eq!(q.defaults[0], Some(Value::Int(7)));
        // Fable r7 #3: a sign before what is not a number makes it one, an integer goes to a
        // TEXT column as its value, and `_` is dropped.
        assert_eq!(d("TEXT", "-'abc'"), "0");
        assert_eq!(d("", "-X'01'"), "0");
        assert_eq!(d("TEXT", "+'abc'"), "abc");
        assert_eq!(d("TEXT", "-'7.0'"), "-7");
        assert_eq!(d("TEXT", "-'12abc'"), "-12");
        assert_eq!(d("TEXT", "1_000"), "1000");
        assert_eq!(d("TEXT", "-1_0"), "-10");
        assert_eq!(d("TEXT", "-007"), "-7");
        assert_eq!(d("TEXT", "-1.50"), "-1.50");
        assert_eq!(d("INTEGER", "9223372036854775808"), "9.22337203685478e+18");
        assert_eq!(d("", "1e18"), "1.0e+18");
        // Codex r7 #1: the rows before the ALTER and after it, in the eighth table; SQLite
        // itself stores `-7.0` in the row written after, where the older one reads `-7`.
        assert!(
            text.contains("\nold\tplus default\tcurrent_word\t7\t0\t-7\t1.0e-05\n"),
            "{text}"
        );
        assert!(
            text.contains("\nnew\tplus default\tcurrent_word\t7\t0\t-7.0\t1.0e-05\n"),
            "{text}"
        );
        assert_eq!(text.matches("plus default").count(), 2);
        assert_eq!(text.matches("current_word").count(), 2);
        assert_eq!(trail(&e, "plus default"), ["signed", "row 1", "p"]);
        assert_eq!(trail(&e, "current_word"), ["signed", "row 1", "w"]);
    }

    #[test]
    fn interior_pages_keep_every_row_in_order() {
        let e = extracted("many.sqlite");
        let text = String::from_utf8_lossy(&e.text);
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 4000);
        assert_eq!(rows[0], "1\tword1");
        assert_eq!(rows[1999], "2000\tword2000");
        assert_eq!(rows[2000], "1\tword1");
        assert_eq!(rows[3999], "2000\tword2000");
        assert_eq!(trail(&e, "word1999\n"), ["t", "row 1999", "word"]);
    }

    #[test]
    fn utf16_databases_are_transcoded() {
        let e = extracted("utf16.sqlite");
        assert_eq!(e.text, "año needle\n".as_bytes());
        assert_eq!(trail(&e, "needle"), ["t", "row 1", "name"]);
    }

    /// Codex M4 code r5 #3: a cell that cannot be read (its pointer past its page, its payload
    /// past its page or its overflow pages, its record not one) makes the database `broken`,
    /// as SQLite says `database disk image is malformed`, and it is then searched as rg
    /// searches it; the walk a full text stops is no error.
    #[test]
    fn a_cell_that_cannot_be_read_breaks_the_database() {
        let broken = |name: &str, patch: &dyn Fn(&mut Vec<u8>)| {
            let mut f = fixture(name);
            patch(&mut f);
            let got = super::super::extract(super::super::Kind::Sqlite, &f);
            match got {
                Err(DocError::Broken(m)) => m,
                other => panic!("{name}: {:?}", other.map(|e| e.text)),
            }
        };
        // utf16.sqlite: one table, its row on page 2 (512 bytes), its cell at 488.
        let page = 512;
        let cell =
            |f: &mut Vec<u8>, at: u16| f[page + 8..page + 10].copy_from_slice(&at.to_be_bytes());
        assert_eq!(
            broken("utf16.sqlite", &|f| cell(f, 0xffff)),
            "page 2 has a cell past its end"
        );
        // The payload's size says 400 bytes, where 24 are left in the page.
        assert_eq!(
            broken("utf16.sqlite", &|f| f[page + 488] = 0x83),
            "page 2 has a cell that cannot be read"
        );
        // The record's header says serial type 10, which SQLite reserves.
        assert_eq!(
            broken("utf16.sqlite", &|f| f[page + 488 + 3] = 10),
            "page 2 has a cell that cannot be read"
        );
        // The record's header says it is one byte long, with no value, before the bytes of
        // its value: SQLite says `database disk image is malformed` (Codex M4 code r6 #2).
        assert_eq!(
            broken("utf16.sqlite", &|f| f[page + 488 + 2] = 1),
            "page 2 has a cell that cannot be read"
        );
        assert_eq!(
            broken("utf16.sqlite", &|f| f[page + 488 + 2] = 0),
            "page 2 has a cell that cannot be read"
        );
        // keys.sqlite: k1, a WITHOUT ROWID table (an index b-tree), on page 2.
        assert_eq!(
            broken("keys.sqlite", &|f| cell(f, 0xffff)),
            "page 2 has a cell past its end"
        );
        assert_eq!(
            broken("keys.sqlite", &|f| f[page + 497] = 0x83),
            "page 2 has a cell that cannot be read"
        );
        // A text cut at its limit is no error.
        let mut b = Builder::new(10);
        assert!(extract(&fixture("many.sqlite"), &mut b).is_ok());
        assert!(b.full());
    }

    /// Writes random tables with Python's sqlite3 module, and gives, for each, its file, its
    /// schema and the lines `SELECT` gives, sorted, as the Extractor prints a row: tabs between
    /// the values, NULL and a VIRTUAL column empty, a blob `[blob N bytes]`, anything else as
    /// SQLite casts it to text. Arguments: directory, seed, count. The test leaves out the
    /// lines of rows without text, which the Builder does not write.
    const RANDOM_TABLES: &str = r##"
import json, os, random, sqlite3, sys

out, seed, count = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
rng = random.Random(seed)
COLLS = ["BINARY", "NOCASE", "RTRIM", "binary", "nocase", "rtrim"]
TYPES = ["INTEGER", "TEXT", "REAL", "BLOB", "NUMERIC", "", "INT", "VARCHAR(10)", "DOUBLE"]
DEFAULTS = ["'dflt'", "42", "-3", "1.5", "NULL", "x'00ff'", "('par')", "-2.5", "+7", "0x10",
            "'NOT NULL'", "('PRIMARY KEY')", "'UNIQUE DESC'", "'x COLLATE nocase'", "'GENERATED AS (a)'",
            # SQLite evaluates a CAST of a constant (Fable M4 code r8 #1).
            "(CAST(1 AS TEXT))", "(CAST('12abc' AS INTEGER))", "(CAST(2 AS REAL))", "(CAST('A' AS BLOB))",
            "(CAST(1.50 AS TEXT))", "(-CAST('5' AS TEXT))", "(CAST('7.0' AS NUMERIC))"]

# Constraints whose expressions or strings hold the words of other constraints, which are no
# constraints of the column (Codex M4 code r7 #1). Each holds for any row.
def checks(c):
    return rng.choice([
        f" CHECK(c0 IS NOT NULL OR c0 IS NULL)",
        f" CHECK({c} IS NULL OR {c} NOT IN ('PRIMARY KEY', 'UNIQUE'))",
        f" CHECK(CAST({c} AS TEXT) IS NOT NULL OR {c} IS NULL)",
        f" CHECK(({c} COLLATE {rng.choice(COLLS)}) <> 'DEFAULT' OR 1)",
        f" CHECK(1 NOTNULL)",
        f" CHECK ((c0) NOT NULL OR (c0) IS NULL)",
        f" REFERENCES p(id) ON DELETE SET DEFAULT",
        f" REFERENCES p ON UPDATE SET NULL NOT DEFERRABLE",
    ])

def default():
    d = f" DEFAULT {rng.choice(DEFAULTS)}"
    # Of two DEFAULT clauses, SQLite keeps the last.
    return d + f" DEFAULT {rng.choice(DEFAULTS)}" if rng.random() < 0.15 else d

def collates():
    return "".join(f" COLLATE {rng.choice(COLLS)}" for _ in range(rng.choice([0, 0, 1, 1, 2, 3])))

def text():
    base = "".join(rng.choice("aAbBzZ ñ") for _ in range(rng.randint(0, 6)))
    if rng.random() < 0.05:
        base += "x" * rng.randint(300, 5000)
    return base + rng.choice(["", "", " ", "  "])

def value(ty):
    r = rng.random()
    if r < 0.12:
        return None
    if r < 0.35:
        return rng.randint(-2**40, 2**40) if rng.random() < 0.3 else rng.randint(-5, 300)
    if r < 0.5:
        return rng.choice([0.5, 1.25, -3.0, 1e-5, 1e16, 123.456, 0.1 + 0.2, 3.0, -0.0])
    if r < 0.9:
        return text()
    return bytes(rng.randint(0, 255) for _ in range(rng.randint(0, 20)))

cases = []
for n in range(count):
    path = os.path.join(out, f"t{n}.sqlite")
    db = sqlite3.connect(path)
    db.execute(f"PRAGMA page_size={rng.choice([512, 1024, 4096])}")
    if rng.random() < 0.2:
        db.execute(f"PRAGMA encoding='{rng.choice(['UTF-16le', 'UTF-16be'])}'")
    without = rng.random() < 0.5
    ncol = rng.randint(1, 5)
    defs, plain = [], []
    inline_pk = None
    if not without and rng.random() < 0.3:
        inline_pk = 0
    for j in range(ncol):
        ty = rng.choice(TYPES)
        d = f"c{j} {ty}{collates()}"
        if j == inline_pk:
            d += " PRIMARY KEY" + rng.choice(["", " ASC", " DESC"])
            plain.append(j)
        elif j > 0 and rng.random() < 0.2:
            d += f" GENERATED ALWAYS AS (c0) {rng.choice(['VIRTUAL', 'STORED'])}" if rng.random() < 0.5 \
                else f" AS (c0){rng.choice(['', ' STORED', ' VIRTUAL'])}"
        else:
            if rng.random() < 0.2:
                d += checks(f"c{j}")
            if rng.random() < 0.2:
                d += default()
            elif rng.random() < 0.1:
                d += " NOT NULL DEFAULT 'nn'"
            if rng.random() < 0.1:
                d += " UNIQUE"
            plain.append(j)
        defs.append(d)
    pk = ""
    if inline_pk is None and (without or rng.random() < 0.5):
        terms = []
        for _ in range(rng.randint(1, 4)):
            terms.append(f"c{rng.choice(plain)}{collates()}{rng.choice(['', '', ' ASC', ' DESC'])}")
        pk = f", PRIMARY KEY({', '.join(terms)})"
    if rng.random() < 0.15:
        pk += f", UNIQUE(c{rng.choice(plain)}{collates()})"
    sql = f"CREATE TABLE t({', '.join(defs)}{pk})" + (" WITHOUT ROWID" if without else "")
    try:
        db.execute(sql)
    except sqlite3.Error as e:
        db.close(); os.remove(path); continue
    names = [f"c{j}" for j in plain]
    def insert(k):
        for _ in range(k):
            vals = [value(None) for _ in names]
            try:
                db.execute(f"INSERT OR IGNORE INTO t({', '.join(names)}) VALUES ({', '.join('?' * len(names))})", vals)
            except sqlite3.Error:
                pass
    insert(rng.randint(0, 12))
    db.commit()
    for a in range(rng.choice([0, 0, 1, 2])):
        name = f"a{a}"
        if rng.random() < 0.2:
            add = f"{name} AS (c0) VIRTUAL"
        else:
            add = f"{name} {rng.choice(TYPES)}{collates()}"
            parts = [checks(name) for _ in range(rng.choice([0, 1, 1, 2]))]
            if rng.random() < 0.7:
                parts.append(default())
            rng.shuffle(parts)
            add += "".join(parts)
            if rng.random() < 0.2:
                add += " NOT NULL DEFAULT 5" if "DEFAULT" not in add else " NOT NULL"
        try:
            db.execute(f"ALTER TABLE t ADD COLUMN {add}")
            if "AS (" not in add:
                names.append(name)
        except sqlite3.Error:
            pass
        insert(rng.randint(0, 6))
        db.commit()
    info = db.execute("PRAGMA table_xinfo(t)").fetchall()
    exprs = []
    for cid, name, ty, notnull, dflt, pkk, hidden in info:
        q = '"' + name + '"'
        exprs.append("''" if hidden == 2 else
                     f"CASE typeof({q}) WHEN 'null' THEN '' WHEN 'blob' THEN '[blob ' || length({q}) || ' bytes]' ELSE CAST({q} AS TEXT) END")
    lines = ["\t".join(r) for r in db.execute(f"SELECT {', '.join(exprs)} FROM t")]
    final = db.execute("SELECT sql FROM sqlite_schema WHERE name = 't'").fetchone()[0]
    db.close()
    cases.append({"file": path, "sql": final, "lines": sorted(lines)})
json.dump(cases, sys.stdout)
"##;

    /// Codex M4 code r5 and r6: a row rebuilt wrongly loses values silently. Random tables,
    /// with several COLLATE clauses, keys that repeat a column, WITHOUT ROWID, DESC, generated
    /// columns, `ALTER TABLE ADD COLUMN`, and CHECK, REFERENCES and DEFAULT clauses whose
    /// expressions and strings hold other constraints' words (Codex M4 code r7 #1), written by
    /// Python's sqlite3, are read as it reads them: each table's lines are its `SELECT`'s, and
    /// none is broken.
    /// `SEARCHFOR_SQLITE_TABLES` sets how many (400 by default), `SEARCHFOR_SQLITE_SEED` the
    /// seed. The Python is `SEARCHFOR_PYTHON` when set (CI's), else `python3` (`python` on
    /// Windows). Skipped with a note without it, unless SEARCHFOR_REQUIRE_REFERENCE=1.
    #[test]
    fn random_tables_read_as_sqlite_reads_them() {
        let var = |k: &str, d: u64| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d)
        };
        let (count, seed) = (
            var("SEARCHFOR_SQLITE_TABLES", 400),
            var("SEARCHFOR_SQLITE_SEED", 1),
        );
        let dir = tempfile::tempdir().unwrap();
        let python = std::env::var_os("SEARCHFOR_PYTHON")
            .unwrap_or_else(|| if cfg!(windows) { "python" } else { "python3" }.into());
        let out = std::process::Command::new(&python)
            .args(["-I", "-c", RANDOM_TABLES])
            .arg(dir.path())
            .args([seed.to_string(), count.to_string()])
            .output();
        let out = match out {
            Ok(o) if o.status.success() => o,
            other => {
                if std::env::var_os("SEARCHFOR_REQUIRE_REFERENCE").is_some_and(|v| v == "1") {
                    panic!("{python:?} with sqlite3 is required and failed: {other:?}");
                }
                eprintln!("note: {python:?} with sqlite3 is missing; random tables skipped");
                return;
            }
        };
        let cases: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
        let (mut differ, mut broken) = (Vec::new(), 0);
        for c in &cases {
            let bytes = std::fs::read(c["file"].as_str().unwrap()).unwrap();
            let want: Vec<&str> = c["lines"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap())
                // A row without text writes nothing (`Builder::end_row`).
                .filter(|l| !l.split('\t').all(|f| f.trim().is_empty()))
                .collect();
            let got = match super::super::extract(super::super::Kind::Sqlite, &bytes) {
                Ok(e) => {
                    let text = String::from_utf8(e.text).unwrap();
                    let mut lines: Vec<String> =
                        text.split_terminator('\n').map(str::to_string).collect();
                    lines.sort();
                    Ok(lines)
                }
                Err(e) => Err(format!("{e:?}")),
            };
            broken += usize::from(got.is_err());
            if got.as_ref().map(|l| l != &want).unwrap_or(true) {
                differ.push(format!("{}\n  want {want:?}\n  got  {got:?}", c["sql"]));
            }
        }
        eprintln!(
            "random tables (seed {seed}): {} compared, {} differ, {broken} of them as broken",
            cases.len(),
            differ.len()
        );
        assert!(
            cases.len() * 10 >= count as usize * 9,
            "{} of {count}",
            cases.len()
        );
        assert!(
            differ.is_empty(),
            "{} of {} tables differ:\n{}",
            differ.len(),
            cases.len(),
            differ[..differ.len().min(8)].join("\n")
        );
    }

    #[test]
    fn what_is_not_a_database_is_an_error() {
        assert!(Db::open(b"SQLite format 3\0short").is_err());
        assert!(Db::open(&[0u8; 200]).is_err());
    }
}
