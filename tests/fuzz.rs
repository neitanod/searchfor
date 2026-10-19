//! A small fuzz of the keypath, treesitter, markup, csv and subtitles Processors (review
//! m2-code-r1; M3 plan, Task 6.2): generated inputs
//! with lone CRs, CRLF, BOMs, multi-byte keys and values, and truncated files, looking for
//! panics and for name pieces outside their text, outside their Source, or over other bytes.
//! The generator is seeded, so a failure reproduces; `SEARCHFOR_FUZZ_SEED` and `SEARCHFOR_FUZZ_N` (inputs
//! per Format) change the seed and the count.
mod common;

use searchfor::model::{Input, Name};
use searchfor::processors::Processor;
use searchfor::processors::csv::Csv;
use searchfor::processors::keypath::Keypath;
use searchfor::processors::markup::Markup;
use searchfor::processors::subtitles::Subtitles;
use searchfor::processors::treesitter::TreeSitter;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

/// xorshift64*: enough for a reproducible generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn pick<'a>(&mut self, pool: &[&'a str]) -> &'a str {
        pool[self.below(pool.len())]
    }

    /// A line break: LF half of the time, else a lone CR or a CRLF.
    fn br(&mut self) -> &'static str {
        self.pick(&["\n", "\n", "\r", "\r\n"])
    }
}

/// Line breaks and the other bytes every generator mixes in.
#[rustfmt::skip]
const COMMON: &[&str] = &[
    "\n", "\n", "\r", "\r\n", "\r\r", "\u{feff}", " ", "  ", "\t", "needle", "ñandú", "é",
    "日本", "😀", "\u{2028}", "\u{85}", "\\", "\"", "'", "#", "\0",
];

#[rustfmt::skip]
const YAML: &[&str] = &[
    "a", "key", "ñ", "é: ", r#""é x""#, "'ü'''", ": ", ":", "- ", "-", "? ", "[", "]", "{", "}",
    ", ", "|", "|-\n  ", ">", "&x ", "*x", "---\n", "...\n", "!!str ", "%YAML 1.2\n",
    r#""\u00e9""#, "\"a\\\n  b\"", "# ñ comment", "? |\n  ", "? >-\n  ", "|+\n",
];

#[rustfmt::skip]
const JSON: &[&str] = &[
    "{", "}", "[", "]", ":", ",", r#""a""#, r#""é""#, r#""k\u00e9""#, r#""\ud83d\ude00""#, "1",
    "-0.5e+3", "true", "null", r#""needle""#, "// ñ\n", "/* é */", r#""\n""#, r#""\"""#,
];

#[rustfmt::skip]
const TOML: &[&str] = &[
    "[", "]", "[[", "]]", "[[a]]", "[a.b]", r#"["é"]"#, "a", "ñ", "=", " = ", r#""v""#, "'é'",
    r#"""""#, "'''", "1", "[1, 2]", "{ x = 1 }", ".", r#"a."b".c"#, "# é\n", "true",
    "1979-05-27", r#""\u00e9""#, r#""needle""#,
];

#[rustfmt::skip]
const CODE: &[&str] = &[
    "fn ", "struct ", "impl ", "mod ", "trait ", "enum ", "def ", "class ", "function ", "func ",
    "type ", "interface ", "namespace ", "<?php ", "$x", "ñandú", "Café", "日本", "(", ")", "{",
    "}", ":", ";", "=>", "->", "=", ".", "::", "<", ">", "//", "/*", "*/", r#""s""#, "'c'", "`t`",
    "<div>", "</div>", "export ", "async ", "self", "return ", "if ", "then\n", "fi\n", "() {",
    r#""a\u0062""#, r#"'\x41'"#, "\\\n", r#""\uD83D""#, r#"{ "k\u{31}": () => needle }"#,
];

#[rustfmt::skip]
const MARKUP: &[&str] = &[
    "<", ">", "</", "/>", "<div>", "</div>", "<li>", "<ul>", "</ul>", "<p class=\"a b\">", "<td>",
    "<tr>", "<table>", "<br>", "<script>", "</script>", "<style>", "<!--", "-->", "<![CDATA[",
    "]]>", "<?xml version=\"1.0\"?>", "<!DOCTYPE x [", "]>", "<x:y>", "</x:y>", "<a id='ñ'>",
    "</a>", " id=", "\"", "'", "=", "<é>", "</é>", "<b/>", "&lt;", "<html>", "<body>",
];

#[rustfmt::skip]
const TABLE: &[&str] = &[
    ",", ";", "\t", "|", "\"", "\"\"", "\n", "\r\n", "a", "ñandú", "1", "needle", "\"x,y\"",
    "\"a\nb\"", " ", "\u{feff}",
];

#[rustfmt::skip]
const SUBS: &[&str] = &[
    "1", "12", "00:00:01,000", "00:01.500", " --> ", "-->", "00:00:02,000", "WEBVTT", "NOTE ",
    "\n", "\r\n", "\n\n", "<i>", "</i>", "needle", "ñ", " align:start", ":", ",", ".",
];

/// Key spellings, plain and multi-byte; the generators quote and escape some.
#[rustfmt::skip]
const KEYS: &[&str] = &["a", "key", "ñandú", "é", "日本", "😀k", "k_1", "Café"];

