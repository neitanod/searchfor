//! M6 mail and chat (spec §10.1, §10.4): RFC 5322 headers, mbox messages and MIME parts,
//! WhatsApp days and messages (plan, Chunk 9).
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
    // The plan's prelude says `run(&mut fx.s4(…)…)`: `args` already gives `&mut Command`,
    // which clippy's `needless_borrow` refuses.
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
fn m6_mail() {
    let fx = Fixture::new();
    let eml = b"From: ana@example.com\nDate: Wed, 31 Jan 2024 10:00:00 +0000\nSubject: Your needle invoice\nX-Long: first\n needle folded\n\nneedle body\n";
    seed(&fx, "mail/m6.eml", eml);
    check(&fx, "eml", "mail/m6.eml", &["Subject", "X-Long", "body"]);
    flags(&fx, "eml", "mail/m6.eml", "body", "Subject", "^body$");
    let mbox = b"From ana@example.com Wed Jan 31 10:00:00 2024\nSubject: Budget\nFrom: Ana\nDate: Today\n\nneedle one\n>From escaped needle\n\nFrom bob@example.com Wed Jan 31 11:00:00 2024\nSubject: Next\n\nneedle two\n";
    seed(&fx, "mail/m6.mbox", mbox);
    check(
        &fx,
        "mbox",
        "mail/m6.mbox",
        &["Budget › body", "Budget › body", "Next › body"],
    );
    flags(&fx, "mbox", "mail/m6.mbox", "Budget", "Budget", "^body$");
    put(
        &fx,
        "bad.eml",
        b"Subject: One\nX-A: needle\nnot a header needle\nneedle tail\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "eml",
            "bad.eml",
            "needle",
            &["--json"]
        ))),
        ["X-A", "body", "body"]
    );
    put(
        &fx,
        "subject.mbox",
        b"From a x\nSubject: needle name\n\nneedle body\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "mbox",
            "subject.mbox",
            "needle",
            &["--json"]
        ))),
        ["Subject", "needle name › body"]
    );
}

/// Each Format's committed broken fixture, with its three snapshots (plan, «Committed broken
/// fixtures, with three snapshots each»).
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["eml", "mbox"];
    #[allow(clippy::type_complexity)]
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "eml",
            "mail/broken-m6.eml",
            b"Subject: One\nX: needle\nnot a header needle\n",
            &["X", "body"],
        ),
        (
            "mbox",
            "mail/broken-m6.mbox",
            b"From a x\nSubject: One\n\nneedle\n>From needle\n",
            &["One › body", "One › body"],
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

/// UTF-16 and a BOM read as UTF-8, and a lone CR breaks no line (plan, «Meaningful
/// generated-name exclusion and encoding tests»): eml is this topic's representative.
#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    let (format, src) = ("eml", "Subject: Café\n\nneedle\n");
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["body"]);
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
    // A lone CR in the content before `needle` is no line break.
    put(
        &fx,
        "lone-cr.fixture",
        b"Subject: Caf\xc3\xa9\n\n\rneedle\n",
    );
    let rows = matches(&output(
        &fx,
        format,
        "lone-cr.fixture",
        "needle",
        &["--json"],
    ));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 3);
}

#[test]
fn m6_mime() {
    let fx = Fixture::new();
    let src = b"Subject: =?UTF-8?Q?Budget_=C3=A9?=\nContent-Type: multipart/mixed; boundary=OUT\n\n--OUT\nContent-Type: multipart/alternative; boundary=IN\n\n--IN\nContent-Type: text/plain\n\nneedle plain\n--IN--\n--OUT\nContent-Type: text/plain\nContent-Disposition: attachment; filename*=UTF-8''caf%C3%A9.txt\n\nneedle attachment\n--OUT--\n";
    seed(&fx, "mail/m6-mime.eml", src);
    check(
        &fx,
        "eml",
        "mail/m6-mime.eml",
        &[
            "part 1 multipart/alternative › part 1.1 text/plain",
            "attachment café.txt",
        ],
    );
    flags(
        &fx,
        "eml",
        "mail/m6-mime.eml",
        "café.txt",
        "café",
        "^part [0-9.]+ ",
    );
    put(
        &fx,
        "encoded.mbox",
        b"From a x\nSubject: =?UTF-8?B?QnVkZ2V0?=\n\nneedle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "mbox",
            "encoded.mbox",
            "needle",
            &["--json"]
        ))),
        ["Budget › body"]
    );
    let decoded = output(
        &fx,
        "mbox",
        "encoded.mbox",
        "Budget",
        &["--json", "--in-headings"],
    );
    assert_eq!(matches(&decoded).len(), 1);
    put(
        &fx,
        "bad-mime.eml",
        b"Content-Type: multipart/mixed; boundary=X\n\n--X\nContent-Type: text/plain\n\nneedle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "eml",
            "bad-mime.eml",
            "needle",
            &["--json"]
        ))),
        ["part 1 text/plain"]
    );
    put(
        &fx,
        "encoded-body.eml",
        b"Content-Transfer-Encoding: base64\n\nbmVlZGxl\n",
    );
    let absent = output(&fx, "eml", "encoded-body.eml", "needle", &["--json"]);
    assert_eq!(code(&absent), 1);
}

/// A generated name (`part 1`) has no Source piece, so `--in-headings` never finds it (plan,
/// «Meaningful generated-name exclusion and encoding tests»).
#[test]
fn m6_real_generated_names() {
    let fx = Fixture::new();
    let owned: &[&str] = &["eml"];
    let cases: &[(&str, &[u8], &str)] = &[(
        "eml",
        b"Content-Type: multipart/mixed; boundary=X\n\n--X\nContent-Type: text/plain\n\nneedle\n--X--\n",
        "^part [0-9]+ ",
    )];
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
        // The media type comes from the Source: it is found.
        assert_eq!(
            code(&output(
                &fx,
                format,
                "generated.fixture",
                "text/plain",
                &["--json", "--in-headings"]
            )),
            0
        );
    }
}
