//! sql: statements, `CREATE TABLE` columns and the rows of `INSERT` and `COPY` (M6 spec §8.1).
//! One permissive dialect reads PostgreSQL, MySQL, SQLite and SQL Server scripts and dumps.
//! Preparation is lazy (M6 plan D8): a cursor finds statements one after another only as far
//! as its Matches, keeps the statement it is in and, of an `INSERT` or a `COPY`, the row it is
//! in, and builds Segments only for those.
mod lex;
mod units;

use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, Segment, SharedTrail, Trail};
use crate::text::{CutName, Pieces};
use lex::{Lexer, Tok, Token, is_kw, is_punct, is_space};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use units::Units;

pub struct Sql;

/// The schema table's bounds: tables remembered, and columns of each.
const MAX_TABLES: usize = 10_000;
const MAX_COLUMNS: usize = 1_000;

fn name_chars(t: &toml::Table) -> usize {
    t.get("name_chars")
        .and_then(|v| v.as_integer())
        .map_or(40, |n| n.max(0) as usize)
}

/// The line of a byte, counted on from the last one asked, either way.
#[derive(Debug, Clone, Copy)]
struct Lines {
    at: usize,
    line: u64,
}

impl Default for Lines {
    fn default() -> Self {
        Self { at: 0, line: 1 }
    }
}

impl Lines {
    fn of(&mut self, src: &[u8], byte: usize) -> u64 {
        let byte = byte.min(src.len());
        if byte >= self.at {
            self.line += memchr::memchr_iter(b'\n', &src[self.at..byte]).count() as u64;
        } else {
            self.line -= memchr::memchr_iter(b'\n', &src[byte..self.at]).count() as u64;
        }
        self.at = byte;
        self.line
    }

    /// The first and last lines of `r`.
    fn span(&mut self, src: &[u8], r: &Range<usize>) -> (u64, u64) {
        let first = self.of(src, r.start);
        (first, self.of(src, r.end.saturating_sub(1).max(r.start)))
    }
}

fn seg(kind: &str, name: String, (line, end_line): (u64, u64)) -> Segment {
    Segment {
        kind: kind.into(),
        name,
        line,
        end_line,
        level: None,
        signature: None,
    }
}

/// What a statement holds below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// `CREATE TABLE`: columns.
    Table,
    /// `INSERT` or `REPLACE … INTO`: rows of `VALUES`.
    Insert,
    /// `COPY`: rows of its data.
    Copy,
    Other,
}

/// A statement's leading keywords and object (M6 spec §8.1, Statements).
struct Head {
    name: String,
    pieces: Pieces,
    /// The statement's own name: from its first byte to the end of its last name token
    /// (M6 spec §3, point 5).
    label: Range<usize>,
    shape: Shape,
    /// The object's identity: its parts, unquoted words in lower case.
    table: Vec<String>,
    /// Where the tokens after the name start.
    after: usize,
}

/// Words between `CREATE`, `ALTER` or `DROP` and the object's kind, kept in the name.
const MODIFIERS: &[&str] = &[
    "OR",
    "REPLACE",
    "ALTER",
    "TEMP",
    "TEMPORARY",
    "UNLOGGED",
    "GLOBAL",
    "LOCAL",
    "UNIQUE",
    "MATERIALIZED",
    "RECURSIVE",
    "FULLTEXT",
    "SPATIAL",
    "CLUSTERED",
    "NONCLUSTERED",
    "CONSTRAINT",
    "FOREIGN",
    "TRUSTED",
    "PROCEDURAL",
];

/// Clauses between `CREATE` and the object's kind left out of the name, up to the next word of
/// `MODIFIERS` or `OBJECTS` (MySQL's `DEFINER=…`, `ALGORITHM=…`, `SQL SECURITY …`).
const CLAUSES: &[&str] = &["DEFINER", "ALGORITHM", "SQL", "SECURITY"];

/// The kinds of objects a statement creates, alters or drops.
const OBJECTS: &[&str] = &[
    "TABLE",
    "VIEW",
    "INDEX",
    "FUNCTION",
    "PROCEDURE",
    "PROC",
    "TRIGGER",
    "SCHEMA",
    "DATABASE",
    "SEQUENCE",
    "TYPE",
    "EXTENSION",
    "DOMAIN",
    "ROLE",
    "USER",
    "EVENT",
    "POLICY",
    "AGGREGATE",
    "OPERATOR",
    "RULE",
    "TABLESPACE",
    "SERVER",
    "COLLATION",
    "LANGUAGE",
    "PUBLICATION",
    "SUBSCRIPTION",
];

/// Words of a `CREATE TABLE` element that make it a table constraint, text of the statement.
const CONSTRAINTS: &[&str] = &[
    "CONSTRAINT",
    "PRIMARY",
    "UNIQUE",
    "FOREIGN",
    "CHECK",
    "KEY",
    "INDEX",
    "FULLTEXT",
    "SPATIAL",
    "EXCLUDE",
    "LIKE",
    "PERIOD",
];