/// Identifiers for the code templates.
#[rustfmt::skip]
const IDS: &[&str] = &["a", "ñandú", "Café", "日本", "run", "x😀", "Ünï"];

/// Values: ASCII, multi-byte, and `needle` for the anchors.
#[rustfmt::skip]
const VALUES: &[&str] = &["needle", "é needle", "1", "ñandú", "日本 needle", "x"];

fn yaml_key(rng: &mut Rng) -> String {
    let k = rng.pick(KEYS);
    match rng.below(5) {
        0 => format!("'{k}'"),
        1 => format!("\"{k} x\""),
        2 => format!("\"\\u00e9{k}\""),
        _ => k.to_string(),
    }
}

fn yaml_value(rng: &mut Rng, depth: usize, indent: usize) -> String {
    let pad = " ".repeat(indent + 2);
    match rng.below(if depth > 3 { 3 } else { 6 }) {
        0 => format!(" {}", rng.pick(VALUES)),
        1 => format!(" \"{}\"", rng.pick(VALUES)),
        2 => format!(" [{}, {}]", rng.pick(VALUES), rng.pick(KEYS)),
        3 => {
            let br = rng.br();
            format!("{br}{}", yaml_map(rng, depth + 1, indent + 2))
        }
        4 => {
            let mut s = String::new();
            for _ in 0..1 + rng.below(3) {
                let br = rng.br();
                s.push_str(&format!("{br}{pad}- {}", rng.pick(VALUES)));
            }
            s
        }
        _ => {
            let (br, br2) = (rng.br(), rng.br());
            let (a, b) = (rng.pick(VALUES), rng.pick(VALUES));
            format!(" |{br}{pad}{a}{br2}{pad}{b}")
        }
    }
}

/// A block mapping at `indent`, its keys on lines broken by LF, CR or CRLF.
fn yaml_map(rng: &mut Rng, depth: usize, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let mut s = String::new();
    for i in 0..1 + rng.below(4) {
        if i > 0 {
            s.push_str(rng.br());
        }
        // A block scalar key (`? |`) now and then (review m2-code-r2, Fable #1).
        if rng.below(6) == 0 {
            let (b1, b2, b3) = (rng.br(), rng.br(), rng.br());
            let style = rng.pick(&["|", ">", "|-", ">+", "|2"]);
            let (k, k2) = (rng.pick(KEYS), rng.pick(VALUES));
            let v = yaml_value(rng, depth, indent);
            s.push_str(&format!(
                "{pad}? {style}{b1}{pad}  {k}{b2}{pad}  {k2}{b3}{pad}:{v}"
            ));
            continue;
        }
        let k = yaml_key(rng);
        let v = yaml_value(rng, depth, indent);
        s.push_str(&format!("{pad}{k}:{v}"));
    }
    s
}

fn json_key(rng: &mut Rng) -> String {
    let k = rng.pick(KEYS);
    match rng.below(3) {
        0 => format!("\"\\u00e9{k}\""),
        _ => format!("\"{k}\""),
    }
}

/// Blanks between JSON tokens: a space or a line break.
fn json_ws(rng: &mut Rng) -> &'static str {
    if rng.below(2) == 0 { rng.br() } else { " " }
}

fn json_value(rng: &mut Rng, depth: usize) -> String {
    match rng.below(if depth > 3 { 2 } else { 4 }) {
        0 => format!("\"{}\"", rng.pick(VALUES)),
        1 => "1".to_string(),
        2 => {
            let mut parts = Vec::new();
            for _ in 0..1 + rng.below(4) {
                let (w, k, v) = (json_ws(rng), json_key(rng), json_value(rng, depth + 1));
                parts.push(format!("{w}{k}: {v}"));
            }
            let w = json_ws(rng);
            format!("{{{}{w}}}", parts.join(","))
        }
        _ => {
            let mut parts = Vec::new();
            for _ in 0..1 + rng.below(3) {
                let (w, v) = (json_ws(rng), json_value(rng, depth + 1));
                parts.push(format!("{w}{v}"));
            }
            format!("[{}]", parts.join(","))
        }
    }
}

fn toml_key(rng: &mut Rng) -> String {
    let k = rng.pick(KEYS);
    match rng.below(5) {
        0 => format!("\"{k} x\""),
        1 => format!("'{k}'"),
        2 => format!("{k}.\"{}\"", rng.pick(KEYS)),
        _ => k.to_string(),
    }
}

/// Headers, arrays of tables and key-values, on lines broken by LF, CRLF or a lone CR (invalid
/// in TOML, so the recovery runs).
fn toml_doc(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..1 + rng.below(8) {
        let line = match rng.below(5) {
            0 => format!("[{}]", toml_key(rng)),
            1 => format!("[[{}]]", toml_key(rng)),
            2 => {
                let (k, k2, v) = (toml_key(rng), toml_key(rng), rng.pick(VALUES));
                format!("{k} = {{ {k2} = \"{v}\" }}")
            }
            3 => format!("{} = [\"{}\", 1]", toml_key(rng), rng.pick(VALUES)),
            _ => format!("{} = \"{}\"", toml_key(rng), rng.pick(VALUES)),
        };
        s.push_str(&line);
        s.push_str(rng.br());
    }
    s
}

