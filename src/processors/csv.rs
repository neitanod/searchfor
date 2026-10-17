//! csv: rows and columns of CSV and TSV files (spec §5.5, M3 plan design note 4). csv-core
//! reads the fields; this module keeps their byte ranges and walks only as far as the last
//! anchor, so a Match on row 3 of a 1 GB file reads three rows.
use super::Processor;
use crate::model::{Input, Name, Segment, Trail};
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

/// The delimiter of a CSV file (D4): of `,`, `;`, tab and `|`, the one that appears most often
/// in its first line outside quotes; `,` on a tie or when none appears.
pub fn sniff(src: &[u8], quote: Option<u8>) -> u8 {
    const CANDIDATES: [u8; 4] = [b',', b';', b'\t', b'|'];
    let line = &src[..memchr::memchr(b'\n', src).unwrap_or(src.len())];
    let mut counts = [0usize; 4];
    let mut quoted = false;
    for &b in line {
        if Some(b) == quote {
            quoted = !quoted;
        } else if !quoted && let Some(k) = CANDIDATES.iter().position(|&c| c == b) {
            counts[k] += 1;
        }
    }
    let mut best = 0;
    for k in 1..4 {
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
    let raw = &src[f.raw.clone()];
    // (decoded bytes, their Source range): verbatim runs, and one entry per `""`.
    let mut runs: Vec<(Vec<u8>, Range<usize>)> = Vec::new();
    match quote {
        Some(q) if raw.len() >= 2 && raw[0] == q && raw[raw.len() - 1] == q => {
            let (s0, s1) = (f.raw.start + 1, f.raw.end - 1);
            let mut i = s0;
            let mut run = s0;
            while i < s1 {
                if src[i] == q && i + 1 < s1 && src[i + 1] == q {
                    if run < i {
                        runs.push((src[run..i].to_vec(), run..i));
                    }
                    runs.push((vec![q], i..i + 2));
                    i += 2;
                    run = i;
                } else {
                    i += 1;
                }
            }
            if run < s1 {
                runs.push((src[run..s1].to_vec(), run..s1));
            }
        }
        _ => runs.push((raw.to_vec(), f.raw.clone())),
    }
    let mine: Vec<u8> = runs.iter().flat_map(|(b, _)| b.iter().copied()).collect();
    let searchable = mine == decoded;
    // Trim the white space at both ends, which only verbatim runs hold.
    while let Some((b, r)) = runs.first_mut() {
        let lead = b.len() - b.trim_ascii_start().len();
        if lead == b.len() && b.len() == r.len() {
            runs.remove(0);
            continue;
        }
        if b.len() == r.len() {
            b.drain(..lead);
            r.start += lead;
        }
        break;
    }
    while let Some((b, r)) = runs.last_mut() {
        let trail = b.len() - b.trim_ascii_end().len();
        if trail == b.len() && b.len() == r.len() {
            runs.pop();
            continue;
        }
        if b.len() == r.len() {
            b.truncate(b.len() - trail);
            r.end -= trail;
        }
        break;
    }
    let mut text = String::new();
    let mut pieces = Vec::new();
    for (b, r) in &runs {
        if b.len() == r.len() {
            crate::text::push_lossy(&mut text, &mut pieces, src, r.clone());
        } else {
            let at = text.len();
            text.push_str(&String::from_utf8_lossy(b));
            pieces.push((at..text.len(), r.clone()));
        }
    }
    if !searchable {
        text = String::from_utf8_lossy(decoded).trim_ascii().to_string();
        pieces.clear();
    }
    if text.is_empty() {
        return None;
    }
    crate::text::cut_name(&mut text, &mut pieces, name_chars);
    Some(HeaderName {
        text,
        pieces,
        searchable,
    })
}

impl Csv {
    /// The header names (`None` for an empty cell), the header's fields and the line range of
    /// the header row.
    #[allow(clippy::type_complexity)]
    fn header(
        recs: &mut Records,
        p: &Params,
        lines: &mut Lines,
    ) -> Option<(Vec<Option<HeaderName>>, Vec<Field>, (u64, u64))> {
        let mut fields = Vec::new();
        if !matches!(recs.next(&mut fields, true), Next::Record) {
            return None;
        }
        let mut names = Vec::new();
        let mut from = 0;
        for (f, &to) in fields.iter().zip(&recs.ends) {
            names.push(header_name(
                recs.src,
                f,
                &recs.out[from..to],
                p.quote,
                p.name_chars,
            ));
            from = to;
        }
        let first = lines.of(fields[0].raw.start);
        let last = lines.of(fields
            .last()
            .unwrap()
            .raw
            .end
            .saturating_sub(1)
            .max(fields[0].raw.start));
        Some((names, fields, (first, last)))
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
        let mut names: Vec<Option<String>> = Vec::new();
        if p.headers {
            match Csv::header(&mut recs, &p, &mut lines) {
                Some((n, fields, _)) => {
                    names = n.into_iter().map(|n| n.map(|h| h.text)).collect();
                    // Matches in the header row are in a column's name: no Trail (spec §5.5).
                    let end = fields.last().unwrap().end;
                    while next < order.len() && anchors[order[next]] < end {
                        next += 1;
                    }
                }
                None => return Ok(out),
            }
        }
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
                    let col = names
                        .get(k)
                        .cloned()
                        .flatten()
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
        let Some((names, fields, header_lines)) = Csv::header(&mut recs, &p, &mut lines) else {
            return Ok(Vec::new());
        };
        Ok(names
            .into_iter()
            .zip(&fields)
            .filter_map(|(n, f)| {
                let h = n.filter(|h| h.searchable)?;
                Some(Name {
                    trail: vec![seg("column", h.text.clone(), header_lines)],
                    labels: vec![f.raw.clone()],
                    anchor: f.raw.start,
                    text: h.text,
                    pieces: h.pieces,
                })
            })
            .collect())
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
    fn sniffs_the_delimiter_of_the_first_line() {
        assert_eq!(sniff(b"a;b;c\n1,5;2;3\n", Some(b'"')), b';');
        assert_eq!(sniff(b"\"a;b\",c\n", Some(b'"')), b',');
        assert_eq!(sniff(b"a\tb|c\n", Some(b'"')), b'\t');
        assert_eq!(sniff(b"plain\n", Some(b'"')), b',');
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
    fn walks_only_to_the_last_anchor() {
        // Everything after row 1 is never read: an unclosed quote there changes nothing.
        let src = b"a,b\nneedle,1\n2,\"open\n";
        assert_eq!(needles(src, ""), [vec!["row 1:2-2", "a:2-2"]]);
    }
}
