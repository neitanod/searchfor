//! End-to-end tests of M6's keypath syntaxes (M6 plan, Chunk 2): INI and systemd,
//! `.properties`, `.env`, SSH configuration, iCalendar, vCard and BibTeX.
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
fn m6_ini() {
    let fx = Fixture::new();
    let ini = b"root=top\n[server]\nhost = needle.example\n  continued needle\n  port: 80\n[remote \"origin\"]\nurl=needle.git\n[a.b]\nk=needle\n";
    seed(&fx, "config/m6.ini", ini);
    check(
        &fx,
        "ini",
        "config/m6.ini",
        &[
            "server › host",
            "server › host",
            "remote \"origin\" › url",
            "a.b › k",
        ],
    );
    flags(&fx, "ini", "config/m6.ini", "server", "host", "");
    let service = b"[Unit]\nDescription=demo\n[Service]\nExecStart=/usr/bin/needle \\\n --serve\n";
    seed(&fx, "config/m6.service", service);
    check(
        &fx,
        "systemd",
        "config/m6.service",
        &["Service › ExecStart"],
    );
    flags(
        &fx,
        "systemd",
        "config/m6.service",
        "Service",
        "ExecStart",
        "",
    );
    let bad = b"[s]\nk=needle\n[broken\nneedle tail\n[s]\nk=needle again\n";
    put(&fx, "broken.ini", bad);
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "ini",
            "broken.ini",
            "needle",
            &["--json"]
        ))),
        ["s › k", "s › k", "s › k"]
    );
    let names = matches(&output(&fx, "ini", "config/m6.ini", "server", &["--json"]));
    assert_eq!(paths(&names), [""]);
    let same = b"[s]\nk=needle\n[s]\nk=needle\n";
    put(&fx, "repeat.ini", same);
    let rows = matches(&output(&fx, "ini", "repeat.ini", "needle", &["--json"]));
    assert_ne!(rows[0]["trail"][0]["line"], rows[1]["trail"][0]["line"]);
}

#[test]
fn m6_properties_env() {
    let fx = Fixture::new();
    let prop = b"# comment\nspring.datasource.url=jdbc:needle://db\ncaf\\u00e9\\:key : needle\\\n continued\n";
    seed(&fx, "config/m6.properties", prop);
    check(
        &fx,
        "properties",
        "config/m6.properties",
        &["spring.datasource.url", "café:key"],
    );
    flags(
        &fx,
        "properties",
        "config/m6.properties",
        "spring.datasource.url",
        "café",
        "",
    );
    let env = b"# config\nexport DATABASE_URL=postgres://needle@db/app\nTEXT=\"first\nneedle # literal\nlast\" # end\n";
    seed(&fx, "config/.env.m6", env);
    check(&fx, "env", "config/.env.m6", &["DATABASE_URL", "TEXT"]);
    flags(&fx, "env", "config/.env.m6", "TEXT", "DATABASE_URL", "");
    put(
        &fx,
        "bad.properties",
        b"k=needle\nbad\\uQQQQ=needle\nlast=needle\n",
    );
    let bad = output(&fx, "properties", "bad.properties", "needle", &["--json"]);
    assert!(bad.stderr.is_empty());
    assert_eq!(matches(&bad).len(), 3);
    put(&fx, "bad.env", b"A=needle\nB=\"needle\nneedle tail\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "env",
            "bad.env",
            "needle",
            &["--json"]
        ))),
        ["A", "B", "B"]
    );
    let own = matches(&output(
        &fx,
        "env",
        "config/.env.m6",
        "DATABASE_URL",
        &["--json"],
    ));
    assert_eq!(paths(&own), [""]);
}

#[test]
fn m6_ssh() {
    let fx = Fixture::new();
    let src = b"User root\nHost github.com gitlab.com\n  IdentityFile ~/.ssh/needle_ed25519\nMatch user ana\n  ProxyCommand=needle --proxy\nInclude /never/read/needle\n";
    seed(&fx, "config/ssh/ssh_config", src);
    check(
        &fx,
        "ssh-config",
        "config/ssh/ssh_config",
        &[
            "Host github.com gitlab.com › IdentityFile",
            "Match user ana › ProxyCommand",
            "Match user ana › Include",
        ],
    );
    flags(
        &fx,
        "ssh-config",
        "config/ssh/ssh_config",
        "IdentityFile",
        "github",
        "",
    );
    put(
        &fx,
        "broken-ssh",
        b"hOsT example\nidentityfile needle\nUnrecognized needle\nMatch\nUser needle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "ssh-config",
            "broken-ssh",
            "needle",
            &["--json"]
        ))),
        [
            "hOsT example › identityfile",
            "hOsT example › Unrecognized",
            "Match › User"
        ]
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "ssh-config",
            "config/ssh/ssh_config",
            "IdentityFile",
            &["--json"]
        ))),
        ["Host github.com gitlab.com"]
    );
}

#[test]
fn m6_broken_fixtures() {
    // The keypath row of the plan's table: every Format of this topic file.
    let owned: &[&str] = &["ini", "systemd", "properties", "env", "ssh-config"];
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "ini",
            "config/broken-m6.ini",
            b"[s]\nk=needle\n[broken\nneedle\n",
            &["s › k", "s › k"],
        ),
        (
            "systemd",
            "config/broken-m6.service",
            b"[Service]\nExecStart=needle\\\n needle\n",
            &["Service › ExecStart", "Service › ExecStart"],
        ),
        (
            "properties",
            "config/broken-m6.properties",
            b"k=needle\nbad\\uQQQQ=needle\n",
            &["k", "bad\\uQQQQ"],
        ),
        (
            "env",
            "config/broken-env.m6",
            b"A=needle\nB=\"needle\nneedle\n",
            &["A", "B", "B"],
        ),
        (
            "ssh-config",
            "config/ssh/broken-ssh_config",
            b"Host x\nK needle\nUnknown needle\n",
            &["Host x › K", "Host x › Unknown"],
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

#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    // keypath's representative Format and valid source (M6 plan, supplemental matrices).
    let (format, src) = ("ini", "[Café]\nk=needle\n");
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["Café › k"]);
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
    // A lone CR in the content value before `needle` is no line break.
    put(&fx, "lone-cr.fixture", b"[Caf\xc3\xa9]\nk=\rneedle\n");
    let rows = matches(&output(
        &fx,
        format,
        "lone-cr.fixture",
        "needle",
        &["--json"],
    ));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 2);
}
