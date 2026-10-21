//! csv: rows and columns of CSV and TSV files (spec §5.5, M3 plan design note 4). csv-core
//! reads the fields; this module keeps their byte ranges and walks only as far as the last
//! anchor, so a Match on row 3 of a 1 GB file reads three rows.
use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, Segment, SharedTrail, Trail};
use std::ops::Range;

use crate::text::Pieces;

pub struct Csv;

pub const DEFAULT_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Params {
    /// None: sniffed from the first line (D4).
    delimiter: Option<u8>,
    /// None: quotes are text (TSV).
    quote: Option<u8>,
    headers: bool,
    name_chars: usize,
}

fn one_byte(v: &str, what: &str) -> anyhow::Result<u8> {
    match v.as_bytes() {
        [b] if b.is_ascii() => Ok(*b),
        _ => anyhow::bail!("csv: `{what}` must be one ASCII character, got {v:?}"),
    }
}

fn params(t: &toml::Table) -> anyhow::Result<Params> {
    let delimiter = match t
        .get("delimiter")
        .and_then(|v| v.as_str())
        .unwrap_or("auto")
    {
        "auto" => None,
        d => Some(one_byte(d, "delimiter")?),
    };
    let quote = match t.get("quote").and_then(|v| v.as_str()).unwrap_or("\"") {
        "" => None,
        q => Some(one_byte(q, "quote")?),
    };
    if delimiter.is_some() && delimiter == quote {
        // csv-core cannot tell them apart (M3 code review r1, Fable #7).
        anyhow::bail!("csv: `delimiter` and `quote` must differ");
    }
    Ok(Params {
        delimiter,
        quote,
        headers: t.get("headers").and_then(|v| v.as_bool()).unwrap_or(true),
        name_chars: t
            .get("name_chars")
            .and_then(|v| v.as_integer())
            .filter(|&n| n > 0)
            .map_or(DEFAULT_NAME_CHARS, |n| n as usize),
    })
}

/// How much of a CSV file `sniff` reads at most: 1 MiB (M3 code review r4, Fable #1).
const SNIFF_LIMIT: usize = 1 << 20;

/// The delimiter of a CSV file (D4): of `,`, `;`, tab and `|`, the one that appears most often
/// in its first record outside quotes; `,` on a tie or when none appears. The first record is
/// the first line that is not blank, as csv-core skips blank lines, and it goes on across line
/// ends inside quotes; `\r` ends it as `\n` does (M3 code review r3, Fable #2). It is read once,
/// with csv-core's quoting rules (csv-core 0.1.13, `reader.rs`) and every candidate ending a
/// field: a quote opens a quoted field only as the field's first byte, whichever candidate is
/// before it; inside it `""` is a quote and a lone one closes it; a quote anywhere else, after
/// a closing quote included, is text. What a quoted field holds counts for no candidate (M3 code
/// review r1 and r2, Codex #2: `ID;"Family, Given";Plan` is `;`, and `Size";Plan;Note` too). A
/// candidate that is the quote is never picked (Fable #7): then the first other one is the
/// default. Only the file's first `SNIFF_LIMIT` bytes are read, so a first record that never
/// ends, as one whose first byte opens a quote that nothing closes, is not read to the file's end.
pub fn sniff(src: &[u8], quote: Option<u8>) -> u8 {
    const CANDIDATES: [u8; 4] = [b',', b';', b'\t', b'|'];
    #[derive(Clone, Copy)]
    enum State {
        Start,
        Unquoted,
        Quoted,
        AfterQuote,
    }
    let mut counts = [0usize; 4];
    let mut state = State::Start;
    let mut started = false;
    for &b in &src[..src.len().min(SNIFF_LIMIT)] {
        if (b == b'\n' || b == b'\r') && !matches!(state, State::Quoted) {
            if started {
                break;
            }
            // A blank line before the first record.
            continue;
        }
        started = true;
        let q = Some(b) == quote;
        let candidate = if q {
            None
        } else {
            CANDIDATES.iter().position(|&c| c == b)
        };
        state = match (state, candidate) {
            (State::Quoted, _) if q => State::AfterQuote,
            (State::Quoted, _) => State::Quoted,
            (State::AfterQuote, _) if q => State::Quoted,
            (_, Some(k)) => {
                counts[k] += 1;
                State::Start
            }
            (State::Start, _) if q => State::Quoted,
            _ => State::Unquoted,
        };
    }
    let mut best = usize::from(Some(CANDIDATES[0]) == quote);
    for k in best + 1..4 {
        if counts[k] > counts[best] {
            best = k;
        }
    }
    CANDIDATES[best]
}

