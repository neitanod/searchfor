//! End-to-end tests of M6's JSON views (M6 plan, Chunk 6): Jupyter notebooks, HAR captures and
//! Telegram's JSON exports, each a keypath JSON Outline projected into the Format's own Trails.
mod common;
use common::{Fixture, code, run, stderr, stdout};
use serde_json::Value;

fn put(fx: &Fixture, file: &str, src: &[u8]) {
    let path = fx.file(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, src).unwrap();
}
fn seed(fx: &Fixture, file: &str, src: &[u8]) {
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(file);
    assert_eq!(
        std::fs::read(original).unwrap(),
        src,
        "fixture bytes: {file}"
    );
    assert_eq!(std::fs::read(fx.file(file)).unwrap(), src);
}
fn output(
    fx: &Fixture,
    format: &str,
    file: &str,
    pattern: &str,
    flags: &[&str],
) -> std::process::Output {
    run(fx
        .s4(["--no-docs", "--format", format])
        .args(flags)
        .args([pattern, file]))
}
fn matches(out: &std::process::Output) -> Vec<Value> {
    assert_eq!(code(out), 0, "{}", stderr(out));
    assert!(out.stderr.is_empty(), "{}", stderr(out));
    stdout(out)
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["type"] == "match")
        .map(|v| v["data"].clone())
        .collect()
}
fn paths(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .map(|r| {
            r["trail"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["name"].as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" › ")
        })
        .collect()
}
fn check(fx: &Fixture, format: &str, file: &str, expected: &[&str]) {
    let json = output(fx, format, file, "needle", &["--json"]);
    let rows = matches(&json);
    assert_eq!(paths(&rows), expected, "{file}");
    assert!(rows.iter().all(|r| r["format"] == format));
    common::assert_valid_json_schema(&stdout(&json));
    for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
        let out = output(fx, format, file, "needle", &[flag]);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        assert!(out.stderr.is_empty());
        let text = if layout == "json" {
            common::mask_json_times(&stdout(&out))
        } else {
            stdout(&out)
        };
        let stem = file.replace(['/', '.'], "_");
        insta::assert_snapshot!(format!("m6_{format}_{stem}_{layout}"), text);
    }
    if std::env::var_os("SEARCHFOR_M6_AUTO").is_some_and(|s| s == "1") && !file.contains("/broken-")
    {
        let auto = run(&mut fx.s4(["--no-docs", "--json", "needle", file]));
        assert_eq!(matches(&auto), rows, "automatic detection: {file}");
    }
    let src = std::fs::read(fx.file(file)).unwrap();
    let crlf = String::from_utf8_lossy(&src).replace('\n', "\r\n");
    put(fx, "m6-crlf.fixture", crlf.as_bytes());
    let copy = output(fx, format, "m6-crlf.fixture", "needle", &["--json"]);
    let copy = matches(&copy);
    assert_eq!(paths(&copy), paths(&rows));
    for (a, b) in rows.iter().zip(copy) {
        assert_eq!(a["trail"], b["trail"], "CRLF Sections: {file}");
    }
}
fn flags(
    fx: &Fixture,
    format: &str,
    file: &str,
    within: &str,
    source_name: &str,
    generated_pattern: &str,
) {
    let rows = matches(&output(
        fx,
        format,
        file,
        "needle",
        &["--json", "--within", within, "--expand"],
    ));
    assert!(!rows.is_empty());
    for row in rows {
        let section = &row["section"];
        assert!(section["start_line"].as_u64().unwrap() > 0);
        assert!(section["end_line"].as_u64().unwrap() >= section["start_line"].as_u64().unwrap());
        assert!(section["text"].as_str().unwrap().contains("needle"));
    }
    let found = output(fx, format, file, source_name, &["--json", "--in-headings"]);
    assert!(!matches(&found).is_empty(), "name: {source_name}");
    // "" where the fixture's names hold no added word. Otherwise the pattern
    // must match the name's text (no `^…$` around a name that also holds file
    // text, as `VEVENT Team sync`), so it fails only because the added word has
    // no source piece, never because the anchors miss.
    if !generated_pattern.is_empty() {
        let absent = output(
            fx,
            format,
            file,
            generated_pattern,
            &["--json", "--in-headings"],
        );
        assert_eq!(
            code(&absent),
            1,
            "generated name matched: {}",
            stdout(&absent)
        );
        assert!(absent.stderr.is_empty());
    }
}