/// Functions, classes and methods of the compiled-in languages, with multi-byte names.
fn code_doc(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..1 + rng.below(5) {
        let (id, id2, br) = (rng.pick(IDS), rng.pick(IDS), rng.br());
        let t = match rng.below(10) {
            0 => format!("fn {id}() {{{br}    let x = \"needle\";{br}}}"),
            1 => format!("impl {id} {{{br}    fn {id2}(&self) {{ needle(); }}{br}}}"),
            2 => format!("struct {id} {{ é: u8 }}"),
            3 => format!("def {id}():{br}    return \"needle\""),
            4 => format!("class {id}:{br}    def {id2}(self):{br}        needle"),
            5 => format!("function {id}() {{{br}  return 'needle';{br}}}"),
            6 => format!("func ({id} T) {id2}() {{ needle() }}"),
            7 => format!("<?php class {id} {{ function {id2}() {{ needle(); }} }}"),
            // Quoted keys with escapes (review m2-code-r2, Codex #1).
            8 => format!(
                "const o = {{ \"{id}\\u0062\": () => needle, '': function () {{{br}return 'needle'; }}, \"\\uD83D\\uDE00\\{br}{id2}\": async () => {{ needle }} }};"
            ),
            _ => format!("{id}() {{{br}  echo needle{br}}}"),
        };
        s.push_str(&t);
        s.push_str(rng.br());
    }
    s
}

/// Nested elements with attributes, text and the constructs the scanner skips.
fn markup_doc(rng: &mut Rng, depth: usize) -> String {
    let mut s = String::new();
    for _ in 0..1 + rng.below(4) {
        let tag = rng.pick(&["div", "li", "p", "td", "tr", "x:y", "é", "b", "script"]);
        let attrs = match rng.below(4) {
            0 => format!(" id=\"{}\"", rng.pick(KEYS)),
            1 => format!(" class='{} {}'", rng.pick(KEYS), rng.pick(KEYS)),
            2 => " a=b".to_string(),
            _ => String::new(),
        };
        let inner = if depth < 4 && rng.below(2) == 0 {
            markup_doc(rng, depth + 1)
        } else {
            rng.pick(VALUES).to_string()
        };
        let close = if rng.below(4) == 0 {
            String::new()
        } else {
            format!("</{tag}>")
        };
        s.push_str(&format!("<{tag}{attrs}>{inner}{close}{}", rng.br()));
        if rng.below(5) == 0 {
            s.push_str(rng.pick(&["<!-- c -->", "<![CDATA[ < ]]>", "<br>", "<?p?>"]));
        }
    }
    s
}

/// A header and rows, quoted fields with delimiters, `""` and line breaks.
fn table_doc(rng: &mut Rng) -> String {
    let d = rng.pick(&[",", ";", "\t"]);
    let mut s = String::new();
    for _ in 0..1 + rng.below(6) {
        let cells: Vec<String> = (0..1 + rng.below(4))
            .map(|_| match rng.below(4) {
                0 => format!("\"{}{d}\"\"{}\"", rng.pick(VALUES), rng.pick(KEYS)),
                1 => format!("\"{}{}x\"", rng.pick(VALUES), rng.br()),
                _ => rng.pick(VALUES).to_string(),
            })
            .collect();
        s.push_str(&cells.join(d));
        s.push_str(rng.br());
    }
    s
}

/// Cues with and without identifiers, settings and blank lines, SubRip or WebVTT timing.
fn subs_doc(rng: &mut Rng) -> String {
    let mut s = if rng.below(2) == 0 {
        String::from("WEBVTT\n\n")
    } else {
        String::new()
    };
    for i in 0..1 + rng.below(5) {
        let br = rng.br();
        if rng.below(2) == 0 {
            s.push_str(&format!("{}{br}", i + 1));
        }
        let ts = rng.pick(&[
            "00:00:01,000 --> 00:00:02,000",
            "00:01.000 --> 00:02.000 align:end",
            "1:2:3.4 --> 5:6:7.8",
        ]);
        s.push_str(&format!("{ts}{br}{}{br}", rng.pick(VALUES)));
        if rng.below(3) > 0 {
            s.push_str(br);
        }
    }
    s
}

/// The structured generator of a target.
#[derive(Clone, Copy)]
enum Shape {
    Yaml,
    Json,
    Toml,
    Code,
    Markup,
    Table,
    Subs,
}

fn structured(rng: &mut Rng, shape: Shape) -> Vec<u8> {
    match shape {
        Shape::Yaml => yaml_map(rng, 0, 0),
        Shape::Json => json_value(rng, 0),
        Shape::Toml => toml_doc(rng),
        Shape::Code => code_doc(rng),
        Shape::Markup => markup_doc(rng, 0),
        Shape::Table => table_doc(rng),
        Shape::Subs => subs_doc(rng),
    }
    .into_bytes()
}