/// Whether csv-core would still be inside the quotes of `field` at its end (M3 review r1,
/// Codex #7): a field quoted from its first byte, where `""` is a quote inside the field and a
/// closing quote followed by anything but a quote turns the rest into plain text, whose quotes
/// are literal (csv-core 0.1.13, `reader.rs`, states `InQuotedField` and `InDoubleEscapedQuote`).
fn ends_quoted(field: &[u8], q: u8) -> bool {
    if field.first() != Some(&q) {
        return false;
    }
    let mut quoted = true;
    let mut after_quote = false;
    for &b in &field[1..] {
        if quoted {
            if b == q {
                quoted = false;
                after_quote = true;
            }
        } else if after_quote && b == q {
            quoted = true;
            after_quote = false;
        } else {
            // Plain text from here to the end of the field.
            return false;
        }
    }
    quoted
}

/// One field: `raw` is its bytes in the Source, quotes included, without the delimiter or line
/// terminator that ends it; `end` is the byte after that delimiter or terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    raw: Range<usize>,
    end: usize,
}

/// The records of a Source, in order.
struct Records<'s> {
    src: &'s [u8],
    pos: usize,
    rdr: csv_core::Reader,
    quote: Option<u8>,
    /// The decoded text of each field of the last record, when asked for (the header).
    out: Vec<u8>,
    ends: Vec<usize>,
}

/// What `Records::next` found.
enum Next {
    Record,
    /// The file ended inside a quoted field: the structure ends at that record (spec §5.5).
    Unclosed,
    End,
}

impl<'s> Records<'s> {
    fn new(src: &'s [u8], p: &Params) -> Self {
        let delimiter = p.delimiter.unwrap_or_else(|| sniff(src, p.quote));
        let rdr = csv_core::ReaderBuilder::new()
            .delimiter(delimiter)
            .quote(p.quote.unwrap_or(b'"'))
            .quoting(p.quote.is_some())
            .build();
        Self {
            src,
            pos: 0,
            rdr,
            quote: p.quote,
            out: vec![0; 1024],
            ends: Vec::new(),
        }
    }

    /// Reads the next record into `fields`; with `text`, also the decoded text of each field
    /// (`self.out`, split at `self.ends`).
    fn next(&mut self, fields: &mut Vec<Field>, text: bool) -> Next {
        fields.clear();
        self.ends.clear();
        let src = self.src;
        let mut start = self.pos;
        let mut o = 0;
        loop {
            let (res, nin, nout) = self.rdr.read_field(&src[self.pos..], &mut self.out[o..]);
            self.pos += nin;
            if text {
                o += nout;
            }
            match res {
                csv_core::ReadFieldResult::InputEmpty => {}
                csv_core::ReadFieldResult::OutputFull => {
                    if text {
                        self.out.resize(self.out.len() * 2, 0);
                    } else {
                        o = 0;
                    }
                }
                csv_core::ReadFieldResult::End => return Next::End,
                csv_core::ReadFieldResult::Field { record_end } => {
                    // csv-core consumes the terminators of the previous record and blank lines
                    // with a record's first field.
                    if fields.is_empty() {
                        while start < self.pos && matches!(src[start], b'\n' | b'\r') {
                            start += 1;
                        }
                    }
                    let mut raw_end = self.pos;
                    if raw_end > start {
                        let last = src[raw_end - 1];
                        let ended = if record_end {
                            matches!(last, b'\n' | b'\r')
                        } else {
                            true
                        };
                        if ended {
                            raw_end -= 1;
                        }
                    }
                    let raw = start..raw_end.max(start);
                    if record_end
                        && self.pos == src.len()
                        && let Some(q) = self.quote
                        && ends_quoted(&src[raw.clone()], q)
                    {
                        return Next::Unclosed;
                    }
                    let mut end = self.pos;
                    if record_end
                        && end > 0
                        && src[end - 1] == b'\r'
                        && src.get(end) == Some(&b'\n')
                    {
                        end += 1;
                    }
                    fields.push(Field { raw, end });
                    if text {
                        self.ends.push(o);
                    }
                    start = self.pos;
                    if record_end {
                        return Next::Record;
                    }
                }
            }
        }
    }
}

/// 1-based line numbers of bytes asked in non-decreasing order.
struct Lines<'s> {
    src: &'s [u8],
    at: usize,
    line: u64,
}

impl Lines<'_> {
    fn of(&mut self, byte: usize) -> u64 {
        let to = byte.min(self.src.len());
        if to > self.at {
            self.line += memchr::memchr_iter(b'\n', &self.src[self.at..to]).count() as u64;
            self.at = to;
        }
        self.line
    }
}

