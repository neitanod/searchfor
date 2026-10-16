//! ripgrep's JSON Lines from the model, with `format`, `trail` and `fragment` (spec §6.4).
use super::fragment::fragments;
use crate::model::{Described, Item, Segment, Submatch};
use crate::text;
use serde_json::{Value, json};
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
    let path = path_data(&d.file.path);
    let mut written = emit(w, "begin", json!({"path": path}))?;
    let lines = o.expand.then(|| line_starts(src));
    let mut i = 0;
    for item in &d.file.items {
        match item {
            Item::Match(m) => {
                let mut data = rg_data(
                    &path,
                    &m.bytes,
                    m.line_number,
                    m.absolute_offset,
                    &m.submatches,
                    o,
                );
                let trails: Vec<&[Segment]> = (0..m.submatches.len().max(1))
                    .map(|k| super::trail_of(d, i, k))
                    .collect();
                let sections: Vec<Option<(u64, u64)>> = (0..trails.len())
                    .map(|k| d.sections.get(i).and_then(|v| v.get(k)).copied().flatten())
                    .collect();
                let section = |s: Option<(u64, u64)>| match (s, &lines) {
                    (Some((a, b)), Some(starts)) => json!({
                        "start_line": a,
                        "end_line": b,
                        "text": section_text(src, starts, a, b),
                    }),
                    _ => Value::Null,
                };
                if trails.iter().any(|t| *t != trails[0]) {
                    let subs = data["submatches"].as_array_mut().expect("submatches");
                    for (k, sub) in subs.iter_mut().enumerate() {
                        let sub = sub.as_object_mut().expect("submatch");
                        sub.insert("trail".into(), json!(trails[k]));
                        if o.expand && sections[k] != sections[0] {
                            sub.insert("section".into(), section(sections[k]));
                        }
                    }
                }
                let obj = data.as_object_mut().expect("data");
                obj.insert("format".into(), json!(d.format));
                obj.insert("trail".into(), json!(trails[0]));
                obj.insert(
                    "fragment".into(),
                    fragment(&m.bytes, &m.submatches, o.width),
                );
                if o.expand {
                    obj.insert("section".into(), section(sections[0]));
                }
                written += emit(w, "match", data)?;
                i += 1;
            }
            Item::Context(c) => {
                let data = rg_data(
                    &path,
                    &c.bytes,
                    c.line_number,
                    c.absolute_offset,
                    &c.submatches,
                    o,
                );
                written += emit(w, "context", data)?;
            }
        }
    }
    let st = &d.file.stats;
    let stats = RunStats {
        elapsed_total: Default::default(),
        elapsed: st.elapsed,
        searches: 1,
        searches_with_match: u64::from(d.file.matches().next().is_some()),
        bytes_searched: st.bytes_searched,
        bytes_printed: written,
        matched_lines: st.matched_lines,
        matches: st.matches,
    };
    let end = emit(
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
    Ok((written + end, stats))
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

/// The fields ripgrep writes for a match or context message, in its order.
fn rg_data(
    path: &Value,
    bytes: &[u8],
    line_number: u64,
    absolute_offset: u64,
    subs: &[Submatch],
    o: &JsonOptions,
) -> Value {
    let subs: Vec<Value> = subs
        .iter()
        .map(|s| {
            let (a, b) = (s.start.min(bytes.len()), s.end.min(bytes.len()));
            json!({"match": data(&bytes[a..b.max(a)]), "start": s.start, "end": s.end})
        })
        .collect();
    json!({
        "path": path,
        "lines": data(bytes),
        "line_number": o.line_numbers.then_some(line_number),
        "absolute_offset": absolute_offset,
        "submatches": subs,
    })
}

/// grep-printer's `Data`: `{"text": …}` for UTF-8, `{"bytes": base64}` otherwise.
fn data(bytes: &[u8]) -> Value {
    match std::str::from_utf8(bytes) {
        Ok(t) => json!({"text": t}),
        Err(_) => json!({"bytes": base64(bytes)}),
    }
}

fn path_data(p: &std::path::Path) -> Value {
    data(&super::path_bytes(p))
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
    let (t, map) = text::lossy_with_map(raw);
    let subs: Vec<(usize, usize)> = subs
        .iter()
        .map(|s| (map[s.start.min(raw.len())], map[s.end.min(raw.len())]))
        .collect();
    let Some(f) = fragments(&t, &subs, width).and_then(|f| f.into_iter().next()) else {
        return Value::Null;
    };
    json!({
        "text": f.text,
        // Back to a byte offset in the line, which differs from the text's after U+FFFD.
        "start": map.partition_point(|&m| m < f.start),
        "omitted_before": f.omitted_before,
        "omitted_after": f.omitted_after,
    })
}

/// Byte offset of the start of every line, plus the end of `src`.
fn line_starts(src: &[u8]) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(memchr::memchr_iter(b'\n', src).map(|i| i + 1));
    if *v.last().expect("non-empty") != src.len() {
        v.push(src.len());
    }
    v
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
    use std::time::Duration;

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
