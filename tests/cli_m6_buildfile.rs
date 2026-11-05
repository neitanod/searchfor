//! M6 build files (spec §7.9–§7.11): Dockerfile stages and instructions, Makefile targets,
//! variables and conditionals, just recipes and aliases (plan, Chunk 4).
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
fn m6_dockerfile() {
    let fx = Fixture::new();
    let src = b"ARG BASE=alpine\nFROM ${BASE} AS build\nRUN echo start \\\n # continuation comment\n needle-build --release\nRUN <<EOF\nneedle heredoc\nEOF\nFROM scratch\nCOPY --from=build /needle /app\n";
    seed(&fx, "config/m6.dockerfile", src);
    check(
        &fx,
        "dockerfile",
        "config/m6.dockerfile",
        &["build › RUN", "build › RUN", "stage 1 › COPY"],
    );
    flags(
        &fx,
        "dockerfile",
        "config/m6.dockerfile",
        "build",
        "build",
        "^stage [0-9]+$",
    );
    put(
        &fx,
        "docker-escape",
        b"# escape=`\nFROM alpine\nRUN echo `\n needle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "dockerfile",
            "docker-escape",
            "needle",
            &["--json"]
        ))),
        ["stage 0 › RUN"]
    );
    put(
        &fx,
        "bad-docker",
        b"FROM alpine AS build\nRUN <<END\nneedle\nFROM fake needle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "dockerfile",
            "bad-docker",
            "needle",
            &["--json"]
        ))),
        ["build › RUN", "build › RUN"]
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "dockerfile",
            "config/m6.dockerfile",
            "build",
            &["--json", "-m1"]
        ))),
        [""]
    );
}

/// Each Format's committed broken fixture, with its three snapshots (plan, «Committed broken
/// fixtures, with three snapshots each»).
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["dockerfile"];
    #[allow(clippy::type_complexity)]
    let cases: &[(&str, &str, &[u8], &[&str])] = &[(
        "dockerfile",
        "config/broken-m6.dockerfile",
        b"FROM alpine AS build\nRUN <<END\nneedle\nFROM needle\n",
        &["build › RUN", "build › RUN"],
    )];
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

/// A generated name (`stage 1`) has no Source piece, so `--in-headings` never finds it (plan,
/// «Meaningful generated-name exclusion and encoding tests»).
#[test]
fn m6_real_generated_names() {
    let fx = Fixture::new();
    let owned: &[&str] = &["dockerfile"];
    let cases: &[(&str, &[u8], &str)] =
        &[("dockerfile", b"FROM alpine\nRUN needle\n", "^stage [0-9]+$")];
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
