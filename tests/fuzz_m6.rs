//! M6 (plan Chunk 11, Task 11.1): every text Format of M6 through its native cursor, compared
//! at every byte with its Processor's model, and over mutated fixtures (cut, a byte flipped,
//! repeated, garbage inserted), each case supervised in a child process for 2 s. The JSON views
//! (Jupyter, HAR, Telegram) build their model on the same cursor they stream with, so they are
//! also compared with an independent reading of the JSON (`oracle`). `SEARCHFOR_FUZZ_SEED`,
//! `SEARCHFOR_FUZZ_N` and `SEARCHFOR_FUZZ_SECONDS` work as in tests/fuzz_m5.rs; a failing case
//! is saved under `SEARCHFOR_FUZZ_ARTIFACT_DIR` with its exact bytes, and replays through
//! `m6_mutation_child` with `SEARCHFOR_M6_FUZZ_CASE` and `SEARCHFOR_M6_FUZZ_FORMAT`.
mod common;

use searchfor::{
    formats::Registry,
    model::Input,
    processors::{Capability, Processor, Processors},
};
use std::path::Path;
use std::time::Duration;

/// The 32 text Formats of M6.
const WANTED: [&str; 32] = [
    "asciidoc",
    "rst",
    "org",
    "latex",
    "jupyter",
    "ini",
    "systemd",
    "properties",
    "env",
    "ssh-config",
    "nginx",
    "apache",
    "hcl",
    "dockerfile",
    "make",
    "just",
    "sql",
    "diff",
    "vue",
    "svelte",
    "blade",
    "graphql",
    "protobuf",
    "har",
    "eml",
    "mbox",
    "icalendar",
    "vcard",
    "bibtex",
    "whatsapp",
    "telegram",
    "telegram-html",
];

/// Their primary fixtures, under tests/fixtures.
const FILES: [&str; 32] = [
    "docs-a/m6.adoc",
    "docs-a/m6.rst",
    "docs-a/m6.org",
    "docs-a/m6.tex",
    "docs-a/m6.ipynb",
    "config/m6.ini",
    "config/m6.service",
    "config/m6.properties",
    "config/.env.m6",
    "config/ssh/ssh_config",
    "config/nginx/nginx.conf",
    "config/apache2/apache2.conf",
    "config/m6.tf",
    "config/m6.dockerfile",
    "config/m6.mk",
    "config/m6.just",
    "code-c/m6-dump.sql",
    "code-c/m6.patch",
    "code-c/m6.vue",
    "code-c/m6.svelte",
    "code-c/m6.blade.php",
    "code-c/m6.graphql",
    "code-c/m6.proto",
    "code-c/m6.har",
    "mail/m6.eml",
    "mail/m6.mbox",
    "config/m6.ics",
    "config/m6.vcf",
    "docs-a/m6.bib",
    "chat/_chat.txt",
    "chat/ChatExport_m6/result.json",
    "chat/ChatExport_m6/messages.html",
];

/// The primary fixture of Format `name`.
fn fixture_of(name: &str) -> &'static str {
    FILES[WANTED.iter().position(|w| *w == name).unwrap()]
}

/// Whether the automatic variants run (plan, chunk graph): s4 without `--format` too.
fn auto() -> bool {
    std::env::var_os("SEARCHFOR_M6_AUTO").is_some_and(|s| s == "1")
}

/// The 32 text Formats of M6 and their primary fixtures.
fn targets() -> Vec<(String, toml::Table, Vec<u8>)> {
    let (r, w) = Registry::load(None, &Processors::known);
    assert!(w.is_empty());
    let (wanted, files) = (WANTED, FILES);
    wanted
        .into_iter()
        .zip(files)
        .map(|(name, file)| {
            let d = r.get(name).unwrap();
            let src = std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures")
                    .join(file),
            )
            .unwrap();
            (name.into(), d.params.clone(), src)
        })
        .collect()
}

fn processor(name: &str) -> &'static dyn Processor {
    let (r, w) = Registry::load(None, &Processors::known);
    assert!(w.is_empty());
    searchfor::processors::by_name(&r.get(name).unwrap().processor).unwrap()
}

/// The native cursor at every byte of `bytes`, against the Processor's model.
fn check_case(name: &str, params: &toml::Table, bytes: &[u8]) {
    let at: Vec<_> = (0..=bytes.len()).collect();
    check_at(name, params, bytes, &at, true);
    // The model of a JSON view is its own cursor: compare it with a reading of the JSON that
    // shares no code with keypath's, whenever the bytes are valid JSON.
    if params.contains_key("view") && params.get("syntax").and_then(|s| s.as_str()) == Some("json")
    {
        oracle::compare(name, params, bytes);
    }
}

/// `check_case` at the offsets `at` only, in non-decreasing order (a large or deep input).
/// Returns the length of the longest Trail. `names`: also check every name's pieces (a deep
/// input leaves them to s4's `--in-headings`, which `supervised` runs).
fn check_at(name: &str, params: &toml::Table, bytes: &[u8], at: &[usize], names: bool) -> usize {
    let p = processor(name);
    let input = Input {
        bytes,
        path: Path::new("m6"),
        format: name,
        disk: None,
    };
    assert_eq!(p.capability(params), Capability::Native);
    let expected = p.trails(&input, params, at).unwrap();
    let prepared = p.prepare(input, params).unwrap();
    assert!(prepared.streams());
    let lines = 1 + bytes.iter().filter(|&&b| b == b'\n').count() as u64;
    let mut c = prepared.cursor();
    for (&a, want) in at.iter().zip(&expected) {
        assert_eq!(
            c.trail_at(a).unwrap().as_ref(),
            want.as_slice(),
            "{name}/{a}"
        );
        assert!(
            c.open_from().is_some_and(|s| s <= a),
            "{name}/{a}: no safe open_from"
        );
        for s in want {
            assert!(s.line > 0 && s.end_line >= s.line, "{name}/{a}: {s:?}");
            assert!(s.end_line <= lines, "{name}/{a}: {s:?} past line {lines}");
        }
        let one = c.trail_at(a).unwrap();
        let two = c.trail_at(a).unwrap();
        assert!(
            std::rc::Rc::ptr_eq(&one, &two),
            "{name}/{a}: equal Trails not shared"
        );
    }
    for (k, &a) in at.iter().enumerate().rev().step_by(3) {
        assert_eq!(
            c.trail_at(a).unwrap().as_ref(),
            expected[k].as_slice(),
            "{name}/{a} asked backwards"
        );
    }
    for n in if names {
        prepared.names().unwrap()
    } else {
        Vec::new()
    } {
        assert!(n.anchor <= bytes.len());
        for (text, source) in n.pieces {
            assert!(text.start <= text.end && text.end <= n.text.len());
            assert!(source.start <= source.end && source.end <= bytes.len());
            assert!(n.text.is_char_boundary(text.start) && n.text.is_char_boundary(text.end));
        }
    }
    expected.iter().map(Vec::len).max().unwrap_or(0)
}

