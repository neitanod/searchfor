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