/// A header cell's name (spec §5.5): its text, quotes removed and `""` decoded, trimmed of
/// ASCII white space and cut at `name_chars`, with the pieces that map it to the cell; None
/// when it is empty. `searchable` is false when csv-core read the cell in a way the pieces
/// cannot map (a lenient reading of stray quotes): the name still names the column, and
/// `--in-headings` skips it.
struct HeaderName {
    text: String,
    pieces: Pieces,
    searchable: bool,
}

fn header_name(
    src: &[u8],
    f: &Field,
    decoded: &[u8],
    quote: Option<u8>,
    name_chars: usize,
) -> Option<HeaderName> {
    let raw = f.raw.clone();
    // The cell's content, and the quote whose `""` it decodes.
    let (body, q) = match quote {
        Some(q) if raw.len() >= 2 && src[raw.start] == q && src[raw.end - 1] == q => {
            (raw.start + 1..raw.end - 1, Some(q))
        }
        _ => (raw, None),
    };
    // Built only up to its cut (`text::CutName`): a cell of megabytes costs no memory per
    // escape or per invalid byte (M3 code review r1, Codex #1).
    let mut n = crate::text::CutName::new(name_chars);
    if !decodes_to(src, body.clone(), q, decoded) {
        let d = decoded.trim_ascii();
        n.push_lossy(d, 0..d.len());
        let (text, _) = n.finish();
        if text.is_empty() {
            return None;
        }
        return Some(HeaderName {
            text,
            pieces: Vec::new(),
            searchable: false,
        });
    }
    // Trim the white space at both ends; an escape is never white space.
    let space = |b: u8| b.is_ascii_whitespace() && Some(b) != q;
    let (mut a, mut b) = (body.start, body.end);
    while a < b && space(src[a]) {
        a += 1;
    }
    while b > a && space(src[b - 1]) {
        b -= 1;
    }
    // Verbatim runs and `""` escapes, in order, until the cut is decided.
    let (mut i, mut run) = (a, a);
    while i < b && !n.full() {
        if q.is_some_and(|q| src[i] == q && i + 1 < b && src[i + 1] == q) {
            n.push_lossy(src, run..i);
            n.push_decoded(src[i] as char, i..i + 2);
            i += 2;
            run = i;
        } else {
            i += 1;
        }
    }
    n.push_lossy(src, run..i);
    let (text, pieces) = n.finish();
    if text.is_empty() {
        return None;
    }
    Some(HeaderName {
        text,
        pieces,
        searchable: true,
    })
}

/// Whether the cell `src[body]`, with `""` as `q` when quoted, decodes to what csv-core read:
/// false for a lenient reading of stray quotes, which the pieces cannot map.
fn decodes_to(src: &[u8], body: Range<usize>, q: Option<u8>, decoded: &[u8]) -> bool {
    let Some(q) = q else {
        return &src[body] == decoded;
    };
    let mut j = 0;
    let mut i = body.start;
    while i < body.end {
        let b = src[i];
        i += if b == q && i + 1 < body.end && src[i + 1] == q {
            2
        } else {
            1
        };
        if decoded.get(j) != Some(&b) {
            return false;
        }
        j += 1;
    }
    j == decoded.len()
}

/// The header row: its fields, the decoded text of each (`decoded`, split at `ends`) and its
/// line range. A name is built when asked for, so `trails` names only the columns some anchor
/// is in (M3 code review r2, Fable #3: a header of a million cells cost 270 MiB).
struct Header {
    fields: Vec<Field>,
    decoded: Vec<u8>,
    ends: Vec<usize>,
    lines: (u64, u64),
}

impl Header {
    /// Reads the header row; None when the file has no record.
    fn read(recs: &mut Records, lines: &mut Lines) -> Option<Self> {
        let mut fields = Vec::new();
        if !matches!(recs.next(&mut fields, true), Next::Record) {
            return None;
        }
        // Taken, not copied: the next records decode into a new buffer.
        let ends = std::mem::take(&mut recs.ends);
        let decoded = std::mem::replace(&mut recs.out, vec![0; 1024]);
        let first = lines.of(fields[0].raw.start);
        let last = lines.of(fields
            .last()
            .unwrap()
            .raw
            .end
            .saturating_sub(1)
            .max(fields[0].raw.start));
        Some(Self {
            fields,
            decoded,
            ends,
            lines: (first, last),
        })
    }