fn upper(src: &[u8], t: &Token) -> Option<String> {
    (t.tok == Tok::Word)
        .then(|| String::from_utf8_lossy(&src[t.range.clone()]).to_ascii_uppercase())
}

fn is_name(t: &Token) -> bool {
    matches!(t.tok, Tok::Word | Tok::Quoted)
}

/// Appends a word or a quoted identifier to the name, its quotes removed and a doubled quote
/// decoded.
fn push_ident(src: &[u8], t: &Token, name: &mut CutName) {
    let r = t.range.clone();
    if t.tok != Tok::Quoted {
        name.push_lossy(src, r);
        return;
    }
    let close = src[r.end - 1];
    let (mut at, end) = (r.start + 1, r.end - 1);
    while at < end {
        match memchr::memchr(close, &src[at..end]) {
            Some(k) => {
                name.push_lossy(src, at..at + k);
                name.push_decoded(close as char, at + k..at + k + 2);
                at += k + 2;
            }
            None => {
                name.push_lossy(src, at..end);
                break;
            }
        }
    }
}

/// A word in lower case, or a quoted identifier decoded: one part of an object's identity.
fn identity(src: &[u8], t: &Token) -> String {
    let mut n = CutName::new(0);
    push_ident(src, t, &mut n);
    let (text, _) = n.finish();
    if t.tok == Tok::Quoted {
        text
    } else {
        text.to_lowercase()
    }
}

/// An object's name, `schema.table` with its `.`s, appended after a space: its identity, or
/// None when no name follows.
fn object(src: &[u8], lx: &mut Lexer, name: &mut CutName, end: &mut usize) -> Option<Vec<String>> {
    let first = lx.peek().filter(is_name)?;
    lx.next();
    name.push_str(" ");
    push_ident(src, &first, name);
    *end = first.range.end;
    let mut parts = vec![identity(src, &first)];
    loop {
        let mut look = lx.clone();
        let Some(dot) = look.next().filter(|t| is_punct(src, t, b'.')) else {
            break;
        };
        let Some(part) = look.next().filter(is_name) else {
            break;
        };
        name.push_lossy(src, dot.range.clone());
        push_ident(src, &part, name);
        *end = part.range.end;
        parts.push(identity(src, &part));
        *lx = look;
    }
    Some(parts)
}

/// The words up to `upto` (one of them) within the next eight, appended to the name; false,
/// with nothing appended, when none of them is.
fn keywords(
    src: &[u8],
    lx: &mut Lexer,
    name: &mut CutName,
    end: &mut usize,
    upto: &[&str],
) -> bool {
    let mut look = lx.clone();
    let mut words = Vec::new();
    for _ in 0..8 {
        let Some(t) = look.next() else { return false };
        let Some(w) = upper(src, &t) else {
            return false;
        };
        words.push(t);
        if upto.contains(&w.as_str()) {
            for t in words {
                name.push_str(" ");
                name.push_lossy(src, t.range.clone());
                *end = t.range.end;
            }
            *lx = look;
            return true;
        }
    }
    false
}

/// The head of the statement whose SQL text is `sql` (M6 spec §8.1, Statements).
fn head(src: &[u8], sql: Range<usize>, name_chars: usize, esc: bool) -> Head {
    let mut lx = Lexer::new(src, sql.start, sql.end, esc);
    let mut name = CutName::new(name_chars);
    let mut shape = Shape::Other;
    let mut table = Vec::new();
    let Some(first) = lx.next() else {
        return Head {
            name: String::new(),
            pieces: Pieces::new(),
            label: sql.start..sql.start,
            shape,
            table,
            after: sql.end,
        };
    };
    let mut end = first.range.end;
    name.push_lossy(src, first.range.clone());
    let kw = upper(src, &first).unwrap_or_default();
    // `IF [NOT] EXISTS`, `ONLY` and `CONCURRENTLY`, left out of the name.
    let skip_noise = |lx: &mut Lexer| {
        while let Some(t) = lx.peek() {
            match upper(src, &t).as_deref() {
                Some("IF" | "NOT" | "EXISTS" | "ONLY" | "CONCURRENTLY") => {
                    lx.next();
                }
                _ => break,
            }
        }
    };
    match kw.as_str() {
        "CREATE" | "ALTER" | "DROP" => {
            let mut clause = false;
            let mut kind = None;
            for _ in 0..32 {
                let Some(t) = lx.peek() else { break };
                let w = upper(src, &t);
                match w.as_deref() {
                    Some(w) if OBJECTS.contains(&w) => {
                        lx.next();
                        name.push_str(" ");
                        name.push_lossy(src, t.range.clone());
                        end = t.range.end;
                        kind = Some(w.to_string());
                        break;
                    }
                    Some(w) if MODIFIERS.contains(&w) => {
                        lx.next();
                        clause = false;
                        name.push_str(" ");
                        name.push_lossy(src, t.range.clone());
                        end = t.range.end;
                    }
                    Some(w) if CLAUSES.contains(&w) => {
                        lx.next();
                        clause = true;
                    }
                    _ if clause => {
                        lx.next();
                    }
                    _ => break,
                }
            }
            if let Some(kind) = kind {
                skip_noise(&mut lx);
                if let Some(t) = object(src, &mut lx, &mut name, &mut end) {
                    if kw == "CREATE" && kind == "TABLE" {
                        shape = Shape::Table;
                    }
                    table = t;
                }
            }
        }
        "INSERT" | "REPLACE" => {
            if keywords(src, &mut lx, &mut name, &mut end, &["INTO"])
                && let Some(t) = object(src, &mut lx, &mut name, &mut end)
            {
                shape = Shape::Insert;
                table = t;
            }
        }
        "DELETE" => {
            if keywords(src, &mut lx, &mut name, &mut end, &["FROM"]) {
                skip_noise(&mut lx);
                object(src, &mut lx, &mut name, &mut end);
            }
        }
        "UPDATE" | "TRUNCATE" | "COPY" => {
            if kw == "TRUNCATE" && lx.peek().is_some_and(|t| is_kw(src, &t, "TABLE")) {
                keywords(src, &mut lx, &mut name, &mut end, &["TABLE"]);
            }
            while lx.peek().is_some_and(|t| {
                ["ONLY", "LOW_PRIORITY", "IGNORE"]
                    .iter()
                    .any(|k| is_kw(src, &t, k))
            }) {
                lx.next();
            }
            if let Some(t) = object(src, &mut lx, &mut name, &mut end)
                && kw == "COPY"
            {
                shape = Shape::Copy;
                table = t;
            }
        }
        _ => {}
    }
    let (name, pieces) = name.finish();
    Head {
        name,
        pieces,
        label: sql.start..end,
        shape,
        table,
        after: lx.pos,
    }
}

