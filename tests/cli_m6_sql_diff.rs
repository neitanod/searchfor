//! End-to-end tests of M6's `sql` and `diff` Processors (M6 plan, Chunk 8): statements,
//! schema columns and dump rows, and the commits, files and hunks of diffs and patches.
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
    // The plan's prelude writes `run(&mut fx.s4(…).args(…)…)`, a needless borrow under
    // clippy: `args` already gives the `&mut Command`.
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
fn m6_sql_statements() {
    let fx = Fixture::new();
    let src = b"CREATE TABLE users (\n id integer,\n email text NOT NULL, -- needle\n CONSTRAINT pk PRIMARY KEY(id)\n);\nCREATE OR REPLACE VIEW active_users AS SELECT 'needle' AS label;\n";
    seed(&fx, "code-c/m6-schema.sql", src);
    check(
        &fx,
        "sql",
        "code-c/m6-schema.sql",
        &[
            "CREATE TABLE users › email",
            "CREATE OR REPLACE VIEW active_users",
        ],
    );
    flags(&fx, "sql", "code-c/m6-schema.sql", "email", "email", "");
    put(
        &fx,
        "sql-quotes",
        br#"SELECT '; needle', "semi;name", `back;tick`, [brack;et];
GO
DELIMITER $$
CREATE PROCEDURE p() BEGIN SELECT 'needle;'; END$$
DELIMITER ;
SELECT $tag$needle ;$tag$;
"#,
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "sql",
            "sql-quotes",
            "needle",
            &["--json"]
        ))),
        ["SELECT", "CREATE PROCEDURE p", "SELECT"]
    );
    put(
        &fx,
        "bad.sql",
        b"SELECT 'needle';\nSELECT 'unclosed\nneedle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "sql",
            "bad.sql",
            "needle",
            &["--json"]
        ))),
        ["SELECT", ""]
    );
    put(&fx, "sql-own", b"CREATE TABLE needle (id int);\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "sql",
            "sql-own",
            "needle",
            &["--json"]
        ))),
        [""]
    );
}

#[test]
fn m6_sql_rows() {
    let fx = Fixture::new();
    let src = b"CREATE TABLE users (id int,email text);\nINSERT INTO users VALUES (1,'plain'),(2,'needle@example.com');\nINSERT INTO users (email,id) VALUES ('needle',3);\nCOPY public.users (id,email) FROM stdin;\n1\tplain\n2\tneedle@example.com\n\\.\nINSERT INTO unknown VALUES (1,'needle');\n\
        /*!50001 CREATE ALGORITHM=UNDEFINED */\n/*!50013 DEFINER=`root`@`localhost` SQL SECURITY DEFINER */\n/*!50001 VIEW `active_users` AS select `email` AS `email` from `users` where (`email` like '%needle%') */;\nDELIMITER ;;\n/*!50003 CREATE*/ /*!50020 DEFINER=`root`@`localhost`*/ /*!50003 PROCEDURE `find_users`()\nBEGIN\n  SELECT email FROM users WHERE email LIKE 'needle%';\nEND */;;\nDELIMITER ;\n";
    seed(&fx, "code-c/m6-dump.sql", src);
    check(
        &fx,
        "sql",
        "code-c/m6-dump.sql",
        &[
            "INSERT INTO users › row 2 › email",
            "INSERT INTO users › row 1 › email",
            "COPY public.users › row 2 › email",
            "INSERT INTO unknown › row 1 › column 2",
            "CREATE VIEW active_users",
            "CREATE PROCEDURE find_users",
        ],
    );
    flags(
        &fx,
        "sql",
        "code-c/m6-dump.sql",
        "email",
        "email",
        "^row [0-9]+$",
    );
    put(
        &fx,
        "tuple.sql",
        b"INSERT INTO t (a,b) VALUES ('needle',fn(1,2)),('x','needle');\n",
    );
    let row = matches(&output(&fx, "sql", "tuple.sql", "needle", &["--json"])).remove(0);
    assert_eq!(row["submatches"].as_array().unwrap().len(), 2);
    assert_eq!(row["submatches"][1]["trail"][1]["name"], "row 2");
    assert_eq!(row["submatches"][1]["trail"][2]["name"], "b");
    put(&fx, "broken-copy.sql", b"COPY t (a) FROM stdin;\nneedle\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "sql",
            "broken-copy.sql",
            "needle",
            &["--json"]
        ))),
        ["COPY t › row 1 › a"]
    );
}

#[test]
fn m6_diff() {
    use std::io::Write;
    let fx = Fixture::new();
    let src = b"commit 3f2a9c1234567890\nAuthor: A\n\n    Fix the needle crash\n\ndiff --git a/src/app.rs b/src/app.rs\n--- a/src/app.rs\n+++ b/src/app.rs\n@@ -1,1 +120,1 @@ fn describe(\n+    let needle = 1;\n";
    seed(&fx, "code-c/m6.patch", src);
    check(
        &fx,
        "diff",
        "code-c/m6.patch",
        &[
            "",
            "3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe(",
        ],
    );
    flags(&fx, "diff", "code-c/m6.patch", "describe", "describe", "");
    let mut child = fx
        .s4(["--no-docs", "--json", "--format", "diff", "needle"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(src).unwrap();
    assert_eq!(
        paths(&matches(&child.wait_with_output().unwrap())),
        [
            "",
            "3f2a9c1 Fix the needle crash › src/app.rs › +120 fn describe("
        ]
    );
    put(
        &fx,
        "combined.patch",
        b"--- a/gone\n+++ /dev/null\n@@@ -3,1 -3,1 +0,0 @@@ context\n--needle\n--- a/next\n+++ b/next\n@@ -1,99 +2,99 @@\n+needle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "diff",
            "combined.patch",
            "needle",
            &["--json"]
        ))),
        ["gone › -3 context", "next › +2"]
    );
}

/// M6 plan, «Meaningful generated-name exclusion and encoding tests»: `row N` and
/// `column K` are words s4 adds, which `--in-headings` never finds.
#[test]
fn m6_real_generated_names() {
    let fx = Fixture::new();
    let owned: &[&str] = &["sql"];
    let cases: &[(&str, &[u8], &str)] = &[(
        "sql",
        b"INSERT INTO t VALUES ('needle');\n",
        "^row [0-9]+$|^column [0-9]+$",
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
    }
}

/// M6 plan, «Committed broken fixtures, with three snapshots each», sql_diff's rows.
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["sql", "diff"];
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "sql",
            "code-c/broken-m6.sql",
            b"SELECT 'needle';\nSELECT 'open\nneedle\n",
            &["SELECT", ""],
        ),
        (
            "diff",
            "code-c/broken-m6.patch",
            b"--- a/x\n+++ b/x\n@@ -1,99 +2,99 @@\n+needle\n--- a/y\n+++ b/y\n@@ -1 +3 @@\n+needle\n",
            &["x › +2", "y › +3"],
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
/// generated-name exclusion and encoding tests»): sql is this topic's representative.
#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    let (format, src) = ("sql", "SELECT 'needle';\n");
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["SELECT"]);
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
    put(&fx, "cr.fixture", b"SELECT '\rneedle';\n");
    let rows = matches(&output(&fx, format, "cr.fixture", "needle", &["--json"]));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 1);
}
