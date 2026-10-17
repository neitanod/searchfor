//! The YAML walker of `keypath` (spec §5.2), over saphyr-parser's events. A scalar's span covers
//! it (a literal block scalar through its last line); a block collection's end event sits at the
//! next token, so a container closes at the end of its last scalar or alias. `Marker` counts
//! characters, so bytes come from its line and column. On a syntax error the walk stops there,
//! and the Segments still open close at the error (D3).
use super::collect::Collector;
use crate::trail::{LineIndex, OutlineSegment};
use saphyr_parser::{Event, Marker, Parser, ScalarStyle};
use std::ops::Range;

/// A complex key (`? [a, b]`) is named by its first 40 characters (spec §5.2).
const COMPLEX_KEY_CHARS: usize = 40;

/// `Name::pieces`: (range in the name's text, range in the Source).
type Pieces = Vec<(Range<usize>, Range<usize>)>;

enum Frame {
    /// A mapping; `key` is true between a key and the end of its value.
    Map {
        key: bool,
        key_name: String,
    },
    /// A sequence and the name its elements extend (`ports` → `ports[0]`).
    Seq {
        holder: String,
        n: usize,
    },
    /// A complex key (`? [a, b]`) being skipped: where it started and its nesting.
    Complex {
        start: usize,
        depth: usize,
    },
    Doc,
}

/// Byte offsets of saphyr's markers, from their line and character column. Markers mostly move
/// forward, so the last position is reused within a line instead of counting from its start.
struct Bytes<'t> {
    text: &'t str,
    lines: LineIndex,
    /// (line, column, byte) of the last marker converted.
    last: (usize, usize, usize),
}

impl Bytes<'_> {
    fn of(&mut self, m: &Marker) -> usize {
        let len = self.text.len();
        let (line, col) = (m.line(), m.col());
        let (from, skip) = if self.last.0 == line && self.last.1 <= col {
            (self.last.2, col - self.last.1)
        } else {
            let start = self.lines.line_start(line as u64).unwrap_or(len).min(len);
            (start, col)
        };
        let b = if skip == 0 {
            from
        } else {
            from + self.text[from..]
                .char_indices()
                .nth(skip)
                .map_or(len - from, |(b, _)| b)
        };
        self.last = (line, col, b);
        b
    }
}

/// Walks `src`, reporting its Segments to `c`; returns where the walk ended (the end of the
/// file, or a syntax error), where `Collector::finish` closes what is still open.
pub fn walk(src: &[u8], c: &mut Collector) -> usize {
    let Ok(text) = std::str::from_utf8(src) else {
        return 0;
    };
    let mut bytes = Bytes {
        text,
        lines: LineIndex::new(src),
        last: (0, 0, 0),
    };
    let mut frames: Vec<Frame> = Vec::new();
    let mut docs = 0;
    let mut last_end = 0;
    let parser = Parser::new_from_str(text);
    for next in parser {
        let (ev, span) = match next {
            Ok(e) => e,
            Err(e) => return bytes.of(e.marker()).min(src.len()),
        };
        let (s, e) = (bytes.of(&span.start), bytes.of(&span.end));
        if let Some(Frame::Complex { start, depth }) = frames.last_mut() {
            match ev {
                Event::MappingStart(..) | Event::SequenceStart(..) => *depth += 1,
                Event::MappingEnd | Event::SequenceEnd => {
                    *depth -= 1;
                    // A flow collection's end event spans its `]` or `}`; a block one is empty.
                    if e > s {
                        last_end = last_end.max(e);
                    }
                }
                Event::Scalar(..) | Event::Alias(_) => last_end = e,
                _ => {}
            }
            if *depth == 0 {
                let start = *start;
                frames.pop();
                let raw = &text[start..last_end.max(start)];
                let first = raw.lines().next().unwrap_or("");
                let name: String = first.chars().take(COMPLEX_KEY_CHARS).collect();
                let mut k = OutlineSegment::new("key", name.clone(), start..start, start..last_end);
                // The name is a prefix of the key's first line: verbatim.
                k.name = Some(vec![(0..name.len(), start..start + name.len())]);
                k.name_anchor = start;
                c.open(k);
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = name;
                }
            }
            continue;
        }
        match ev {
            Event::DocumentStart(_) => {
                docs += 1;
                if docs == 2 {
                    c.reveal("document");
                }
                let mut d = OutlineSegment::new("document", format!("document {docs}"), s..s, s..s);
                d.hidden = docs == 1;
                c.open(d);
                frames.push(Frame::Doc);
            }
            Event::DocumentEnd => {
                frames.pop();
                c.close(last_end.max(s));
            }
            Event::MappingStart(..) | Event::SequenceStart(..) if expecting_key(&frames) => {
                frames.push(Frame::Complex { start: s, depth: 1 });
            }
            Event::MappingStart(..) => {
                begin_value(&mut frames, c, s);
                frames.push(Frame::Map {
                    key: false,
                    key_name: String::new(),
                });
            }
            Event::SequenceStart(..) => {
                let at_top = matches!(frames.last(), None | Some(Frame::Doc));
                let holder = begin_value(&mut frames, c, s);
                if !at_top {
                    // The key or element that holds the sequence is hidden (spec §5.2).
                    c.hide_top();
                }
                frames.push(Frame::Seq { holder, n: 0 });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                // A flow collection's end event spans its `]` or `}` (`a: {}` ends at byte 5); a
                // block collection's is empty, at the next token, so it ends at its last scalar.
                if e > s {
                    last_end = last_end.max(e);
                }
                frames.pop();
                end_value(&mut frames, c, last_end);
            }
            Event::Scalar(value, style, ..) if expecting_key(&frames) => {
                last_end = e;
                let mut k = OutlineSegment::new("key", value.to_string(), s..s, s..e);
                let pieces = name_pieces(text, s..e, &value, style);
                k.name_anchor = pieces.first().map_or(s, |(_, r)| r.start);
                k.name = Some(pieces);
                c.open(k);
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = value.to_string();
                }
            }
            Event::Alias(_) if expecting_key(&frames) => {
                // An alias used as a key (`*a : x`) is named by its source text.
                last_end = e;
                let name = text[s..e].to_string();
                let mut k = OutlineSegment::new("key", name.clone(), s..s, s..e);
                k.name = Some(vec![(0..name.len(), s..e)]);
                k.name_anchor = s;
                c.open(k);
                if let Some(Frame::Map { key, key_name }) = frames.last_mut() {
                    *key = true;
                    *key_name = name;
                }
            }
            Event::Scalar(..) | Event::Alias(_) => {
                last_end = e;
                begin_value(&mut frames, c, s);
                end_value(&mut frames, c, e);
            }
            _ => {}
        }
    }
    src.len()
}