/// A column of a `CREATE TABLE`.
struct Column {
    range: Range<usize>,
    /// Its name token.
    label: Range<usize>,
    name: String,
    pieces: Pieces,
}

/// Where an element that ends with its line ends: past its last byte `last` and the rest of
/// that line (an end-of-line comment), but before the next token at `next` and white space.
fn tail_end(src: &[u8], last: usize, next: usize) -> usize {
    let last = last.min(src.len());
    let mut end = lex::line_end(src, last).min(next.max(last));
    while end > last && is_space(src[end - 1]) {
        end -= 1;
    }
    end
}

/// The columns of the `CREATE TABLE` whose tokens after its name are `from..to`: each column
/// definition, not the table constraints (M6 spec §8.1, Columns).
fn columns(src: &[u8], from: usize, to: usize, name_chars: usize, esc: bool) -> Vec<Column> {
    let mut lx = Lexer::new(src, from, to, esc);
    let mut out = Vec::new();
    if !lx.next().is_some_and(|t| is_punct(src, &t, b'(')) {
        return out;
    }
    loop {
        let Some(first) = lx.next() else { break };
        if is_punct(src, &first, b')') {
            break;
        }
        let column = match &first.tok {
            Tok::Quoted => true,
            Tok::Word => !CONSTRAINTS
                .iter()
                .any(|k| src[first.range.clone()].eq_ignore_ascii_case(k.as_bytes())),
            _ => false,
        };
        let (mut depth, mut last, mut closed) = (0usize, first.range.end, false);
        if is_punct(src, &first, b'(') {
            depth = 1;
        }
        let mut terminator = None;
        for t in lx.by_ref() {
            if depth == 0 && (is_punct(src, &t, b',') || is_punct(src, &t, b')')) {
                closed = is_punct(src, &t, b')');
                terminator = Some(t);
                break;
            }
            if is_punct(src, &t, b'(') {
                depth += 1;
            } else if is_punct(src, &t, b')') {
                depth -= 1;
            }
            last = t.range.end;
        }
        let Some(terminator) = terminator else { break };
        if column {
            let (tail, next) = if closed {
                (last, terminator.range.start)
            } else {
                let next = lx.clone().skip_blank();
                (terminator.range.end, next)
            };
            let mut name = CutName::new(name_chars);
            push_ident(src, &first, &mut name);
            let (name, pieces) = name.finish();
            out.push(Column {
                range: first.range.start..tail_end(src, tail, next),
                label: first.range.clone(),
                name,
                pieces,
            });
        }
        if closed {
            break;
        }
    }
    out
}

/// The column names of the tables created so far (M6 spec §8.1, Rows), bounded.
#[derive(Default)]
struct Schema {
    tables: HashMap<Vec<String>, Rc<[String]>>,
    /// By a table's last part: its identity, None when two tables share it.
    by_last: HashMap<String, Option<Vec<String>>>,
}

impl Schema {
    fn insert(&mut self, table: Vec<String>, columns: Vec<String>) {
        if table.is_empty()
            || (self.tables.len() >= MAX_TABLES && !self.tables.contains_key(&table))
        {
            return;
        }
        let last = table.last().cloned().unwrap_or_default();
        self.by_last
            .entry(last)
            .and_modify(|t| {
                if t.as_ref() != Some(&table) {
                    *t = None;
                }
            })
            .or_insert_with(|| Some(table.clone()));
        let mut columns = columns;
        columns.truncate(MAX_COLUMNS);
        self.tables.insert(table, columns.into());
    }