    /// The name of column `k` (0-based); None for an empty cell.
    fn name(&self, src: &[u8], k: usize, p: &Params) -> Option<HeaderName> {
        let from = if k == 0 { 0 } else { self.ends[k - 1] };
        header_name(
            src,
            &self.fields[k],
            &self.decoded[from..self.ends[k]],
            p.quote,
            p.name_chars,
        )
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

impl Processor for Csv {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let p = self::params(params)?;
        let src = input.bytes;
        let mut out = vec![Vec::new(); anchors.len()];
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut next = 0; // first anchor of `order` not yet placed
        let mut recs = Records::new(src, &p);
        let mut lines = Lines {
            src,
            at: 0,
            line: 1,
        };
        let mut header = None;
        if p.headers {
            match Header::read(&mut recs, &mut lines) {
                Some(h) => {
                    // Matches in the header row are in a column's name: no Trail (spec §5.5).
                    let end = h.fields.last().unwrap().end;
                    while next < order.len() && anchors[order[next]] < end {
                        next += 1;
                    }
                    header = Some(h);
                }
                None => return Ok(out),
            }
        }
        // The names of the columns asked for so far.
        let mut names: std::collections::HashMap<usize, Option<String>> = Default::default();
        let mut fields = Vec::new();
        let mut row = 0u64;
        while next < order.len() {
            match recs.next(&mut fields, false) {
                Next::End => break,
                Next::Unclosed => break,
                Next::Record => {}
            }
            row += 1;
            let (first, last) = (&fields[0], fields.last().unwrap());
            let rec_end = last.end;
            let lines_of = (
                lines.of(first.raw.start),
                lines.of(last.raw.end.saturating_sub(1).max(first.raw.start)),
            );
            let mut k = 0;
            while next < order.len() && anchors[order[next]] < rec_end {
                let a = anchors[order[next]];
                if a >= first.raw.start {
                    while k + 1 < fields.len() && a >= fields[k].end {
                        k += 1;
                    }
                    let col = header
                        .as_ref()
                        .filter(|h| k < h.fields.len())
                        .and_then(|h| {
                            names
                                .entry(k)
                                .or_insert_with(|| h.name(src, k, &p).map(|n| n.text))
                                .clone()
                        })
                        .unwrap_or_else(|| format!("column {}", k + 1));
                    out[order[next]] = vec![
                        seg("row", format!("row {row}"), lines_of),
                        seg("column", col, lines_of),
                    ];
                }
                next += 1;
            }
        }
        Ok(out)
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        Ok(Box::new(CsvPrepared::new(input, params)?))
    }

    fn capability(&self, _params: &toml::Table) -> Capability {
        Capability::Native
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let p = self::params(params)?;
        if !p.headers {
            return Ok(Vec::new());
        }
        let src = input.bytes;
        let mut recs = Records::new(src, &p);
        let mut lines = Lines {
            src,
            at: 0,
            line: 1,
        };
        let Some(header) = Header::read(&mut recs, &mut lines) else {
            return Ok(Vec::new());
        };
        Ok(header
            .fields
            .iter()
            .enumerate()
            .filter_map(|(k, f)| {
                let h = header.name(src, k, &p).filter(|h| h.searchable)?;
                Some(Name {
                    trail: vec![seg("column", h.text.clone(), header.lines)],
                    labels: vec![f.raw.clone()],
                    anchor: f.raw.start,
                    text: h.text,
                    pieces: h.pieces,
                })
            })
            .collect())
    }
}

/// A CSV file prepared once (plan Task 9.4): its Params and its header, read once; each cursor
/// reads the records after it in one pass.
struct CsvPrepared<'a> {
    input: Input<'a>,
    params: &'a toml::Table,
    p: Params,
    /// None: the file has no header row (`headers = false`), or no record at all.
    header: Option<Header>,
}

impl<'a> CsvPrepared<'a> {
    fn new(input: Input<'a>, params: &'a toml::Table) -> anyhow::Result<Self> {
        let p = self::params(params)?;
        let src = input.bytes;
        let header = if p.headers {
            let mut lines = Lines {
                src,
                at: 0,
                line: 1,
            };
            Header::read(&mut Records::new(src, &p), &mut lines)
        } else {
            None
        };
        Ok(Self {
            input,
            params,
            p,
            header,
        })
    }

    /// A reader and a line counter past the header; None when `headers` is set and the file
    /// has no record, whose Matches have no Trail. A new reader each time: csv-core 0.1.13's
    /// `Reader::clone` loses its tables, and reads every byte as a record. It starts where the
    /// header row ends, as the header is never read again: a header of a million cells is 24 MB
    /// of fields (code review r3).
    fn body(&self) -> Option<(Records<'a>, Lines<'a>)> {
        let src = self.input.bytes;
        let mut recs = Records::new(src, &self.p);
        let mut lines = Lines {
            src,
            at: 0,
            line: 1,
        };
        if self.p.headers {
            let h = self.header.as_ref()?;
            let (first, last) = (&h.fields[0], h.fields.last()?);
            // A record ends at a record boundary, where a new csv-core reader starts too.
            recs.pos = last.end;
            lines.at = last.raw.end.saturating_sub(1).max(first.raw.start);
            lines.line = h.lines.1;
        }
        Some((recs, lines))
    }
}

