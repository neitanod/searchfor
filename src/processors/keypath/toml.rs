//! The TOML Outline of `keypath` (spec §5.2), over toml_edit's spans. TOML files are small, so
//! every Segment is built (Cargo.lock at toml_edit's 150 ms/MB is the large case). Sections
//! follow headers: a header's Section runs to the next header outside it, and an ancestor with no
//! explicit table whose Section covers that header gets a Segment per header (D36). Paths are
//! lists of components, never dotted strings. On a syntax error the longest prefix found that
//! parses, ending at a statement start, is used (D3, Task 3.2).
use crate::trail::{Outline, OutlineSegment};
use std::collections::HashSet;
use std::ops::Range;
use toml_edit::{Array, InlineTable, Item, Key, Table, Value};

/// The recovery parses at most this many times the file's size…
const BUDGET_FACTOR: usize = 64;
/// …and never more than this many bytes in all.
const BUDGET_MAX: usize = 32 << 20;

type Pieces = Vec<(Range<usize>, Range<usize>)>;

/// One component of a key path: `["a.b"]` is one `Key("a.b")`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Key(String),
    Index(usize),
}

/// An explicit header: a table that is neither implicit nor dotted, or an element of an array
/// of tables.
struct Header {
    path: Vec<Step>,
    /// The header itself (`[server]`, `[[products]]`).
    span: Range<usize>,
    /// The source of each key of the header, in order.
    parts: Vec<Range<usize>>,
    /// From the header to the next header whose path is not under it.
    section: Range<usize>,
}

pub fn outline(src: &[u8]) -> Outline {
    let Ok(text) = std::str::from_utf8(src) else {
        return Outline::default();
    };
    let (doc, end) = match text.parse::<toml_edit::Document<String>>() {
        Ok(doc) => (doc, text.len()),
        Err(e) => {
            let Some(len) = recover(text, &e).0 else {
                return Outline::default();
            };
            match text[..len].parse::<toml_edit::Document<String>>() {
                Ok(doc) => (doc, len),
                Err(_) => return Outline::default(),
            }
        }
    };
    let mut b = Build {
        text,
        headers: Vec::new(),
        segments: Vec::new(),
    };
    b.table(doc.as_table(), &mut Vec::new(), &[]);
    b.header_segments(end);
    Outline::build(b.segments, src)
}

/// The bytes the recovery of `text` parsed, for the budget test.
#[cfg(test)]
pub(crate) fn recovery_parsed_bytes(text: &str) -> usize {
    match text.parse::<toml_edit::Document<String>>() {
        Ok(_) => 0,
        Err(e) => recover(text, &e).1,
    }
}

/// The length of the longest prefix of `text` found that parses and ends at a statement start,
/// within the budget, and the bytes parsed to find it (Task 3.2, step 1).
fn recover(text: &str, err: &toml_edit::TomlError) -> (Option<usize>, usize) {
    let mut r = Recovery::new(text);
    let best = r.run(err);
    (best.map(|l| r.starts[l - 1]), r.parsed)
}

/// The search for a prefix that parses. A cut at line `l` is the prefix that ends before line
/// `l`; line 1 gives the empty document.
struct Recovery<'t> {
    text: &'t str,
    /// `starts[l - 1]` is where line `l` starts; the last entry is the end of the text.
    starts: Vec<usize>,
    nlines: usize,
    tried: HashSet<usize>,
    parsed: usize,
    budget: usize,
}

impl<'t> Recovery<'t> {
    fn new(text: &'t str) -> Self {
        let mut starts = vec![0];
        starts.extend(memchr::memchr_iter(b'\n', text.as_bytes()).map(|i| i + 1));
        if starts.last() != Some(&text.len()) {
            starts.push(text.len());
        }
        Self {
            text,
            nlines: starts.len() - 1,
            starts,
            tried: HashSet::new(),
            parsed: 0,
            budget: (BUDGET_FACTOR * text.len()).min(BUDGET_MAX),
        }
    }

    fn over_budget(&self) -> bool {
        self.parsed > self.budget
    }

