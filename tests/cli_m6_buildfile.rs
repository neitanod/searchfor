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

#[test]
fn m6_make() {
    let fx = Fixture::new();
    let src = b"ifeq ($(OS),Windows_NT)\nrelease test: deps\n\t@echo needle\n\n# within recipe gap\n\techo needle again\nelse\nNAME := needle\nendif\ndefine BODY\nneedle body\nendef\n.RECIPEPREFIX = >\n%.o: %.c\n>echo needle pattern\n";
    seed(&fx, "config/m6.mk", src);
    check(
        &fx,
        "make",
        "config/m6.mk",
        &[
            "ifeq ($(OS),Windows_NT) › release test",
            "ifeq ($(OS),Windows_NT) › release test",
            "ifeq ($(OS),Windows_NT) › NAME",
            "BODY",
            "%.o",
        ],
    );
    flags(&fx, "make", "config/m6.mk", "release test", "release", "");
    put(
        &fx,
        "make-lex",
        b"OBJ := $(OBJ:.o=.c)\nKEY += needle \\\n more\nfoo:: deps\n\techo needle\nendif\nendef\nneedle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "make",
            "make-lex",
            "needle",
            &["--json"]
        ))),
        ["KEY", "foo", ""]
    );
    for operator in ["=", ":=", "::=", "?=", "+=", "!="] {
        let src = format!("override export NAME {operator} needle\n");
        put(&fx, "operators.mk", src.as_bytes());
        assert_eq!(
            paths(&matches(&output(
                &fx,
                "make",
                "operators.mk",
                "needle",
                &["--json"]
            ))),
            ["NAME"]
        );
    }
}

#[test]
fn m6_just() {
    let fx = Fixture::new();
    let src = b"# needle documentation\n[linux]\n@deploy host='prod': build\n    rsync -a build/ host:/srv/needle/\n\nURL := 'needle'\nalias ship := deploy # needle\n";
    seed(&fx, "config/m6.just", src);
    check(
        &fx,
        "just",
        "config/m6.just",
        &["", "deploy", "URL", "ship"],
    );
    flags(&fx, "just", "config/m6.just", "deploy", "deploy", "");
    put(
        &fx,
        "bad.just",
        b"run:\n    echo needle\nstray line needle\nother:\n    needle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "just",
            "bad.just",
            "needle",
            &["--json"]
        ))),
        ["run", "", "other"]
    );
    let own = matches(&output(
        &fx,
        "just",
        "config/m6.just",
        "@deploy",
        &["--json"],
    ));
    assert_eq!(paths(&own), [""]);
}

/// Each Format's committed broken fixture, with its three snapshots (plan, «Committed broken
/// fixtures, with three snapshots each»).
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["dockerfile", "make", "just"];
    #[allow(clippy::type_complexity)]
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "dockerfile",
            "config/broken-m6.dockerfile",
            b"FROM alpine AS build\nRUN <<END\nneedle\nFROM needle\n",
            &["build › RUN", "build › RUN"],
        ),
        (
            "make",
            "config/broken-m6.mk",
            b"a:\n\techo needle\nendif\nendef\nneedle\n",
            &["a", ""],
        ),
        (
            "just",
            "config/broken-m6.just",
            b"a:\n    needle\nstray needle\n",
            &["a", ""],
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

/// UTF-16 and a BOM read as UTF-8, and a lone CR breaks no line (plan, «Meaningful
/// generated-name exclusion and encoding tests»): make is this topic's representative.
#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    let (format, src) = ("make", "KEY=needle\n");
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["KEY"]);
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
    // A lone CR before the needle, in the value: the same line and the same Trail.
    put(&fx, "cr.fixture", b"KEY=\rneedle\n");
    let rows = matches(&output(&fx, format, "cr.fixture", "needle", &["--json"]));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 1);
}

/// Codex r1 #1: 300 000 nested variable references, which recursed once each, overflowed the
/// stack; the walk is iterative, and the rule after them is still read.
#[test]
fn m6_make_deep_references() {
    let fx = Fixture::new();
    let n = 300_000;
    let src = format!("{}x{}:\n\techo needle\n", "$(".repeat(n), ")".repeat(n));
    put(&fx, "deep-references.mk", src.as_bytes());
    let out = output(&fx, "make", "deep-references.mk", "needle", &["--json"]);
    let rows = matches(&out);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["trail"][0]["kind"], "target");
}