#[test]
fn m6_cursor_model() {
    for (name, params, src) in targets() {
        check_case(&name, &params, &src);
    }
    // SEARCHFOR_M6_AUTO=1: s4 finds each fixture's Format by its name or path.
    if auto() {
        let fx = common::Fixture::new();
        for name in WANTED {
            let out = common::run(&mut fx.s4(["--no-docs", "--json", "needle", fixture_of(name)]));
            assert_eq!(common::code(&out), 0, "{name}: {}", common::stderr(&out));
            let formats: Vec<String> = common::stdout(&out)
                .lines()
                .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                .filter(|v| v["type"] == "match")
                .map(|v| v["data"]["format"].as_str().unwrap_or("").to_owned())
                .collect();
            assert!(
                !formats.is_empty() && formats.iter().all(|f| f == name),
                "{name}: {formats:?}"
            );
        }
    }
}

/// One supervised case, run by `m6_mutations` and `m6_depth_and_names` in a child process:
/// the file `SEARCHFOR_M6_FUZZ_CASE` as Format `SEARCHFOR_M6_FUZZ_FORMAT`, at every byte, or
/// at the single offset `SEARCHFOR_M6_FUZZ_ANCHOR` (a deep input, whose every-byte Trails
/// would take gigabytes).
#[test]
fn m6_mutation_child() {
    let Some(path) = std::env::var_os("SEARCHFOR_M6_FUZZ_CASE") else {
        return;
    };
    let name = std::env::var("SEARCHFOR_M6_FUZZ_FORMAT").unwrap();
    let (r, w) = Registry::load(None, &Processors::known);
    assert!(w.is_empty());
    let bytes = std::fs::read(path).unwrap();
    let params = &r.get(&name).unwrap().params;
    match std::env::var("SEARCHFOR_M6_FUZZ_ANCHOR") {
        Ok(a) => {
            let a: usize = a.parse().unwrap();
            let longest = check_at(&name, params, &bytes, &[a], false);
            assert!(longest <= 256, "{name}: a Trail of {longest} Segments");
        }
        Err(_) => check_case(&name, params, &bytes),
    }
}

#[test]
fn m6_mutations() {
    let fx = common::Fixture::new();
    let seed = common::fuzz_seed(0x6_2026_1104);
    let mut rng = seed;
    let cases = targets();
    let n = common::fuzz_n().unwrap_or(64);
    let budget = common::fuzz_budget(cases.len() * n);
    let flight = common::FuzzInFlight::new("m6_mutations");
    // First the Format × operation product, before any deadline is checked, so every Format
    // meets all four operations; after it the RNG picks them.
    let product = cases.len() * 4;
    let mut seen = vec![[false; 4]; cases.len()];
    let mut completed = 0;
    while completed < product || budget.permits(completed) {
        let target = completed % cases.len();
        let (name, _, src) = &cases[target];
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let op = if completed < product {
            completed / cases.len()
        } else {
            ((rng >> 32) as usize) % 4
        };
        seen[target][op] = true;
        let mut b = src.clone();
        let at = (rng as usize) % (b.len() + 1);
        match op {
            0 => b.truncate(at),
            1 => {
                if at < b.len() {
                    b[at] ^= 0xff
                }
            }
            2 => {
                let part = b[..at].to_vec();
                b.extend(part);
            }
            _ => {
                b.splice(at..at, b"\0\xff}\n".iter().copied());
            }
        }
        std::fs::write(fx.file("mutated"), &b).unwrap();
        flight.at(|| format!("seed={seed} case={completed} rng={rng} format={name}"));
        supervised(&fx, "m6_mutations", seed, n, completed, name, &b, None);
        completed += 1;
    }
    assert!(completed >= product);
    for ((name, _, _), ops) in cases.iter().zip(&seen) {
        assert!(
            ops.iter().all(|&o| o),
            "{name}: an operation never ran: {ops:?}"
        );
    }
    flight.finish();
}