/// One generated input: a structured document, a fixture, or tokens of `pool` and `COMMON`;
/// then mutated.
fn generate(rng: &mut Rng, t: &Target) -> Vec<u8> {
    let from = rng.below(8);
    let mut out: Vec<u8> = if from < 5 {
        structured(rng, t.shape)
    } else if from == 5 && !t.seeds.is_empty() {
        t.seeds[rng.below(t.seeds.len())].clone()
    } else {
        let mut v = Vec::new();
        for _ in 0..1 + rng.below(60) {
            let tok = if rng.below(3) == 0 {
                rng.pick(COMMON)
            } else {
                rng.pick(t.pool)
            };
            v.extend_from_slice(tok.as_bytes());
        }
        v
    };
    for _ in 0..rng.below(4) {
        match rng.below(6) {
            // Truncated, possibly inside a character.
            0 => {
                let at = rng.below(out.len() + 1);
                out.truncate(at);
            }
            // A line break becomes a lone CR or a CRLF.
            1 | 2 => {
                let lfs: Vec<usize> = (0..out.len()).filter(|&i| out[i] == b'\n').collect();
                if !lfs.is_empty() {
                    let i = lfs[rng.below(lfs.len())];
                    if rng.below(2) == 0 {
                        out[i] = b'\r';
                    } else {
                        out.insert(i, b'\r');
                    }
                }
            }
            // A BOM in front.
            3 => {
                let mut v = "\u{feff}".as_bytes().to_vec();
                v.extend_from_slice(&out);
                out = v;
            }
            // A token anywhere, possibly inside a character.
            _ => {
                let tok = if rng.below(2) == 0 {
                    rng.pick(COMMON)
                } else {
                    rng.pick(t.pool)
                };
                let at = rng.below(out.len() + 1);
                out.splice(at..at, tok.bytes());
            }
        }
    }
    out
}

/// The `[params]` of a built-in Definition.
fn params(format: &str) -> toml::Table {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("formats")
            .join(format!("{format}.toml")),
    )
    .unwrap();
    let t: toml::Table = toml::from_str(&src).unwrap();
    // srt and vtt have no [params].
    t.get("params")
        .and_then(|p| p.as_table())
        .cloned()
        .unwrap_or_default()
}

fn fixtures(dir: &str, ext: &[&str]) -> Vec<Vec<u8>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(dir);
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| ext.contains(&x))
        {
            out.push(std::fs::read(p).unwrap());
        }
    }
    out
}

/// A piece is inside its name's text, on character boundaries, and inside the Source; a
/// verbatim piece (as long as its Source) is the Source's bytes, decoded lossily.
fn check_names(names: &[Name], src: &[u8]) -> Result<(), String> {
    for n in names {
        for (t, s) in &n.pieces {
            if t.start > t.end
                || t.end > n.text.len()
                || !n.text.is_char_boundary(t.start)
                || !n.text.is_char_boundary(t.end)
            {
                return Err(format!("text piece {t:?} of {:?}", n.text));
            }
            if s.start > s.end || s.end > src.len() {
                return Err(format!(
                    "source piece {s:?} of {:?} over {} bytes",
                    n.text,
                    src.len()
                ));
            }
            // Invalid UTF-8 shows as U+FFFD: a three-byte run of it maps byte for byte.
            if t.len() == s.len() && n.text[t.clone()] != String::from_utf8_lossy(&src[s.clone()]) {
                return Err(format!(
                    "verbatim piece {t:?} of {:?} over \"{}\"",
                    n.text,
                    src[s.clone()].escape_ascii()
                ));
            }
        }
    }
    Ok(())
}

/// Runs the Processor on `bytes`: Trails at every `needle`, at random offsets (out of order)
/// and at the end, then the names. Err describes a panic or a broken piece.
fn run(t: &Target, p: &dyn Processor, bytes: &[u8], rng: &mut Rng) -> Result<(), String> {
    let mut anchors: Vec<usize> = memchr::memmem::find_iter(bytes, b"needle").collect();
    for _ in 0..3 {
        anchors.push(rng.below(bytes.len() + 1));
    }
    anchors.push(bytes.len());
    let mut sorted = anchors.clone();
    sorted.sort();
    let input = Input {
        bytes,
        path: Path::new("fuzz"),
        format: t.format,
        disk: None,
    };
    let got = catch_unwind(AssertUnwindSafe(|| {
        let t1 = p.trails(&input, &t.params, &anchors).ok();
        let t2 = p.trails(&input, &t.params, &sorted).ok();
        let names = p.names(&input, &t.params).ok();
        (t1, t2, names)
    }));
    let (t1, t2, names) = got.map_err(|e| {
        let msg = e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        format!("panic: {msg}")
    })?;
    for tr in [t1, t2].into_iter().flatten() {
        if tr.len() != anchors.len() {
            return Err(format!("{} Trails for {} anchors", tr.len(), anchors.len()));
        }
    }
    check_names(&names.unwrap_or_default(), bytes)
}

fn count() -> usize {
    std::env::var("SEARCHFOR_FUZZ_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000)
}

fn seed() -> u64 {
    std::env::var("SEARCHFOR_FUZZ_SEED")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0x5eed_f0f0_1234_5678)
}

/// A Format fuzzed: its Definition's name, a file extension, its Processor and generators.
struct Target {
    format: &'static str,
    ext: &'static str,
    proc: Proc,
    shape: Shape,
    pool: &'static [&'static str],
    seeds: Vec<Vec<u8>>,
    params: toml::Table,
}

/// The Processor a target runs.
#[derive(Clone, Copy)]
enum Proc {
    Keypath,
    TreeSitter,
    Markup,
    Csv,
    Subtitles,
}

/// (format, extension, Processor, shape, token pool, fixture directory, fixture extensions).
type Row = (
    &'static str,
    &'static str,
    Proc,
    Shape,
    &'static [&'static str],
    &'static str,
    &'static [&'static str],
);