#[test]
fn m6_jupyter() {
    let fx = Fixture::new();
    // `###` delimiters: the sources hold `"#` and `"##`, which end shorter ones.
    let src = br###"{
 "cells": [
  {"cell_type":"markdown","source":["# Analysis\n","## Load data\n"]},
  {"cell_type":"code","source":["df = load('needle.csv')\n"],
   "outputs":[{"output_type":"execute_result","data":{"text/plain":["needle"]}}]},
  {"cell_type":"raw","source":["needle raw"]}
 ],
 "nbformat":4
}
"###;
    seed(&fx, "docs-a/m6.ipynb", src);
    check(
        &fx,
        "jupyter",
        "docs-a/m6.ipynb",
        &[
            "Analysis › Load data › cell 2",
            "Analysis › Load data › cell 2 › output 1 › text/plain",
            "Analysis › Load data › cell 3",
        ],
    );
    flags(
        &fx,
        "jupyter",
        "docs-a/m6.ipynb",
        "cell 2",
        "Analysis",
        "^cell [0-9]+$",
    );
    put(&fx, "not-notebook", br#"{"source":{"k":"needle"}}"#);
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "jupyter",
            "not-notebook",
            "needle",
            &["--json"]
        ))),
        ["source › k"]
    );
    put(
        &fx,
        "bad-notebook",
        b"{\"cells\":[{\"cell_type\":\"code\",\"source\":[\"needle\"]}],\"bad\":nope}\nneedle\n",
    );
    let rows = matches(&output(
        &fx,
        "jupyter",
        "bad-notebook",
        "needle",
        &["--json"],
    ));
    assert_eq!(paths(&rows), ["cell 1", ""]);
}

#[test]
fn m6_har() {
    let fx = Fixture::new();
    let src = br#"{"log":{
 "entries":[{
  "response":{"content":{"text":"needle"},"status":200},
  "request":{"method":"GET","url":"https://api.example.com/v1/users",
   "headers":[{"name":"Authorization","value":"needle"}],
   "cookies":[{"name":"session","value":"needle"}],
   "queryString":[{"name":"q","value":"needle"}],
   "postData":{"params":[{"name":"body","value":"needle"}]}}
 }],"pages":[{"title":"Login","id":"needle"}]}}
"#;
    seed(&fx, "code-c/m6.har", src);
    check(
        &fx,
        "har",
        "code-c/m6.har",
        &[
            "GET api.example.com/v1/users › response › content › text",
            "GET api.example.com/v1/users › request › header Authorization › value",
            "GET api.example.com/v1/users › request › cookie session › value",
            "GET api.example.com/v1/users › request › queryString q › value",
            "GET api.example.com/v1/users › request › postData › param body › value",
            "Login › id",
        ],
    );
    flags(
        &fx,
        "har",
        "code-c/m6.har",
        "response",
        "Authorization",
        "^header ",
    );
    put(
        &fx,
        "bad-har",
        b"{\"log\":{\"entries\":[{\"request\":{\"method\":\"GET\",\"url\":\"http://x\"},\"x\":\"needle\"}]}}\nnope\nneedle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "har",
            "bad-har",
            "needle",
            &["--json"]
        ))),
        ["GET x › x", ""]
    );
    let rows = matches(&output(&fx, "har", "code-c/m6.har", "needle", &["--json"]));
    assert!(
        rows[0]["trail"][0]["signature"]
            .as_str()
            .unwrap()
            .contains("https://api.example.com/v1/users")
    );
}

