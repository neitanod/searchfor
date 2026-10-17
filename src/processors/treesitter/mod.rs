//! treesitter: code Trails from the query of a Format Definition (spec §5.3, design note 7).
//! `compile`, `parse` and `outline` take any Language, so the Grammar Helper template (C8)
//! reuses them for a Grammar that is not compiled in.
pub mod grammars;

use super::Processor;
use crate::model::{Input, Name, Trail};
use crate::trail::{Outline, OutlineSegment, TrailCursor, names_of, trail_for};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};
use streaming_iterator::StreamingIterator;
// `trail_for` serves anchors out of order; `TrailCursor` the sorted ones.
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

pub const DEFAULT_MAX_BYTES: i64 = 16 * 1024 * 1024;

/// A Definition's query, compiled for its Language.
pub struct Compiled {
    pub grammar: String,
    pub language: Language,
    query: Query,
    segment: u32,
    name: u32,
    prefix: Option<u32>,
    signature: Option<u32>,
    kinds: HashMap<String, String>,
}

/// Compiles `params.query` for `language`; `@segment` and `@name` are required captures.
pub fn compile(
    grammar: &str,
    language: &Language,
    params: &toml::Table,
) -> anyhow::Result<Compiled> {
    let src = params
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("`query` is missing"))?;
    let query = Query::new(language, src)
        .map_err(|e| anyhow::anyhow!("the query does not compile: {e}"))?;
    let idx = |n: &str| query.capture_index_for_name(n);
    let (Some(segment), Some(name)) = (idx("segment"), idx("name")) else {
        anyhow::bail!("the query needs the captures @segment and @name");
    };
    let kinds = params
        .get("kinds")
        .and_then(|v| v.as_table())
        .map(|t| {
            t.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Ok(Compiled {
        grammar: grammar.into(),
        language: language.clone(),
        segment,
        name,
        prefix: idx("prefix"),
        signature: idx("signature"),
        kinds,
        query,
    })
}

thread_local! {
    static PARSERS: RefCell<HashMap<String, Parser>> = RefCell::new(HashMap::new());
}

/// Parses `src` with this thread's Parser for the Grammar.
pub fn parse(c: &Compiled, src: &[u8]) -> Option<Tree> {
    PARSERS.with(|ps| {
        let mut ps = ps.borrow_mut();
        let p = ps.entry(c.grammar.clone()).or_insert_with(|| {
            let mut p = Parser::new();
            p.set_language(&c.language)
                .expect("ABI checked when the Grammar was linked");
            p
        });
        p.parse(src, None)
    })
}

/// The first line of a Segment, without a trailing `{` (spec §5.3), and without its body when
/// the body starts on that line (`fn top() -> u32 { needle }`, review m2-code-r1, Fable #2).
fn first_line(src: &[u8], n: Node) -> String {
    let end = n
        .child_by_field_name("body")
        .map_or(n.end_byte(), |b| b.start_byte())
        .clamp(n.start_byte(), n.end_byte());
    let text = String::from_utf8_lossy(&src[n.start_byte()..end]);
    let line = text.lines().next().unwrap_or("").trim_end();
    let line = line.strip_suffix('{').unwrap_or(line).trim_end();
    line.chars().take(200).collect()
}

/// The first line of a node's text, decoded lossily, and the Source bytes it comes from. The
/// bytes are counted in the Source: invalid UTF-8 decodes to more bytes than it takes, and the
/// piece must not reach past the name (fuzz, review m2-code-r1).
fn first_line_of(src: &[u8], n: Node) -> (String, Range<usize>) {
    let r = n.start_byte().min(src.len())..n.end_byte().min(src.len());
    let bytes = &src[r.clone()];
    let len = match memchr::memchr(b'\n', bytes) {
        Some(k) if k > 0 && bytes[k - 1] == b'\r' => k - 1,
        Some(k) => k,
        None => bytes.len(),
    };
    let text = String::from_utf8_lossy(&bytes[..len]).into_owned();
    (text, r.start..r.start + len)
}

/// Every `@segment` of the query, or only those that touch `range` (D24).
pub fn outline(c: &Compiled, tree: &Tree, src: &[u8], range: Option<Range<usize>>) -> Outline {
    let mut cursor = QueryCursor::new();
    if let Some(r) = range {
        cursor.set_byte_range(r);
    }
    let mut segs = Vec::new();
    let mut matches = cursor.matches(&c.query, tree.root_node(), src);
    while let Some(m) = matches.next() {
        let (mut seg, mut name, mut prefix, mut sig) = (None, None, None, None);
        // tree-sitter 0.24: `captures` is a field (0.27 made it a method).
        for cap in m.captures {
            match cap.index {
                i if i == c.segment => seg = Some(cap.node),
                i if i == c.name => name = Some(cap.node),
                i if Some(i) == c.prefix => prefix = Some(cap.node),
                i if Some(i) == c.signature => sig = Some(cap.node),
                _ => {}
            }
        }
        let (Some(node), Some(name)) = (seg, name) else {
            continue;
        };
        let (own, own_src) = first_line_of(src, name);
        let (label, pieces) = match prefix {
            Some(p) => {
                let (pt, pt_src) = first_line_of(src, p);
                let at = pt.len() + 1;
                (
                    format!("{pt}.{own}"),
                    vec![(0..pt.len(), pt_src), (at..at + own.len(), own_src)],
                )
            }
            None => (own.clone(), vec![(0..own.len(), own_src)]),
        };
        let kind = c
            .kinds
            .get(node.kind())
            .cloned()
            .unwrap_or_else(|| node.kind().to_string());
        let mut s = OutlineSegment::new(&kind, label, node.byte_range(), name.byte_range());
        s.segment.signature = Some(first_line(src, sig.unwrap_or(node)));
        s.name = Some(pieces);
        s.name_anchor = name.start_byte();
        segs.push(s);
    }
    Outline::build(segs, src)
}

/// A Definition's slot: the compiled query, or None after a failure.
type Slot = Arc<OnceLock<Option<Arc<Compiled>>>>;

/// The run's treesitter (D39): compiled queries and warnings per Format Definition, for one
/// run. `Processors` owns it.
#[derive(Default)]
pub struct TreeSitter {
    /// One slot per Definition, initialized once even under concurrent first calls: the
    /// compiled query, or None after a failure, which only the initializing call reports.
    queries: Mutex<HashMap<String, Slot>>,
    /// Files over `max_bytes`, and the Formats already warned about them.
    big: SizeLimit,
}

/// D21: a file over its Definition's `max_bytes` is not parsed, with one warning per Format
/// per run. Shared by the compiled-in treesitter and the Grammar Helpers (spec §5.3).
#[derive(Default)]
pub struct SizeLimit {
    /// Formats already warned about.
    warned: Mutex<HashSet<String>>,
}

impl SizeLimit {
    /// True when the file is over `max_bytes`; the first time per Format it is an error.
    pub fn over(&self, input: &Input, params: &toml::Table) -> anyhow::Result<bool> {
        let max = params
            .get("max_bytes")
            .and_then(|v| v.as_integer())
            .unwrap_or(DEFAULT_MAX_BYTES);
        if (input.bytes.len() as i64) <= max {
            return Ok(false);
        }
        if self.warned.lock().unwrap().insert(input.format.to_string()) {
            anyhow::bail!(
                "larger than `max_bytes` ({max}); files this large are not parsed (this warning is shown once)"
            );
        }
        Ok(true)
    }
}

impl TreeSitter {
    pub(crate) fn compiled(
        &self,
        format: &str,
        params: &toml::Table,
    ) -> anyhow::Result<Option<Arc<Compiled>>> {
        let slot = self
            .queries
            .lock()
            .unwrap()
            .entry(format.to_string())
            .or_default()
            .clone();
        let mut failure = None;
        let c = slot.get_or_init(|| {
            let grammar = params.get("grammar").and_then(|v| v.as_str()).unwrap_or("");
            let built = grammars::language(grammar)
                .ok_or_else(|| anyhow::anyhow!("grammar `{grammar}` is not compiled in"))
                .and_then(|l| compile(grammar, &l, params));
            match built {
                Ok(c) => Some(Arc::new(c)),
                Err(e) => {
                    failure = Some(e);
                    None
                }
            }
        });
        if let Some(e) = failure {
            return Err(e.context(format!("Format `{format}`")));
        }
        Ok(c.clone())
    }

    /// D21: true when the file is over `max_bytes`; the first time per Format it is an error.
    fn too_big(&self, input: &Input, params: &toml::Table) -> anyhow::Result<bool> {
        self.big.over(input, params)
    }
}

impl Processor for TreeSitter {
    /// A failed query or a file over `max_bytes` is an error the first time and `Unavailable`
    /// afterwards: the file is Plain either way (D33).
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let empty = || vec![Vec::new(); anchors.len()];
        let Some(c) = self.compiled(input.format, params)? else {
            return Err(crate::helper::Unavailable.into());
        };
        if anchors.is_empty() {
            return Ok(empty());
        }
        if self.too_big(input, params)? {
            return Err(crate::helper::Unavailable.into());
        }
        let Some(tree) = parse(&c, input.bytes) else {
            return Ok(empty());
        };
        let lo = *anchors.iter().min().unwrap();
        let hi = *anchors.iter().max().unwrap();
        let o = outline(&c, &tree, input.bytes, Some(lo..hi + 1));
        if anchors.is_sorted() {
            let mut cur = TrailCursor::new(&o);
            return Ok(anchors.iter().map(|&a| cur.trail(a)).collect());
        }
        Ok(anchors.iter().map(|&a| trail_for(&o, a)).collect())
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let Some(c) = self.compiled(input.format, params)? else {
            return Err(crate::helper::Unavailable.into());
        };
        if self.too_big(input, params)? {
            return Err(crate::helper::Unavailable.into());
        }
        let Some(tree) = parse(&c, input.bytes) else {
            return Ok(Vec::new());
        };
        Ok(names_of(&outline(&c, &tree, input.bytes, None)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Input;
    use std::path::Path;

    /// The `[params]` of a built-in Definition.
    fn params(format: &str) -> toml::Table {
        let src = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("formats")
                .join(format!("{format}.toml")),
        )
        .unwrap();
        let t: toml::Table = toml::from_str(&src).unwrap();
        t["params"].as_table().unwrap().clone()
    }

    fn fixture(file: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/code")
                .join(file),
        )
        .unwrap()
    }

    /// (line, Trail names, signatures) of every `needle`.
    fn needles(format: &str, file: &str) -> Vec<(u64, String, String)> {
        needles_in(format, file, &fixture(file))
    }

    /// `needles` over `src`.
    fn needles_in(format: &str, file: &str, src: &[u8]) -> Vec<(u64, String, String)> {
        let src = src.to_vec();
        let anchors: Vec<usize> = memchr::memmem::find_iter(&src, b"needle").collect();
        let lines = crate::trail::LineIndex::new(&src);
        let input = Input {
            bytes: &src,
            path: Path::new(file),
            format,
            disk: None,
        };
        TreeSitter::default()
            .trails(&input, &params(format), &anchors)
            .unwrap()
            .iter()
            .zip(&anchors)
            .map(|(t, &a)| {
                let names: Vec<&str> = t.iter().map(|s| s.name.as_str()).collect();
                let sigs: Vec<&str> = t
                    .iter()
                    .map(|s| s.signature.as_deref().unwrap_or(""))
                    .collect();
                (lines.line_of(a), names.join(" › "), sigs.join(" ‖ "))
            })
            .collect()
    }

    fn t(line: u64, names: &str, sigs: &str) -> (u64, String, String) {
        (line, names.into(), sigs.into())
    }

    #[test]
    fn a_one_line_body_stays_out_of_the_signature() {
        // Review m2-code-r1, Fable #2: the signature ends where the body starts.
        let src = b"pub async fn top(needle: u32) -> u32 { needle }\ntrait Tr { fn d(&self) { let _ = \"needle\"; } }\n";
        assert_eq!(
            needles_in("rust", "x.rs", src),
            [
                t(1, "top", "pub async fn top(needle: u32) -> u32"),
                t(1, "top", "pub async fn top(needle: u32) -> u32"),
                t(2, "Tr › d", "trait Tr ‖ fn d(&self)"),
            ]
        );
        let py = b"def f(): return \"needle\"\n";
        assert_eq!(needles_in("python", "x.py", py), [t(1, "f", "def f():")]);
        let go = b"package p\nfunc g() int { return needle }\n";
        assert_eq!(needles_in("go", "x.go", go), [t(2, "g", "func g() int")]);
    }

    #[test]
    fn php() {
        assert_eq!(
            needles("php", "cart.php"),
            [
                t(
                    10,
                    "Cart › addItem",
                    "class Cart ‖ public function addItem(Item $item, int $qty = 1): void"
                ),
                // In the function's own name: not inside it (D18).
                t(15, "", ""),
                t(17, "needle_helper", "function needle_helper(): string"),
            ]
        );
    }

    #[test]
    fn javascript() {
        assert_eq!(
            needles("javascript", "widget.js"),
            [
                t(7, "Widget › render", "class Widget ‖ render()"),
                t(11, "Widget › handler", "class Widget ‖ handler = () =>"),
                t(17, "format", "format(x)"),
            ]
        );
    }

    #[test]
    fn object_methods_with_string_keys() {
        // Review m2-code-r1, Fable #3: a quoted key names its method as a bare one does.
        let src = b"const o = {\n  m() { return 1; },\n  n: function () { return 'needle'; },\n  \"quoted-key\": () => \"needle\",\n  'single': async () => 'needle',\n};\n";
        for (format, file) in [
            ("javascript", "x.js"),
            ("typescript", "x.ts"),
            ("tsx", "x.tsx"),
        ] {
            let got: Vec<String> = needles_in(format, file, src)
                .into_iter()
                .map(|(_, names, _)| names)
                .collect();
            assert_eq!(got, ["n", "quoted-key", "single"], "{format}");
        }
    }

    #[test]
    fn typescript_and_tsx() {
        assert_eq!(
            needles("typescript", "shape.ts"),
            [
                t(
                    6,
                    "Geometry › Base › describe",
                    "namespace Geometry ‖ abstract class Base ‖ describe(): string"
                ),
                t(16, "make", "make = (n: number): string =>"),
            ]
        );
        assert_eq!(
            needles("tsx", "view.tsx"),
            [
                t(
                    3,
                    "View › onClick",
                    "function View(props: { title: string }) ‖ onClick = () =>"
                ),
                t(5, "View", "function View(props: { title: string })"),
            ]
        );
    }

    #[test]
    fn bash() {
        assert_eq!(
            needles("bash", "deploy.sh"),
            [t(5, "greet", "greet()"), t(9, "deploy", "function deploy")]
        );
    }

    #[test]
    fn go_methods_carry_their_receiver() {
        assert_eq!(
            needles("go", "server.go"),
            [
                t(4, "Server", "Server struct"),
                t(8, "Server.Start", "func (s *Server) Start() error"),
                t(12, "helper", "func helper() string"),
            ]
        );
    }

    #[test]
    fn rust() {
        assert_eq!(
            needles("rust", "thing.rs"),
            [
                t(3, "outer › Thing", "mod outer ‖ pub struct Thing"),
                t(
                    8,
                    "outer › Thing › helper",
                    "mod outer ‖ impl Thing ‖ pub fn helper(&self) -> u32"
                ),
                t(
                    14,
                    "outer › Greeter › greet",
                    "mod outer ‖ pub trait Greeter ‖ fn greet(&self) -> String"
                ),
            ]
        );
    }

    #[test]
    fn python() {
        assert_eq!(
            needles("python", "outer.py"),
            [
                t(
                    4,
                    "Outer › method › inner",
                    "class Outer: ‖ def method(self): ‖ def inner():"
                ),
                t(8, "Outer › fetch", "class Outer: ‖ async def fetch(self):"),
                t(12, "top", "def top():"),
            ]
        );
    }

    #[test]
    fn kinds_come_from_the_definition() {
        let src = fixture("cart.php");
        let at = memchr::memmem::find(&src, b"needle: merge").unwrap();
        let input = Input {
            bytes: &src,
            path: Path::new("cart.php"),
            format: "php",
            disk: None,
        };
        let tr = &TreeSitter::default()
            .trails(&input, &params("php"), &[at])
            .unwrap()[0];
        let kinds: Vec<&str> = tr.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["class", "method"]);
        assert_eq!((tr[1].line, tr[1].end_line), (8, 12));
    }

    #[test]
    fn the_byte_range_changes_nothing() {
        // D24: the query runs only between the first and the last anchor.
        for (format, file) in [
            ("php", "cart.php"),
            ("rust", "thing.rs"),
            ("python", "outer.py"),
        ] {
            let src = fixture(file);
            let c = TreeSitter::default()
                .compiled(format, &params(format))
                .unwrap()
                .unwrap();
            let tree = parse(&c, &src).unwrap();
            let full = outline(&c, &tree, &src, None);
            for a in memchr::memmem::find_iter(&src, b"needle") {
                let part = outline(&c, &tree, &src, Some(a..a + 1));
                assert_eq!(
                    crate::trail::trail_for(&part, a),
                    crate::trail::trail_for(&full, a),
                    "{file} at {a}"
                );
            }
        }
    }

    #[test]
    fn names_for_in_headings() {
        let src = fixture("server.go");
        let input = Input {
            bytes: &src,
            path: Path::new("server.go"),
            format: "go",
            disk: None,
        };
        let names = TreeSitter::default().names(&input, &params("go")).unwrap();
        let texts: Vec<&str> = names.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["Server", "Server.Start", "helper"]);
        // "Server.Start": the receiver and the name are two pieces; the dot is not in the file.
        assert_eq!(names[1].pieces.len(), 2);
        assert_eq!(names[1].pieces[1].0, 7..12);
    }

    #[test]
    fn a_name_with_invalid_utf8_points_at_its_own_bytes() {
        // The name is decoded lossily: its pieces must still cover the name's bytes, not as
        // many bytes as the decoded text takes (fuzz, review m2-code-r1).
        for (src, format) in [
            (&b"function \xba() {\n  echo needle\n}\n"[..], "bash"),
            (b"function a\xc3() {\n  echo needle\n}\n", "bash"),
            (b"fn x\xf0\x9f\x98() {}", "rust"),
            (b"package p\nfunc (s *S\xc3) M\xc3() {}\n", "go"),
        ] {
            let input = Input {
                bytes: src,
                path: Path::new("x"),
                format,
                disk: None,
            };
            let names = TreeSitter::default()
                .names(&input, &params(format))
                .unwrap();
            assert!(!names.is_empty(), "{format}");
            for n in &names {
                for (t, s) in &n.pieces {
                    assert!(s.end <= src.len(), "{:?} past the end", n.pieces);
                    let got = String::from_utf8_lossy(&src[s.clone()]);
                    if t.len() == s.len() {
                        assert_eq!(got, &n.text[t.clone()], "{:?}", n.pieces);
                    }
                    assert!(
                        !got.contains(['(', ')', ' ']),
                        "{:?} reaches past the name: {got:?}",
                        n.pieces
                    );
                }
            }
        }
    }

    #[test]
    fn a_broken_query_fails_once_then_gives_empty_trails() {
        let mut p = params("php");
        p.insert(
            "query".into(),
            toml::Value::String("(no_such_node) @segment".into()),
        );
        let src = fixture("cart.php");
        let input = Input {
            bytes: &src,
            path: Path::new("cart.php"),
            format: "php",
            disk: None,
        };
        let ts = TreeSitter::default();
        let e = ts.trails(&input, &p, &[0]).unwrap_err();
        assert!(e.to_string().contains("php"), "{e}");
        // Afterwards the Format is unavailable: Plain, without another warning (D33).
        let again = ts.trails(&input, &p, &[0]).unwrap_err();
        assert!(
            again.downcast_ref::<crate::helper::Unavailable>().is_some(),
            "{again}"
        );
    }

    #[test]
    fn concurrent_first_calls_compile_and_fail_once() {
        // D39: one OnceLock per Definition.
        let mut p = params("php");
        p.insert(
            "query".into(),
            toml::Value::String("(no_such_node) @segment".into()),
        );
        let src = fixture("cart.php");
        let ts = TreeSitter::default();
        let errors = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    let input = Input {
                        bytes: &src,
                        path: Path::new("cart.php"),
                        format: "php",
                        disk: None,
                    };
                    if let Err(e) = ts.trails(&input, &p, &[0])
                        && e.downcast_ref::<crate::helper::Unavailable>().is_none()
                    {
                        errors.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(errors.into_inner(), 1);
    }

    #[test]
    fn each_run_compiles_its_own_params() {
        // A second run (M5's MCP server runs many in one process) sees the new query.
        let src = fixture("cart.php");
        let at = memchr::memmem::find(&src, b"needle: merge").unwrap();
        let input = Input {
            bytes: &src,
            path: Path::new("cart.php"),
            format: "php",
            disk: None,
        };
        let full = TreeSitter::default()
            .trails(&input, &params("php"), &[at])
            .unwrap();
        let mut only_classes = params("php");
        only_classes.insert(
            "query".into(),
            toml::Value::String("(class_declaration name: (name) @name) @segment".into()),
        );
        let classes = TreeSitter::default()
            .trails(&input, &only_classes, &[at])
            .unwrap();
        assert_eq!(full[0].len(), 2);
        assert_eq!(classes[0].len(), 1);
    }

    #[test]
    fn files_over_max_bytes_are_not_parsed() {
        let mut p = params("php");
        p.insert("max_bytes".into(), toml::Value::Integer(10));
        let src = fixture("cart.php");
        let input = Input {
            bytes: &src,
            path: Path::new("cart.php"),
            format: "php",
            disk: None,
        };
        let ts = TreeSitter::default();
        assert!(
            ts.trails(&input, &p, &[0])
                .unwrap_err()
                .to_string()
                .contains("max_bytes")
        );
        let again = ts.trails(&input, &p, &[0]).unwrap_err();
        assert!(again.downcast_ref::<crate::helper::Unavailable>().is_some());
    }

    #[test]
    fn every_compiled_in_grammar_loads() {
        for g in grammars::COMPILED_IN {
            let l = grammars::language(g).unwrap_or_else(|| panic!("{g}"));
            // tree-sitter 0.24 loads ABI 13–14 (D1).
            assert_eq!(l.version(), 14, "{g}");
        }
        assert!(grammars::language("lua").is_none());
    }

    /// D24's assumption, measured: the share of the query in parse + query, over the Rust
    /// files of the cargo registry that match `impl Drop for`, with and without the byte range
    /// of the real anchors. Run with `cargo test --release -- --ignored --nocapture query_share`.
    #[test]
    #[ignore]
    fn query_share_on_the_cargo_registry() {
        use std::time::{Duration, Instant};
        let home = std::env::var("HOME").unwrap();
        let roots: Vec<_> = std::fs::read_dir(Path::new(&home).join(".cargo/registry/src"))
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("index.crates.io-"))
            })
            .collect();
        let out = std::process::Command::new("rg")
            .args(["-l", "-t", "rust", "impl Drop for"])
            .args(&roots)
            .output()
            .unwrap();
        let files: Vec<String> = String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        let c = TreeSitter::default()
            .compiled("rust", &params("rust"))
            .unwrap()
            .unwrap();
        let (mut bytes, mut t_parse, mut t_full, mut t_range) =
            (0, Duration::ZERO, Duration::ZERO, Duration::ZERO);
        let mut segments = (0, 0);
        for f in &files {
            let src = std::fs::read(f).unwrap();
            let anchors: Vec<usize> = memchr::memmem::find_iter(&src, b"impl Drop for").collect();
            let (lo, hi) = (anchors[0], *anchors.last().unwrap());
            bytes += src.len();
            let t = Instant::now();
            let tree = parse(&c, &src).unwrap();
            t_parse += t.elapsed();
            let t = Instant::now();
            segments.0 += outline(&c, &tree, &src, None).segments.len();
            t_full += t.elapsed();
            let t = Instant::now();
            segments.1 += outline(&c, &tree, &src, Some(lo..hi + 1)).segments.len();
            t_range += t.elapsed();
        }
        println!(
            "{} files, {:.1} MB: parse {:.2} s; query over the whole file {:.2} s ({} Segments); \
             query over first..last anchor {:.2} s ({} Segments)",
            files.len(),
            bytes as f64 / 1e6,
            t_parse.as_secs_f64(),
            t_full.as_secs_f64(),
            segments.0,
            t_range.as_secs_f64(),
            segments.1
        );
    }
}
