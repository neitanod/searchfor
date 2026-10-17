//! keypath: YAML, JSON and TOML key paths (spec §5.2, design note 6).
pub mod collect;
pub mod json;
pub mod toml;
pub mod yaml;

use super::Processor;
use crate::model::{Input, Name, Trail};
use crate::trail::{Outline, TrailCursor, names_of};
use collect::Collector;

pub struct Keypath;
// Tests of yaml.rs and toml.rs reach `outline_for`, `Syntax` and `tests::params` through
// `super::super`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    Json,
    Yaml,
    Toml,
}

fn syntax(params: &::toml::Table) -> anyhow::Result<Syntax> {
    match params.get("syntax").and_then(|v| v.as_str()) {
        Some("json") => Ok(Syntax::Json),
        Some("yaml") => Ok(Syntax::Yaml),
        Some("toml") => Ok(Syntax::Toml),
        _ => anyhow::bail!("keypath needs `syntax` = \"json\", \"yaml\" or \"toml\" in [params]"),
    }
}

/// The Outline of `src` restricted to the Segments that hold an anchor (`anchors` sorted).
pub(crate) fn outline_for(src: &[u8], syntax: Syntax, anchors: &[usize]) -> Outline {
    match syntax {
        Syntax::Toml => toml::outline(src), // C3: TOML files are small; the whole Outline.
        _ => {
            let mut c = Collector::for_anchors(anchors);
            let end = walk(syntax, src, &mut c);
            c.finish(end, src)
        }
    }
}

fn walk(syntax: Syntax, src: &[u8], c: &mut Collector) -> usize {
    match syntax {
        Syntax::Json => json::walk(src, c),
        Syntax::Yaml => yaml::walk(src, c),
        Syntax::Toml => unreachable!("TOML builds its Outline directly"),
    }
}