    /// The columns of `table`: its own entry; else, for `schema.t`, an unqualified `t`; else,
    /// for an unqualified `t`, the one table named `t` in some schema. Unrelated schemas'
    /// tables of one name are never taken for each other.
    fn get(&self, table: &[String]) -> Option<Rc<[String]>> {
        if let Some(c) = self.tables.get(table) {
            return Some(c.clone());
        }
        match table {
            [] => None,
            [one] => {
                let t = self.by_last.get(one)?.as_ref()?;
                self.tables.get(t).cloned()
            }
            [.., last] => self.tables.get(std::slice::from_ref(last)).cloned(),
        }
    }
}

/// Reads the rows of a statement one after another.
#[derive(Debug, Clone, Copy)]
enum Rows {
    /// The tuples of `VALUES`, from `pos` to `end`.
    Values { pos: usize, end: usize, count: u64 },
    /// The tab-separated data lines of a `COPY`, from `pos` to `end`.
    Copy { pos: usize, end: usize, count: u64 },
}

impl Rows {
    /// The next row's bytes, with the bytes of each of its values in `values`.
    fn next_row(
        &mut self,
        src: &[u8],
        esc: bool,
        values: &mut Vec<Range<usize>>,
    ) -> Option<Range<usize>> {
        values.clear();
        match self {
            Rows::Values { pos, end, count } => {
                let mut lx = Lexer::new(src, *pos, *end, esc);
                if *count > 0 && !lx.next().is_some_and(|t| is_punct(src, &t, b',')) {
                    *pos = *end;
                    return None;
                }
                let Some(open) = lx.next().filter(|t| is_punct(src, t, b'(')) else {
                    *pos = *end;
                    return None;
                };
                let (mut depth, mut first, mut last) = (0usize, None, 0);
                let close = loop {
                    let Some(t) = lx.next() else {
                        *pos = *end;
                        return None;
                    };
                    if depth == 0 && is_punct(src, &t, b')') {
                        if let Some(f) = first {
                            values.push(f..last);
                        }
                        break t;
                    }
                    if depth == 0 && is_punct(src, &t, b',') {
                        values.push(first.take().unwrap_or(t.range.start)..last.max(t.range.start));
                        continue;
                    }
                    if is_punct(src, &t, b'(') {
                        depth += 1;
                    } else if is_punct(src, &t, b')') {
                        depth -= 1;
                    }
                    first.get_or_insert(t.range.start);
                    last = t.range.end;
                };
                *pos = lx.pos;
                *count += 1;
                Some(open.range.start..close.range.end)
            }
            Rows::Copy { pos, end, count } => {
                if *pos >= *end {
                    return None;
                }
                let start = *pos;
                let eol = memchr::memchr(b'\n', &src[start..*end]).map_or(*end, |k| start + k);
                let text_end = if eol > start && src[eol - 1] == b'\r' {
                    eol - 1
                } else {
                    eol
                };
                let mut at = start;
                for tab in memchr::memchr_iter(b'\t', &src[start..text_end]) {
                    values.push(at..start + tab);
                    at = start + tab + 1;
                }
                values.push(at..text_end);
                *pos = eol + 1;
                *count += 1;
                Some(start..text_end)
            }
        }
    }

    fn count(&self) -> u64 {
        match self {
            Rows::Values { count, .. } | Rows::Copy { count, .. } => *count,
        }
    }
}

/// The row of an `INSERT` or a `COPY` the cursor is in.
struct Row {
    range: Range<usize>,
    seg: Segment,
    values: Vec<Range<usize>>,
}

struct RowState {
    rows: Rows,
    /// The column names: the statement's list, else the table's `CREATE TABLE` columns.
    names: Option<Rc<[String]>>,
    lines: Lines,
    row: Option<Row>,
    /// No more rows.
    done: bool,
}

enum Detail {
    None,
    Columns(Vec<(Column, (u64, u64))>),
    Rows(Box<RowState>),
}

/// The statement the cursor is in, with what it holds.
struct Stmt {
    range: Range<usize>,
    label: Range<usize>,
    seg: Segment,
    detail: Detail,
}

/// The column names an `INSERT` or a `COPY` lists after its table, from `lx` on; the lexer
/// left after the list.
fn column_list(src: &[u8], lx: &mut Lexer, name_chars: usize) -> Option<Vec<String>> {
    if !lx.peek().is_some_and(|t| is_punct(src, &t, b'(')) {
        return None;
    }
    let mut look = lx.clone();
    look.next();
    let mut names = Vec::new();
    loop {
        let t = look.next().filter(is_name)?;
        let mut n = CutName::new(name_chars);
        push_ident(src, &t, &mut n);
        names.push(n.finish().0);
        let sep = look.next()?;
        if is_punct(src, &sep, b')') {
            break;
        }
        if !is_punct(src, &sep, b',') {
            return None;
        }
    }
    *lx = look;
    Some(names)
}

