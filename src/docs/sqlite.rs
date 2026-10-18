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

    fn record(&self, p: &[u8]) -> Option<Vec<Value>> {
        let (hlen, mut at) = varint(p, 0)?;
        let hlen = hlen as usize;
        let mut body = hlen;
        let mut out = Vec::new();
        while at < hlen {
            let (t, n) = varint(p, at)?;
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
        Some(out)
    }

    /// Calls `row` with (rowid, values) for every row of the table b-tree rooted at `root`, in
    /// rowid order; for a `WITHOUT ROWID` table (an index b-tree), with the 1-based position
    /// as its id. Stops when `row` returns false. Err: a page that is not what its parent says.
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
                    if !self.index_cell(page, c, ordinal, row) {
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
                        let Some((size, a)) = varint(page, c) else {
                            continue;
                        };
                        let Some((rowid, b)) = varint(page, c + a) else {
                            continue;
                        };
                        let Some(p) = self.payload(page, c + a + b, size as usize, false) else {
                            continue;
                        };
                        if let Some(v) = self.record(&p)
                            && !row(rowid as i64, v)
                        {
                            return Ok(());
                        }
                    }
                }
                0x0a => {
                    for i in 0..cells {
                        let c = cell(i).ok_or_else(|| DocError::broken("a cell past its page"))?;
                        ordinal += 1;
                        if !self.index_cell(page, c, ordinal, row) {
                            return Ok(());
                        }
                    }
                }
                _ => return Err(DocError::broken(format!("page {n} is not a b-tree page"))),
            }
        }
        Ok(())
    }

    fn index_cell(
        &self,
        page: &[u8],
        c: usize,
        id: i64,
        row: &mut dyn FnMut(i64, Vec<Value>) -> bool,
    ) -> bool {
        let Some((size, a)) = varint(page, c) else {
            return true;
        };
        let Some(p) = self.payload(page, c + a, size as usize, true) else {
            return true;
        };
        match self.record(&p) {
            Some(v) => row(id, v),
            None => true,
        }
    }
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
    /// then the others (Codex r1 #4).
    stored: Vec<usize>,
    /// Each column's `DEFAULT`, when it is a constant: a column added by `ALTER TABLE ADD
    /// COLUMN` has it in the rows written before, whose records end earlier (M4 plan r5,
    /// Codex #2).
    defaults: Vec<Option<Value>>,
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

