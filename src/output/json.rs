//! ripgrep's JSON Lines from the model, with `format`, `trail` and `fragment` (spec §6.4).
use super::fragment::fragments;
use crate::model::{Described, FileStats, Item, Line, Segment, Submatch};
use crate::text;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Write;

/// Run-wide totals for the final `summary` message. Lives here; C9 accumulates it.
#[derive(Debug, Clone, Default)]
pub struct RunStats {
    pub elapsed_total: std::time::Duration,
    pub elapsed: std::time::Duration,
    pub searches: u64,
    pub searches_with_match: u64,
    pub bytes_searched: u64,
    pub bytes_printed: u64,
    pub matched_lines: u64,
    pub matches: u64,
}

pub struct JsonOptions {
    pub line_numbers: bool,
    pub width: usize,
    /// --no-trail never reaches this printer (C9 uses grep-printer's JSON); kept for clarity.
    pub expand: bool,
}

impl std::ops::AddAssign<&RunStats> for RunStats {
    fn add_assign(&mut self, o: &RunStats) {
        self.elapsed_total += o.elapsed_total;
        self.elapsed += o.elapsed;
        self.searches += o.searches;
        self.searches_with_match += o.searches_with_match;
        self.bytes_searched += o.bytes_searched;
        self.bytes_printed += o.bytes_printed;
        self.matched_lines += o.matched_lines;
        self.matches += o.matches;
    }
}

/// Writes begin, match and context messages, and end, for one Described file, in ripgrep's
/// format, adding `format`, `trail`, `fragment` (and `section` with --expand) to each match.
/// `bytes_printed` in the `end` stats is, as in grep-printer (json.rs:815), the bytes written
/// for this file **before** the `end` message. Returns all bytes written, `end` included.
pub fn render(
    d: &Described,
    src: &[u8],
    o: &JsonOptions,
    w: &mut dyn Write,
) -> std::io::Result<u64> {
    render_with_stats(d, src, o, w).map(|(n, _)| n)
}

/// As `render`, and also returns the file's stats as its `end` message reports them, which
/// is what ripgrep adds up for the `summary` (C9 accumulates them with `+=`).
pub fn render_with_stats(
    d: &Described,
    src: &[u8],
    o: &JsonOptions,
    w: &mut dyn Write,
) -> std::io::Result<(u64, RunStats)> {
    let (n, stats) = render_messages(d, src, o, w, true)?;
    Ok((n, stats.expect("`end` is written")))
}

/// As `render_with_stats`, written straight into `w`, a file's bounded output (M5b spec
/// §6.1); without `end`, the file's messages but its `end`, and no stats: a search that failed
/// in its read never reached the end where rg's printer writes it.
pub fn render_messages(
    d: &Described,
    src: &[u8],
    o: &JsonOptions,
    w: &mut dyn Write,
    end: bool,
) -> std::io::Result<(u64, Option<RunStats>)> {
    let mut f = JsonFile::new(&d.file.path, d.format.as_deref(), o);
    f.begin(w)?;
    let lines = o.expand.then(|| super::line_starts(src));
    let mut i = 0;
    for item in &d.file.items {
        match item {
            Item::Match(m) => {
                let trails: Vec<&[Segment]> = (0..m.submatches.len().max(1))
                    .map(|k| super::trail_of(d, i, k))
                    .collect();
                let section = |s: Option<(u64, u64)>| match (s, &lines) {
                    (Some((a, b)), Some(starts)) => json!({
                        "start_line": a,
                        "end_line": b,
                        "text": section_text(src, starts, a, b),
                    }),
                    _ => Value::Null,
                };
                // Each distinct Section of the line made once, its submatches pointing at it:
                // its text made for each submatch made a line of N Matches in one Section
                // cost N × the Section (review r5 of the M5a plan, the audit).
                let mut made: Vec<Value> = Vec::new();
                let mut at: HashMap<Option<(u64, u64)>, usize> = HashMap::new();
                let index: Vec<usize> = (0..trails.len())
                    .map(|k| {
                        let s = d.sections.get(i).and_then(|v| v.get(k)).copied().flatten();
                        *at.entry(s).or_insert_with(|| {
                            made.push(section(s));
                            made.len() - 1
                        })
                    })
                    .collect();
                let sections: Vec<&Value> = index.iter().map(|&k| &made[k]).collect();
                f.matched(w, m, m.bytes(src), &trails, &sections)?;
                i += 1;
            }
            Item::Context(c) => f.context(w, c, c.bytes(src))?,
        }
    }
    if !end {
        return Ok((f.written, None));
    }
    let stats = f.end(w, &d.file.stats)?;
    Ok((f.written, Some(stats)))
}