/// The Trails of a SQL file in file order (M6 spec §8.1, Large files).
struct SqlCursor<'s> {
    src: &'s [u8],
    name_chars: usize,
    /// A backslash escapes in strings (`lex::backslash_escapes`).
    esc: bool,
    units: Units<'s>,
    schema: Schema,
    lines: Lines,
    cur: Option<Stmt>,
    last: usize,
    /// The Trail given last and where it was: (statement start, row, column, own label).
    shared: Option<((usize, u64, usize), SharedTrail)>,
    /// What `open_from_with` found: (statement start, offset, the first byte at or past it
    /// in the statement's own bytes where a Match starts).
    found: Option<(usize, usize, Option<usize>)>,
}

impl<'s> SqlCursor<'s> {
    fn new(src: &'s [u8], name_chars: usize) -> Self {
        let units = Units::new(src);
        Self {
            src,
            name_chars,
            esc: units.esc(),
            units,
            schema: Schema::default(),
            lines: Lines::default(),
            cur: None,
            last: 0,
            shared: None,
            found: None,
        }
    }

    /// Records the columns of a `CREATE TABLE` the cursor passes.
    fn pass(&mut self, range: Range<usize>, sql_end: usize) {
        let src = self.src;
        if !src[range.start..]
            .get(..6)
            .is_some_and(|w| w.eq_ignore_ascii_case(b"CREATE"))
        {
            return;
        }
        let h = head(src, range.start..sql_end, self.name_chars, self.esc);
        if h.shape == Shape::Table {
            let cols = columns(src, h.after, sql_end, self.name_chars, self.esc);
            self.schema
                .insert(h.table, cols.into_iter().map(|c| c.name).collect());
        }
    }

    /// The statement `range`, read for its Trails.
    fn open(&mut self, range: Range<usize>, sql_end: usize) -> Stmt {
        let (src, esc) = (self.src, self.esc);
        let h = head(src, range.start..sql_end, self.name_chars, esc);
        let lines = self.lines.span(src, &range);
        let seg = seg("statement", h.name.clone(), lines);
        let mut at = Lines {
            at: range.start,
            line: lines.0,
        };
        let detail = match h.shape {
            Shape::Table => {
                let cols = columns(src, h.after, sql_end, self.name_chars, esc);
                self.schema.insert(
                    h.table.clone(),
                    cols.iter().map(|c| c.name.clone()).collect(),
                );
                Detail::Columns(
                    cols.into_iter()
                        .map(|c| {
                            let l = at.span(src, &c.range);
                            (c, l)
                        })
                        .collect(),
                )
            }
            Shape::Insert | Shape::Copy => {
                let mut lx = Lexer::new(src, h.after, sql_end, esc);
                let list = column_list(src, &mut lx, self.name_chars);
                // PostgreSQL's identity columns: `OVERRIDING SYSTEM|USER VALUE`.
                let mut look = lx.clone();
                if look.next().is_some_and(|t| is_kw(src, &t, "OVERRIDING"))
                    && look
                        .next()
                        .is_some_and(|t| is_kw(src, &t, "SYSTEM") || is_kw(src, &t, "USER"))
                    && look.next().is_some_and(|t| is_kw(src, &t, "VALUE"))
                {
                    lx = look;
                }
                let names = list.map(Rc::from).or_else(|| self.schema.get(&h.table));
                let rows = if h.shape == Shape::Insert {
                    lx.next()
                        .filter(|t| is_kw(src, t, "VALUES") || is_kw(src, t, "VALUE"))
                        .map(|_| Rows::Values {
                            pos: lx.pos,
                            end: sql_end,
                            count: 0,
                        })
                } else {
                    units::copy_rows(src, &range, sql_end, esc).map(|r| Rows::Copy {
                        pos: r.start,
                        end: r.end,
                        count: 0,
                    })
                };
                match rows {
                    Some(rows) => Detail::Rows(Box::new(RowState {
                        rows,
                        names,
                        lines: at,
                        row: None,
                        done: false,
                    })),
                    None => Detail::None,
                }
            }
            Shape::Other => Detail::None,
        };
        Stmt {
            range,
            label: h.label,
            seg,
            detail,
        }
    }

    /// Moves to the statement that holds `byte` or follows it; false when none does.
    fn reach(&mut self, byte: usize) -> bool {
        if byte < self.last {
            self.units = Units::new(self.src);
            self.schema = Schema::default();
            self.lines = Lines::default();
            self.cur = None;
            self.shared = None;
            self.found = None;
        }
        self.last = byte;
        loop {
            if let Some(s) = &self.cur
                && byte < s.range.end
            {
                return true;
            }
            self.cur = None;
            let Some(r) = self.units.next_statement() else {
                return false;
            };
            let sql_end = self.units.sql_end();
            if r.end <= byte {
                self.pass(r, sql_end);
            } else {
                let s = self.open(r, sql_end);
                self.cur = Some(s);
            }
        }
    }