/// Runs the case `b`, already written to the fixture's `mutated`, in a child within `limit()`
/// (at every byte, or at `anchor` alone, a deep input, whose s4 runs also get `--in-headings`),
/// then s4 over it within `limit()`. The case is saved before it runs and removed when both
/// pass, so a crash leaves its exact bytes behind.
#[allow(clippy::too_many_arguments)]
fn supervised(
    fx: &common::Fixture,
    test: &str,
    seed: u64,
    n: usize,
    case: usize,
    name: &str,
    b: &[u8],
    anchor: Option<usize>,
) {
    let argv = [std::ffi::OsStr::new(name)];
    let saved = common::save_fuzz_case(&common::FuzzCase {
        test,
        seed,
        n,
        case,
        argv: &argv,
        files: &[("case.bin", b)],
        stderr: b"",
        what: "in flight",
    });
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "m6_mutation_child", "--nocapture"])
        .env("SEARCHFOR_M6_FUZZ_CASE", fx.file("mutated"))
        .env("SEARCHFOR_M6_FUZZ_FORMAT", name);
    match anchor {
        Some(a) => cmd.env("SEARCHFOR_M6_FUZZ_ANCHOR", a.to_string()),
        None => cmd.env_remove("SEARCHFOR_M6_FUZZ_ANCHOR"),
    };
    let started = std::time::Instant::now();
    let (status, err) = common::run_limited(&mut cmd, limit(anchor.is_some()));
    assert!(
        status.is_some_and(|s| s.success()),
        "seed={seed} case={case} {name} ({:?}): {}",
        started.elapsed(),
        String::from_utf8_lossy(&err)
    );
    // SEARCHFOR_M6_AUTO=1: also without `--format`, at the fixture's own path, so the
    // Detector picks the Format from the name as for the fixture.
    let mut runs: Vec<(Vec<&str>, String)> = vec![(
        vec!["--no-docs", "--json", "--format", name],
        "mutated".into(),
    )];
    if anchor.is_some() {
        runs.push((
            vec!["--no-docs", "--json", "--in-headings", "--format", name],
            "mutated".into(),
        ));
    }
    if auto() && WANTED.contains(&name) {
        let file = format!("auto/{}", fixture_of(name));
        let path = fx.file(&file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b).unwrap();
        runs.push((vec!["--no-docs", "--json"], file));
    }
    for (args, file) in &runs {
        let started = std::time::Instant::now();
        let (status, err) =
            common::run_limited(fx.s4(args).args(["needle", file]), limit(anchor.is_some()));
        assert!(
            status
                .and_then(|s| s.code())
                .is_some_and(|c| c == 0 || c == 1),
            "seed={seed} case={case} {name} {args:?} {file} ({:?}): {}",
            started.elapsed(),
            String::from_utf8_lossy(&err)
        );
    }
    if let Some(saved) = saved {
        std::fs::remove_dir_all(saved).unwrap();
    }
}

/// How long a case may run: 2 s (M6 spec §13, criterion 48), in the release build CI fuzzes
/// with. A 200 000-level input in a debug build takes 1.0–2.7 s on this machine, linear in the
/// depth (25 000 to 200 000 levels measured), so a debug run gives it 10 s.
fn limit(deep: bool) -> Duration {
    if deep && cfg!(debug_assertions) {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(2)
    }
}

#[test]
fn m6_depth_and_names() {
    let (r, w) = Registry::load(None, &Processors::known);
    assert!(w.is_empty());
    let fx = common::Fixture::new();
    // 200 000 levels of nesting: a bounded Trail, within 2 s, in the library and in s4.
    for (case, (format, open, close)) in [
        ("nginx", "x {", "}"),
        ("hcl", "x {", "}"),
        ("protobuf", "message X {", "}"),
        ("vue", "<x>", "</x>"),
        ("svelte", "{#if x}", "{/if}"),
        ("blade", "@if($x)\n", "@endif\n"),
        ("latex", "\\begin{x}\n", "\\end{x}\n"),
        ("icalendar", "BEGIN:X\n", "END:X\n"),
    ]
    .into_iter()
    .enumerate()
    {
        let src = format!("{}needle{}", open.repeat(200000), close.repeat(200000));
        assert!(r.get(format).is_some(), "{format}");
        let at = src.find("needle").unwrap();
        std::fs::write(fx.file("mutated"), &src).unwrap();
        supervised(
            &fx,
            "m6_depth_and_names",
            0,
            1,
            case,
            format,
            src.as_bytes(),
            Some(at),
        );
    }
    for (name, params, src) in targets() {
        let p = processor(&name);
        let input = Input {
            bytes: &src,
            path: Path::new("names"),
            format: &name,
            disk: None,
        };
        for n in p.names(&input, &params).unwrap() {
            for (text, source) in n.pieces {
                assert!(text.end <= n.text.len() && source.end <= src.len());
            }
            for s in &n.trail {
                assert!(s.name.chars().count() <= 60, "{name}: {}", s.name);
            }
        }
    }
    hostile_names(&fx);
}