/// One file's messages, written as they come: `begin` before the first line, `end` last.
/// `match` and `context` are written straight into a reused buffer, in the bytes `serde_json`
/// gives for the same values (code review r2, Fable #3).
pub struct JsonFile<'a> {
    /// The path's `Data`, serialized.
    path: Vec<u8>,
    /// The Format's name or null, serialized.
    format: Vec<u8>,
    o: &'a JsonOptions,
    /// Bytes written so far for this file.
    pub written: u64,
    begun: bool,
    scratch: Vec<u8>,
}

impl<'a> JsonFile<'a> {
    pub fn new(path: &std::path::Path, format: Option<&str>, o: &'a JsonOptions) -> Self {
        let mut p = Vec::new();
        put_data(&mut p, &super::path_bytes(path));
        let mut f = Vec::new();
        put_json(&mut f, &format);
        Self {
            path: p,
            format: f,
            o,
            written: 0,
            begun: false,
            scratch: Vec::new(),
        }
    }

    /// Writes `begin` the first time.
    pub fn begin(&mut self, w: &mut dyn Write) -> std::io::Result<()> {
        if !self.begun {
            self.begun = true;
            let s = &mut self.scratch;
            s.clear();
            s.extend_from_slice(b"{\"type\":\"begin\",\"data\":{\"path\":");
            s.extend_from_slice(&self.path);
            s.extend_from_slice(b"}}\n");
            w.write_all(s)?;
            self.written += s.len() as u64;
        }
        Ok(())
    }

    pub fn begun(&self) -> bool {
        self.begun
    }

    /// The fields ripgrep writes for a match or context message, in its order, up to the
    /// submatches' array, open; `sub` adds the extra fields of submatch `k`.
    fn line_fields(
        &mut self,
        kind: &str,
        line: &Line,
        bytes: &[u8],
        sub: impl Fn(&mut Vec<u8>, usize),
    ) {
        let s = &mut self.scratch;
        s.clear();
        s.extend_from_slice(b"{\"type\":\"");
        s.extend_from_slice(kind.as_bytes());
        s.extend_from_slice(b"\",\"data\":{\"path\":");
        s.extend_from_slice(&self.path);
        s.extend_from_slice(b",\"lines\":");
        put_data(s, bytes);
        s.extend_from_slice(b",\"line_number\":");
        if self.o.line_numbers {
            put_u64(s, line.line_number);
        } else {
            s.extend_from_slice(b"null");
        }
        s.extend_from_slice(b",\"absolute_offset\":");
        put_u64(s, line.absolute_offset);
        s.extend_from_slice(b",\"submatches\":[");
        for (k, m) in line.submatches.iter().enumerate() {
            if k > 0 {
                s.push(b',');
            }
            let (a, b) = (m.start.min(bytes.len()), m.end.min(bytes.len()));
            s.extend_from_slice(b"{\"match\":");
            put_data(s, &bytes[a..b.max(a)]);
            s.extend_from_slice(b",\"start\":");
            put_u64(s, m.start as u64);
            s.extend_from_slice(b",\"end\":");
            put_u64(s, m.end as u64);
            sub(s, k);
            s.push(b'}');
        }
        s.push(b']');
    }

    /// Writes the message built in `scratch`, closed.
    fn emit_scratch(&mut self, w: &mut dyn Write) -> std::io::Result<()> {
        self.scratch.extend_from_slice(b"}}\n");
        w.write_all(&self.scratch)?;
        self.written += self.scratch.len() as u64;
        Ok(())
    }