fn targets() -> Vec<Target> {
    #[rustfmt::skip]
    let list: [Row; 17] = [
        ("yaml", "yaml", Proc::Keypath, Shape::Yaml, YAML, "data", &["yaml"]),
        ("json", "json", Proc::Keypath, Shape::Json, JSON, "data", &["json", "jsonl"]),
        ("toml", "toml", Proc::Keypath, Shape::Toml, TOML, "data", &["toml"]),
        ("rust", "rs", Proc::TreeSitter, Shape::Code, CODE, "code", &["rs"]),
        ("python", "py", Proc::TreeSitter, Shape::Code, CODE, "code", &["py"]),
        ("javascript", "js", Proc::TreeSitter, Shape::Code, CODE, "code", &["js"]),
        ("typescript", "ts", Proc::TreeSitter, Shape::Code, CODE, "code", &["ts"]),
        ("tsx", "tsx", Proc::TreeSitter, Shape::Code, CODE, "code", &["tsx"]),
        ("go", "go", Proc::TreeSitter, Shape::Code, CODE, "code", &["go"]),
        ("php", "php", Proc::TreeSitter, Shape::Code, CODE, "code", &["php"]),
        ("bash", "sh", Proc::TreeSitter, Shape::Code, CODE, "code", &["sh"]),
        ("html", "html", Proc::Markup, Shape::Markup, MARKUP, "markup", &["html"]),
        ("xml", "xml", Proc::Markup, Shape::Markup, MARKUP, "markup", &["xml"]),
        ("csv", "csv", Proc::Csv, Shape::Table, TABLE, "table", &["csv"]),
        ("tsv", "tsv", Proc::Csv, Shape::Table, TABLE, "table", &["tsv"]),
        ("srt", "srt", Proc::Subtitles, Shape::Subs, SUBS, "subs", &["srt"]),
        ("vtt", "vtt", Proc::Subtitles, Shape::Subs, SUBS, "subs", &["vtt"]),
    ];
    list.into_iter()
        .map(|(format, ext, proc, shape, pool, dir, exts)| Target {
            format,
            ext,
            proc,
            shape,
            pool,
            seeds: fixtures(dir, exts),
            params: params(format),
        })
        .collect()
}

/// `n` generated inputs per target, in the order of `targets`.
fn inputs(targets: &[Target], n: usize) -> Vec<Vec<Vec<u8>>> {
    let mut rng = Rng(seed());
    targets
        .iter()
        .map(|t| (0..n).map(|_| generate(&mut rng, t)).collect())
        .collect()
}

#[test]
fn generated_inputs_do_not_panic_the_processors() {
    let targets = targets();
    let mut rng = Rng(seed() ^ 1);
    let mut failures = Vec::new();
    let mut tried = 0;
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    for (t, set) in targets.iter().zip(inputs(&targets, count())) {
        let ts = TreeSitter::default();
        let p: &dyn Processor = match t.proc {
            Proc::Keypath => &Keypath,
            Proc::TreeSitter => &ts,
            Proc::Markup => &Markup,
            Proc::Csv => &Csv,
            Proc::Subtitles => &Subtitles,
        };
        for bytes in &set {
            tried += 1;
            if let Err(e) = run(t, p, bytes, &mut rng) {
                failures.push(format!(
                    "{}: {e}\n  input: \"{}\"",
                    t.format,
                    bytes.escape_ascii()
                ));
            }
        }
    }
    std::panic::set_hook(hook);
    eprintln!("fuzz: {tried} inputs, {} failures", failures.len());
    assert!(
        failures.is_empty(),
        "{} of {tried} inputs failed; the first ones:\n{}",
        failures.len(),
        failures[..failures.len().min(6)].join("\n")
    );
}

/// A tenth of the same inputs through the binary: the output that maps the Trails and the
/// names back to the Source must not panic either.
#[test]
fn generated_files_do_not_panic_sfor() {
    let targets = targets();
    let fx = common::Fixture::new();
    let dir = fx.path().join("fuzz");
    std::fs::create_dir(&dir).unwrap();
    let mut files = 0;
    for (t, set) in targets.iter().zip(inputs(&targets, count() / 10)) {
        for (k, bytes) in set.iter().enumerate() {
            std::fs::write(dir.join(format!("{k}.{}", t.ext)), bytes).unwrap();
            files += 1;
        }
    }
    for args in [
        &["-e", "needle|é|a"][..],
        &["--json", "-e", "needle|é|a"],
        &["--flat", "-e", "needle|é|a"],
        &["--in-headings", "-e", "é|a|^|$"],
        &["--in-headings", "--json", "-e", "é|a"],
        &["--expand", "-e", "needle"],
    ] {
        let out = fx
            .s4(args.iter().copied().chain(["fuzz"]))
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.code() != Some(101) && !err.contains("panicked"),
            "s4 {args:?} over {files} files: {:?}\n{err}",
            out.status
        );
    }
    eprintln!("fuzz: {files} files through s4");
}

// M4 (plan, Task 12.1): the Extractors and the `object` Processor over mutated files. Each seed
// is a file the fixture writers make; a mutation flips, deletes, repeats or inserts bytes,
// cuts the file, or, inside a zip, does the same to the XML of one part and zips it again.
// Nothing may panic, take more than 2 s, or give an Outline outside its text.

use searchfor::docs::{self, Kind, fixture as doc};
use searchfor::processors::object::{self, fixture as obj};
use std::time::Duration;