impl Prepared for CsvPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(CsvCursor {
            prepared: self,
            body: self.body(),
            fields: Vec::new(),
            row: 0,
            record: None,
            k: 0,
            names: Vec::new(),
            last_at: 0,
            last: None,
            key: None,
            row_name: String::new(),
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Csv.names(&self.input, self.params)
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a CSV file in file order: the record holding the Match, read once and kept
/// alone (its fields, its end and its lines), and the column names asked for so far.
struct CsvCursor<'p, 'a> {
    prepared: &'p CsvPrepared<'a>,
    /// None: no more records (the file ended, inside quotes too, or it has no record).
    body: Option<(Records<'a>, Lines<'a>)>,
    fields: Vec<Field>,
    row: u64,
    /// The current record: where its first field starts, where it ends, and its lines.
    record: Option<(usize, usize, (u64, u64))>,
    /// The column of the last Match in the current record.
    k: usize,
    /// By column: its name once asked for (`Some`), None for a column without one.
    names: Vec<Option<Option<String>>>,
    last_at: usize,
    last: Option<SharedTrail>,
    /// The record and column of `last`.
    key: Option<(u64, usize)>,
    /// `row N` of the current record.
    row_name: String,
}

impl CsvCursor<'_, '_> {
    /// The record and column of `at`, as `Csv::trails` finds them for an anchor in file order;
    /// None where it gives no Trail.
    fn locate(&mut self, at: usize) -> Option<(u64, usize)> {
        let pr = self.prepared;
        // Matches in the header row are in a column's name: no Trail (spec §5.5).
        if let Some(h) = &pr.header
            && at < h.fields.last().expect("a record has a field").end
        {
            return None;
        }
        loop {
            if let Some((_, end, _)) = self.record
                && at < end
            {
                break;
            }
            let (recs, lines) = self.body.as_mut()?;
            match recs.next(&mut self.fields, false) {
                Next::End | Next::Unclosed => {
                    self.body = None;
                    self.record = None;
                    return None;
                }
                Next::Record => {}
            }
            self.row += 1;
            let mut digits = [0u8; 20];
            let n = crate::output::flat::decimal(self.row, &mut digits);
            self.row_name.clear();
            self.row_name.push_str("row ");
            self.row_name
                .push_str(std::str::from_utf8(n).expect("ASCII digits"));
            let (first, last) = (&self.fields[0], self.fields.last().unwrap());
            let lines_of = (
                lines.of(first.raw.start),
                lines.of(last.raw.end.saturating_sub(1).max(first.raw.start)),
            );
            self.record = Some((first.raw.start, last.end, lines_of));
            self.k = 0;
        }
        let (start, _, _) = self.record.expect("read above");
        if at < start {
            return None;
        }
        while self.k + 1 < self.fields.len() && at >= self.fields[self.k].end {
            self.k += 1;
        }
        Some((self.row, self.k))
    }

    /// The Trail of column `k` of the current record.
    fn trail(&mut self, k: usize) -> Trail {
        let pr = self.prepared;
        let (_, _, lines_of) = self.record.expect("located");
        let src = pr.input.bytes;
        let col = pr
            .header
            .as_ref()
            .filter(|h| k < h.fields.len())
            .and_then(|h| {
                if self.names.len() <= k {
                    self.names.resize(k + 1, None);
                }
                self.names[k]
                    .get_or_insert_with(|| h.name(src, k, &pr.p).map(|n| n.text))
                    .clone()
            })
            .unwrap_or_else(|| format!("column {}", k + 1));
        vec![
            seg("row", self.row_name.clone(), lines_of),
            seg("column", col, lines_of),
        ]
    }
}

impl Cursor for CsvCursor<'_, '_> {
    fn trail_at(&mut self, at: usize) -> anyhow::Result<SharedTrail> {
        if at < self.last_at {
            // Before the last one: read again from the header.
            self.body = self.prepared.body();
            self.row = 0;
            self.record = None;
            self.k = 0;
            self.key = None;
        }
        self.last_at = at;
        // The same record and column as the last answer: its Trail, built once (code review
        // r1, Fable #2).
        let Some(key) = self.locate(at) else {
            self.key = None;
            return Ok(super::cursor::share(&mut self.last, Vec::new()));
        };
        if self.key == Some(key)
            && let Some(t) = &self.last
        {
            return Ok(t.clone());
        }
        let trail = self.trail(key.1);
        self.key = Some(key);
        Ok(super::cursor::share(&mut self.last, trail))
    }

    /// The current record's start when it holds the last offset: every Segment is a record's.
    fn open_from(&mut self) -> Option<usize> {
        let at = self.last_at;
        Some(match self.record {
            Some((start, end, _)) if at < end => start.min(at),
            _ => at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn table(extra: &str) -> toml::Table {
        toml::from_str(extra).unwrap()
    }

    /// (line, Trail names) of every `needle` in `src`.
    fn needles(src: &[u8], params: &str) -> Vec<Vec<String>> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        let input = Input {
            bytes: src,
            path: Path::new("x.csv"),
            format: "csv",
            disk: None,
        };
        Csv.trails(&input, &table(params), &anchors)
            .unwrap()
            .iter()
            .map(|t| {
                t.iter()
                    .map(|s| format!("{}:{}-{}", s.name, s.line, s.end_line))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn sniffs_the_delimiter_of_the_first_record() {
        // M3 code review r3, Fable #2: the blank lines before it are skipped, as csv-core skips
        // them, a quoted field goes on across lines, and `\r` ends a record as `\n` does.
        assert_eq!(sniff(b"\na;b;c\n1;2;needle\n", Some(b'"')), b';');
        assert_eq!(sniff(b"\r\n\r\na|b|c\n", Some(b'"')), b'|');
        assert_eq!(sniff(b"\"a\nb,c,d\";e;f\n", Some(b'"')), b';');
        assert_eq!(
            sniff(b"\"Name\nfull\";Plan;Note\nx;needle;y\n", Some(b'"')),
            b';'
        );
        assert_eq!(sniff(b"a,b\rc;d;e;f\r", Some(b'"')), b',');
        // Without quoting a quote is text, and the record ends at the line's end.
        assert_eq!(sniff(b"\"a\nb;c;d\n", None), b',');
        assert_eq!(sniff(b"\n\n", Some(b'"')), b',');
    }

    #[test]
    fn sniffs_only_the_first_mib() {
        // M3 code review r4, Fable #1: a first record that never ends, as an unclosed quote at
        // the first byte, is not read to the end of the file: the sniff stops at 1 MiB.
        assert_eq!(SNIFF_LIMIT, 1 << 20);
        let record = |pad: usize| {
            let mut v = b"a;b;c".to_vec();
            v.resize(5 + pad, b'x');
            v.extend_from_slice(b",,,\n");
            v
        };
        // The three `,` end at the limit's last byte, so they count.
        assert_eq!(sniff(&record(SNIFF_LIMIT - 8), Some(b'"')), b',');
        // Two of them are past it.
        assert_eq!(sniff(&record(SNIFF_LIMIT - 6), Some(b'"')), b';');
    }

    #[test]
    fn sniffs_the_delimiter_of_the_first_line() {
        assert_eq!(sniff(b"a;b;c\n1,5;2;3\n", Some(b'"')), b';');
        assert_eq!(sniff(b"\"a;b\",c\n", Some(b'"')), b',');
        assert_eq!(sniff(b"a\tb|c\n", Some(b'"')), b'\t');
        assert_eq!(sniff(b"plain\n", Some(b'"')), b',');
        // A quote inside an unquoted field is text, and so is one after a closing quote
        // (M3 code review r1, Codex #2): they open no quotes.
        assert_eq!(sniff(b"Size\";Plan;Note\n1;needle;ok\n", Some(b'"')), b';');
        assert_eq!(sniff(b"\"a\"b\";c;d,e\n", Some(b'"')), b';');
        assert_eq!(sniff(b"\"a\";b,c,d\n", Some(b'"')), b',');
        // A quoted field is recognized whichever candidate is before it, and what it holds is
        // counted for none (M3 code review r2, Codex #2).
        assert_eq!(
            sniff(
                b"ID;\"Family, Given, Middle, Suffix\";Plan\n1;\"Doe, John, Paul, Jr\";needle\n",
                Some(b'"')
            ),
            b';'
        );
        assert_eq!(sniff(b"\"a, b\"|\"c, d\"|e\n", Some(b'"')), b'|');
        assert_eq!(sniff(b"x\t\"a;b;c\"\ty\n", Some(b'"')), b'\t');
        assert_eq!(sniff(b"\"q \"\"x, y\"\" z\";a;b\n", Some(b'"')), b';');
        assert_eq!(sniff(b"a|\"b;c;d\"|\"e,f,g\"\n", Some(b'"')), b'|');
        // A closing quote followed by text: the rest of the field is text up to a candidate.
        assert_eq!(sniff(b"\"x\"y,z;w;v\n", Some(b'"')), b';');
        // A quote that never closes holds the rest of the line.
        assert_eq!(sniff(b"a;b;\"c,d,e,f\n", Some(b'"')), b';');
        // Another quote character.
        assert_eq!(sniff(b"ID;'a, b, c';x\n", Some(b'\'')), b';');
    }

    #[test]
    fn rows_and_columns_with_quotes_crlf_and_blank_lines() {
        let src = b"id,Plan,Note\r\n1,free,x\r\n\r\n2,needle,\"a\r\nneedle\"\r\n3,pro,needle\r\n";
        assert_eq!(
            needles(src, ""),
            [
                vec!["row 2:4-5", "Plan:4-5"],
                vec!["row 2:4-5", "Note:4-5"],
                vec!["row 3:6-6", "Note:6-6"]
            ]
        );
    }

    #[test]
    fn header_matches_ragged_rows_and_no_headers() {
        let src = b"needle,b\n1,2,needle\n";
        assert_eq!(
            needles(src, ""),
            [
                vec![],
                vec!["row 1:2-2".to_string(), "column 3:2-2".to_string()]
            ]
        );
        assert_eq!(
            needles(src, "headers = false"),
            [
                vec!["row 1:1-1", "column 1:1-1"],
                vec!["row 2:2-2", "column 3:2-2"]
            ]
        );
    }

    #[test]
    fn an_unclosed_quote_ends_the_structure() {
        let src = b"a,b\n1,needle\n2,\"open\nneedle\n";
        assert_eq!(
            needles(src, ""),
            [vec!["row 1:2-2".to_string(), "b:2-2".to_string()], vec![]]
        );
        // A stray quote after the closing one is text: the field is closed (Fable r1 #2).
        assert_eq!(
            needles(b"a,b\nneedle,\"x\"y\"", ""),
            [vec!["row 1:2-2", "a:2-2"]]
        );
    }

    #[test]
    fn text_after_a_closed_quote_is_not_an_open_quote() {
        // csv-core reads `"needle"junk"` as `needlejunk"`: the field is closed, at the end of
        // the file or not (M3 review r1, Codex #7).
        assert_eq!(
            needles(b"a,b\n1,\"needle\"junk\"\n", ""),
            [vec!["row 1:2-2", "b:2-2"]]
        );
        assert_eq!(
            needles(b"a,b\n1,\"needle\"junk\"\n2,end\n", ""),
            [vec!["row 1:2-2", "b:2-2"]]
        );
        // `""` inside quotes keeps the field open: this one never closes.
        assert_eq!(
            needles(b"a,b\n1,\"needle\"\"x\n", ""),
            [Vec::<String>::new()]
        );
    }

    #[test]
    fn a_quote_inside_an_unquoted_header_cell_is_text() {
        // M3 code review r1, Codex #2: the `;` file is read with `;`.
        assert_eq!(
            needles(b"Size\";Plan;Note\n1;needle;ok\n", ""),
            [vec!["row 1:2-2", "Plan:2-2"]]
        );
    }

    #[test]
    fn the_delimiter_is_never_the_quote() {
        // M3 code review r1, Fable #7: a Definition that makes them one is an error, and
        // `auto` never picks the quote.
        let e = params(&table("delimiter = \";\"\nquote = \";\"")).unwrap_err();
        assert!(e.to_string().contains("delimiter"), "{e}");
        assert_eq!(sniff(b"a;b;c\n", Some(b';')), b',');
        assert_eq!(sniff(b"a,b,c\n", Some(b',')), b';');
        assert_eq!(sniff(b"a,b|c\n", Some(b',')), b'|');
    }

    #[test]
    fn tsv_quotes_are_text() {
        let src = b"a\tb\n\"x\tneedle\n";
        assert_eq!(
            needles(src, "delimiter = \"\\t\"\nquote = \"\""),
            [vec!["row 1:2-2", "b:2-2"]]
        );
    }

    #[test]
    fn header_names_and_their_pieces() {
        let src = b" Plan ,\"Note \"\"x\"\"\",,id\n1,2,3,4\n";
        let input = Input {
            bytes: src,
            path: Path::new("x.csv"),
            format: "csv",
            disk: None,
        };
        let n = Csv.names(&input, &table("")).unwrap();
        let got: Vec<(&str, Pieces)> = n
            .iter()
            .map(|n| (n.text.as_str(), n.pieces.clone()))
            .collect();
        assert_eq!(
            got,
            [
                ("Plan", vec![(0..4, 1..5)]),
                (
                    "Note \"x\"",
                    vec![
                        (0..5, 8..13),
                        (5..6, 13..15),
                        (6..7, 15..16),
                        (7..8, 16..18)
                    ]
                ),
                ("id", vec![(0..2, 21..23)])
            ]
        );
        assert_eq!(n[0].trail_at(1), Vec::new());
    }

    #[test]
    fn a_long_header_cell_keeps_only_its_visible_pieces() {
        // Built up to the cut (M3 code review r1, Codex #1): the pieces map what stays.
        let mut src = b"\"ab\"\"".to_vec();
        src.extend(b"c".repeat(50));
        src.extend(b"\"\n1\n");
        let input = Input {
            bytes: &src,
            path: Path::new("x.csv"),
            format: "csv",
            disk: None,
        };
        let n = Csv.names(&input, &table("")).unwrap();
        assert_eq!(n[0].text, format!("ab\"{}…", "c".repeat(36)));
        assert_eq!(
            n[0].pieces,
            vec![(0..2, 1..3), (2..3, 3..5), (3..39, 5..41)]
        );
        // A Latin-1 byte is one character, before the cut or past it.
        let mut src = vec![0xe9];
        src.extend(b"d".repeat(45));
        src.push(0xe9);
        src.extend(b"\n1\n");
        let input = Input {
            bytes: &src,
            ..input
        };
        let n = Csv.names(&input, &table("")).unwrap();
        assert_eq!(n[0].text, format!("\u{FFFD}{}…", "d".repeat(38)));
        assert_eq!(n[0].pieces, vec![(0..3, 0..1), (3..41, 1..39)]);
    }

    #[test]
    fn walks_only_to_the_last_anchor() {
        // Everything after row 1 is never read: an unclosed quote there changes nothing.
        let src = b"a,b\nneedle,1\n2,\"open\n";
        assert_eq!(needles(src, ""), [vec!["row 1:2-2", "a:2-2"]]);
    }

    #[test]
    fn csv_cursor_matches_model_across_multiline_records() {
        let src = b"Name,Plan\nfirst,needle\n\"two\nlines\",needle\nlast,needle\n";
        let params = toml::Table::new();
        let input = crate::model::Input {
            bytes: src,
            path: std::path::Path::new("x.csv"),
            format: "csv",
            disk: None,
        };
        let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
        let expected = Csv.trails(&input, &params, &anchors).unwrap();
        let prepared = Csv.prepare(input, &params).unwrap();
        assert!(prepared.streams());
        let mut cursor = prepared.cursor();
        for (at, trail) in anchors.into_iter().zip(expected) {
            assert_eq!(cursor.trail_at(at).unwrap().as_ref(), trail.as_slice());
        }
    }

    /// The cursor asked at every byte of each file, in order and once more from the start,
    /// gives the model's Trails: the header, blank lines, CRLF, quotes across lines, an
    /// unclosed quote, no header row, and a file without records.
    #[test]
    fn csv_cursor_matches_model_at_every_byte() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/table");
        let mut cases: Vec<(Vec<u8>, &str)> = ["plans.csv", "semi.csv", "people.tsv"]
            .iter()
            .map(|f| (std::fs::read(dir.join(f)).unwrap(), ""))
            .collect();
        for (s, extra) in [
            (&b"a,b\r\n\r\n1,\"x\r\ny\"\r\n,\r\n3"[..], ""),
            (b"\n\na;b;c\n1;2\n1;2;3;4\n", ""),
            (b"h1,h2\n1,\"open\n2,3\n", ""),
            (b"1,2\n3,4\n", "headers = false"),
            (b"x|y\n\"q\"|z\n", "delimiter = '|'\nquote = ''"),
            (b"", ""),
            (b"\n\n", ""),
        ] {
            cases.push((s.to_vec(), extra));
        }
        for (src, extra) in &cases {
            let params = table(extra);
            let input = Input {
                bytes: src,
                path: Path::new("x.csv"),
                format: "csv",
                disk: None,
            };
            let anchors: Vec<usize> = (0..=src.len()).collect();
            let expected = Csv.trails(&input, &params, &anchors).unwrap();
            let prepared = Csv.prepare(input, &params).unwrap();
            let mut cursor = prepared.cursor();
            for (&at, trail) in anchors.iter().zip(&expected) {
                let got = cursor.trail_at(at).unwrap();
                assert_eq!(
                    got.as_ref(),
                    trail.as_slice(),
                    "{} at {at}",
                    String::from_utf8_lossy(src)
                );
            }
            assert_eq!(cursor.trail_at(0).unwrap().as_ref(), expected[0].as_slice());
        }
    }
}