    /// A `match` message: `bytes` is the line, `trails` one per submatch (one for an inverted
    /// Match) and `sections` the same with --expand (else empty).
    pub fn matched(
        &mut self,
        w: &mut dyn Write,
        m: &Line,
        bytes: &[u8],
        trails: &[&[Segment]],
        sections: &[&Value],
    ) -> std::io::Result<()> {
        self.begin(w)?;
        let expand = self.o.expand;
        let differ = trails.iter().any(|t| *t != trails[0]);
        self.line_fields("match", m, bytes, |s, k| {
            if differ {
                s.extend_from_slice(b",\"trail\":");
                put_json(s, trails[k]);
                if expand && !same(sections[k], sections[0]) {
                    s.extend_from_slice(b",\"section\":");
                    put_json(s, sections[k]);
                }
            }
        });
        let s = &mut self.scratch;
        s.extend_from_slice(b",\"format\":");
        s.extend_from_slice(&self.format);
        s.extend_from_slice(b",\"trail\":");
        put_json(s, trails[0]);
        s.extend_from_slice(b",\"fragment\":");
        // A line that fits has no Fragment: it has no more characters than bytes.
        if text::strip_terminator(bytes).len() <= self.o.width {
            s.extend_from_slice(b"null");
        } else {
            put_json(s, &fragment(bytes, &m.submatches, self.o.width));
        }
        if expand {
            s.extend_from_slice(b",\"section\":");
            put_json(s, sections.first().copied().unwrap_or(&Value::Null));
        }
        self.emit_scratch(w)
    }

    pub fn context(&mut self, w: &mut dyn Write, c: &Line, bytes: &[u8]) -> std::io::Result<()> {
        self.begin(w)?;
        self.line_fields("context", c, bytes, |_, _| {});
        self.emit_scratch(w)
    }

    /// The `end` message with the search's stats; returns them as the summary adds them up.
    pub fn end(&mut self, w: &mut dyn Write, st: &FileStats) -> std::io::Result<RunStats> {
        let stats = file_stats(st, self.written);
        let path: Value = serde_json::from_slice(&self.path).expect("a serialized path");
        self.written += emit(
            w,
            "end",
            json!({
                "path": path,
                "binary_offset": st.binary_offset,
                "stats": {
                    "elapsed": elapsed(stats.elapsed),
                    "searches": stats.searches,
                    "searches_with_match": stats.searches_with_match,
                    "bytes_searched": stats.bytes_searched,
                    "bytes_printed": stats.bytes_printed,
                    "matched_lines": stats.matched_lines,
                    "matches": stats.matches,
                },
            }),
        )?;
        Ok(stats)
    }
}

/// A searched file, as the summary counts it: its search's stats (before filters, spec §6.4)
/// and `bytes_printed`. It is a search with a Match when a line matched: with `-v` rg counts
/// the inverted lines and leaves `matches` at 0 (code review r2, Fable #1).
pub fn file_stats(st: &FileStats, bytes_printed: u64) -> RunStats {
    RunStats {
        elapsed_total: Default::default(),
        elapsed: st.elapsed,
        searches: 1,
        searches_with_match: u64::from(st.matched_lines > 0),
        bytes_searched: st.bytes_searched,
        bytes_printed,
        matched_lines: st.matched_lines,
        matches: st.matches,
    }
}

/// Whether two Sections are equal: the same value for the submatches of one Section.
fn same(a: &Value, b: &Value) -> bool {
    std::ptr::eq(a, b) || a == b
}

/// One JSON Lines message; returns the bytes written.
fn emit(w: &mut dyn Write, kind: &str, data: Value) -> std::io::Result<u64> {
    let line = format!("{}\n", json!({"type": kind, "data": data}));
    w.write_all(line.as_bytes())?;
    Ok(line.len() as u64)
}

/// ripgrep's final message, keys in ripgrep's order.
pub fn summary(s: &RunStats) -> String {
    // rg 14.1.1 builds this message without preserve_order: its keys are sorted.
    let d = |d: std::time::Duration| json!({"human": human(d), "nanos": d.subsec_nanos(), "secs": d.as_secs()});
    json!({
        "data": {
            "elapsed_total": d(s.elapsed_total),
            "stats": {
                "bytes_printed": s.bytes_printed,
                "bytes_searched": s.bytes_searched,
                "elapsed": d(s.elapsed),
                "matched_lines": s.matched_lines,
                "matches": s.matches,
                "searches": s.searches,
                "searches_with_match": s.searches_with_match,
            },
        },
        "type": "summary",
    })
    .to_string()
}

