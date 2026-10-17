//! End-to-end tests of M2's Formats (plan, Chunk 10): fixtures under tests/fixtures/{data,code}.
mod common;

use common::{Fixture, code, run, stderr, stdout};

const FORMATS: &[(&str, &str)] = &[
    ("json", "data/config.json"),
    ("yaml", "data/config.yaml"),
    ("toml", "data/shop.toml"),
    ("php", "code/cart.php"),
    ("javascript", "code/widget.js"),
    ("typescript", "code/shape.ts"),
    ("tsx", "code/view.tsx"),
    ("bash", "code/deploy.sh"),
    ("go", "code/server.go"),
    ("rust", "code/thing.rs"),
    ("python", "code/outer.py"),
];

#[test]
fn m2_01_every_format_in_three_layouts() {
    // Spec §11 #2: a fixture and a snapshot of tree, one-line and JSON output per Format.
    let fx = Fixture::new();
    for (name, file) in FORMATS {
        for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
            let o = run(&mut fx.sfor(["needle", file, flag]));
            assert_eq!(code(&o), 0, "{name} {layout}: {}", stderr(&o));
            assert!(stderr(&o).is_empty(), "{name} {layout}: {}", stderr(&o));
            let mut out = stdout(&o);
            if layout == "json" {
                out = common::mask_json_times(&out);
            }
            insta::assert_snapshot!(format!("m2_01_{name}_{layout}"), out);
        }
    }
}

#[test]
fn m2_02_yaml_key_path() {
    // Spec §11 #4.
    let fx = Fixture::new();
    let o = run(&mut fx.sfor(["needle.example", "data/config.yaml", "--flat"]));
    assert_eq!(
        stdout(&o),
        "9: server › ports[2] › host │     - host: needle.example\n"
    );
}

#[test]
fn m2_03_php_method_and_signatures() {
    // Spec §11 #5.
    let fx = Fixture::new();
    let flat = stdout(&run(&mut fx.sfor([
        "needle: merge",
        "code/cart.php",
        "--flat",
    ])));
    assert_eq!(
        flat,
        "10: Cart › addItem │         // needle: merge equal items\n"
    );
    let tree = stdout(&run(&mut fx.sfor([
        "needle: merge",
        "code/cart.php",
        "--tree",
    ])));
    assert!(tree.contains("class Cart\n"), "{tree}");
    assert!(
        tree.contains("public function addItem(Item $item, int $qty = 1): void\n"),
        "{tree}"
    );
}

#[test]
fn m2_04_within_and_expand_on_code() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor([
        "needle",
        "code",
        "--within",
        "Cart › addItem",
        "--flat",
    ])));
    assert_eq!(o.lines().count(), 1, "{o}");
    assert!(o.starts_with("code/cart.php:10:"), "{o}");
    let o = stdout(&run(&mut fx.sfor([
        "self.needle",
        "code/thing.rs",
        "--expand",
        "--flat",
    ])));
    // The Section of `helper`: lines 7 to 9.
    assert!(o.contains("pub fn helper(&self) -> u32"), "{o}");
    assert!(o.contains("self.needle + 1"), "{o}");
    assert!(!o.contains("pub trait Greeter"), "{o}");
}

#[test]
fn m2_05_in_headings_on_names_and_keys() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor([
        "--in-headings",
        "^addItem$",
        "code",
        "--flat",
    ])));
    assert_eq!(
        o,
        "code/cart.php:8: Cart │     public function addItem(Item $item, int $qty = 1): void\n"
    );
    let o = stdout(&run(&mut fx.sfor([
        "--in-headings",
        "^host$",
        "data/config.yaml",
        "--flat",
    ])));
    assert_eq!(o.lines().count(), 3, "{o}");
    // D35: keys that hold arrays are names too, in the three syntaxes, and --expand prints
    // their Section (review r1, Codex #11, Fable #3).
    for (file, key, line) in [
        ("data/config.json", "^ports$", 5),
        ("data/config.yaml", "^ports$", 4),
        ("data/shop.toml", "^tags$", 13),
    ] {
        let o = stdout(&run(&mut fx.sfor(["--in-headings", key, file, "--flat"])));
        assert_eq!(o.lines().count(), 1, "{file}: {o}");
        assert!(o.starts_with(&format!("{line}:")), "{file}: {o}");
    }
    let o = stdout(&run(&mut fx.sfor([
        "--in-headings",
        "^ports$",
        "data/config.yaml",
        "--expand",
        "--flat",
    ])));
    assert!(o.contains("host: needle.example"), "{o}");
}