    /// A statement start candidate: a line whose first non-blank character is `[`, or a key
    /// character on a line that contains `=`; line 1 always counts.
    fn is_start(&self, l: usize) -> bool {
        if l == 1 {
            return true;
        }
        let line = &self.text[self.starts[l - 1]..self.starts[l]];
        let Some(c) = line.trim_start().chars().next() else {
            return false;
        };
        c == '['
            || ((c.is_ascii_alphanumeric() || matches!(c, '"' | '\'' | '_' | '-'))
                && line.contains('='))
    }

    /// The nearest statement start at or before line `l`.
    fn snap(&self, mut l: usize) -> usize {
        while l > 1 && !self.is_start(l) {
            l -= 1;
        }
        l
    }

    /// Whether the cut at line `l` parses; None when it was tried before or the budget is spent.
    fn attempt(&mut self, l: usize) -> Option<bool> {
        if self.over_budget() || !self.tried.insert(l) {
            return None;
        }
        let prefix = &self.text[..self.starts[l - 1]];
        self.parsed += prefix.len();
        Some(prefix.parse::<toml_edit::Document<String>>().is_ok())
    }

    /// The line of the best cut found.
    fn run(&mut self, err: &toml_edit::TomlError) -> Option<usize> {
        if self.nlines == 0 {
            return None;
        }
        let at = err
            .span()
            .map_or(self.text.len(), |s| s.start)
            .min(self.text.len());
        let reported = match self.starts.binary_search(&at) {
            Ok(i) => i + 1,
            Err(i) => i,
        };
        // An error at the end of the file (an unclosed `"""` or array, or no span): the whole
        // text already failed, so the search starts at the last line.
        let e = reported.clamp(1, self.nlines);
        // a. Back off 1, 2, 4, 8… lines from the error's line, each cut snapped.
        let mut best = None;
        let mut failed = None;
        let mut c = self.snap(e);
        let mut step = 1;
        loop {
            match self.attempt(c) {
                Some(true) => {
                    best = Some(c);
                    break;
                }
                Some(false) => failed = Some(c),
                None if self.over_budget() => break,
                None => {}
            }
            if c == 1 {
                break;
            }
            c = self.snap(c.saturating_sub(step).max(1));
            step *= 2;
        }
        // b. Bisect between the candidate and the nearest cut that failed, over snapped cuts.
        if let (Some(mut lo), Some(mut hi)) = (best, failed) {
            while hi > lo + 1 && !self.over_budget() {
                let mid = (lo + hi) / 2;
                let mut cut = self.snap(mid);
                if cut <= lo || self.tried.contains(&cut) {
                    // The first statement start after the midpoint and before the failed cut.
                    match (mid + 1..hi).find(|&n| self.is_start(n) && !self.tried.contains(&n)) {
                        Some(n) => cut = n,
                        None => break,
                    }
                }
                match self.attempt(cut) {
                    Some(true) => {
                        lo = cut;
                        best = best.max(Some(cut));
                    }
                    Some(false) => hi = cut,
                    None => break,
                }
            }
        }
        // c. Sweep the statement starts not tried yet, from the error towards the best prefix:
        // valid prefixes are not monotone.
        let floor = best.unwrap_or(0);
        let mut l = e;
        while l > floor && !self.over_budget() {
            if self.is_start(l) && !self.tried.contains(&l) && self.attempt(l) == Some(true) {
                best = Some(l);
                break;
            }
            l -= 1;
        }
        best
    }
}

/// Collects the headers and the key-value Segments of one document.
struct Build<'t> {
    text: &'t str,
    headers: Vec<Header>,
    segments: Vec<OutlineSegment>,
}

