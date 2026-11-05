//! End-to-end tests of M6's markup Formats (M6 plan, Chunk 7): Vue, Svelte and Blade
//! components, Telegram's HTML export and HTML's scripts, each under its explicit `--format`.
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
fn m6_vue() {
    let fx = Fixture::new();
    let src = b"<template>\n<ul class=\"items\"><li>x</li><li>needle template</li></ul>\n</template>\n<script lang=\"ts\">\nfunction addItem() {\n  const value: string = 'needle';\n}\n</script>\n<style>.needle { color:red }</style>\n";
    seed(&fx, "code-c/m6.vue", src);
    check(
        &fx,
        "vue",
        "code-c/m6.vue",
        &["template › ul.items › li[2]", "script › addItem", "style"],
    );
    flags(&fx, "vue", "code-c/m6.vue", "addItem", "addItem", "\\[2\\]");
    let expanded = matches(&output(
        &fx,
        "vue",
        "code-c/m6.vue",
        "needle",
        &["--json", "--within", "addItem", "--expand"],
    ));
    assert_eq!(expanded[0]["section"]["start_line"], 5);
    assert_eq!(expanded[0]["section"]["end_line"], 7);
    // HTML's scripts are still off here; Task 7.4 turns them on and changes this
    // expectation to `script › addItem` (the `lang="ts"` picks TypeScript there too).
    put(&fx, "same.html", src);
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "html",
            "same.html",
            "needle",
            &["--json"]
        ))),
        ["template › ul.items › li[2]", "script", "style"]
    );
    put(
        &fx,
        "unknown.vue",
        b"<script lang='coffee'>\nneedle\n</script>\n<script>function f(){\nneedle\n}</script>\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "vue",
            "unknown.vue",
            "needle",
            &["--json"]
        ))),
        ["script[1]", "script[2] › f"]
    );
    // A user copy of the TypeScript Definition, to which the test adds `max_bytes`
    // (the built-in has none), applies to this script range alone. toml 1.1.8's
    // `IndexMut` panics on a missing key, so the key is inserted.
    let mut definition: toml::Table =
        toml::from_str(include_str!("../formats/typescript.toml")).unwrap();
    definition
        .get_mut("params")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .insert("max_bytes".into(), toml::Value::Integer(1));
    std::fs::write(
        fx.formats.path().join("typescript.toml"),
        toml::to_string(&definition).unwrap(),
    )
    .unwrap();
    let capped = output(&fx, "vue", "code-c/m6.vue", "needle", &["--json"]);
    assert_eq!(
        paths(&matches(&capped)),
        ["template › ul.items › li[2]", "script", "style"]
    );
}

#[test]
fn m6_templates() {
    let fx = Fixture::new();
    let svelte = b"<ul>\n{#each items as item}\n<li>{item.needle}</li>\n{:else}\n<li>needle empty</li>\n{/each}\n</ul>\n";
    seed(&fx, "code-c/m6.svelte", svelte);
    check(
        &fx,
        "svelte",
        "code-c/m6.svelte",
        &[
            "ul › #each items as item › li[1]",
            "ul › #each items as item › li[2]",
        ],
    );
    flags(&fx, "svelte", "code-c/m6.svelte", "items", "items", "");
    let blade = b"@section('content')\n<div class=\"cart\">\n@foreach($items as $item)\n<li>{{ $item->needle }}</li>\n@endforeach\n</div>\n@endsection\n";
    seed(&fx, "code-c/m6.blade.php", blade);
    check(
        &fx,
        "blade",
        "code-c/m6.blade.php",
        &["@section('content') › div.cart › @foreach($items as $item) › li"],
    );
    flags(&fx, "blade", "code-c/m6.blade.php", "items", "items", "");
    put(
        &fx,
        "blade-inline",
        b"@section('x','needle')\n{{-- @if(fake) needle --}}\n@verbatim\n@if(fake) needle\n@endverbatim\nneedle\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "blade",
            "blade-inline",
            "needle",
            &["--json"]
        ))),
        ["", "", "@verbatim", ""]
    );
    put(
        &fx,
        "svelte-close",
        b"{#if ready}\n<div>needle\n{/if}\nneedle\n{/missing}\n",
    );
    assert_eq!(
        paths(&matches(&output(
            &fx,
            "svelte",
            "svelte-close",
            "needle",
            &["--json"]
        ))),
        ["#if ready › div", ""]
    );
}