    fn trail(&mut self, byte: usize) -> SharedTrail {
        let (src, esc) = (self.src, self.esc);
        let empty = (usize::MAX, 0, 0);
        let key = if !self.reach(byte) {
            empty
        } else {
            let s = self.cur.as_mut().expect("reached");
            if byte < s.range.start || s.label.contains(&byte) {
                empty
            } else {
                match &mut s.detail {
                    Detail::None => (s.range.start, 0, 0),
                    Detail::Columns(cols) => {
                        let k = cols.partition_point(|(c, _)| c.range.start <= byte);
                        match k.checked_sub(1).map(|k| (k, &cols[k].0)) {
                            Some((k, c)) if byte < c.range.end && !c.label.contains(&byte) => {
                                (s.range.start, 0, k + 1)
                            }
                            _ => (s.range.start, 0, 0),
                        }
                    }
                    Detail::Rows(rs) => {
                        while !rs.done && rs.row.as_ref().is_none_or(|r| r.range.end <= byte) {
                            let mut values = rs.row.take().map(|r| r.values).unwrap_or_default();
                            match rs.rows.next_row(src, esc, &mut values) {
                                Some(range) => {
                                    let lines = rs.lines.span(src, &range);
                                    let n = rs.rows.count();
                                    rs.row = Some(Row {
                                        seg: seg("row", format!("row {n}"), lines),
                                        range,
                                        values,
                                    });
                                }
                                None => rs.done = true,
                            }
                        }
                        match &rs.row {
                            Some(r) if r.range.start <= byte && byte < r.range.end => {
                                let k = r.values.partition_point(|v| v.start <= byte);
                                let col = match k.checked_sub(1) {
                                    Some(k) if byte < r.values[k].end => k + 1,
                                    _ => 0,
                                };
                                (s.range.start, rs.rows.count(), col)
                            }
                            _ => (s.range.start, 0, 0),
                        }
                    }
                }
            }
        };
        if let Some((k, t)) = &self.shared
            && *k == key
        {
            return t.clone();
        }
        let trail: SharedTrail = if key == empty {
            Rc::from(Vec::new())
        } else {
            let s = self.cur.as_mut().expect("reached");
            let mut t = vec![s.seg.clone()];
            match &mut s.detail {
                Detail::None => {}
                Detail::Columns(cols) => {
                    if key.2 > 0 {
                        let (c, lines) = &cols[key.2 - 1];
                        t.push(seg("column", c.name.clone(), *lines));
                    }
                }
                Detail::Rows(rs) => {
                    if key.1 > 0
                        && let Some(r) = &mut rs.row
                    {
                        t.push(r.seg.clone());
                        if key.2 > 0 {
                            let k = key.2 - 1;
                            let name = rs
                                .names
                                .as_ref()
                                .and_then(|n| n.get(k).cloned())
                                .unwrap_or_else(|| format!("column {}", k + 1));
                            // A value's Section is its row, as a CSV cell's (review r1,
                            // Fable #68).
                            let lines = (r.seg.line, r.seg.end_line);
                            t.push(seg("column", name, lines));
                        }
                    }
                }
            }
            Rc::from(t)
        };
        self.shared = Some((key, trail.clone()));
        trail
    }
}

impl Cursor for SqlCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        Ok(self.trail(byte))
    }

    /// The start of the statement that holds the last offset (its rows' Trails all end below
    /// it, but a Match between rows ends with it), or the offset.
    fn open_from(&mut self) -> Option<usize> {
        let at = self.last;
        Some(match &self.cur {
            Some(s) if s.range.start <= at && at < s.range.end => s.range.start,
            _ => at,
        })
    }

    /// In a row of an `INSERT` or a `COPY`, the row's start when no Match lies in the
    /// statement's own bytes past it (between the rows and after them): the closed rows go,
    /// and the statement stays only while a later Trail may end with it.
    fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        let at = self.last;
        let (src, esc) = (self.src, self.esc);
        let Some(s) = &self.cur else {
            return Some(at);
        };
        if !(s.range.start <= at && at < s.range.end) {
            return Some(at);
        }
        let Detail::Rows(rs) = &s.detail else {
            return Some(s.range.start);
        };
        let Some(row) = rs.row.as_ref().filter(|r| r.range.contains(&at)) else {
            return Some(s.range.start);
        };
        let found = match self.found {
            Some((stmt, from, next))
                if stmt == s.range.start && from <= at && next.is_none_or(|b| at <= b) =>
            {
                next
            }
            _ => {
                let mut rows = rs.rows;
                let mut values = Vec::new();
                let mut prev = row.range.end;
                let mut next = None;
                if !rs.done {
                    while let Some(r) = rows.next_row(src, esc, &mut values) {
                        if let Some(b) = first(prev..r.start) {
                            next = Some(b);
                            break;
                        }
                        prev = r.end;
                    }
                }
                if next.is_none() && prev < s.range.end {
                    next = first(prev..s.range.end);
                }
                self.found = Some((s.range.start, at, next));
                next
            }
        };
        Some(if found.is_some() {
            s.range.start
        } else {
            row.range.start
        })
    }
}