impl Build<'_> {
    /// Walks `table`, whose path is `path`; `dotted` holds the keys of the dotted tables that
    /// lead to it (`a` of `a.x = 1`), empty for a header's own table.
    fn table(&mut self, table: &Table, path: &mut Vec<Step>, dotted: &[String]) {
        for (k, item) in table.iter() {
            let Some(key) = table.key(k) else { continue };
            match item {
                Item::Value(v) => self.key_value(dotted, key, v, false),
                Item::Table(t) => {
                    path.push(Step::Key(k.to_string()));
                    if t.is_dotted() {
                        let mut chain = dotted.to_vec();
                        chain.push(k.to_string());
                        self.table(t, path, &chain);
                    } else {
                        if !t.is_implicit() {
                            self.header(path, t.span());
                        }
                        self.table(t, path, &[]);
                    }
                    path.pop();
                }
                Item::ArrayOfTables(a) => {
                    for (i, t) in a.iter().enumerate() {
                        path.push(Step::Key(k.to_string()));
                        path.push(Step::Index(i));
                        self.header(path, t.span());
                        self.table(t, path, &[]);
                        path.pop();
                        path.pop();
                    }
                }
                Item::None => {}
            }
        }
    }

    fn header(&mut self, path: &[Step], span: Option<Range<usize>>) {
        let Some(span) = span else { return };
        let mut at = span.start;
        let bytes = self.text.as_bytes();
        while at < span.end && matches!(bytes[at], b'[' | b' ' | b'\t') {
            at += 1;
        }
        let parts = key_parts(self.text, at, span.end);
        self.headers.push(Header {
            path: path.to_vec(),
            span,
            parts,
            section: 0..0,
        });
    }

    /// A key with a value, inside a table or an inline table (`inline`): its Segment runs from
    /// the key's start to the value's end; a dotted key gives each component one over the same
    /// key-value. An array value hides its key and gives each element a Segment.
    fn key_value(&mut self, dotted: &[String], key: &Key, value: &Value, inline: bool) {
        let (Some(kspan), Some(vspan)) = (key.span(), value.span()) else {
            return;
        };
        let mut start = kspan.start;
        let mut labels: Vec<Range<usize>> = Vec::new();
        if !dotted.is_empty() {
            match dotted_parts(self.text, kspan.clone(), dotted.len()) {
                Some(parts) => {
                    start = parts[0].start;
                    labels = parts[..dotted.len()].to_vec();
                }
                None => {
                    // From the first byte of its line after indentation (in a table; an inline
                    // table's key-value starts at its leaf key).
                    if !inline {
                        let line = self.text[..kspan.start].rfind('\n').map_or(0, |i| i + 1);
                        let lead = self.text[line..kspan.start].len()
                            - self.text[line..kspan.start].trim_start().len();
                        start = line + lead;
                    }
                    labels = vec![start..start; dotted.len()];
                }
            }
        }
        let range = start..vspan.end.max(start);
        for (name, label) in dotted.iter().zip(labels) {
            self.segments
                .push(named("key", name, range.clone(), label, self.text));
        }
        let mut leaf = named("key", key.get(), range, kspan, self.text);
        match value {
            Value::Array(a) => {
                leaf.hidden = true;
                self.segments.push(leaf);
                self.elements(a, key.get());
            }
            Value::InlineTable(t) => {
                self.segments.push(leaf);
                self.inline(t, &[]);
            }
            _ => self.segments.push(leaf),
        }
    }

    fn inline(&mut self, table: &InlineTable, dotted: &[String]) {
        for (k, v) in table.iter() {
            let Some(key) = table.key(k) else { continue };
            match v {
                Value::InlineTable(t) if t.is_dotted() => {
                    let mut chain = dotted.to_vec();
                    chain.push(k.to_string());
                    self.inline(t, &chain);
                }
                _ => self.key_value(dotted, key, v, true),
            }
        }
    }

    /// The elements of an array held by `holder`: `holder[i]`, recursively.
    fn elements(&mut self, array: &Array, holder: &str) {
        for (i, v) in array.iter().enumerate() {
            let Some(span) = v.span() else { continue };
            let name = format!("{holder}[{i}]");
            let mut seg = OutlineSegment::new(
                "element",
                name.clone(),
                span.clone(),
                span.start..span.start,
            );
            match v {
                Value::Array(a) => {
                    seg.hidden = true;
                    self.segments.push(seg);
                    self.elements(a, &name);
                }
                Value::InlineTable(t) => {
                    self.segments.push(seg);
                    self.inline(t, &[]);
                }
                _ => self.segments.push(seg),
            }
        }
    }

    /// The Sections of the headers, and their Segments (Task 3.2, steps 3 and 4).
    fn header_segments(&mut self, end: usize) {
        self.headers.sort_by_key(|h| h.span.start);
        for i in 0..self.headers.len() {
            let to = self.headers[i + 1..]
                .iter()
                .find(|o| !o.path.starts_with(&self.headers[i].path))
                .map_or(end, |o| o.span.start);
            let from = self.headers[i].span.start;
            self.headers[i].section = from..to.max(from);
        }
        for h in &self.headers {
            let mut parts = h.parts.iter();
            for p in 1..=h.path.len() {
                let part = match &h.path[p - 1] {
                    Step::Key(_) => parts.next().cloned(),
                    Step::Index(_) => None,
                };
                let own = p == h.path.len();
                if !own {
                    let prefix = &h.path[..p];
                    let covered = self
                        .headers
                        .iter()
                        .any(|o| o.path == prefix && o.section.contains(&h.span.start));
                    if covered {
                        continue;
                    }
                }
                let seg = match &h.path[p - 1] {
                    Step::Key(k) => {
                        let holds_array = matches!(h.path.get(p), Some(Step::Index(_)));
                        let label = if holds_array {
                            part.clone().unwrap_or(h.span.start..h.span.start)
                        } else {
                            h.span.clone()
                        };
                        let mut seg =
                            OutlineSegment::new("key", k.clone(), h.section.clone(), label);
                        if let Some(part) = part {
                            let pieces = key_pieces(self.text, part, k);
                            seg.name_anchor = pieces.first().map_or(0, |(_, r)| r.start);
                            seg.name = Some(pieces);
                        }
                        seg.hidden = holds_array;
                        seg
                    }
                    Step::Index(i) => {
                        let holder = match p.checked_sub(2).map(|j| &h.path[j]) {
                            Some(Step::Key(k)) => k.as_str(),
                            _ => "",
                        };
                        OutlineSegment::new(
                            "element",
                            format!("{holder}[{i}]"),
                            h.section.clone(),
                            h.span.clone(),
                        )
                    }
                };
                self.segments.push(seg);
            }
        }
    }
}