/// Codex r1 #6 = Fable r1 #8: a heredoc opens only where a word of the line starts with `<<`,
/// as buildkit splits them, quotes kept; a `<<` inside a quoted word or inside another word
/// (`$((1<<3))`) opens none, so the next instructions and stages are their own.
#[test]
fn m6_dockerfile_heredocs_start_words() {
    let fx = Fixture::new();
    for (src, want) in [
        (
            "FROM alpine AS build\nRUN echo \"<<EOF\"\nFROM alpine AS final\nRUN echo needle\n",
            vec!["final › RUN"],
        ),
        (
            "FROM alpine\nRUN echo \"<<EOF\" && echo needle1\nRUN echo needle2\nENV x=needle3\n",
            vec!["stage 0 › RUN", "stage 0 › RUN", "stage 0 › ENV"],
        ),
        (
            "FROM alpine\nRUN x=$((1<<3)) && echo needle1\nRUN echo needle2\n",
            vec!["stage 0 › RUN", "stage 0 › RUN"],
        ),
        (
            "FROM alpine\nRUN echo 'a <<EOF' \\\n && cat <<E\nneedle1\nE\nENV needle2=1\n",
            vec!["stage 0 › RUN", "stage 0 › ENV"],
        ),
        (
            "FROM alpine\nRUN echo a\\ <<EOF\nRUN echo needle1\n",
            vec!["stage 0 › RUN"],
        ),
        (
            "FROM alpine\nRUN cat <<EOF >x\nRUN needle1\nEOF\nENV needle2=1\n",
            vec!["stage 0 › RUN", "stage 0 › ENV"],
        ),
        // Codex r2 #2 = Fable r2 #7: the quote goes on over the continuation line, so its
        // `<<EOF` is text (`docker-h-11.dockerfile`).
        (
            "FROM alpine AS build\nRUN echo \"before \\\n <<EOF\"\nFROM alpine AS final\nRUN echo needle\n",
            vec!["final › RUN"],
        ),
        (
            "FROM a\nRUN echo \"abc \\\n<<EOF\"\nRUN echo needle2\nENV x=needle3\n",
            vec!["stage 0 › RUN", "stage 0 › ENV"],
        ),
    ] {
        put(&fx, "heredoc.dockerfile", src.as_bytes());
        let rows = matches(&output(
            &fx,
            "dockerfile",
            "heredoc.dockerfile",
            "needle",
            &["--json"],
        ));
        assert_eq!(paths(&rows), want, "{src}");
    }
}

/// Fable r1 #12: past 256 open Segments no more open (M6 spec §3, point 8), in a Makefile's
/// conditionals too; each `endif` still closes its own `ifeq`.
#[test]
fn m6_make_depth_stops_at_256() {
    let fx = Fixture::new();
    let n = 300;
    let src = format!(
        "{}K := needle\n{}a:\n\techo needle\n",
        "ifeq (a,b)\n".repeat(n),
        "endif\n".repeat(n)
    );
    put(&fx, "deep.mk", src.as_bytes());
    let rows = matches(&output(&fx, "make", "deep.mk", "needle", &["--json"]));
    let lens: Vec<usize> = rows
        .iter()
        .map(|r| r["trail"].as_array().unwrap().len())
        .collect();
    assert_eq!(lens, [256, 1]);
    assert_eq!(rows[1]["trail"][0]["kind"], "target");
}

/// Fable r1 #13, orchestrator decision 3: a rule's label is its targets and a recipe's its
/// name, `@` included; prerequisites, parameters, dependencies and attributes are inside the
/// Segment, as a function's signature is inside its function.
#[test]
fn m6_rule_and_recipe_labels_are_their_names() {
    let fx = Fixture::new();
    put(
        &fx,
        "labels.mk",
        b"a b: deps needle\n\techo x\nneedle-t: x\n",
    );
    let rows = matches(&output(&fx, "make", "labels.mk", "needle", &["--json"]));
    assert_eq!(paths(&rows), ["a b", ""]);
    let within = matches(&output(
        &fx,
        "make",
        "labels.mk",
        "needle",
        &["--json", "--within", "a b"],
    ));
    assert_eq!(paths(&within), ["a b"]);
    put(
        &fx,
        "labels.just",
        b"[group(\"needle-grp\")]\nbuild target=\"x:y\" +args: (dep \"needle\") needle-dep\n    echo\n@needle-r:\n    echo\n",
    );
    let rows = matches(&output(&fx, "just", "labels.just", "needle", &["--json"]));
    assert_eq!(paths(&rows), ["build", "build", ""]);
    let within = matches(&output(
        &fx,
        "just",
        "labels.just",
        "needle-dep",
        &["--json", "--within", "build"],
    ));
    assert_eq!(paths(&within), ["build"]);
}

/// Fable r1 #20: `define` nests, as GNU make 4.4.1 reads it (checked: the outer variable's
/// value holds the inner `define` … `endef`), so the `endef` that matches closes the key.
/// `override define` inside a body opens none.
#[test]
fn m6_make_defines_nest() {
    let fx = Fixture::new();
    put(
        &fx,
        "define.mk",
        b"define needle-def\n\tdefine inner\n\tendef\nneedle-body\nendef\nK := needle\ndefine D\noverride define E\nendef\nneedle-after\nendef\n",
    );
    let rows = matches(&output(&fx, "make", "define.mk", "needle", &["--json"]));
    assert_eq!(paths(&rows), ["", "needle-def", "K", ""]);
}