/// Bytes the XML mutations insert.
#[rustfmt::skip]
const XML: &[&str] = &[
    "<", ">", "</", "/>", "<w:p>", "</w:p>", "<w:t>", "</w:t>", "<w:tbl>", "<w:tr>", "<w:tc>",
    "</w:tc>", "<a:p>", "<text:p>", "<text:h text:outline-level=\"9\">", "<table:table-cell table:number-columns-repeated=\"99999999\"/>",
    "<row r=\"999999999\">", "<c r=\"XFD1048576\" t=\"s\"><v>99999</v></c>", "&amp;", "&#x0;", "&#xFFFFFF;", "&bogus;",
    "<![CDATA[", "]]>", "<!--", "-->", "\"", "'", "=", "needle", "ñandú", "\0", "\u{feff}",
    "<h1>", "</h1>", "<br/>", "<script>", "<nav epub:type=\"toc\">", "<a href=\"../../../x\">",
    // Namespace declarations (Codex M4 code r2 #1).
    " xmlns=\"\"", " xmlns:text=\"urn:x\"", " xmlns:o=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\"",
    "<o:text>", "</o:text>", " xmlns", "xmlns:=\"\"",
    // Text boxes, cell comments, spaces and breaks (M4 code r3).
    "<w:txbxContent>", "</w:txbxContent>", "<office:annotation>", "</office:annotation>",
    "<text:s text:c=\"99999\"/>", "<text:tab/>", "<text:line-break/>", "<w:ptab/>", "<pre>", "</pre>",
];

fn mutate(rng: &mut Rng, mut b: Vec<u8>, pool: &[&str]) -> Vec<u8> {
    for _ in 0..1 + rng.below(4) {
        let at = rng.below(b.len() + 1);
        match rng.below(6) {
            0 if !b.is_empty() => {
                let i = at.min(b.len() - 1);
                b[i] ^= 1 << rng.below(8);
            }
            1 => {
                let end = (at + rng.below(64)).min(b.len());
                b.drain(at..end);
            }
            2 => {
                let end = (at + rng.below(256)).min(b.len());
                let copy = b[at..end].to_vec();
                b.splice(at..at, copy);
            }
            3 => b.truncate(at),
            _ => {
                let s = rng.pick(pool).as_bytes().to_vec();
                b.splice(at..at, s);
            }
        }
    }
    b
}

/// The seeds of each document kind.
fn document_seeds() -> Vec<(Kind, Vec<u8>)> {
    let blocks = [
        doc::h(1, "1 Intro"),
        doc::p("needle é"),
        doc::h(2, "1.1 ñandú"),
        doc::table(&[&["A", "B"], &["needle", "日本"], &["x"]]),
        doc::p("after"),
    ];
    let sheet: &[&[&str]] = &[&["Item", "Price"], &["needle", "1.5"], &["", "", "x"]];
    let slides: &[(&[&str], Option<&str>)] =
        &[(&["T", "needle"], Some("notes needle")), (&["2"], None)];
    let sqlite = |n: &str| std::fs::read(common::source_dir().join("docs").join(n)).unwrap();
    // A Word document whose body is UTF-16, as its BOM says (Codex r1 #5).
    let utf16 = doc::rezip(&doc::docx(&blocks), |name, data| {
        if name != "word/document.xml" {
            return data;
        }
        let x = String::from_utf8(data).unwrap();
        let mut out = vec![0xff, 0xfe];
        out.extend(x.encode_utf16().flat_map(u16::to_le_bytes));
        out
    });
    // An OpenDocument text whose namespaces are bound to other prefixes (Codex M4 code r2 #1).
    let renamed = doc::rezip(&doc::odt(&blocks), |name, data| {
        if name != "content.xml" {
            return data;
        }
        String::from_utf8(data)
            .unwrap()
            .replace("office:", "o:")
            .replace("xmlns:office=", "xmlns:o=")
            .replace("text:", "t:")
            .replace("xmlns:text=", "xmlns:t=")
            .into_bytes()
    });
    vec![
        (Kind::Docx, doc::docx(&blocks)),
        (Kind::Docx, utf16),
        (Kind::Odt, doc::odt(&blocks)),
        (Kind::Odt, renamed),
        (Kind::Xlsx, doc::xlsx(&[("S", sheet), ("T", sheet)])),
        (Kind::Ods, doc::ods(&[("S", sheet)])),
        (Kind::Pptx, doc::pptx(slides)),
        (Kind::Odp, doc::odp(slides)),
        (
            Kind::Epub,
            doc::epub(&[("1", "<h1>H</h1><p>needle</p>"), ("2", "<p>x</p>")], true),
        ),
        (Kind::Epub, doc::epub(&[("1", "<p>needle</p>")], false)),
        (
            Kind::Pdf,
            doc::pdf(
                &[&["1 Intro", "needle"], &["2 Costs", "é needle"]],
                &[(1, "1 Intro", 1), (2, "2 Costs", 2)],
            ),
        ),
        (Kind::Sqlite, sqlite("shop.sqlite")),
        (Kind::Sqlite, sqlite("many.sqlite")),
        (Kind::Sqlite, sqlite("constraints.sqlite")),
    ]
}