/// grep-printer's `NiceDuration`: seconds with six decimals.
fn human(d: std::time::Duration) -> String {
    let secs = d.as_secs() as f64 + f64::from(d.subsec_nanos()) / 1_000_000_000.0;
    format!("{secs:0.6}s")
}

fn elapsed(d: std::time::Duration) -> Value {
    json!({"secs": d.as_secs(), "nanos": d.subsec_nanos(), "human": human(d)})
}

/// `value` as `serde_json` serializes it.
fn put_json<T: serde::Serialize + ?Sized>(out: &mut Vec<u8>, value: &T) {
    serde_json::to_writer(out, value).expect("serializing to memory");
}

fn put_u64(out: &mut Vec<u8>, n: u64) {
    let _ = write!(out, "{n}");
}

/// grep-printer's `Data`, serialized: `{"text": …}` for UTF-8, `{"bytes": base64}` otherwise.
fn put_data(out: &mut Vec<u8>, bytes: &[u8]) {
    match std::str::from_utf8(bytes) {
        Ok(t) => {
            out.extend_from_slice(b"{\"text\":");
            put_json(out, t);
        }
        Err(_) => {
            out.extend_from_slice(b"{\"bytes\":\"");
            out.extend_from_slice(base64(bytes).as_bytes());
            out.push(b'"');
        }
    }
    out.push(b'}');
}

/// Standard base64 with padding, as grep-printer's `base64_standard`.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * k)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The line's first Fragment at `width`, or null when the line fits.
fn fragment(bytes: &[u8], subs: &[Submatch], width: usize) -> Value {
    let raw = text::strip_terminator(bytes);
    // Valid UTF-8 is its own text: no offset map, which costs a word per byte.
    let (t, map) = match std::str::from_utf8(raw) {
        Ok(t) => (std::borrow::Cow::Borrowed(t), None),
        Err(_) => {
            let (t, map) = text::lossy_with_map(raw);
            (std::borrow::Cow::Owned(t), Some(map))
        }
    };
    let at = |b: usize| {
        let b = b.min(raw.len());
        map.as_ref().map_or(b, |m| m[b])
    };
    let subs: Vec<(usize, usize)> = subs.iter().map(|s| (at(s.start), at(s.end))).collect();
    let Some(f) = fragments(&t, &subs, width).and_then(|f| f.into_iter().next()) else {
        return Value::Null;
    };
    json!({
        "text": f.text,
        // Back to a byte offset in the line, which differs from the text's after U+FFFD.
        "start": map.as_ref().map_or(f.start, |m| m.partition_point(|&m| m < f.start)),
        "omitted_before": f.omitted_before,
        "omitted_after": f.omitted_after,
    })
}