/// Every statement name and `CREATE TABLE` column name (`--in-headings`): the generated names
/// of rows and of unnamed columns have no Source pieces, so they are not searched.
fn names(src: &[u8], name_chars: usize) -> Vec<Name> {
    let mut units = Units::new(src);
    let esc = units.esc();
    let mut lines = Lines::default();
    let mut out = Vec::new();
    while let Some(range) = units.next_statement() {
        let sql_end = units.sql_end();
        let h = head(src, range.start..sql_end, name_chars, esc);
        let l = lines.span(src, &range);
        let stmt = seg("statement", h.name.clone(), l);
        if !h.pieces.is_empty() {
            out.push(Name {
                text: h.name.clone(),
                pieces: h.pieces.clone(),
                trail: vec![stmt.clone()],
                labels: vec![h.label.clone()],
                anchor: h.label.start,
            });
        }
        if h.shape == Shape::Table {
            let mut at = Lines {
                at: range.start,
                line: l.0,
            };
            for c in columns(src, h.after, sql_end, name_chars, esc) {
                let col = seg("column", c.name.clone(), at.span(src, &c.range));
                out.push(Name {
                    text: c.name,
                    pieces: c.pieces,
                    trail: vec![stmt.clone(), col],
                    labels: vec![h.label.clone(), c.label.clone()],
                    anchor: c.label.start,
                });
            }
        }
    }
    out
}

/// A SQL file prepared: nothing is read before a cursor asks (M6 plan D8).
struct SqlPrepared<'a> {
    src: &'a [u8],
    name_chars: usize,
}

impl Prepared for SqlPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(SqlCursor::new(self.src, self.name_chars))
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names(self.src, self.name_chars))
    }

    fn streams(&self) -> bool {
        true
    }
}

impl Processor for Sql {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let mut c = SqlCursor::new(input.bytes, name_chars(params));
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut out = vec![Vec::new(); anchors.len()];
        for i in order {
            out[i] = c.trail(anchors[i]).to_vec();
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(names(input.bytes, name_chars(params)))
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        Ok(Box::new(SqlPrepared {
            src: input.bytes,
            name_chars: name_chars(params),
        }))
    }