#[test]
fn m2_06_content_signals_only_without_extension() {
    // D15: a shebang decides an extension-less script, never a `.txt`.
    let fx = Fixture::new();
    std::fs::write(
        fx.file("deploy"),
        "#!/usr/bin/env bash\nrun() {\n  echo needle\n}\n",
    )
    .unwrap();
    std::fs::write(
        fx.file("notes.txt"),
        "#!/usr/bin/env bash\nrun() {\n  echo needle\n}\n",
    )
    .unwrap();
    assert_eq!(
        stdout(&run(&mut fx.sfor(["needle", "deploy", "--flat"]))),
        "3: run │   echo needle\n"
    );
    let plain = stdout(&run(&mut fx.sfor(["needle", "notes.txt", "--flat"])));
    assert_eq!(plain.lines().count(), 1, "{plain}");
    assert!(!plain.contains('›'), "notes.txt must be Plain: {plain}");
}

#[test]
fn m2_07_list_and_export_the_new_formats() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor(["--list-formats"])));
    for (name, _) in FORMATS {
        assert!(
            o.lines()
                .any(|l| l.starts_with(&format!("{name} ")) && l.contains("built-in")),
            "{name}: {o}"
        );
    }
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("t");
    assert_eq!(
        code(&run(&mut fx.sfor([
            std::ffi::OsStr::new("--export-formats"),
            target.as_os_str()
        ]))),
        0
    );
    assert_eq!(
        std::fs::read_dir(&target).unwrap().count(),
        1 + FORMATS.len()
    );
}