impl Processor for Keypath {
    fn trails(
        &self,
        input: &Input,
        params: &::toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let syntax = syntax(params)?;
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let sorted: Vec<usize> = order.iter().map(|&i| anchors[i]).collect();
        let o = outline_for(input.bytes, syntax, &sorted);
        let mut cursor = TrailCursor::new(&o);
        let mut out = vec![Vec::new(); anchors.len()];
        for (k, &i) in order.iter().enumerate() {
            out[i] = cursor.trail(sorted[k]);
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &::toml::Table) -> anyhow::Result<Vec<Name>> {
        let o = match syntax(params)? {
            Syntax::Toml => toml::outline(input.bytes),
            s => {
                let mut c = Collector::for_names();
                let end = walk(s, input.bytes, &mut c);
                c.finish(end, input.bytes)
            }
        };
        Ok(names_of(&o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Input;
    use std::path::Path;

    // `toml` here is the sibling module: the crate is `::toml`.
    pub(super) fn params(syntax: &str) -> ::toml::Table {
        ::toml::from_str(&format!("syntax = '{syntax}'")).unwrap()
    }

    /// The Trail names of every `needle` in `src`, in order.
    pub(super) fn needles(src: &[u8], syntax: &str) -> Vec<Vec<String>> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: syntax,
            disk: None,
        };
        Keypath
            .trails(&input, &params(syntax), &anchors)
            .unwrap()
            .iter()
            .map(|t| t.iter().map(|s| s.name.clone()).collect())
            .collect()
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/data")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn json_keys_indexes_and_comments() {
        assert_eq!(
            needles(&fixture("config.json"), "json"),
            vec![
                vec!["server", "ports[2]", "host"],
                vec!["server", "matrix[1][1]"],
                vec![], // inside a comment, between top-level keys
                vec!["café"],
                vec![], // in the key's own text (its label)
            ]
        );
    }

    #[test]
    fn json_sections_and_kinds() {
        let src = fixture("config.json");
        let at = memchr::memmem::find(&src, b"needle.example").unwrap();
        let input = Input {
            bytes: &src,
            path: Path::new("x"),
            format: "json",
            disk: None,
        };
        let t = &Keypath.trails(&input, &params("json"), &[at]).unwrap()[0];
        let got: Vec<(&str, &str, u64, u64)> = t
            .iter()
            .map(|s| (s.kind.as_str(), s.name.as_str(), s.line, s.end_line))
            .collect();
        assert_eq!(
            got,
            [
                ("key", "server", 3, 11),
                ("element", "ports[2]", 8, 8),
                ("key", "host", 8, 8)
            ]
        );
    }

    #[test]
    fn json_top_level_arrays_lines_and_errors() {
        assert_eq!(
            needles(b"[1, [2, \"needle\"]]\n", "json"),
            vec![vec!["[1][1]"]]
        );
        assert_eq!(
            needles(&fixture("lines.jsonl"), "json"),
            vec![vec!["a"], vec!["b", "c"]]
        );
        // The Trails before the syntax error stay; the ones after it are empty (spec §5.2).
        assert_eq!(
            needles(&fixture("broken.json"), "json"),
            vec![vec!["a", "b"], vec![]]
        );
    }

    #[test]
    fn json_names_carry_decoded_pieces() {
        let src = fixture("config.json");
        let input = Input {
            bytes: &src,
            path: Path::new("x"),
            format: "json",
            disk: None,
        };
        let names = Keypath.names(&input, &params("json")).unwrap();
        let cafe = names.iter().find(|n| n.text == "café").unwrap();
        let raw = memchr::memmem::find(&src, b"caf\\u00e9").unwrap();
        assert_eq!(
            cafe.pieces,
            vec![(0.."café".len(), raw..raw + "caf\\u00e9".len())]
        );
        let host = names.iter().find(|n| n.text == "host").unwrap();
        assert_eq!(host.pieces[0].0.len(), host.pieces[0].1.len());
        assert_eq!(names.iter().filter(|n| n.text == "host").count(), 4);
    }

    #[test]
    fn anchors_in_any_order_get_their_own_trails() {
        let src = br#"{"a": "needle", "b": "needle"}"#;
        let first = memchr::memmem::find(src, b"needle").unwrap();
        let second = memchr::memmem::rfind(src, b"needle").unwrap();
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: "json",
            disk: None,
        };
        let t = Keypath
            .trails(&input, &params("json"), &[second, first])
            .unwrap();
        assert_eq!((t[0][0].name.as_str(), t[1][0].name.as_str()), ("b", "a"));
    }

    /// The Outline kept for some anchors gives each of them the Trail the whole Outline gives
    /// (kinds, names and lines),
    /// at every offset of valid, broken, nested and escaped JSON.
    #[test]
    fn json_trails_of_kept_segments_are_those_of_the_whole_outline() {
        let docs: Vec<Vec<u8>> = vec![
            fixture("config.json"),
            fixture("broken.json"),
            fixture("lines.jsonl"),
            br#"{"a": [[1, {"b\u00e9": [2, []]}], {}], "c": {"d": [ ]}, "e": "x"} [3]"#.to_vec(),
            br#"{"a": {"b": [1, {"c": 2"#.to_vec(),
        ];
        for src in &docs {
            let mut all = Collector::for_names();
            let end = walk(Syntax::Json, src, &mut all);
            let whole = all.finish(end, src);
            for at in 0..=src.len() {
                for anchors in [vec![at], vec![0, at], vec![at, src.len()]] {
                    let mut anchors = anchors;
                    anchors.sort();
                    let kept = outline_for(src, Syntax::Json, &anchors);
                    for &a in &anchors {
                        assert_eq!(
                            TrailCursor::new(&kept).trail(a),
                            TrailCursor::new(&whole).trail(a),
                            "offset {a} of {}",
                            String::from_utf8_lossy(src)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_big_file_keeps_only_the_segments_of_its_anchors() {
        let mut src = String::from("{");
        for i in 0..20_000 {
            src.push_str(&format!("\"k{i}\": {{\"v\": {i}}},"));
        }
        src.push_str("\"last\": {\"v\": \"needle\"}}");
        let at = src.find("needle").unwrap();
        let o = outline_for(src.as_bytes(), Syntax::Json, &[at]);
        assert_eq!(o.segments.len(), 2);
    }

    #[test]
    fn lexical_errors_end_the_walk() {
        // JSONC allows comments and trailing commas, nothing else (D34, review r1, Codex #9).
        for bad in [
            r#"{"a": nope, "b": "needle"}"#,
            r#"{"a": 01, "b": "needle"}"#,
            r#"{"a": 1.e5, "b": "needle"}"#,
            r#"{"a": "x\q", "b": "needle"}"#,
            "{\"a\": \"tab\there\", \"b\": \"needle\"}",
            r#"{"a": truex, "b": "needle"}"#,
        ] {
            assert_eq!(
                needles(bad.as_bytes(), "json"),
                vec![Vec::<String>::new()],
                "{bad}"
            );
        }
        assert_eq!(
            needles(br#"{"a": -0.5e+3, "b": "needle"}"#, "json"),
            vec![vec!["b"]]
        );
        assert_eq!(
            needles(br#"{"a": "\u00e9\n", "b": "needle"}"#, "json"),
            vec![vec!["b"]]
        );
    }

    #[test]
    fn deep_nesting_does_not_touch_the_call_stack() {
        // 200 000 nested objects, far past what recursion would survive on an 8 MiB stack:
        // the walk is iterative, and Segments stop at MAX_DEPTH (D34).
        let n = 200_000;
        let mut src = "{\"a\":".repeat(n);
        src.push_str("\"needle\"");
        src.push_str(&"}".repeat(n));
        let t = &needles(src.as_bytes(), "json")[0];
        assert_eq!(t.len(), json::MAX_DEPTH);
        let mut arrays = "[".repeat(n);
        arrays.push_str("\"needle\"");
        arrays.push_str(&"]".repeat(n));
        assert_eq!(needles(arrays.as_bytes(), "json").len(), 1);
    }

    #[test]
    fn keys_that_hold_arrays_are_searchable_names() {
        let src = br#"{"server": {"ports": [80, 443]}}"#;
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: "json",
            disk: None,
        };
        let names = Keypath.names(&input, &params("json")).unwrap();
        let ports = names
            .iter()
            .find(|n| n.text == "ports")
            .expect("ports is a key");
        // Its Trail is itself under `server`, as for any named Segment.
        let t: Vec<&str> = ports.trail.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(t, ["server", "ports"]);
    }

    #[test]
    fn syntax_is_required() {
        let input = Input {
            bytes: b"{}",
            path: Path::new("x"),
            format: "json",
            disk: None,
        };
        let e = Keypath
            .trails(&input, &::toml::Table::new(), &[0])
            .unwrap_err();
        assert!(e.to_string().contains("`syntax`"), "{e}");
    }
}