/// Each primary fixture with its first Source name replaced by exactly 1 MiB of `a`, then
/// `\x1b[31m` and an invalid `\xff`; and once more with only the escape and the byte before a
/// short name. Library names stay bounded, valid UTF-8; the one-line and tree layouts print no
/// raw control byte in a Trail; `--json` keeps the decoded name; the search sees the original
/// bytes: the Matches are rg's.
fn hostile_names(fx: &common::Fixture) {
    let mut skipped = Vec::new();
    let mut undecoded = Vec::new();
    let mut in_json = Vec::new();
    for (name, params, src) in targets() {
        let p = processor(&name);
        let input = Input {
            bytes: &src,
            path: Path::new("names"),
            format: &name,
            disk: None,
        };
        let names = p.names(&input, &params).unwrap();
        // The first name with a verbatim piece of the Source, which a replacement keeps a name.
        let Some(piece) = names.iter().find_map(|n| {
            n.pieces
                .iter()
                .find(|(t, s)| !s.is_empty() && t.len() == s.len())
                .map(|(_, s)| s.clone())
        }) else {
            skipped.push(name.clone());
            continue;
        };
        for (variant, hostile) in [
            (
                "long",
                [vec![b'a'; 1 << 20], b"\x1b[31m\xff".to_vec()].concat(),
            ),
            ("short", b"\x1b[31m\xffname".to_vec()),
        ] {
            let mut b = src[..piece.start].to_vec();
            b.extend(&hostile);
            b.extend(&src[piece.end..]);
            if variant == "long" {
                assert_eq!(b.len(), src.len() - piece.len() + (1 << 20) + 6);
            }
            let input = Input {
                bytes: &b,
                path: Path::new("hostile"),
                format: &name,
                disk: None,
            };
            // `name_chars`, 40 when absent (M6 spec §3 rule 4).
            let max = params
                .get("name_chars")
                .and_then(|v| v.as_integer())
                .unwrap_or(40) as usize;
            for n in p.names(&input, &params).unwrap() {
                for s in &n.trail {
                    assert!(
                        s.name.chars().count() <= max,
                        "{name}/{variant}: {}",
                        s.name
                    );
                }
            }
            // The cursor and the model at every Match and every 4096th byte.
            let mut at: Vec<usize> = memchr::memmem::find_iter(&b, b"needle").collect();
            at.extend((0..=b.len()).step_by(4096));
            at.sort_unstable();
            at.dedup();
            check_at(&name, &params, &b, &at, true);
            let file = format!("hostile-{variant}");
            std::fs::write(fx.file(&file), &b).unwrap();
            let rg = common::run(&mut fx.rg(["-c", "needle", &file]));
            let json = common::run(&mut fx.s4([
                "--no-docs",
                "--json",
                "--format",
                &name,
                "needle",
                &file,
            ]));
            assert_eq!(json.status.code(), rg.status.code(), "{name}/{variant}");
            let rows: Vec<serde_json::Value> = common::stdout(&json)
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .filter(|v: &serde_json::Value| v["type"] == "match")
                .collect();
            assert_eq!(
                rows.len().to_string(),
                common::stdout(&rg).trim(),
                "{name}/{variant}: the Matches are rg's"
            );
            if variant == "short" {
                // The library keeps the decoded name, the invalid byte as U+FFFD; --json keeps
                // it so in a Match's Trail that holds it.
                let names = p.names(&input, &params).unwrap();
                if !names
                    .iter()
                    .any(|n| n.text.contains("\u{1b}[31m\u{fffd}name"))
                {
                    undecoded.push(name.clone());
                }
                let json = common::run(&mut fx.s4([
                    "--no-docs",
                    "--json",
                    "--format",
                    &name,
                    "needle",
                    &file,
                ]));
                for l in common::stdout(&json).lines() {
                    let v: serde_json::Value = serde_json::from_str(l).unwrap();
                    if v["type"] != "match" {
                        continue;
                    }
                    for seg in v["data"]["trail"].as_array().unwrap() {
                        let n = seg["name"].as_str().unwrap();
                        if n.contains("\u{fffd}name") {
                            assert!(n.contains("\u{1b}[31m"), "{name}: {n:?}");
                            in_json.push(name.clone());
                        }
                    }
                }
            }
            for layout in ["--flat", "--tree"] {
                let out = common::run(&mut fx.s4([
                    "--no-docs",
                    "--color",
                    "never",
                    layout,
                    "--format",
                    &name,
                    "needle",
                    &file,
                ]));
                assert_eq!(
                    out.status.code(),
                    rg.status.code(),
                    "{name}/{variant}{layout}"
                );
                for line in out.stdout.split(|&b| b == b'\n') {
                    // The Trail is what comes before the line's text (`│`) in either layout.
                    let trail = match memchr::memmem::find(line, "│".as_bytes()) {
                        Some(i) => &line[..i],
                        None if layout == "--tree" => line,
                        None => continue,
                    };
                    if layout == "--tree" && memchr::memmem::find(line, b"needle").is_some() {
                        continue;
                    }
                    assert!(
                        !trail.contains(&0x1b),
                        "{name}/{variant}{layout}: a raw escape in {:?}",
                        String::from_utf8_lossy(trail)
                    );
                }
            }
        }
    }
    in_json.dedup();
    eprintln!(
        "hostile names: no verbatim Source name in {skipped:?}; name not kept decoded in \
         {undecoded:?}; decoded in a --json Trail in {in_json:?}"
    );
}

/// The fixtures of the JSON views and the cases of their unit tests, each also with every
/// token on its own line and with no whitespace at all: the independent reading must agree
/// with the view at every byte of every value.
#[test]
fn m6_views_against_an_independent_json_reading() {
    let (r, w) = Registry::load(None, &Processors::known);
    assert!(w.is_empty());
    let mut seeds: Vec<(&str, Vec<u8>)> = targets()
        .into_iter()
        .filter(|(n, _, _)| ["jupyter", "har", "telegram"].contains(&n.as_str()))
        .map(|(n, _, s)| {
            (
                if n == "jupyter" {
                    "jupyter"
                } else if n == "har" {
                    "har"
                } else {
                    "telegram"
                },
                s,
            )
        })
        .collect();
    seeds.push((
        "har",
        br#"{"log":{"version":"1.2","entries":[
{"response":{"status":404,"content":{"text":"needle"}},"request":{"url":"ftp+x://h/p?needle"}},
{"request":{"method":"POST","url":"/rel","headers":[{"value":"needle"},{"name":"A\u00e9","value":"needle"}]}},
{"x":"needle"}
],"pages":[{"id":"needle"},{"title":"T","id":"needle"}]}}"#
            .to_vec(),
    ));
    seeds.push((
        "telegram",
        br#"{"messages":[
{"from":"Ana","text":["needle",{"type":"link","text":"needle"}]},
{"id":7,"text":"needle"}
],"name":"Late","x":"needle"}"#
            .to_vec(),
    ));
    seeds.push((
        "telegram",
        br#"{"about":"needle","chats":{"about":"x","list":[{"name":"W","messages":[{"date":"d","text":"needle"}]},{"messages":[]}]}}"#
            .to_vec(),
    ));
    seeds.push((
        "jupyter",
        br###"{"cells":[
{"cell_type":"markdown","source":"# A\n```\n# not\n```\n## B\u000a  ### C ##\n"},
{"source":["needle"],"cell_type":"code"},
{"cell_type":"markdown","source":["#no\n","# D\r\n"]},
{"cell_type":"code","source":["needle"],"outputs":[{"output_type":"stream","text":["needle"]}]}
],"metadata":{"k":"needle"}}"###
            .to_vec(),
    ));
    let mut compared = std::collections::BTreeMap::new();
    for (view, src) in seeds {
        let params = &r.get(view).unwrap().params;
        for variant in [src.clone(), oracle::spread(&src), oracle::minified(&src)] {
            check_case(view, params, &variant);
            *compared.entry(view).or_insert(0) += oracle::compare(view, params, &variant);
        }
    }
    eprintln!("bytes compared with the independent reading: {compared:?}");
    for view in ["jupyter", "har", "telegram"] {
        assert!(compared[view] > 500, "{view}: {compared:?}");
    }
}

/// An independent reading of a JSON view (M6 spec §6.5, §8.8, §10.5): the bytes parsed by a
/// parser of this test, keypath's plain JSON Trails (its model, without the view) for the
/// members it keeps, and the view's projection of them written again from the spec. It
/// compares the view's cursor with it at every byte of every scalar value; it shares with the
/// view only keypath's plain Segments, which `m6_cursor_model` compares for JSON elsewhere.
mod oracle {
    use searchfor::model::{Input, Segment};
    use std::ops::Range;
    use std::path::Path;