#[test]
fn m2_08_list_grammars_offline() {
    let fx = Fixture::new();
    let o = run(&mut fx.sfor(["--list-grammars"]));
    assert_eq!(code(&o), 0);
    assert_eq!(
        stdout(&o)
            .lines()
            .filter(|l| l.contains("compiled in"))
            .count(),
        8,
        "{}",
        stdout(&o)
    );
    assert!(
        stderr(&o).contains("Grammar index is not available"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn m2_09_utf16_and_bom_code_keeps_its_offsets() {
    let fx = Fixture::new();
    let src = std::fs::read_to_string(fx.file("code/cart.php")).unwrap();
    let mut bom = b"\xEF\xBB\xBF".to_vec();
    bom.extend_from_slice(src.as_bytes());
    std::fs::write(fx.file("bom.php"), bom).unwrap();
    let mut utf16 = b"\xFF\xFE".to_vec();
    utf16.extend(src.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(fx.file("utf16.php"), utf16).unwrap();
    for f in ["bom.php", "utf16.php"] {
        let o = stdout(&run(&mut fx.sfor(["needle: merge", f, "--flat"])));
        assert!(o.starts_with("10: Cart › addItem │"), "{f}: {o}");
    }
}

#[test]
fn m2_10_a_missing_grammar_is_one_warning() {
    let fx = Fixture::new();
    std::fs::write(fx.formats.path().join("lua.toml"), "name='lua'\nextensions=['lua']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'\n").unwrap();
    std::fs::write(fx.file("a.lua"), "-- needle\n").unwrap();
    std::fs::write(fx.file("b.lua"), "-- needle\n").unwrap();
    // Two files are searched in parallel, as rg: -j1 fixes their order for the comparison.
    let o = run(&mut fx.sfor(["needle", "a.lua", "b.lua", "--flat", "-j1"]));
    assert_eq!(code(&o), 0);
    assert_eq!(stderr(&o).lines().count(), 1, "{}", stderr(&o));
    // Plain exactly as a file no Definition claims (D33): the same bytes as a `.txt` copy.
    std::fs::copy(fx.file("a.lua"), fx.file("a.txt")).unwrap();
    std::fs::copy(fx.file("b.lua"), fx.file("b.txt")).unwrap();
    let plain = stdout(&run(
        &mut fx.sfor(["needle", "a.txt", "b.txt", "--flat", "-j1"])
    ));
    assert_eq!(stdout(&o).replace(".lua", ".txt"), plain);
    let json = stdout(&run(&mut fx.sfor(["needle", "a.lua", "--json"])));
    let m: serde_json::Value = json
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["type"] == "match")
        .unwrap();
    assert!(m["data"]["format"].is_null(), "{m}");
    let list = stdout(&run(&mut fx.sfor(["--list-formats"])));
    assert!(
        list.lines()
            .any(|l| l.starts_with("lua ") && l.ends_with("(unavailable)")),
        "{list}"
    );
}

#[test]
fn m2_12_deep_json_is_not_a_crash() {
    // D34, Codex #8: a million nested arrays, in a separate process: no stack overflow.
    let fx = Fixture::new();
    let n = 1_000_000;
    let mut s = "[".repeat(n);
    s.push_str("\"needle\"");
    s.push_str(&"]".repeat(n));
    std::fs::write(fx.file("deep.json"), s).unwrap();
    let o = run(&mut fx.sfor(["needle", "deep.json", "--flat", "--fragment-width", "60"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stdout(&o).lines().count(), 1);
}

#[test]
fn m2_13_files_without_matches_cost_nothing() {
    // Design note 2, Codex #5: a missing Grammar and an `external` helper whose command does not
    // exist give no warning when none of their files has a Match.
    let fx = Fixture::new();
    std::fs::write(fx.formats.path().join("lua.toml"), "name='lua'\nextensions=['lua']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'\n").unwrap();
    std::fs::write(fx.formats.path().join("ini.toml"), "name='ini'\nextensions=['ini']\nprocessor='external'\n[params]\ncommand=['/no/such/helper']\n").unwrap();
    std::fs::write(fx.file("a.lua"), "-- nothing\n").unwrap();
    std::fs::write(fx.file("a.ini"), "[s]\nk = v\n").unwrap();
    let o = run(&mut fx.sfor(["zzqq-absent", "."]));
    assert_eq!(code(&o), 1);
    assert_eq!(stderr(&o), "");
}

#[test]
fn m2_11_json_carries_signatures() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor([
        "needle: merge",
        "code/cart.php",
        "--json",
    ])));
    let m: serde_json::Value = o
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["type"] == "match")
        .unwrap();
    assert_eq!(m["data"]["format"], "php");
    assert_eq!(
        m["data"]["trail"][1]["signature"],
        "public function addItem(Item $item, int $qty = 1): void"
    );
    common::assert_valid_json_schema(&o);
}

#[test]
fn m2_14_a_quoted_key_with_escapes_is_one_segment() {
    // Review m2-code-r2, Codex #1: `"abc"` printed the Trail `a › c`.
    let fx = Fixture::new();
    for ext in ["js", "ts", "tsx"] {
        let file = format!("keys.{ext}");
        std::fs::write(
            fx.file(&file),
            "const o = { \"a\\u0062c\": () => \"needle\", \"\": () => 'needle' };\n",
        )
        .unwrap();
        let o = run(&mut fx.sfor(["needle", file.as_str(), "--flat"]));
        assert_eq!(code(&o), 0, "{}", stderr(&o));
        let lines: Vec<String> = stdout(&o).lines().map(String::from).collect();
        assert_eq!(lines.len(), 2, "{ext}: {lines:?}");
        assert!(lines[0].starts_with("1: abc │ "), "{ext}: {lines:?}");
        let o = stdout(&run(&mut fx.sfor([
            "--in-headings",
            "^abc$",
            file.as_str(),
            "--json",
        ])));
        let m: serde_json::Value = o
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .find(|v| v["type"] == "match")
            .unwrap_or_else(|| panic!("{ext}: no match in {o}"));
        // One submatch per piece, as Markdown's entities: `a`, the escape, `c`.
        let subs: Vec<(u64, u64)> = m["data"]["submatches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| (s["start"].as_u64().unwrap(), s["end"].as_u64().unwrap()))
            .collect();
        assert_eq!(subs, [(13, 14), (14, 20), (20, 21)], "{ext}: {o}");
    }
}

#[test]
fn m2_15_a_block_scalar_key_stays_on_one_line() {
    // Review m2-code-r2, Fable #1: `? |` kept its line breaks in the Segment name.
    let fx = Fixture::new();
    std::fs::write(
        fx.file("k.yaml"),
        "a: 1\n? |\n  ñ multi\n  line\n: needle\n",
    )
    .unwrap();
    let o = run(&mut fx.sfor(["needle", "k.yaml", "--flat"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stdout(&o), "5: ñ multi line │ : needle\n");
    let o = stdout(&run(&mut fx.sfor(["needle", "k.yaml", "--json"])));
    let m: serde_json::Value = o
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["type"] == "match")
        .unwrap();
    assert_eq!(m["data"]["trail"][0]["name"], "ñ multi line", "{o}");
    // A Match in the name maps to the line it is on.
    let o = stdout(&run(&mut fx.sfor([
        "--in-headings",
        "multi line",
        "k.yaml",
        "--flat",
    ])));
    assert_eq!(o.lines().count(), 2, "{o}");
    assert!(o.starts_with("3: "), "{o}");
    assert!(o.contains("\n4: "), "{o}");
}

#[test]
fn m2_16_a_function_value_signature_ends_at_its_body() {
    // Review m2-code-r2, Fable #2: `f = () => needle` showed the whole line as its signature.
    let fx = Fixture::new();
    std::fs::write(
        fx.file("sig.js"),
        "const o = { n: function () { return 'needle'; } };\nconst f = () => needle;\n",
    )
    .unwrap();
    let o = run(&mut fx.sfor(["needle", "sig.js", "--tree"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let tree = stdout(&o);
    assert!(tree.contains("  n: function ()\n"), "{tree}");
    assert!(tree.contains("  f = () =>\n"), "{tree}");
    let o = stdout(&run(&mut fx.sfor(["needle", "sig.js", "--json"])));
    let sigs: Vec<String> = o
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|v| v["type"] == "match")
        .map(|v| {
            v["data"]["trail"][0]["signature"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(sigs, ["n: function ()", "f = () =>"], "{o}");
}