#[test]
fn m6_telegram_json() {
    let fx = Fixture::new();
    let src = br#"{
 "name":"Ana",
 "messages":[
  {"text":"needle","date":"2024-01-31T10:04:12","from":"Ana"},
  {"date":"2024-01-31T10:05:00","from":"Ana",
   "text":["needle",{"type":"bold","text":"needle"}],
   "text_entities":[{"type":"plain","text":"needle"}]}
 ]
}
"#;
    seed(&fx, "chat/ChatExport_m6/result.json", src);
    // SEARCHFOR_M6_AUTO=1 also checks detection by the ChatExport_* path.
    check(
        &fx,
        "telegram",
        "chat/ChatExport_m6/result.json",
        &[
            "Ana › 2024-01-31T10:04:12 Ana › text",
            "Ana › 2024-01-31T10:05:00 Ana › text",
            "Ana › 2024-01-31T10:05:00 Ana › text_entities › text",
        ],
    );
    flags(
        &fx,
        "telegram",
        "chat/ChatExport_m6/result.json",
        "Ana",
        "Ana",
        "",
    );
    put(
        &fx,
        "account",
        br#"{"chats":{"list":[{"name":"Work","messages":[{"text":"needle","from":"Ana","date":"2024"}]}]}}"#,
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "telegram",
            "account",
            "needle",
            &["--json"]
        ))),
        ["Work › 2024 Ana › text"]
    );
    put(
        &fx,
        "bad-telegram",
        b"{\"name\":\"Ana\",\"messages\":[{\"date\":\"2024\",\"from\":\"Ana\",\"text\":\"needle\"}]}\nnope\nneedle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "telegram",
            "bad-telegram",
            "needle",
            &["--json"]
        ))),
        ["Ana › 2024 Ana › text", ""]
    );
}

/// Each Format's committed broken fixture, with its three snapshots (plan, «Committed broken
/// fixtures, with three snapshots each»).
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["jupyter", "har", "telegram"];
    #[allow(clippy::type_complexity)]
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "jupyter",
            "docs-a/broken-m6.ipynb",
            b"{\"cells\":[{\"cell_type\":\"code\",\"source\":[\"needle\"]}]}\nnope\nneedle\n",
            &["cell 1", ""],
        ),
        (
            "har",
            "code-c/broken-m6.har",
            b"{\"log\":{\"entries\":[{\"request\":{\"method\":\"GET\",\"url\":\"http://x\"},\"x\":\"needle\"}]}}\nnope\nneedle\n",
            &["GET x › x", ""],
        ),
        (
            "telegram",
            "chat/ChatExport_m6/broken-result.json",
            b"{\"name\":\"Ana\",\"messages\":[{\"date\":\"2024\",\"from\":\"Ana\",\"text\":\"needle\"}]}\nnope\nneedle\n",
            &["Ana › 2024 Ana › text", ""],
        ),
    ];
    let fx = Fixture::new();
    let mut count = 0;
    for &(format, file, src, want) in cases {
        if !owned.contains(&format) {
            continue;
        }
        seed(&fx, file, src);
        check(&fx, format, file, want);
        count += 1;
    }
    assert_eq!(count, owned.len());
}

/// A generated name (`cell 1`) has no Source piece, so `--in-headings` never finds it (plan,
/// «Meaningful generated-name exclusion and encoding tests»).
#[test]
fn m6_real_generated_names() {
    let fx = Fixture::new();
    let owned: &[&str] = &["jupyter", "har"];
    let cases: &[(&str, &[u8], &str)] = &[
        (
            "jupyter",
            b"{\"cells\":[{\"cell_type\":\"code\",\"source\":[\"needle\"]}]}",
            "^cell [0-9]+$",
        ),
        (
            "har",
            b"{\"log\":{\"entries\":[{\"request\":{\"method\":\"GET\",\"url\":\"http://x\",\"headers\":[{\"name\":\"Auth\",\"value\":\"needle\"}]}}]}}",
            "^header ",
        ),
    ];
    for &(format, src, generated) in cases {
        if !owned.contains(&format) {
            continue;
        }
        put(&fx, "generated.fixture", src);
        assert_eq!(
            code(&output(
                &fx,
                format,
                "generated.fixture",
                generated,
                &["--json", "--in-headings"]
            )),
            1
        );
    }
}