/// The top frame is a mapping waiting for a key.
fn expecting_key(frames: &[Frame]) -> bool {
    matches!(frames.last(), Some(Frame::Map { key: false, .. }))
}

/// A value starts at `s`. In a sequence it opens the element Segment `holder[n]` and returns
/// that name; in a mapping the open key already holds it, and its name is returned; at the top
/// of a document the name is empty, so a top-level sequence's elements are `[0]`, `[1]`…
fn begin_value(frames: &mut [Frame], c: &mut Collector, s: usize) -> String {
    match frames.last_mut() {
        Some(Frame::Seq { holder, n }) => {
            let name = format!("{holder}[{n}]");
            *n += 1;
            c.open(OutlineSegment::new("element", name.clone(), s..s, s..s));
            name
        }
        Some(Frame::Map {
            key: true,
            key_name,
        }) => key_name.clone(),
        _ => String::new(),
    }
}

/// The value that began last ends at `end`: it closes the key or the element that holds it.
fn end_value(frames: &mut [Frame], c: &mut Collector, end: usize) {
    match frames.last_mut() {
        Some(Frame::Map { key, .. }) if *key => {
            c.close(end);
            *key = false;
        }
        Some(Frame::Seq { .. }) => c.close(end),
        _ => {}
    }
}

/// The name pieces of a key scalar over `span` (`Name::pieces`): one verbatim piece when the
/// source is the value, plain or inside its quotes; otherwise the scalar's source decoded line
/// by line, one piece per line, so a Match maps to the line it is on; and when that decoding
/// does not give saphyr's `value`, one decoded piece over the whole span.
fn name_pieces(text: &str, span: Range<usize>, value: &str, style: ScalarStyle) -> Pieces {
    let (s, e) = (span.start, span.end);
    let raw = &text[s..e];
    let quoted = matches!(style, ScalarStyle::SingleQuoted | ScalarStyle::DoubleQuoted);
    if raw == value {
        return vec![(0..value.len(), s..e)];
    }
    if quoted && raw.len() >= 2 && &raw[1..raw.len() - 1] == value {
        return vec![(0..value.len(), s + 1..e - 1)];
    }
    let decoded = match style {
        ScalarStyle::Plain => Some(by_lines(text, s..e, Quote::None)),
        ScalarStyle::DoubleQuoted if raw.len() >= 2 => {
            Some(by_lines(text, s + 1..e - 1, Quote::Double))
        }
        ScalarStyle::SingleQuoted if raw.len() >= 2 => {
            Some(by_lines(text, s + 1..e - 1, Quote::Single))
        }
        _ => None,
    };
    match decoded {
        Some((joined, pieces)) if joined == value => pieces,
        _ => vec![(0..value.len(), s..e)],
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
}

/// Decodes a flow scalar's source `inner` (inside its quotes) line by line: each line without
/// its indentation and, before a line break, without its trailing blanks; escapes decoded;
/// lines joined by one space, an empty line by a line break, and an escaped line break (a
/// double-quoted line ending in `\`) by nothing. Returns the text and one piece per line.
fn by_lines(text: &str, inner: Range<usize>, quote: Quote) -> (String, Pieces) {
    let body = &text[inner.clone()];
    let mut out = String::new();
    let mut pieces = Vec::new();
    let mut offset = inner.start;
    let parts: Vec<&str> = body.split('\n').collect();
    let last = parts.len() - 1;
    let mut pending = 0;
    let mut glue = false;
    for (i, line) in parts.iter().enumerate() {
        let line_start = offset;
        offset += line.len() + 1;
        let line = line.strip_suffix('\r').unwrap_or(line);
        let lead = if i == 0 {
            0
        } else {
            line.len() - line.trim_start_matches([' ', '\t']).len()
        };
        let mut part = &line[lead..];
        let mut escaped_break = false;
        if i < last {
            if quote == Quote::Double && ends_in_escape(part) {
                part = &part[..part.len() - 1];
                escaped_break = true;
            } else {
                part = part.trim_end_matches([' ', '\t']);
            }
        }
        if i > 0 && i < last && part.is_empty() && !glue {
            pending += 1;
            continue;
        }
        if i > 0 {
            if glue {
                // An escaped line break joins with nothing.
            } else if pending > 0 {
                out.push_str(&"\n".repeat(pending));
            } else {
                out.push(' ');
            }
        }
        pending = 0;
        glue = escaped_break;
        let from = line_start + lead;
        let src = from..from + part.len();
        let decoded = match quote {
            Quote::None => part.to_string(),
            Quote::Single => part.replace("''", "'"),
            Quote::Double => unescape(part),
        };
        if !decoded.is_empty() {
            pieces.push((out.len()..out.len() + decoded.len(), src));
        }
        out.push_str(&decoded);
    }
    (out, pieces)
}

/// A double-quoted line ends in an unescaped `\`: an odd run of backslashes.
fn ends_in_escape(part: &str) -> bool {
    part.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}

/// The escapes of a YAML 1.2 double-quoted scalar, decoded. An unknown escape is kept as is:
/// the result then differs from saphyr's value and the name falls back to one decoded piece.
fn unescape(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    let mut chars = part.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(esc) = chars.next() else {
            out.push('\\');
            break;
        };
        let hex = |chars: &mut std::str::Chars, n: usize| -> Option<char> {
            let h: String = chars.by_ref().take(n).collect();
            u32::from_str_radix(&h, 16).ok().and_then(char::from_u32)
        };
        let decoded = match esc {
            '0' => Some('\0'),
            'a' => Some('\u{7}'),
            'b' => Some('\u{8}'),
            't' | '\t' => Some('\t'),
            'n' => Some('\n'),
            'v' => Some('\u{b}'),
            'f' => Some('\u{c}'),
            'r' => Some('\r'),
            'e' => Some('\u{1b}'),
            ' ' => Some(' '),
            '"' => Some('"'),
            '/' => Some('/'),
            '\\' => Some('\\'),
            'N' => Some('\u{85}'),
            '_' => Some('\u{a0}'),
            'L' => Some('\u{2028}'),
            'P' => Some('\u{2029}'),
            'x' => hex(&mut chars, 2),
            'u' => hex(&mut chars, 4),
            'U' => hex(&mut chars, 8),
            _ => None,
        };
        match decoded {
            Some(d) => out.push(d),
            None => {
                out.push('\\');
                out.push(esc);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::needles;
    use crate::processors::Processor;
    use std::path::Path;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/data")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn yaml_keys_sequences_block_scalars_and_merge_keys() {
        assert_eq!(
            needles(&fixture("config.yaml"), "yaml"),
            vec![
                vec!["server", "ports[2]", "host"], // spec §11 #4
                vec!["server", "motd"],
                vec!["worker", "tag"],
            ]
        );
    }

    #[test]
    fn character_offsets_become_bytes() {
        // "ñandú" is 5 characters and 7 bytes: a Trail after it must still be right.
        let src = "a: ñandú\nb:\n  c: needle\n".as_bytes();
        assert_eq!(needles(src, "yaml"), vec![vec!["b", "c"]]);
    }

    #[test]
    fn documents_appear_only_when_there_are_two() {
        assert_eq!(
            needles(&fixture("multi.yaml"), "yaml"),
            vec![vec!["document 1", "first"], vec!["document 2", "second[0]"]]
        );
        assert_eq!(needles(b"only: needle\n", "yaml"), vec![vec!["only"]]);
    }

    #[test]
    fn a_syntax_error_keeps_the_trails_before_it() {
        // A block sequence entry where a key is expected: saphyr-parser 0.2.2 stops at line 3
        // ("did not find expected key"), verified in the probe.
        let src = b"a:\n  b: needle\n- x\nd: needle\n";
        assert_eq!(needles(src, "yaml"), vec![vec!["a", "b"], vec![]]);
    }

    #[test]
    fn past_the_tree_sitter_yaml_row_limit() {
        // tree-sitter-yaml fails over 32 768 lines (spec §4.3); the native parser does not.
        let mut s = String::new();
        for i in 0..40_000 {
            s.push_str(&format!("k{i}: v\n"));
        }
        s.push_str("last:\n  deep: needle\n");
        assert_eq!(needles(s.as_bytes(), "yaml"), vec![vec!["last", "deep"]]);
    }

    #[test]
    fn flow_collections_end_at_their_bracket() {
        let src = b"a: {}\nb: needle\n";
        let o = super::super::outline_for(src, super::super::Syntax::Yaml, &[3]);
        let a = o.segments.iter().find(|s| s.segment.name == "a").unwrap();
        assert_eq!(a.range, 0..5);
    }

    #[test]
    fn a_multi_line_key_has_one_piece_per_line() {
        let src = b"? multi\n  line key\n: needle\n";
        let input = crate::model::Input {
            bytes: src,
            path: Path::new("x"),
            format: "yaml",
            disk: None,
        };
        let names = super::super::Keypath
            .names(&input, &super::super::tests::params("yaml"))
            .unwrap();
        let n = names
            .iter()
            .find(|n| n.text == "multi line key")
            .expect("the key");
        let lines: Vec<&str> = n
            .pieces
            .iter()
            .map(|(_, r)| std::str::from_utf8(&src[r.clone()]).unwrap())
            .collect();
        assert_eq!(lines, ["multi", "line key"]);
    }

    #[test]
    fn an_escaped_line_break_keeps_each_part_on_its_line() {
        // saphyr gives "foobar": no space marks the join, so the pieces come from the source.
        let src = b"? \"foo\\\n  bar\"\n: needle\n";
        let input = crate::model::Input {
            bytes: src,
            path: Path::new("x"),
            format: "yaml",
            disk: None,
        };
        let names = super::super::Keypath
            .names(&input, &super::super::tests::params("yaml"))
            .unwrap();
        let n = names.iter().find(|n| n.text == "foobar").expect("the key");
        let line_of =
            |r: &std::ops::Range<usize>| src[..r.start].iter().filter(|&&b| b == b'\n').count() + 1;
        let bar = n
            .pieces
            .iter()
            .find(|(t, _)| t.end == "foobar".len())
            .unwrap();
        assert_eq!(line_of(&bar.1), 2, "{:?}", n.pieces);
        assert_eq!(n.pieces.len(), 2);
    }

    #[test]
    fn deep_flow_nesting_is_a_syntax_error_not_a_crash() {
        // saphyr-parser stops at its recursion limit with an error (probe: "recursion limit
        // exceeded at byte 258"); the Trails before it stay.
        let mut s = String::from("a: needle\nb: ");
        s.push_str(&"[".repeat(100_000));
        s.push_str(&"]".repeat(100_000));
        s.push('\n');
        assert_eq!(needles(s.as_bytes(), "yaml"), vec![vec!["a"]]);
    }

    #[test]
    fn complex_keys_are_named_by_their_text() {
        let src = b"? [a, b]\n: needle\n";
        assert_eq!(needles(src, "yaml"), vec![vec!["[a, b]"]]);
    }

    #[test]
    fn empty_values_alias_keys_and_quoted_names() {
        let src = b"a:\nb: needle\nbase: &x k\n? *x\n: needle\n'it''s': needle\n";
        assert_eq!(
            needles(src, "yaml"),
            vec![vec!["b"], vec!["*x"], vec!["it's"]]
        );
    }
}