/// Lines `a..=b` (1-based) of `src`, terminators included.
fn section_text(src: &[u8], starts: &[usize], a: u64, b: u64) -> String {
    let at = |n: u64| starts[(n as usize).min(starts.len() - 1)];
    let (s, e) = (at(a.saturating_sub(1)), at(b));
    String::from_utf8_lossy(&src[s..e.max(s)]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::fixture::*;
    use serde_json::Value;
    use std::path::Path;
    use std::time::Duration;

    /// grep-printer's `Data` as a value: the reference of `put_data`.
    fn data(bytes: &[u8]) -> Value {
        match std::str::from_utf8(bytes) {
            Ok(t) => json!({"text": t}),
            Err(_) => json!({"bytes": base64(bytes)}),
        }
    }

    /// A `match` message built from `serde_json` values, as the printer built it before it
    /// wrote the bytes itself: the reference of `JsonFile::matched`.
    fn reference_match(
        path: &str,
        m: &Line,
        bytes: &[u8],
        trails: &[&[Segment]],
        sections: &[Value],
        o: &JsonOptions,
    ) -> String {
        let subs: Vec<Value> = m
            .submatches
            .iter()
            .map(|s| {
                let (a, b) = (s.start.min(bytes.len()), s.end.min(bytes.len()));
                json!({"match": data(&bytes[a..b.max(a)]), "start": s.start, "end": s.end})
            })
            .collect();
        let mut d = json!({
            "path": data(path.as_bytes()),
            "lines": data(bytes),
            "line_number": o.line_numbers.then_some(m.line_number),
            "absolute_offset": m.absolute_offset,
            "submatches": subs,
        });
        if trails.iter().any(|t| *t != trails[0]) {
            for (k, sub) in d["submatches"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                let sub = sub.as_object_mut().unwrap();
                sub.insert("trail".into(), json!(trails[k]));
                if o.expand && sections[k] != sections[0] {
                    sub.insert("section".into(), sections[k].clone());
                }
            }
        }
        let obj = d.as_object_mut().unwrap();
        obj.insert("format".into(), json!("markdown"));
        obj.insert("trail".into(), json!(trails[0]));
        obj.insert("fragment".into(), fragment(bytes, &m.submatches, o.width));
        if o.expand {
            obj.insert(
                "section".into(),
                sections.first().cloned().unwrap_or(Value::Null),
            );
        }
        format!("{}\n", json!({"type": "match", "data": d}))
    }

    /// Code review r2, Fable #3: the bytes written directly are the ones the values gave.
    #[test]
    fn match_messages_equal_the_value_built_ones() {
        let a = h(1, "A \"quoted\" \\ é", 1, 4);
        let b = seg("column", "Note\ttab", 1, 4);
        let both = [a, b];
        let (ta, tb): (&[Segment], &[Segment]) = (&both[..1], &both);
        let long = format!("{}needle{}\n", "x".repeat(60), "é".repeat(60));
        type Case = (Vec<u8>, Vec<(usize, usize)>);
        let cases: Vec<Case> = vec![
            (b"one needle\n".to_vec(), vec![(4, 10)]),
            (b"tab\t\"q\" \\ needle\n".to_vec(), vec![(10, 16)]),
            (b"bad \xff needle\n".to_vec(), vec![(6, 12)]),
            (b"no terminator needle".to_vec(), vec![(14, 20)]),
            (b"inverted\n".to_vec(), vec![]),
            (long.clone().into_bytes(), vec![(60, 66)]),
            (b"two needle needle\n".to_vec(), vec![(4, 10), (11, 17)]),
        ];
        for line_numbers in [true, false] {
            for expand in [false, true] {
                for width in [12, 200] {
                    let o = JsonOptions {
                        line_numbers,
                        width,
                        expand,
                    };
                    for (bytes, subs) in &cases {
                        let m = Line {
                            line_number: 7,
                            absolute_offset: 42,
                            len: bytes.len(),
                            owned: None,
                            submatches: subs
                                .iter()
                                .map(|&(start, end)| Submatch { start, end })
                                .collect(),
                        };
                        let n = subs.len().max(1);
                        let mut trails = vec![ta; n];
                        let sec = |a: u64| json!({"start_line": a, "end_line": 9, "text": "x\n"});
                        let mut sections = vec![sec(1); n];
                        if n > 1 {
                            trails[1] = tb;
                            sections[1] = sec(2);
                        }
                        let sections = if expand { sections } else { Vec::new() };
                        let want = reference_match("dir/ä.md", &m, bytes, &trails, &sections, &o);
                        let mut f = JsonFile::new(Path::new("dir/ä.md"), Some("markdown"), &o);
                        f.begun = true;
                        let mut got = Vec::new();
                        let refs: Vec<&Value> = sections.iter().collect();
                        f.matched(&mut got, &m, bytes, &trails, &refs).unwrap();
                        assert_eq!(String::from_utf8(got).unwrap(), want, "{bytes:?} {width}");
                    }
                }
            }
        }
    }

    fn opts() -> JsonOptions {
        JsonOptions {
            line_numbers: true,
            width: 200,
            expand: false,
        }
    }

    fn sample() -> (Vec<u8>, Described) {
        let s = src(&["# A", "one needle", "plain", "| needle x | y needle |"]);
        let a = h(1, "A", 1, 4);
        let d = described(
            &s,
            vec![mat(&s, 2), ctx(&s, 3), mat(&s, 4)],
            vec![
                vec![vec![a.clone()]],
                vec![
                    vec![a.clone(), seg("column", "Name", 1, 4)],
                    vec![a, seg("column", "Note", 1, 4)],
                ],
            ],
        );
        (s, d)
    }

    fn run(d: &Described, s: &[u8], o: &JsonOptions) -> (String, u64) {
        let mut out = Vec::new();
        let n = render(d, s, o, &mut out).unwrap();
        (String::from_utf8(out).unwrap(), n)
    }

    fn keys(v: &Value) -> Vec<&str> {
        v.as_object().unwrap().keys().map(|k| k.as_str()).collect()
    }

    #[test]
    fn messages_in_ripgreps_order_with_our_fields_last() {
        let (s, d) = sample();
        let (out, written) = run(&d, &s, &opts());
        assert_eq!(written as usize, out.len());
        let msgs: Vec<Value> = out
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let types: Vec<&str> = msgs.iter().map(|m| m["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["begin", "match", "context", "match", "end"]);
        for m in &msgs {
            assert_eq!(keys(m), ["type", "data"]);
        }
        assert_eq!(
            keys(&msgs[1]["data"]),
            [
                "path",
                "lines",
                "line_number",
                "absolute_offset",
                "submatches",
                "format",
                "trail",
                "fragment"
            ]
        );
        assert_eq!(
            keys(&msgs[2]["data"]),
            [
                "path",
                "lines",
                "line_number",
                "absolute_offset",
                "submatches"
            ]
        );
        assert_eq!(keys(&msgs[4]["data"]), ["path", "binary_offset", "stats"]);
        assert_eq!(
            keys(&msgs[4]["data"]["stats"]),
            [
                "elapsed",
                "searches",
                "searches_with_match",
                "bytes_searched",
                "bytes_printed",
                "matched_lines",
                "matches"
            ]
        );
        // The first match, byte for byte.
        assert_eq!(
            out.lines().nth(1).unwrap(),
            r#"{"type":"match","data":{"path":{"text":"docs/a.md"},"lines":{"text":"one needle\n"},"line_number":2,"absolute_offset":4,"submatches":[{"match":{"text":"needle"},"start":4,"end":10}],"format":"markdown","trail":[{"kind":"heading","name":"A","line":1,"end_line":4,"level":1}],"fragment":null}}"#
        );
        assert_eq!(
            out.lines().next().unwrap(),
            r#"{"type":"begin","data":{"path":{"text":"docs/a.md"}}}"#
        );
        assert_eq!(
            out.lines().nth(2).unwrap(),
            r#"{"type":"context","data":{"path":{"text":"docs/a.md"},"lines":{"text":"plain\n"},"line_number":3,"absolute_offset":15,"submatches":[]}}"#
        );
    }

    #[test]
    fn a_row_with_two_trails_gives_each_submatch_its_trail() {
        let (s, d) = sample();
        let (out, _) = run(&d, &s, &opts());
        let first: Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
        assert!(first["data"]["submatches"][0].get("trail").is_none());
        let row: Value = serde_json::from_str(out.lines().nth(3).unwrap()).unwrap();
        let subs = &row["data"]["submatches"];
        assert_eq!(keys(&subs[0]), ["match", "start", "end", "trail"]);
        assert_eq!(subs[0]["trail"][1]["name"], "Name");
        assert_eq!(subs[1]["trail"][1]["name"], "Note");
        assert_eq!(row["data"]["trail"][1]["name"], "Name");
    }

    #[test]
    fn end_counts_the_bytes_written_before_it() {
        let (s, d) = sample();
        let (out, _) = run(&d, &s, &opts());
        let lines: Vec<&str> = out.lines().collect();
        let before: usize = lines[..4].iter().map(|l| l.len() + 1).sum();
        let end: Value = serde_json::from_str(lines[4]).unwrap();
        let stats = &end["data"]["stats"];
        assert_eq!(stats["bytes_printed"], before as u64);
        assert_eq!(stats["searches"], 1);
        assert_eq!(stats["searches_with_match"], 1);
        assert_eq!(stats["bytes_searched"], s.len() as u64);
        assert_eq!(stats["matched_lines"], 2);
        assert_eq!(stats["matches"], 3);
        assert_eq!(
            stats["elapsed"],
            serde_json::json!({"secs": 0, "nanos": 28123, "human": "0.000028s"})
        );
        assert_eq!(end["data"]["binary_offset"], Value::Null);
    }

    #[test]
    fn without_line_numbers_they_are_null() {
        let (s, d) = sample();
        let o = JsonOptions {
            line_numbers: false,
            ..opts()
        };
        let (out, _) = run(&d, &s, &o);
        assert!(
            out.lines()
                .nth(1)
                .unwrap()
                .contains(r#""line_number":null"#)
        );
        assert!(
            out.lines()
                .nth(2)
                .unwrap()
                .contains(r#""line_number":null"#)
        );
    }

    #[test]
    fn invalid_utf8_is_base64_as_ripgrep() {
        let s = b"caf\xe9 needle\n".to_vec();
        let d = described(&s, vec![mat(&s, 1)], vec![vec![vec![]]]);
        let (out, _) = run(&d, &s, &opts());
        let m: Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
        assert_eq!(
            m["data"]["lines"],
            serde_json::json!({"bytes": "Y2Fm6SBuZWVkbGUK"})
        );
        assert_eq!(m["data"]["submatches"][0]["match"]["text"], "needle");
        assert_eq!(m["data"]["trail"], serde_json::json!([]));
    }

    #[test]
    fn base64_pads_as_rfc_4648() {
        let b = |s: &str| base64(s.as_bytes());
        assert_eq!(
            [
                b(""),
                b("f"),
                b("fo"),
                b("foo"),
                b("foob"),
                b("fooba"),
                b("foobar")
            ],
            [
                "", "Zg==", "Zm8=", "Zm9v", "Zm9vYg==", "Zm9vYmE=", "Zm9vYmFy"
            ]
        );
    }

    #[test]
    fn long_line_carries_its_first_fragment() {
        let mut long = "x".repeat(3000);
        long.replace_range(1000..1006, "needle");
        let s = src(&[&long]);
        let d = described(&s, vec![mat(&s, 1)], vec![vec![vec![]]]);
        let o = JsonOptions {
            width: 80,
            ..opts()
        };
        let (out, _) = run(&d, &s, &o);
        let m: Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
        let f = &m["data"]["fragment"];
        assert_eq!(
            keys(f),
            ["text", "start", "omitted_before", "omitted_after"]
        );
        let text = f["text"].as_str().unwrap();
        let start = f["start"].as_u64().unwrap() as usize;
        assert_eq!(&long[start..start + text.len()], text);
        assert!(text.contains("needle"));
        assert_eq!(f["omitted_before"].as_u64().unwrap() as usize, start);
    }

    #[test]
    fn expand_adds_the_section_and_per_submatch_sections() {
        let (s, mut d) = sample();
        d.sections = vec![vec![Some((1, 4))], vec![Some((1, 4)), Some((3, 4))]];
        let o = JsonOptions {
            expand: true,
            ..opts()
        };
        let (out, _) = run(&d, &s, &o);
        let first: Value = serde_json::from_str(out.lines().nth(1).unwrap()).unwrap();
        assert_eq!(keys(&first["data"]).last().copied(), Some("section"));
        assert_eq!(
            first["data"]["section"],
            serde_json::json!({"start_line": 1, "end_line": 4,
                "text": "# A\none needle\nplain\n| needle x | y needle |\n"})
        );
        let row: Value = serde_json::from_str(out.lines().nth(3).unwrap()).unwrap();
        let subs = &row["data"]["submatches"];
        assert!(subs[0].get("section").is_none());
        assert_eq!(subs[1]["section"]["start_line"], 3);
        assert_eq!(
            subs[1]["section"]["text"],
            "plain\n| needle x | y needle |\n"
        );
    }

    #[test]
    fn summary_adds_runs_with_ripgreps_key_order() {
        let mut total = RunStats::default();
        let one = RunStats {
            elapsed_total: Duration::ZERO,
            elapsed: Duration::from_nanos(47_810),
            searches: 1,
            searches_with_match: 1,
            bytes_searched: 27,
            bytes_printed: 582,
            matched_lines: 2,
            matches: 3,
        };
        total += &one;
        total += &one;
        total.elapsed_total = Duration::new(1, 10_684_767);
        assert_eq!(
            summary(&total),
            r#"{"data":{"elapsed_total":{"human":"1.010685s","nanos":10684767,"secs":1},"stats":{"bytes_printed":1164,"bytes_searched":54,"elapsed":{"human":"0.000096s","nanos":95620,"secs":0},"matched_lines":4,"matches":6,"searches":2,"searches_with_match":2}},"type":"summary"}"#
        );
    }
}