/// Whether a column definition is `[GENERATED ALWAYS] AS (…)` and not `STORED`: the word
/// right after the expression's closing parenthesis says, never one inside it (a column may
/// be named `stored`; M4 plan r4, Codex #6).
fn generated_virtual(part: &[Tok]) -> bool {
    let Some(open) = part
        .windows(2)
        .position(|w| w[0].is("AS") && w[1] == Tok::Punct('('))
        .map(|i| i + 1)
    else {
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

/// The index of the token after `PRIMARY KEY` in `toks`, if it holds one.
fn primary_key(toks: &[Tok]) -> Option<usize> {
    toks.windows(2)
        .position(|w| w[0].is("PRIMARY") && w[1].is("KEY"))
        .map(|i| i + 2)
}

fn columns(sql: &str) -> Columns {
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
    let mut ipk = None;
    // The primary key's columns, by name, from a column's `PRIMARY KEY` or the table's.
    let mut pk: Vec<String> = Vec::new();
    let mut virtuals: Vec<usize> = Vec::new();
    let mut defaults: Vec<Option<Value>> = Vec::new();
    for part in split_commas(&toks[open + 1..close.max(open + 1)]) {
        let Some(first) = part.first() else { continue };
        if ["CONSTRAINT", "PRIMARY", "UNIQUE", "CHECK", "FOREIGN"]
            .iter()
            .any(|k| first.is(k))
        {
            if let Some(at) = primary_key(part)
                && part.get(at) == Some(&Tok::Punct('('))
            {
                let end = part
                    .iter()
                    .rposition(|t| *t == Tok::Punct(')'))
                    .unwrap_or(part.len());
                pk = split_commas(&part[at + 1..end.max(at + 1)])
                    .iter()
                    .filter_map(|c| c.first().and_then(Tok::name))
                    .collect();
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
        let ty = ty.join(" ");
        // `GENERATED ALWAYS AS (…)` or `AS (…)`, VIRTUAL unless it says STORED: a virtual
        // column takes no place in the record (M4 plan r3, Codex #6, Fable #5).
        if generated_virtual(part) {
            virtuals.push(names.len());
        }
        if let Some(at) = primary_key(part) {
            pk = vec![name.clone()];
            if ty == "INTEGER" && !part.get(at).is_some_and(|t| t.is("DESC")) {
                ipk = Some(names.len());
            }
        }
        defaults.push(
            part.iter()
                .position(|t| t.is("DEFAULT"))
                .and_then(|d| constant(&part[d + 1..]))
                .map(|(lit, _)| lit.with_affinity(affinity(&ty))),
        );
        names.push(name);
        types.push(ty);
    }
    let key: Vec<usize> = pk
        .iter()
        .filter_map(|k| names.iter().position(|n| n.eq_ignore_ascii_case(k)))
        .collect();
    if ipk.is_none()
        && let [only] = key.as_slice()
        && types[*only] == "INTEGER"
    {
        // A table constraint `PRIMARY KEY(id)` over an INTEGER column makes it the rowid too.
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
    Columns {
        names,
        real,
        ipk: if without_rowid { None } else { ipk },
        without_rowid,
        stored,
        defaults,
    }
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
    /// A blob's bytes, which no affinity changes either.
    Blob(Vec<u8>),
}

impl Lit {
    #[cfg(test)]
    fn shown(&self) -> String {
        match self {
            Lit::Text(t) => t.clone(),
            Lit::Num(v, _) | Lit::Other(v) => v.shown(),
            Lit::Blob(b) => Value::Blob(b.len()).shown(),
        }
    }

    /// The value SQLite gives a row whose record lacks the column: the constant with the
    /// column's affinity, as `sqlite3ValueFromExpr` applies it.
    fn with_affinity(self, a: Affinity) -> Value {
        match (self, a) {
            (Lit::Other(v), _) => v,
            (Lit::Blob(b), _) => Value::Blob(b.len()),
            (Lit::Num(_, t), Affinity::Text) => Value::Text(t),
            (Lit::Num(v, _), Affinity::Blob) => v,
            (Lit::Num(v, _), a) => numeric(v, a),
            (Lit::Text(t), Affinity::Text | Affinity::Blob) => Value::Text(t),
            (Lit::Text(t), a) => match decimal(&t) {
                Some(v) => numeric(v, a),
                None => Value::Text(t),
            },
        }
    }
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
    let t = s.trim_matches(|c: char| c.is_ascii_whitespace());
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
    let b = t
        .trim_start_matches(|c: char| c.is_ascii_whitespace())
        .as_bytes();
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
        Lit::Blob(b) => numerify(&String::from_utf8_lossy(&b)),
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
/// string), a sign before one of these, or one of these in parentheses, nested or not. None for
/// an expression, and for `CURRENT_DATE`, `CURRENT_TIME` and `CURRENT_TIMESTAMP`.
fn constant(toks: &[Tok]) -> Option<(Lit, usize)> {
    match toks {
        [Tok::Punct('('), rest @ ..] => {
            let mut depth = 1usize;
            let close = rest.iter().position(|t| {
                match t {
                    Tok::Punct('(') => depth += 1,
                    Tok::Punct(')') => depth -= 1,
                    _ => {}
                }
                depth == 0
            })?;
            let (lit, n) = constant(&rest[..close])?;
            (n == close).then_some((lit, close + 2))
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
        [Tok::Punct('+'), rest @ ..] => constant(rest).map(|(lit, n)| (lit, n + 1)),
        [Tok::Punct('-'), rest @ ..] => {
            let (lit, n) = constant(rest)?;
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
        let cols = columns(&sql);
        let t = b.open("table", &name);
        let r = db.walk(root, &mut |rowid, stored| {
            b.start_row(Some(&format!("row {rowid}")));
            // The values in their declared order: a record may hold fewer (columns added
            // since), which have their DEFAULT, or more, which keep their stored place. A NULL
            // the record holds stays NULL.
            let n = cols.names.len().max(stored.len());
            let mut values = vec![Value::Null; n];
            let held = stored.len();
            for (k, v) in stored.into_iter().enumerate() {
                let at = cols.stored.get(k).copied().unwrap_or(k);
                values[at] = v;
            }
            for &at in cols.stored.iter().skip(held) {
                if let Some(Some(d)) = cols.defaults.get(at) {
                    values[at] = d.clone();
                }
            }
            for (i, v) in values.iter().enumerate() {
                let col = cols
                    .names
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("column {}", i + 1));
                let shown = match v {
                    Value::Null if Some(i) == cols.ipk => rowid.to_string(),
                    Value::Int(n) if cols.real.get(i) == Some(&true) => {
                        Value::Float(*n as f64).shown()
                    }
                    v => v.shown(),
                };
                b.cell("column", &col, &shown);
            }
            b.end_row();
            !b.full()
        });
        b.close(t);
        r?;
        if b.full() {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn what_is_not_a_database_is_an_error() {
        assert!(Db::open(b"SQLite format 3\0short").is_err());
        assert!(Db::open(&[0u8; 200]).is_err());
    }
}
