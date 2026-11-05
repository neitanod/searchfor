//! M6 text documents (spec §6.1–§6.4): AsciiDoc, reStructuredText, Org-mode and LaTeX
//! headings, blocks, columns and environments (plan, Chunk 5).
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
fn m6_adoc_rst() {
    let fx = Fixture::new();
    let adoc = b"= Deploy guide\n== Install\n=== Linux\n[source,bash]\n----\napt install needle-pkg\n== needle literal heading\n----\n";
    seed(&fx, "docs-a/m6.adoc", adoc);
    check(
        &fx,
        "asciidoc",
        "docs-a/m6.adoc",
        &[
            "Deploy guide › Install › Linux › bash",
            "Deploy guide › Install › Linux › bash",
        ],
    );
    flags(&fx, "asciidoc", "docs-a/m6.adoc", "Linux", "Install", "");
    let rst = b"Install\n=======\n\nLinux\n-----\n\n.. code-block:: bash\n   :linenos:\n\n   apt install needle-pkg\n\n.. note::\n\n   needle note\n";
    seed(&fx, "docs-a/m6.rst", rst);
    check(
        &fx,
        "rst",
        "docs-a/m6.rst",
        &["Install › Linux › bash", "Install › Linux › note"],
    );
    flags(&fx, "rst", "docs-a/m6.rst", "Linux", "Linux", "");
    put(&fx, "bad.adoc", b"= Guide\n== A\n....\n== needle\nneedle\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "asciidoc",
            "bad.adoc",
            "needle",
            &["--json"]
        ))),
        ["Guide › A › literal", "Guide › A › literal"]
    );
    put(
        &fx,
        "bad.rst",
        b"Title\n=====\n\nText::\n\n   needle\n\nnext needle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "rst",
            "bad.rst",
            "needle",
            &["--json"]
        ))),
        ["Title › literal", "Title"]
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "asciidoc",
            "docs-a/m6.adoc",
            "Linux",
            &["--json"]
        ))),
        ["Deploy guide › Install"]
    );
}

#[test]
fn m6_org() {
    let fx = Fixture::new();
    let src = b"#+TITLE: Budget\n* TODO Costs :work:\n| Item | Price |\n|------+-------|\n| Hosting | needle 9 |\n#+BEGIN_SRC bash\n* needle text\n#+END_SRC\n#+BEGIN_EXAMPLE\nneedle example\n#+END_EXAMPLE\n";
    seed(&fx, "docs-a/m6.org", src);
    check(
        &fx,
        "org",
        "docs-a/m6.org",
        &[
            "Budget › TODO Costs :work: › Price",
            "Budget › TODO Costs :work: › bash",
            "Budget › TODO Costs :work: › example",
        ],
    );
    flags(&fx, "org", "docs-a/m6.org", "Costs", "TODO", "");
    let rows = matches(&output(
        &fx,
        "org",
        "docs-a/m6.org",
        "needle",
        &["--json", "--expand"],
    ));
    assert_eq!(rows[0]["section"]["start_line"], 3);
    assert_eq!(rows[0]["section"]["end_line"], 5);
    put(
        &fx,
        "deep.org",
        format!("{} Deep\nneedle\n", "*".repeat(300)).as_bytes(),
    );
    let deep = matches(&output(&fx, "org", "deep.org", "needle", &["--json"]));
    assert_eq!(deep[0]["trail"][0]["level"], 300);
    put(&fx, "bad.org", b"* A\n#+begin_src\n* needle\nneedle\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "org",
            "bad.org",
            "needle",
            &["--json"]
        ))),
        ["A › src", "A › src"]
    );
    put(&fx, "no-header.org", b"* A\n| plain | needle |\n");
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "org",
            "no-header.org",
            "needle",
            &["--json"]
        ))),
        ["A"]
    );
}

#[test]
fn m6_latex() {
    let fx = Fixture::new();
    let src = br#"\title{A Study of Needles}
\begin{document}
\section{Methods}
\subsection[Short]{Sampling}
\begin{figure}
\caption{needle distribution}
\end{figure}
\begin{minted}{bash}
\section{needle text}
\end{minted}
\end{document}
"#;
    seed(&fx, "docs-a/m6.tex", src);
    check(
        &fx,
        "latex",
        "docs-a/m6.tex",
        &[
            "A Study of Needles › Methods › Sampling › figure",
            "A Study of Needles › Methods › Sampling › bash",
        ],
    );
    flags(&fx, "latex", "docs-a/m6.tex", "Sampling", "Sampling", "");
    put(
        &fx,
        "tex-edge",
        br#"\section*{One {balanced}
 title}
\begin{figure}
\subsection{Inner}
needle
\end{figure}
needle
% \section{ignored}
\% needle
\end{never}
"#,
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "latex",
            "tex-edge",
            "needle",
            &["--json"]
        ))),
        [
            "One {balanced} title › figure › Inner",
            "One {balanced} title",
            "One {balanced} title"
        ]
    );
    put(
        &fx,
        "bad.tex",
        br#"\section{A}
\begin{verbatim}
\section{needle}
needle
"#,
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "latex",
            "bad.tex",
            "needle",
            &["--json"]
        ))),
        ["A › verbatim", "A › verbatim"]
    );
}

/// Each Format's committed broken fixture, with its three snapshots (plan, «Committed broken
/// fixtures, with three snapshots each»).
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["asciidoc", "rst", "org", "latex"];
    #[allow(clippy::type_complexity)]
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "asciidoc",
            "docs-a/broken-m6.adoc",
            b"= T\n== A\n....\nneedle\n== needle\n",
            &["T › A › literal", "T › A › literal"],
        ),
        (
            "rst",
            "docs-a/broken-m6.rst",
            b"A\n=\n\n.. code:: bash\n\n   needle\n",
            &["A › bash"],
        ),
        (
            "org",
            "docs-a/broken-m6.org",
            b"* A\n#+BEGIN_SRC\nneedle\n* needle\n",
            &["A › src", "A › src"],
        ),
        (
            "latex",
            "docs-a/broken-m6.tex",
            b"\\section{A}\n\\begin{verbatim}\nneedle\n\\section{needle}\n",
            &["A › verbatim", "A › verbatim"],
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
/// generated-name exclusion and encoding tests»): AsciiDoc is this topic's representative.
#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    let (format, src) = ("asciidoc", "= Café\n== A\nneedle\n");
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["Café › A"]);
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
    // A lone CR before the needle, in the content: the same line and the same Trail.
    put(&fx, "cr.fixture", "= Café\n== A\n\rneedle\n".as_bytes());
    let rows = matches(&output(&fx, format, "cr.fixture", "needle", &["--json"]));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 3);
}