/// M6 plan, «Definition-kind and template keyword completeness»: every Svelte block and
/// Blade paired directive opens a block its closer closes.
#[test]
fn m6_template_keyword_pairs() {
    let fx = Fixture::new();
    for name in ["if", "each", "await", "key", "snippet"] {
        let src = format!("{{#{name} x}}\n<p>needle</p>\n{{/{name}}}\nneedle\n");
        put(&fx, "all.svelte", src.as_bytes());
        assert_eq!(
            paths(&matches(&output(
                &fx,
                "svelte",
                "all.svelte",
                "needle",
                &["--json"]
            ))),
            [format!("#{name} x › p"), String::new()]
        );
    }
    for name in [
        "section",
        "if",
        "foreach",
        "forelse",
        "for",
        "while",
        "switch",
        "push",
        "component",
        "slot",
        "php",
        "once",
        "auth",
        "guest",
        "isset",
        "empty",
        "unless",
        "verbatim",
    ] {
        let argument = if ["php", "once", "auth", "guest", "verbatim"].contains(&name) {
            ""
        } else {
            "('x')"
        };
        let src = format!("@{name}{argument}\nneedle\n@end{name}\nneedle\n");
        put(&fx, "all.blade.php", src.as_bytes());
        assert_eq!(
            paths(&matches(&output(
                &fx,
                "blade",
                "all.blade.php",
                "needle",
                &["--json"]
            ))),
            [format!("@{name}{argument}"), String::new()]
        );
    }
    for closer in ["show", "stop", "overwrite"] {
        let src = format!("@section('x')\nneedle\n@{closer}\nneedle\n");
        put(&fx, "section.blade.php", src.as_bytes());
        assert_eq!(
            paths(&matches(&output(
                &fx,
                "blade",
                "section.blade.php",
                "needle",
                &["--json"]
            ))),
            ["@section('x')", ""]
        );
    }
}

/// M6 plan, «Meaningful generated-name exclusion and encoding tests», markup's row: Vue's
/// `[2]` is a word s4 adds, so `--in-headings` never finds it.
#[test]
fn m6_real_generated_names() {
    let fx = Fixture::new();
    let owned: &[&str] = &["vue"];
    let cases: &[(&str, &[u8], &str)] =
        &[("vue", b"<ul><li>x</li><li>needle</li></ul>", "\\[2\\]")];
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

/// M6 plan, «Meaningful generated-name exclusion and encoding tests», markup's representative:
/// Vue. UTF-16 and a BOM give the Trails of the UTF-8 file, and a lone CR is no line break.
#[test]
fn m6_encoding_and_lone_cr() {
    let fx = Fixture::new();
    let (format, src) = ("vue", "<main>needle</main>\n");
    put(&fx, "encoding.fixture", src.as_bytes());
    let lf = paths(&matches(&output(
        &fx,
        format,
        "encoding.fixture",
        "needle",
        &["--json"],
    )));
    assert_eq!(lf, ["main"]);
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
    put(&fx, "cr.fixture", b"<main>\rneedle</main>\n");
    let rows = matches(&output(&fx, format, "cr.fixture", "needle", &["--json"]));
    assert_eq!(paths(&rows), lf);
    assert_eq!(rows[0]["line_number"], 1);
}

/// M6 plan, «Committed broken fixtures, with three snapshots each», markup's rows.
#[test]
fn m6_broken_fixtures() {
    let owned: &[&str] = &["vue", "svelte", "blade"];
    let cases: &[(&str, &str, &[u8], &[&str])] = &[
        (
            "vue",
            "code-c/broken-m6.vue",
            b"<script lang='coffee'>\nneedle\n",
            &["script"],
        ),
        (
            "svelte",
            "code-c/broken-m6.svelte",
            b"{#if x}\n<div>needle\n",
            &["#if x › div"],
        ),
        (
            "blade",
            "code-c/broken-m6.blade.php",
            b"@if($x)\n<div>needle\n",
            &["@if($x) › div"],
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