/// A key Segment named `name`, searchable by the key's source `label`.
fn named(
    kind: &str,
    name: &str,
    range: Range<usize>,
    label: Range<usize>,
    text: &str,
) -> OutlineSegment {
    let mut seg = OutlineSegment::new(kind, name, range, label.clone());
    if !label.is_empty() {
        let pieces = key_pieces(text, label, name);
        seg.name_anchor = pieces.first().map_or(0, |(_, r)| r.start);
        seg.name = Some(pieces);
    }
    seg
}

/// The name pieces of a key whose source is `span`: verbatim when the source is the key, or
/// the inside of its quotes; otherwise one decoded piece over the span.
fn key_pieces(text: &str, span: Range<usize>, key: &str) -> Pieces {
    let raw = &text[span.clone()];
    if raw == key {
        return vec![(0..key.len(), span)];
    }
    let quoted = raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')));
    if quoted && &raw[1..raw.len() - 1] == key {
        return vec![(0..key.len(), span.start + 1..span.end - 1)];
    }
    vec![(0..key.len(), span)]
}

/// A bare key character (TOML's `A-Za-z0-9_-`, and any non-ASCII byte, leniently).
fn bare(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-') || b >= 0x80
}

/// The source of each key of the dotted key that starts at `at` (`a . "b" . c`), up to `end`.
fn key_parts(text: &str, mut at: usize, end: usize) -> Vec<Range<usize>> {
    let b = text.as_bytes();
    let blank = |at: &mut usize| {
        while *at < end && matches!(b[*at], b' ' | b'\t') {
            *at += 1;
        }
    };
    let mut parts = Vec::new();
    loop {
        blank(&mut at);
        let start = at;
        match b.get(at) {
            Some(&q @ (b'"' | b'\'')) if at < end => {
                at += 1;
                while at < end && b[at] != q {
                    at += if q == b'"' && b[at] == b'\\' { 2 } else { 1 };
                }
                at = (at + 1).min(end);
            }
            _ => {
                while at < end && bare(b[at]) {
                    at += 1;
                }
            }
        }
        if at == start {
            break;
        }
        parts.push(start..at);
        blank(&mut at);
        if at < end && b[at] == b'.' {
            at += 1;
        } else {
            break;
        }
    }
    parts
}

