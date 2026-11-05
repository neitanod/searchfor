//! End-to-end tests of M6's `blocks` Processor (M6 plan, Chunk 3): nginx, Apache, HCL,
//! GraphQL and Protobuf, each under its explicit `--format`.
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
fn m6_servers() {
    let fx = Fixture::new();
    let nginx = b"http {\n server {\n  location /api {\n   proxy_pass http://needle:8080;\n  }\n  server_name example.com;\n }\n}\n";
    seed(&fx, "config/nginx/nginx.conf", nginx);
    check(
        &fx,
        "nginx",
        "config/nginx/nginx.conf",
        &["http › server example.com › location /api › proxy_pass"],
    );
    flags(
        &fx,
        "nginx",
        "config/nginx/nginx.conf",
        "example.com",
        "example.com",
        "",
    );
    let apache = b"<VirtualHost *:443>\n <Directory /var/www>\n  Require ip 10.0.0.0/8 # needle\n </Directory>\n ServerName example.com\n</VirtualHost>\n";
    seed(&fx, "config/apache2/apache2.conf", apache);
    check(
        &fx,
        "apache",
        "config/apache2/apache2.conf",
        &["VirtualHost *:443 example.com › Directory /var/www › Require"],
    );
    flags(
        &fx,
        "apache",
        "config/apache2/apache2.conf",
        "example.com",
        "example.com",
        "",
    );
    put(&fx, "bad-nginx", b"http { k needle; }\n}\nk needle;\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "nginx",
            "bad-nginx",
            "needle",
            &["--json"]
        ))),
        ["http › k", ""]
    );
    put(
        &fx,
        "bad-apache",
        b"<Directory /x>\nX needle\n</unknown>\nY needle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "apache",
            "bad-apache",
            "needle",
            &["--json"]
        ))),
        ["Directory /x › X", "Directory /x › Y"]
    );
    put(
        &fx,
        "quoted-nginx",
        b"http { set $x '}; needle'; # { ignored\n k \"{;needle\"; }\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "nginx",
            "quoted-nginx",
            "needle",
            &["--json"]
        ))),
        ["http › set", "http › k"]
    );
}

#[test]
fn m6_hcl() {
    let fx = Fixture::new();
    let src = b"resource \"aws_instance\" \"web\" {\n tags = {\n  Name = \"needle-web\"\n }\n ingress = [\n  { port = 80 },\n  { name = \"needle\" }\n ]\n script = <<-EOT\n  echo needle ${not_a_block}\n  EOT\n}\n";
    seed(&fx, "config/m6.tf", src);
    check(
        &fx,
        "hcl",
        "config/m6.tf",
        &[
            "resource aws_instance web › tags › Name",
            "resource aws_instance web › ingress[1] › name",
            "resource aws_instance web › script",
        ],
    );
    flags(&fx, "hcl", "config/m6.tf", "tags", "Name", "\\[1\\]");
    put(
        &fx,
        "hcl-lex",
        br#"x "y" {
 value = "${join("}", ["needle", "%{if true}x%{endif}"])}"
 /* { */ other = "needle" // }
}
}
needle
"#,
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "hcl",
            "hcl-lex",
            "needle",
            &["--json"]
        ))),
        ["x y › value", "x y › other", ""]
    );
    put(&fx, "hcl-name", b"resource \"needle\" \"web\" {\n k=1\n}\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "hcl",
            "hcl-name",
            "needle",
            &["--json"]
        ))),
        [""]
    );
}

/// M6 plan, «Committed broken fixtures, with three snapshots each», blocks' rows.
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["nginx", "apache", "hcl"];
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "nginx",
            "config/nginx/broken-nginx.conf",
            b"http { k needle; }\n}\nk needle;\n",
            &["http › k", ""],
        ),
        (
            "apache",
            "config/apache2/broken-apache.conf",
            b"<Directory /x>\nK needle\n</unknown>\nK needle\n",
            &["Directory /x › K", "Directory /x › K"],
        ),
        (
            "hcl",
            "config/broken-m6.tf",
            b"x { k=\"needle\" }\n}\nneedle\n",
            &["x › k", ""],
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

/// M6 plan, «Meaningful generated-name exclusion and encoding tests», blocks' representative:
/// nginx. UTF-16 and a BOM give the Trails of the UTF-8 file, and a lone CR is no line break.
#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    let (format, src) = ("nginx", "http { k needle; }\n");
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["http › k"]);
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
    put(&fx, "cr.fixture", b"http { k \rneedle; }\n");
    let rows = matches(&output(&fx, format, "cr.fixture", "needle", &["--json"]));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 1);
}