/// The checks of one extraction: no panic, ended within 2 s whatever it returns, and an
/// Outline inside its text. A case that never ends is the supervisor's (`supervised`).
fn check_doc(kind: Kind, bytes: &[u8]) -> Result<(), String> {
    let started = std::time::Instant::now();
    let got = catch_unwind(AssertUnwindSafe(|| docs::extract(kind, bytes)));
    let took = started.elapsed();
    if took > DEADLINE {
        return Err(format!("took {took:?}"));
    }
    let e = match got {
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            return Err(format!("panic: {msg}"));
        }
        Ok(Err(_)) => return Ok(()),
        Ok(Ok(e)) => e,
    };
    if std::str::from_utf8(&e.text).is_err() || e.text.contains(&0) {
        return Err("text is not UTF-8 without NUL".into());
    }
    for s in &e.outline.segments {
        if s.range.start > s.range.end || s.range.end > e.text.len() {
            return Err(format!("Segment {:?} over {} bytes", s.range, e.text.len()));
        }
    }
    check_names(&searchfor::trail::names_of(&e.outline), &e.text)
}

const DEADLINE: Duration = Duration::from_secs(2);
/// In a child process of `supervised`: the first case it runs.
const CHILD_FROM: &str = "SEARCHFOR_FUZZ_CHILD_FROM";
/// How long `supervised` waits for a child that runs no case.
const BETWEEN: Duration = Duration::from_secs(60);

/// Runs the cases of the test `test` in a child process, this test binary running only that
/// test with `CHILD_FROM` set, and watches it (code review of the M4 plan r2, Codex #7). The
/// child prints `fuzzcase run N` before case N, `fuzzcase end N` after it, followed by its
/// failure if it failed, and `fuzzcase done` at the end. A case that prints nothing for
/// `limit` has its child killed and reaped, and the cases after it run in a new child; one
/// whose child dies (an abort, out of memory) fails too. Between cases, while the child starts
/// or makes the next case, the limit is `BETWEEN`: making a seed's cases takes 0.5 s alone and
/// passed 3 s with the suite running beside it on a loaded machine. Returns the failures.
fn supervised(test: &str, limit: Duration) -> Vec<String> {
    use std::io::BufRead;
    let mut failures = Vec::new();
    let mut from = 0usize;
    'children: loop {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture", "--test-threads", "1"])
            .env(CHILD_FROM, from.to_string())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let out = std::io::BufReader::new(child.stdout.take().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in out.lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut running: Option<usize> = None;
        loop {
            let wait = if running.is_some() { limit } else { BETWEEN };
            match rx.recv_timeout(wait) {
                Ok(line) => {
                    let Some(msg) = line.strip_prefix("fuzzcase ") else {
                        continue;
                    };
                    if msg == "done" {
                        let _ = child.wait();
                        return failures;
                    }
                    let (what, rest) = msg.split_once(' ').unwrap_or((msg, ""));
                    let (n, failure) = rest.split_once(' ').unwrap_or((rest, ""));
                    let n: usize = n.parse().unwrap();
                    match what {
                        "run" => running = Some(n),
                        _ => {
                            running = None;
                            if !failure.is_empty() {
                                failures.push(failure.replace("\\n", "\n"));
                            }
                        }
                    }
                }
                Err(e) => {
                    let timed_out = e == std::sync::mpsc::RecvTimeoutError::Timeout;
                    if timed_out {
                        child.kill().unwrap();
                    }
                    // Reaped: the child is gone, and its memory with it.
                    let status = child.wait().unwrap();
                    let Some(n) = running else {
                        failures.push(format!("the child stopped between cases ({status})"));
                        return failures;
                    };
                    failures.push(if timed_out {
                        format!("case {n}: still running after {limit:?}, killed")
                    } else {
                        format!("case {n}: the child died ({status})")
                    });
                    from = n + 1;
                    continue 'children;
                }
            }
        }
    }
}

/// In a child of `supervised`: runs the cases from `CHILD_FROM` on, reporting each.
fn run_cases(cases: impl Iterator<Item = (Kind, Vec<u8>)>) {
    let from: usize = std::env::var(CHILD_FROM).unwrap().parse().unwrap();
    std::panic::set_hook(Box::new(|_| {}));
    for (i, (kind, bytes)) in cases.enumerate() {
        if i < from {
            continue;
        }
        println!("fuzzcase run {i}");
        let failure = check_doc(kind, &bytes)
            .err()
            .map(|e| {
                format!("{kind:?}: {e}\n  input: \"{}\"", bytes.escape_ascii()).replace('\n', "\\n")
            })
            .unwrap_or_default();
        println!("fuzzcase end {i} {failure}");
    }
    println!("fuzzcase done");
}

/// The mutated documents, in order: the same for every child.
fn mutated_documents() -> impl Iterator<Item = (Kind, Vec<u8>)> {
    let mut rng = Rng(seed() ^ 2);
    let n = count() / 4;
    document_seeds()
        .into_iter()
        .flat_map(move |(kind, seed_bytes)| {
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                let zipped = seed_bytes.starts_with(b"PK\x03\x04");
                let bytes = if zipped && rng.below(2) == 0 {
                    // The XML of one part, changed and zipped again.
                    let mut target = None;
                    doc::rezip(&seed_bytes, |name, data| {
                        if target.is_none() && name.ends_with("ml") && rng.below(3) == 0 {
                            target = Some(name.to_string());
                            return mutate(&mut rng, data, XML);
                        }
                        data
                    })
                } else {
                    mutate(&mut rng, seed_bytes.clone(), XML)
                };
                out.push((kind, bytes));
            }
            out
        })
}