/// The sources of the `before` keys that precede the leaf key at `leaf` in a dotted key, and of
/// the leaf, found by scanning back from the leaf (`a.x` gives `a` and `x`). None when the
/// source does not read as such a key.
fn dotted_parts(text: &str, leaf: Range<usize>, before: usize) -> Option<Vec<Range<usize>>> {
    let b = text.as_bytes();
    let mut i = leaf.start;
    let blank_back = |i: &mut usize| {
        while *i > 0 && matches!(b[*i - 1], b' ' | b'\t') {
            *i -= 1;
        }
    };
    for _ in 0..before {
        blank_back(&mut i);
        if i == 0 || b[i - 1] != b'.' {
            return None;
        }
        i -= 1;
        blank_back(&mut i);
        match b.get(i.checked_sub(1)?) {
            Some(&q @ (b'"' | b'\'')) => {
                i -= 1;
                loop {
                    i = i.checked_sub(1)?;
                    let escaped = q == b'"'
                        && b[..i].iter().rev().take_while(|&&c| c == b'\\').count() % 2 == 1;
                    if b[i] == q && !escaped {
                        break;
                    }
                }
            }
            _ => {
                let end = i;
                while i > 0 && bare(b[i - 1]) {
                    i -= 1;
                }
                if i == end {
                    return None;
                }
            }
        }
    }
    let parts = key_parts(text, i, leaf.end);
    (parts.len() == before + 1 && parts.last() == Some(&leaf)).then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::super::tests::needles;
    use std::path::Path;

    fn fixture() -> Vec<u8> {
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/shop.toml"))
            .unwrap()
    }

    #[test]
    fn toml_tables_arrays_of_tables_dotted_and_inline() {
        assert_eq!(
            needles(&fixture(), "toml"),
            vec![
                vec!["owner", "name"],
                vec!["products[1]", "tags[1]"],
                vec!["products[1]", "variants[0]", "sku"],
                vec!["database", "replica", "url"],
                vec!["database", "replica", "inline", "user"],
            ]
        );
    }

    #[test]
    fn a_table_runs_to_the_next_header_outside_it() {
        let src = fixture();
        let o = super::outline(&src);
        let p1 = o
            .segments
            .iter()
            .find(|s| s.segment.name == "products[1]")
            .unwrap();
        let db = o
            .segments
            .iter()
            .find(|s| s.segment.name == "database")
            .unwrap();
        // products[1] covers its [[products.variants]] and ends where [database.replica] starts.
        assert_eq!((p1.segment.line, p1.segment.end_line), (11, 17));
        assert_eq!(db.segment.line, 18);
        let ports = o
            .segments
            .iter()
            .find(|s| s.segment.name == "ports")
            .unwrap();
        assert!(ports.hidden);
        // The key of an array of tables: hidden in Trails, still a name.
        assert!(
            o.segments
                .iter()
                .filter(|s| s.segment.name == "products")
                .all(|s| s.hidden)
        );
        assert!(
            crate::trail::names_of(&o)
                .iter()
                .any(|n| n.text == "products")
        );
    }

    #[test]
    fn interleaved_tables_and_quoted_keys() {
        let src = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data/interleaved.toml"),
        )
        .unwrap();
        assert_eq!(
            needles(&src, "toml"),
            vec![
                vec!["a", "b", "k"],
                vec!["x", "k"], // never `a › x`: no Segment spans [a.b] to [a.c]
                vec!["a", "c", "k"],
                vec!["a.b", "k"], // one key, quoted
                vec!["site", "x[0]", "k"],
                vec!["p", "k"],
                vec!["p", "r", "k"], // [p]'s Section ended at [q]; `p` is given again
            ]
        );
    }

    #[test]
    fn dotted_keys_spread_over_a_table_do_not_wrap_their_neighbours() {
        let src = b"a.x = \"needle\"\nb = \"needle\"\na.y = \"needle\"\n";
        assert_eq!(
            needles(src, "toml"),
            vec![vec!["a", "x"], vec!["b"], vec!["a", "y"]]
        );
    }

    #[test]
    fn a_syntax_error_keeps_the_trails_before_it() {
        // toml_edit reports these at a later line than where the damage starts (review r1,
        // Codex #10); the walk back line by line finds the last prefix that parses.
        for src in [
            &b"[a]\nb = \"needle\"\n[c\nd = \"needle\"\n"[..],
            b"[a]\nb=\"needle\"\nx=[\n[c]\nd=\"needle\"\n",
            b"[a]\nb = \"needle\"\ns = \"\"\"\nopen\n[c]\nd = \"needle\"\n",
        ] {
            assert_eq!(
                needles(src, "toml"),
                vec![vec!["a", "b"], vec![]],
                "{}",
                String::from_utf8_lossy(src)
            );
        }
    }

    #[test]
    fn recovery_does_not_assume_valid_prefixes_are_monotone() {
        // Review r3, Codex #3: cuts at lines 20, 19, 17, 13, 5 and 1 all fall inside a string or
        // give an empty document; the prefix ending at line 15 is the one that keeps `k`.
        let mut src = String::from("a = \"\"\"\n");
        for i in 2..=12 {
            src.push_str(&format!("line {i}\n"));
        }
        src.push_str("\"\"\"\n");
        src.push_str("k = \"needle\"\n");
        src.push_str("b = \"\"\"\n");
        for i in 16..=19 {
            src.push_str(&format!("open {i}\n"));
        }
        assert_eq!(needles(src.as_bytes(), "toml"), vec![vec!["k"]]);
    }

    #[test]
    fn statement_like_lines_inside_strings_only_cost_budget() {
        // Review r5, Codex #2: a valid string of 100 statement-like lines, `note`, and an open
        // one of 100 more. Every line inside is a candidate and fails; the recovery stays within
        // its budget (64 × the file's size) and never gives a wrong Trail: `note` keeps its own
        // or none, and nothing is attributed to a key inside the strings.
        let mut src = String::from("title = \"x\"\ndoc = \"\"\"\n");
        for i in 0..100 {
            src.push_str(&format!("option{i} = text {i}\n"));
        }
        src.push_str("\"\"\"\nnote = \"needle\"\nopen = \"\"\"\n");
        for i in 0..100 {
            src.push_str(&format!("option{i} = text {i}\n"));
        }
        let t = needles(src.as_bytes(), "toml");
        assert!(
            t == vec![vec!["note".to_string()]] || t == vec![Vec::<String>::new()],
            "{t:?}"
        );
        assert!(
            super::recovery_parsed_bytes(&src) <= 64 * src.len() + src.len(),
            "over budget"
        );
    }

    #[test]
    fn an_open_string_deep_in_a_lock_file_keeps_the_line_before_it() {
        // Review r4, Fable #1: 5 000 lines shaped like a Cargo.lock, a `"""` left open in the
        // middle, and the `needle` on the line just before it.
        let mut src = String::new();
        let mut i = 0;
        while src.lines().count() < 5_000 {
            i += 1;
            src.push_str(&format!("[[package]]\nname = \"crate-{i}\"\nversion = \"1.0.{i}\"\ndependencies = [\n \"crate-{}\",\n]\n\n", i + 1));
        }
        let lines: Vec<&str> = src.lines().collect();
        // A `[[package]]` line near the middle, so the insertion is between two tables, never
        // inside a `dependencies` array.
        let open = (lines.len() / 2..lines.len())
            .find(|&k| lines[k] == "[[package]]")
            .unwrap();
        let mut broken = String::new();
        for (k, l) in lines.iter().enumerate() {
            if k == open {
                broken.push_str("note = \"needle\"\n");
                broken.push_str("description = \"\"\"\n");
            }
            broken.push_str(l);
            broken.push('\n');
        }
        let t = needles(broken.as_bytes(), "toml");
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].last().map(String::as_str), Some("note"), "{t:?}");
    }

    #[test]
    fn nested_arrays_inline_dotted_keys_and_header_lines() {
        let src = b"m = [[1, 2], [3, \"needle\"]]\ni = { c = 1, a.b = \"needle\" }\n[t.needle]\n[fruit]\napple.color = 1\n[fruit.apple.texture]\nsmooth = \"needle\"\n";
        assert_eq!(
            needles(src, "toml"),
            vec![
                vec!["m[1][1]"],
                vec!["i", "a", "b"],
                vec![], // in a header's own text
                vec!["fruit", "apple", "texture", "smooth"],
            ]
        );
    }
}