/// UTF-16 and a BOM read as UTF-8, and a lone CR breaks no line (plan, «Meaningful
/// generated-name exclusion and encoding tests»): HAR is this topic's representative. A raw CR
/// is no character of a JSON string, so the lone CR is whitespace before the value.
#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    let (format, src) = (
        "har",
        "{\"log\":{\"entries\":[{\"request\":{\"method\":\"GET\",\"url\":\"http://x\"},\"x\":\"needle\"}]}}\n",
    );
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["GET x › x"]);
    let utf16: Vec<u8> = [
        vec![0xff, 0xfe],
        src.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    ]
    .concat();
    put(&fx, "utf16.fixture", &utf16);
    assert_eq!(
        paths(&matches(&output(
            &fx,
            format,
            "utf16.fixture",
            "needle",
            &["--json"]
        ))),
        lf
    );
    put(
        &fx,
        "bom.fixture",
        &[b"\xef\xbb\xbf".as_slice(), src.as_bytes()].concat(),
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            format,
            "bom.fixture",
            "needle",
            &["--json"]
        ))),
        lf
    );
    put(
        &fx,
        "cr.fixture",
        src.replace(":\"needle", ":\r\"needle").as_bytes(),
    );
    let rows = matches(&output(&fx, format, "cr.fixture", "needle", &["--json"]));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 1);
}

/// A Match in the values that name a HAR entry, page or header, or a Telegram chat, stays
/// inside that Segment, under its real key (orchestrator's decision 1 on code round 1).
#[test]
fn m6_view_name_values_stay_inside() {
    let fx = Fixture::new();
    let har = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/code-c/m6.har"),
    )
    .unwrap();
    put(&fx, "names.har", &har);
    for (pattern, want) in [
        (
            "Authorization",
            "GET api.example.com/v1/users › request › header Authorization › name",
        ),
        ("Login", "Login › title"),
        (
            "api\\.example",
            "GET api.example.com/v1/users › request › url",
        ),
    ] {
        let rows = matches(&output(&fx, "har", "names.har", pattern, &["--json"]));
        assert_eq!(paths(&rows), [want], "{pattern}");
    }
    put(
        &fx,
        "account",
        br#"{"chats":{"list":[{"name":"Work","messages":[{"text":"x","from":"Ana","date":"2024"}]}]}}"#,
    );
    let rows = matches(&output(&fx, "telegram", "account", "Work", &["--json"]));
    assert_eq!(paths(&rows), ["Work › name"]);
    let rows = matches(&output(&fx, "telegram", "account", "Ana", &["--json"]));
    assert_eq!(paths(&rows), ["Work › 2024 Ana › from"]);
}

/// The root chat of a chat export is a Name, found by `--in-headings` though no message's
/// `from` holds it (code round 1, Codex #8, Fable #11).
#[test]
fn m6_telegram_root_chat_is_a_name() {
    let fx = Fixture::new();
    put(
        &fx,
        "result.json",
        br#"{"name":"Work","messages":[{"text":"needle","from":"Ana","date":"2024"}]}"#,
    );
    let rows = matches(&output(
        &fx,
        "telegram",
        "result.json",
        "Work",
        &["--json", "--in-headings"],
    ));
    assert_eq!(paths(&rows), ["Work"]);
    put(&fx, "empty.json", br#"{"name":"needle","messages":[]}"#);
    let rows = matches(&output(
        &fx,
        "telegram",
        "empty.json",
        "needle",
        &["--json", "--in-headings"],
    ));
    assert_eq!(rows.len(), 1);
    let rows = matches(&output(
        &fx,
        "telegram",
        "result.json",
        "needle",
        &["--json", "--within", "Work"],
    ));
    assert_eq!(paths(&rows), ["Work › 2024 Ana › text"]);
}