#[test]
fn mutated_documents_do_not_panic_the_extractors() {
    if std::env::var_os(CHILD_FROM).is_some() {
        return run_cases(mutated_documents());
    }
    let tried = mutated_documents().count();
    let failures = supervised("mutated_documents_do_not_panic_the_extractors", DEADLINE);
    eprintln!("fuzz: {tried} documents, {} failures", failures.len());
    assert!(
        failures.is_empty(),
        "{} of {tried} documents failed; the first ones:\n{}",
        failures.len(),
        failures[..failures.len().min(3)].join("\n")
    );
}

/// The supervisor's own test: in its child, case 1 never ends, spinning and allocating, and
/// case 3 panics; the supervisor kills and reaps the child at case 1, goes on from case 2 in a
/// new one, and reports both. Making case 4 takes twice the limit, as making a seed's cases may
/// on a loaded machine: no case runs then, so the supervisor waits for it.
#[test]
fn the_supervisor_kills_a_case_that_never_ends() {
    const TEST: &str = "the_supervisor_kills_a_case_that_never_ends";
    if let Ok(from) = std::env::var(CHILD_FROM) {
        let from: usize = from.parse().unwrap();
        for i in from..5 {
            if i == 4 {
                std::thread::sleep(Duration::from_millis(600));
            }
            println!("fuzzcase run {i}");
            if i == 1 {
                let mut hog: Vec<Vec<u8>> = Vec::new();
                loop {
                    hog.push(vec![1; 1 << 20]);
                    if hog.len() > 64 {
                        hog.clear();
                    }
                }
            }
            let failure = if i == 3 { "Pdf: panic: boom" } else { "" };
            println!("fuzzcase end {i} {failure}");
        }
        println!("fuzzcase done");
        return;
    }
    let started = std::time::Instant::now();
    let failures = supervised(TEST, Duration::from_millis(300));
    assert_eq!(
        failures,
        [
            "case 1: still running after 300ms, killed",
            "Pdf: panic: boom"
        ]
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn mutated_objects_do_not_panic_the_object_processor() {
    let mut rng = Rng(seed() ^ 3);
    let spec = obj::Spec::new()
        .section(".text", b"\x55\xc3\0\0")
        .section(".rodata", b"\0hello\0version=needle\0")
        .sized("v", 1, 7, 15)
        .symbol("_ZN4core3fmt5write17h0123456789abcdefE", 0, 0);
    let mo = spec.clone();
    let seeds = vec![
        obj::elf(&spec).file,
        obj::pe(&spec).file,
        obj::macho(&mo).file,
        obj::fat(&[
            (0x0100_0007, obj::macho(&mo).file),
            (
                0x0100_000c,
                obj::macho(&mo.clone().machine(0x0100_000c)).file,
            ),
        ])
        .0,
    ];
    // Where the last panic of this thread was: goblin's own panics on what a file says are what
    // `Map::parse` silences (Codex M4 code r4 #1); a panic anywhere else is ours, and fails the
    // fuzz (Fable M4 code r5 #3), which therefore calls the parser that does not silence them.
    thread_local! {
        static WHERE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    }
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let at = info.location().map(|l| l.to_string()).unwrap_or_default();
        WHERE.with(|w| *w.borrow_mut() = at);
    }));
    let mut failures = Vec::new();
    let (mut tried, mut in_goblin) = (0, 0);
    for s in &seeds {
        for _ in 0..count() / 4 {
            tried += 1;
            let bytes = mutate(&mut rng, s.clone(), COMMON);
            let anchors: Vec<usize> = (0..4).map(|_| rng.below(bytes.len() + 1)).collect();
            let got = catch_unwind(AssertUnwindSafe(|| {
                let lines = searchfor::trail::LineIndex::new(&bytes);
                if let Some(m) = object::Map::parse_loudly(&bytes) {
                    for &a in &anchors {
                        let _ = m.trail(a, &lines);
                    }
                }
                let hits: Vec<_> = anchors
                    .iter()
                    .map(|&a| a..(a + 3).min(bytes.len()))
                    .collect();
                let mut sorted = hits.clone();
                sorted.sort_by_key(|h| h.start);
                object::rows::rows(&bytes, &sorted, None, &lines)
            }));
            match got {
                Err(_) if WHERE.with(|w| w.borrow().contains("/goblin-")) => in_goblin += 1,
                Err(_) => failures.push(format!(
                    "panic at {}\n  input: \"{}\"",
                    WHERE.with(|w| w.borrow().clone()),
                    bytes.escape_ascii()
                )),
                Ok(rows) => {
                    for r in rows {
                        let end = r.absolute_offset as usize + r.len;
                        if end > bytes.len() || r.submatches.iter().any(|s| s.end > r.len) {
                            failures.push(format!(
                                "row {}..{end} over {} bytes",
                                r.absolute_offset,
                                bytes.len()
                            ));
                        }
                    }
                }
            }
        }
    }
    std::panic::set_hook(hook);
    eprintln!(
        "fuzz: {tried} object files, {in_goblin} panics in goblin, {} failures",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {tried} failed:\n{}",
        failures.len(),
        failures[..failures.len().min(3)].join("\n")
    );
}