    fn capability(&self, _params: &toml::Table) -> Capability {
        Capability::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trails(src: &[u8], pattern: &str) -> Vec<String> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, pattern.as_bytes()).collect();
        let mut c = SqlCursor::new(src, 40);
        anchors
            .iter()
            .map(|&a| {
                c.trail(a)
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" › ")
            })
            .collect()
    }

    #[test]
    fn statement_names() {
        let src = b"create table if not exists \"My \"\"T\"\" x\".`b` (k int); -- x\n\
            ALTER TABLE ONLY public.users ADD x int;\nDROP VIEW IF EXISTS v;\n\
            INSERT IGNORE INTO t VALUES (1);\nDELETE FROM ONLY t2;\nUPDATE t3 SET a=1;\n\
            CREATE DEFINER=`root`@`%` PROCEDURE p() SELECT 1;\nTRUNCATE TABLE t4;\nWITH x AS (SELECT 1) SELECT 2;\n";
        let mut u = Units::new(src);
        let mut got = Vec::new();
        while let Some(r) = u.next_statement() {
            got.push(head(src, r.start..u.sql_end(), 40, true).name);
        }
        assert_eq!(
            got,
            [
                "create table My \"T\" x.b",
                "ALTER TABLE public.users",
                "DROP VIEW v",
                "INSERT IGNORE INTO t",
                "DELETE FROM t2",
                "UPDATE t3",
                "CREATE PROCEDURE p",
                "TRUNCATE TABLE t4",
                "WITH",
            ]
        );
    }

    #[test]
    fn rows_values_and_schema_columns() {
        let src = b"CREATE TABLE s.t (a int, \"B\" text, PRIMARY KEY (a));\n\
            INSERT INTO s.t VALUES (1, 'x,needle'), (f(1, 2), 'needle');\n\
            INSERT INTO t VALUES ('needle', 1);\n\
            INSERT INTO other.t VALUES ('needle');\n";
        assert_eq!(
            trails(src, "needle"),
            [
                "INSERT INTO s.t › row 1 › B",
                "INSERT INTO s.t › row 2 › B",
                "INSERT INTO t › row 1 › a",
                "INSERT INTO other.t › row 1 › column 1",
            ]
        );
    }

    #[test]
    fn copy_rows_split_at_tabs() {
        let src = b"COPY t (a, b) FROM stdin;\nx\tneedle\nneedle\ty\n\\.\n-- needle\n";
        assert_eq!(
            trails(src, "needle"),
            ["COPY t › row 1 › b", "COPY t › row 2 › a", ""]
        );
    }

    /// M6 spec §8.1, Large files: the scan stops at the statement of the Match, and a row is
    /// read only as far as the Matches in it.
    #[test]
    fn the_scan_reads_no_further_than_the_matches() {
        let mut src = b"INSERT INTO t VALUES ('needle'),\n".to_vec();
        for _ in 0..10_000 {
            src.extend_from_slice(b"('x'),\n");
        }
        src.extend_from_slice(b"('y');\nSELECT 'never closed\n");
        let src = src.as_slice();
        let mut c = SqlCursor::new(src, 40);
        let t = c.trail(memchr::memmem::find(src, b"needle").unwrap());
        assert_eq!(t.len(), 3);
        let stmt_end = memchr::memmem::find(src, b"('y');").unwrap() + 6;
        assert_eq!(c.units.scanned(), stmt_end);
        let Some(Stmt {
            detail: Detail::Rows(rs),
            ..
        }) = &c.cur
        else {
            panic!("an INSERT");
        };
        assert_eq!(rs.rows.count(), 1);
    }

    /// Review r1, Fable #6: mysqldump writes its views and triggers inside conditional
    /// comments, `/*!NNNNN … */`, whose text MySQL runs.
    #[test]
    fn mysqldump_conditional_comments_are_statements() {
        let src = b"/*!40101 SET NAMES utf8 */;\n\
            /*!50001 CREATE ALGORITHM=UNDEFINED */\n\
            /*!50013 DEFINER=`root`@`localhost` SQL SECURITY DEFINER */\n\
            /*!50001 VIEW `v_users` AS select `email` from `users` where `email` like '%needle%' */;\n\
            DELIMITER ;;\n\
            /*!50003 CREATE*/ /*!50017 DEFINER=`root`@`localhost`*/ /*!50003 TRIGGER `t_users` BEFORE INSERT ON `users` FOR EACH ROW SET NEW.email = 'needle' */;;\n\
            DELIMITER ;\n\
            /*M!100316 CREATE TABLE `m` (`c` int COMMENT 'needle') */;\n\
            /* a needle comment */\n";
        assert_eq!(
            trails(src, "needle"),
            [
                "CREATE VIEW v_users",
                "CREATE TRIGGER t_users",
                "CREATE TABLE m › c",
                ""
            ]
        );
    }

    /// Review r1, Fable #45: a backslash escapes only in MySQL's strings; a PostgreSQL dump
    /// (`standard_conforming_strings = on`) keeps it literal outside `E'…'`.
    #[test]
    fn a_backslash_escapes_only_where_the_dialect_says() {
        let pg = b"SET standard_conforming_strings = on;\n\
            INSERT INTO t VALUES ('C:\\', 'needle');\n\
            INSERT INTO t VALUES (E'it\\'s', 'needle');\n";
        assert_eq!(
            trails(pg, "needle"),
            [
                "INSERT INTO t › row 1 › column 2",
                "INSERT INTO t › row 1 › column 2"
            ]
        );
        let my = b"INSERT INTO t VALUES ('it\\'s', 'needle');\n";
        assert_eq!(trails(my, "needle"), ["INSERT INTO t › row 1 › column 2"]);
    }

    /// Review r1, Fable #67: `#` comments only before white space or at a line's start, so
    /// SQL Server's `#tmp` tables are names.
    #[test]
    fn hash_comments_leave_temporary_tables() {
        let src = b"# needle\nCREATE TABLE #tmp (c int DEFAULT 'needle');\n\
            INSERT INTO ##g VALUES ('needle'); # needle\nSELECT 1;\n";
        assert_eq!(
            trails(src, "needle"),
            [
                "",
                "CREATE TABLE #tmp › c",
                "INSERT INTO ##g › row 1 › column 1",
                ""
            ]
        );
    }

    /// Review r1, Fable #49: `OVERRIDING SYSTEM VALUE` before `VALUES`.
    #[test]
    fn overriding_value_rows() {
        let src = b"INSERT INTO t (a) OVERRIDING SYSTEM VALUE VALUES ('needle');\n\
            INSERT INTO t (a) OVERRIDING USER VALUE VALUES ('needle');\n\
            INSERT INTO t DEFAULT VALUES; -- needle\n";
        assert_eq!(
            trails(src, "needle"),
            ["INSERT INTO t › row 1 › a", "INSERT INTO t › row 1 › a", ""]
        );
    }

    /// Review r1, Fable #68: a value's `column` Segment spans its row's lines, as a CSV
    /// cell's does.
    #[test]
    fn a_columns_lines_are_its_rows() {
        let src = b"INSERT INTO t (a, b) VALUES ('x',\n'needle\nmore');\n";
        let mut c = SqlCursor::new(src, 40);
        let t = c.trail(memchr::memmem::find(src, b"needle").unwrap());
        assert_eq!(t.len(), 3);
        assert_eq!((t[1].line, t[1].end_line), (1, 3));
        assert_eq!((t[2].line, t[2].end_line), (t[1].line, t[1].end_line));
    }

    #[test]
    fn open_from_lets_closed_rows_go() {
        let src = b"INSERT INTO t VALUES ('a'),\n('needle'),\n('b');\n";
        let at = memchr::memmem::find(src, b"needle").unwrap();
        let row = memchr::memmem::find(src, b"('needle'").unwrap();
        let mut c = SqlCursor::new(src, 40);
        c.trail(at);
        assert_eq!(c.open_from(), Some(0));
        assert_eq!(c.open_from_with(&mut |_| None), Some(row));
        let mut c = SqlCursor::new(src, 40);
        c.trail(at);
        assert_eq!(c.open_from_with(&mut |r| Some(r.start)), Some(0));
    }
}