    /// A parsed JSON value and where it is.
    pub struct Val {
        pub at: Range<usize>,
        pub v: V,
    }

    pub enum V {
        /// Members: decoded key, the key's first byte (its quote), the value.
        Obj(Vec<(String, usize, Val)>),
        Arr(Vec<Val>),
        /// Decoded text, and the Source offset of each decoded byte (and one past the end).
        Str(String, Vec<usize>),
        /// A number, `true`, `false` or `null`, as written.
        Lit(String),
    }

    struct P<'a> {
        s: &'a [u8],
        i: usize,
    }

    impl P<'_> {
        fn ws(&mut self) {
            while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            }
        }

        fn value(&mut self, depth: usize) -> Option<Val> {
            if depth > 200 {
                return None;
            }
            self.ws();
            let start = self.i;
            let v = match *self.s.get(self.i)? {
                b'{' => {
                    self.i += 1;
                    let mut members = Vec::new();
                    self.ws();
                    if self.s.get(self.i) == Some(&b'}') {
                        self.i += 1;
                    } else {
                        loop {
                            self.ws();
                            let key_at = self.i;
                            let V::Str(key, _) = self.string()? else {
                                return None;
                            };
                            self.ws();
                            (self.s.get(self.i) == Some(&b':')).then_some(())?;
                            self.i += 1;
                            let value = self.value(depth + 1)?;
                            members.push((key, key_at, value));
                            self.ws();
                            match self.s.get(self.i)? {
                                b',' => self.i += 1,
                                b'}' => {
                                    self.i += 1;
                                    break;
                                }
                                _ => return None,
                            }
                        }
                    }
                    V::Obj(members)
                }
                b'[' => {
                    self.i += 1;
                    let mut items = Vec::new();
                    self.ws();
                    if self.s.get(self.i) == Some(&b']') {
                        self.i += 1;
                    } else {
                        loop {
                            items.push(self.value(depth + 1)?);
                            self.ws();
                            match self.s.get(self.i)? {
                                b',' => self.i += 1,
                                b']' => {
                                    self.i += 1;
                                    break;
                                }
                                _ => return None,
                            }
                        }
                    }
                    V::Arr(items)
                }
                b'"' => self.string()?,
                _ => {
                    let from = self.i;
                    while self.i < self.s.len()
                        && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'0'..=b'9' | b'a'..=b'z' | b'E')
                    {
                        self.i += 1;
                    }
                    let lit = std::str::from_utf8(&self.s[from..self.i]).ok()?.to_owned();
                    let ok = ["true", "false", "null"].contains(&lit.as_str())
                        || lit.parse::<f64>().is_ok() && !lit.starts_with('+');
                    ok.then_some(())?;
                    V::Lit(lit)
                }
            };
            Some(Val {
                at: start..self.i,
                v,
            })
        }

        fn string(&mut self) -> Option<V> {
            (self.s.get(self.i) == Some(&b'"')).then_some(())?;
            self.i += 1;
            let mut text = Vec::new();
            let mut map = Vec::new();
            loop {
                let at = self.i;
                let b = *self.s.get(self.i)?;
                match b {
                    b'"' => {
                        self.i += 1;
                        map.push(at);
                        break;
                    }
                    0..=0x1f => return None,
                    b'\\' => {
                        let e = *self.s.get(self.i + 1)?;
                        self.i += 2;
                        let c = match e {
                            b'"' => '"',
                            b'\\' => '\\',
                            b'/' => '/',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'n' => '\n',
                            b'r' => '\r',
                            b't' => '\t',
                            b'u' => {
                                let hex = |p: &mut Self| -> Option<u32> {
                                    let h = std::str::from_utf8(p.s.get(p.i..p.i + 4)?).ok()?;
                                    p.i += 4;
                                    u32::from_str_radix(h, 16).ok()
                                };
                                let u = hex(self)?;
                                if (0xd800..0xdc00).contains(&u)
                                    && self.s.get(self.i..self.i + 2) == Some(b"\\u")
                                {
                                    self.i += 2;
                                    let lo = hex(self)?;
                                    (0xdc00..0xe000).contains(&lo).then_some(())?;
                                    char::from_u32(0x10000 + ((u - 0xd800) << 10) + (lo - 0xdc00))?
                                } else {
                                    char::from_u32(u)?
                                }
                            }
                            _ => return None,
                        };
                        let mut buf = [0; 4];
                        for &x in c.encode_utf8(&mut buf).as_bytes() {
                            text.push(x);
                            map.push(at);
                        }
                    }
                    _ => {
                        text.push(b);
                        map.push(at);
                        self.i += 1;
                    }
                }
            }
            Some(V::Str(String::from_utf8(text).ok()?, map))
        }
    }

    /// The file's only value, when it is exactly one valid JSON value (whitespace around it).
    pub fn parse(src: &[u8]) -> Option<Val> {
        let mut p = P { s: src, i: 0 };
        let v = p.value(0)?;
        p.ws();
        (p.i == src.len()).then_some(v)
    }

    /// `src` with every structural token on its own line.
    pub fn spread(src: &[u8]) -> Vec<u8> {
        relayout(src, true)
    }

    /// `src` with no whitespace outside its strings.
    pub fn minified(src: &[u8]) -> Vec<u8> {
        relayout(src, false)
    }

    fn relayout(src: &[u8], spread: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut in_str = false;
        let mut esc = false;
        for &b in src {
            if in_str {
                out.push(b);
                if esc {
                    esc = false;
                } else if b == b'\\' {
                    esc = true;
                } else if b == b'"' {
                    in_str = false;
                }
                continue;
            }
            match b {
                b' ' | b'\t' | b'\n' | b'\r' => {}
                b'"' => {
                    in_str = true;
                    out.push(b);
                }
                b'{' | b'[' | b',' | b':' if spread => {
                    out.push(b);
                    out.push(b'\n');
                }
                b'}' | b']' if spread => {
                    out.push(b'\n');
                    out.push(b);
                }
                _ => out.push(b),
            }
        }
        out
    }

    fn line_of(src: &[u8], at: usize) -> u64 {
        1 + memchr::memchr_iter(b'\n', &src[..at.min(src.len())]).count() as u64
    }

    /// keypath's Segment lines for a range: its first byte's line to its last byte's.
    fn lines(src: &[u8], r: Range<usize>) -> (u64, u64) {
        (
            line_of(src, r.start),
            line_of(src, r.end.saturating_sub(1).max(r.start)),
        )
    }

    fn cut(s: &str, max: usize) -> String {
        if max == 0 || s.chars().count() <= max {
            return s.to_owned();
        }
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }

    /// The text of a scalar for a name: a string's, a literal's other than `null`.
    fn text(v: &Val) -> Option<&str> {
        match &v.v {
            V::Str(s, _) => Some(s),
            V::Lit(l) if l != "null" => Some(l),
            _ => None,
        }
    }

    fn get<'v>(v: &'v Val, path: &[&str]) -> Option<&'v Val> {
        let mut v = v;
        for k in path {
            let V::Obj(m) = &v.v else { return None };
            v = &m.iter().find(|(key, _, _)| key == k)?.2;
        }
        Some(v)
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Ctx {
        Plain,
        Lines,
        Notebook,
        Cells,
        Cell,
        Outputs,
        Output,
        Data,
        Har,
        Log,
        Entries,
        Pages,
        Entry,
        Exchange,
        PostData,
        Named(&'static str),
        Chat,
        Account,
        Chats,
        ChatList,
        Messages,
        Message,
    }

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Role {
        Plain,
        Hidden,
        Shown,
        Cell(usize),
        Output(usize),
        Entry,
        Page,
        Named(&'static str),
        Chat,
        Message,
    }

    /// One member on the way to a byte: its key (None for an element), its index, its value.
    struct Step<'v> {
        key: Option<(&'v str, usize)>,
        index: usize,
        val: &'v Val,
    }

    /// The view's rule for a member in `ctx` (M6 spec §6.5, §8.8, §10.5).
    fn project(view: &str, ctx: Ctx, s: &Step) -> (Role, Ctx) {
        let is = |k: &str| s.key.is_some_and(|(key, _)| key == k);
        let obj = matches!(s.val.v, V::Obj(_));
        let arr = matches!(s.val.v, V::Arr(_));
        let elem = s.key.is_none();
        use Ctx as C;
        match (view, ctx) {
            (_, C::Plain) => (Role::Plain, C::Plain),
            (_, C::Lines) => (Role::Hidden, C::Plain),
            ("jupyter", C::Notebook) if is("cells") && arr => (Role::Hidden, C::Cells),
            ("jupyter", C::Cells) if elem && obj => (Role::Cell(s.index), C::Cell),
            ("jupyter", C::Cell) if is("source") => (Role::Hidden, C::Lines),
            ("jupyter", C::Cell) if is("outputs") && arr => (Role::Hidden, C::Outputs),
            ("jupyter", C::Outputs) if elem && obj => (Role::Output(s.index), C::Output),
            ("jupyter", C::Output) if is("data") && obj => (Role::Hidden, C::Data),
            ("jupyter", C::Output) if is("text") => (Role::Hidden, C::Lines),
            ("jupyter", C::Data) if arr => (Role::Shown, C::Lines),
            ("har", C::Har) if is("log") && obj => (Role::Hidden, C::Log),
            ("har", C::Log) if is("entries") && arr => (Role::Plain, C::Entries),
            ("har", C::Log) if is("pages") && arr => (Role::Plain, C::Pages),
            ("har", C::Entries) if elem && obj => (Role::Entry, C::Entry),
            ("har", C::Pages) if elem && obj => (Role::Page, C::Plain),
            ("har", C::Entry) if (is("request") || is("response")) && obj => {
                (Role::Plain, C::Exchange)
            }
            ("har", C::Exchange) if is("headers") && arr => (Role::Plain, C::Named("header")),
            ("har", C::Exchange) if is("cookies") && arr => (Role::Plain, C::Named("cookie")),
            ("har", C::Exchange) if is("queryString") && arr => {
                (Role::Plain, C::Named("queryString"))
            }
            ("har", C::Exchange) if is("postData") && obj => (Role::Plain, C::PostData),
            ("har", C::PostData) if is("params") && arr => (Role::Plain, C::Named("param")),
            ("har", C::Named(w)) if elem && obj => (Role::Named(w), C::Plain),
            ("telegram", C::Account) if is("chats") && obj => (Role::Hidden, C::Chats),
            ("telegram", C::Chats) if is("list") && arr => (Role::Hidden, C::ChatList),
            ("telegram", C::ChatList) if elem && obj => (Role::Chat, C::Chat),
            ("telegram", C::Chat) if is("messages") && arr => (Role::Plain, C::Messages),
            ("telegram", C::Messages) if elem && obj => (Role::Message, C::Message),
            ("telegram", C::Message) if (is("text") || is("text_entities")) && arr => {
                (Role::Shown, C::Lines)
            }
            _ => (Role::Plain, C::Plain),
        }
    }

    /// The shape of the root, by view: the first member of the root that has it.
    fn shape(view: &str, root: &Val) -> Option<Ctx> {
        let V::Obj(members) = &root.v else {
            return None;
        };
        members.iter().find_map(|(k, _, v)| {
            let (obj, arr) = (matches!(v.v, V::Obj(_)), matches!(v.v, V::Arr(_)));
            match view {
                "jupyter" if k == "cells" && arr => Some(Ctx::Notebook),
                "har" if k == "log" && obj => Some(Ctx::Har),
                "telegram" if k == "messages" && arr => Some(Ctx::Chat),
                "telegram" if k == "chats" && obj => Some(Ctx::Account),
                _ => None,
            }
        })
    }

    /// A view's Segment over keypath's `base`, and its label: None when the values its name
    /// needs are absent (it keeps keypath's Segment then).
    fn node(role: Role, val: &Val, base: &Segment, max: usize) -> Option<(Segment, Range<usize>)> {
        let mut seg = base.clone();
        let mut label = val.at.start..val.at.start;
        match role {
            Role::Cell(i) | Role::Output(i) => {
                let (kind, key) = match role {
                    Role::Cell(_) => ("cell", "cell_type"),
                    _ => ("output", "output_type"),
                };
                seg.kind = kind.into();
                seg.name = format!("{kind} {}", i + 1);
                seg.signature = get(val, &[key])
                    .and_then(text)
                    .map(|t| format!("{} ({})", seg.name, cut(t, max)));
            }
            Role::Entry => {
                let method = get(val, &["request", "method"]).and_then(text);
                let url = get(val, &["request", "url"]).and_then(text);
                let status = get(val, &["response", "status"]).and_then(text);
                if method.is_none() && url.is_none() {
                    return None;
                }
                let bare = url.map(|u| match u.find("://") {
                    Some(i)
                        if i > 0
                            && u[..i]
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b)) =>
                    {
                        &u[i + 3..]
                    }
                    _ => u,
                });
                let name: Vec<&str> = [method, bare].into_iter().flatten().collect();
                let sig: Vec<&str> = [method, url, status].into_iter().flatten().collect();
                seg.kind = "entry".into();
                seg.name = cut(&name.join(" "), max);
                seg.signature = Some(sig.join(" "));
            }
            Role::Page | Role::Named(_) | Role::Chat => {
                let key = if role == Role::Page { "title" } else { "name" };
                let v = get(val, &[key])?;
                let t = text(v)?;
                seg.kind = if role == Role::Chat {
                    "chat"
                } else {
                    "element"
                }
                .into();
                seg.name = match role {
                    Role::Named(w) => cut(&format!("{w} {t}"), max),
                    _ => cut(t, max),
                };
                label = v.at.clone();
            }
            Role::Message => {
                let date = get(val, &["date"]).and_then(text);
                let from = get(val, &["from"]).and_then(text);
                if date.is_none() && from.is_none() {
                    return None;
                }
                let name: Vec<&str> = [date, from].into_iter().flatten().collect();
                seg.kind = "message".into();
                seg.name = cut(&name.join(" "), max);
                seg.signature = Some(seg.name.clone());
            }
            _ => return None,
        }
        Some((seg, label))
    }

    /// A Markdown heading of a notebook: its Segment and the Source bytes of its line.
    struct Heading {
        level: usize,
        line: Range<usize>,
        seg: Segment,
    }

    /// The headings of the Markdown cells (M6 spec §6.5), each over the bytes up to the next
    /// heading of the same or a higher level, or the end of `cells`.
    fn headings(src: &[u8], cells: &Val, max: usize) -> Vec<(Heading, Range<usize>)> {
        let V::Arr(cells_v) = &cells.v else {
            return Vec::new();
        };
        let mut found: Vec<Heading> = Vec::new();
        for cell in cells_v {
            let V::Obj(m) = &cell.v else { continue };
            let markdown = m.iter().any(|(k, _, v)| {
                k == "cell_type"
                    && matches!(&v.v, V::Str(s, _) if s == "markdown")
                    && &src[v.at.clone()] == b"\"markdown\""
            });
            if !markdown {
                continue;
            }
            // Lines are decoded line breaks of the source strings in order; a line that runs
            // to its string's end is read there, and the next string does not start a line.
            let mut fence = false;
            let mut line_start = true;
            for (k, _, v) in m {
                if k != "source" {
                    continue;
                }
                let strings: Vec<&Val> = match &v.v {
                    V::Str(..) => vec![v],
                    V::Arr(items) => items.iter().filter(|i| matches!(i.v, V::Str(..))).collect(),
                    _ => Vec::new(),
                };
                for s in strings {
                    let V::Str(t, map) = &s.v else { unreachable!() };
                    let mut read = |from: usize, e: usize, fence: &mut bool| {
                        if let Some((level, title)) = atx(&t[from..e], fence) {
                            let line = map[from]..map[e];
                            let (l, _) = lines(src, line.clone());
                            found.push(Heading {
                                level,
                                line,
                                seg: Segment {
                                    kind: "heading".into(),
                                    name: cut(title, max),
                                    line: l,
                                    end_line: l,
                                    level: Some(level),
                                    signature: Some(cut(&t[from..e], max.saturating_mul(2))),
                                },
                            });
                        }
                    };
                    let mut from = 0;
                    for (i, b) in t.bytes().enumerate() {
                        if b != b'\n' {
                            continue;
                        }
                        let mut e = i;
                        if e > from
                            && t.as_bytes()[e - 1] == b'\r'
                            && src.get(map[e - 1]..map[e - 1] + 2) == Some(b"\\r")
                        {
                            e -= 1;
                        }
                        if line_start {
                            read(from, e, &mut fence);
                        }
                        line_start = true;
                        from = i + 1;
                    }
                    if from < t.len() {
                        if line_start {
                            read(from, t.len(), &mut fence);
                        }
                        line_start = false;
                    }
                }
            }
        }
        let end = cells.at.end;
        let mut out = Vec::new();
        for i in 0..found.len() {
            let until = found[i + 1..]
                .iter()
                .find(|n| n.level <= found[i].level)
                .map_or(end, |n| n.line.start);
            let mut h = Heading {
                level: found[i].level,
                line: found[i].line.clone(),
                seg: found[i].seg.clone(),
            };
            h.seg.end_line = lines(src, h.line.start..until).1;
            let range = h.line.start..until;
            out.push((h, range));
        }
        out
    }

    /// An ATX heading line: `#` to `######` after at most three spaces, then a blank or the
    /// end; its title without the closing run of `#`. Fences of ``` or ~~~ toggle `fence`.
    fn atx<'t>(line: &'t str, fence: &mut bool) -> Option<(usize, &'t str)> {
        let indent = line.bytes().take_while(|&b| b == b' ').count();
        if indent > 3 {
            return None;
        }
        let body = &line[indent..];
        if body.starts_with("```") || body.starts_with("~~~") {
            *fence = !*fence;
            return None;
        }
        if *fence {
            return None;
        }
        let level = body.bytes().take_while(|&b| b == b'#').count();
        if !(1..=6).contains(&level) {
            return None;
        }
        let after = &body[level..];
        if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
            return None;
        }
        let mut t = after.trim_matches([' ', '\t']);
        let k = t.trim_end_matches('#');
        if k.len() < t.len() && (k.is_empty() || k.ends_with(' ')) {
            t = k.trim_end_matches([' ', '\t']);
        }
        Some((level, t))
    }

    /// The members holding `at`, from the root down.
    fn path<'v>(root: &'v Val, at: usize) -> Vec<Step<'v>> {
        let mut out = Vec::new();
        let mut v = root;
        loop {
            match &v.v {
                V::Obj(m) => {
                    let Some((k, key_at, val)) = m
                        .iter()
                        .find(|(_, k_at, val)| *k_at <= at && at < val.at.end)
                    else {
                        break;
                    };
                    out.push(Step {
                        key: Some((k.as_str(), *key_at)),
                        index: 0,
                        val,
                    });
                    v = val;
                }
                V::Arr(items) => {
                    let Some((i, val)) = items
                        .iter()
                        .enumerate()
                        .find(|(_, val)| val.at.contains(&at))
                    else {
                        break;
                    };
                    out.push(Step {
                        key: None,
                        index: i,
                        val,
                    });
                    v = val;
                }
                _ => break,
            }
        }
        out
    }

    /// Every byte of a scalar value, a member's or an element's.
    fn scalars(v: &Val, out: &mut Vec<Range<usize>>) {
        match &v.v {
            V::Obj(m) => m.iter().for_each(|(_, _, v)| scalars(v, out)),
            V::Arr(items) => items.iter().for_each(|v| scalars(v, out)),
            _ => out.push(v.at.clone()),
        }
    }

    /// The view's Trail at `at`, by this reading, from keypath's plain Trail there.
    fn expected(
        view: &str,
        src: &[u8],
        root: &Val,
        plain: &[Segment],
        heads: &[(Heading, Range<usize>)],
        max: usize,
        at: usize,
    ) -> Vec<Segment> {
        let mut out = Vec::new();
        let ctx0 = shape(view, root).expect("checked by the caller");
        // The root chat of a chat export: all of the root, named by its `name`.
        if ctx0 == Ctx::Chat {
            let (line, end_line) = lines(src, root.at.clone());
            let base = Segment {
                kind: "chat".into(),
                name: String::new(),
                line,
                end_line,
                level: None,
                signature: None,
            };
            if let Some((seg, label)) = node(Role::Chat, root, &base, max) {
                if label.contains(&at) {
                    return out;
                }
                out.push(seg);
            }
        }
        // The headings over `at`; on a heading's own line, its parents only.
        for (h, range) in heads {
            if h.line.contains(&at) {
                return out;
            }
            if range.contains(&at) {
                out.push(h.seg.clone());
            }
        }
        let steps = path(root, at);
        // keypath's plain Segments, one per member it shows: an element, or a key whose value
        // is no array (an array's key is in its elements' `key[n]`).
        let all = plain;
        let mut plain = plain.iter();
        let mut ctx = ctx0;
        // A label holding `at` ends the Trail before its Segment (M6 spec §3 rule 5).
        let mut ended = false;
        for s in &steps {
            let array_key = s.key.is_some() && matches!(s.val.v, V::Arr(_));
            let base = if array_key { None } else { plain.next() };
            let (role, next) = project(view, ctx, s);
            ctx = next;
            match role {
                Role::Hidden => {}
                Role::Plain => out.extend(base.cloned()),
                Role::Shown => {
                    let (k, key_at) = s.key.expect("a key");
                    let (line, end_line) = lines(src, key_at..s.val.at.end);
                    let label = key_at..key_at + 1;
                    if label.contains(&at) {
                        ended = true;
                        break;
                    }
                    out.push(Segment {
                        kind: "key".into(),
                        name: k.to_owned(),
                        line,
                        end_line,
                        level: None,
                        signature: None,
                    });
                }
                _ => {
                    let base = base.expect("an element of keypath's");
                    match node(role, s.val, base, max) {
                        Some((seg, label)) => {
                            if label.contains(&at) {
                                ended = true;
                                break;
                            }
                            out.push(seg);
                        }
                        None => out.push(base.clone()),
                    }
                }
            }
        }
        assert!(
            ended || plain.next().is_none(),
            "{view}/{at}: keypath's Trail {:?} past the members {:?}",
            names_of(all),
            steps
                .iter()
                .map(|s| s.key.map_or(format!("[{}]", s.index), |k| k.0.to_owned()))
                .collect::<Vec<_>>()
        );
        out
    }

    fn names_of(t: &[Segment]) -> Vec<String> {
        t.iter().map(|s| s.name.clone()).collect()
    }

    /// Compares the view's cursor with this reading at every byte of every scalar value of
    /// `src`, when it is one valid JSON value of the view's shape; how many bytes it compared.
    pub fn compare(view: &str, params: &toml::Table, src: &[u8]) -> usize {
        let Some(root) = parse(src) else {
            return 0;
        };
        let Some(ctx) = shape(view, &root) else {
            return 0;
        };
        let max = params
            .get("name_chars")
            .and_then(|v| v.as_integer())
            .unwrap_or(40) as usize;
        let keypath = searchfor::processors::by_name("keypath").unwrap();
        let mut plain_params = params.clone();
        plain_params.remove("view");
        let mut at = Vec::new();
        scalars(&root, &mut at);
        let at: Vec<usize> = at.into_iter().flatten().collect();
        let input = Input {
            bytes: src,
            path: Path::new("oracle"),
            format: view,
            disk: None,
        };
        let plain = keypath.trails(&input, &plain_params, &at).unwrap();
        let heads = match ctx {
            Ctx::Notebook => {
                let cells = super::oracle::get(&root, &["cells"]).unwrap();
                headings(src, cells, max)
            }
            _ => Vec::new(),
        };
        let prepared = keypath.prepare(input, params).unwrap();
        let mut cursor = prepared.cursor();
        for (&a, plain) in at.iter().zip(&plain) {
            let want = expected(view, src, &root, plain, &heads, max, a);
            assert_eq!(
                cursor.trail_at(a).unwrap().as_ref(),
                want.as_slice(),
                "{view} at byte {a} of {}",
                String::from_utf8_lossy(src)
            );
        }
        at.len()
    }
}
