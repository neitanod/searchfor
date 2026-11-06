//! The M6 binary layouts of the `object` Processor (M6 spec §9): JPEG, PNG, WebP, TIFF, ID3,
//! MP4, WebAssembly and Java classes, over files the tests write (`object::fixture::m6`).
mod common;
use common::{Fixture, code, run, stderr, stdout};
use searchfor::processors::object::fixture::m6 as binary;
use serde_json::Value;

fn rows(out: &std::process::Output) -> Vec<Value> {
    assert_eq!(code(out), 0, "{}", stderr(out));
    assert!(out.stderr.is_empty(), "{}", stderr(out));
    stdout(out)
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .filter(|v| v["type"] == "match")
        .map(|v| v["data"].clone())
        .collect()
}

fn names(row: &Value) -> String {
    row["trail"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join(" › ")
}

fn binary_case(format: &str, bytes: &[u8], want: &str) {
    let fx = Fixture::new();
    std::fs::write(fx.file("m6-media.bin"), bytes).unwrap();
    let out = run(&mut fx.s4(["--no-docs", "--json", "needle", "m6-media.bin"]));
    let found = rows(&out);
    assert!(!found.is_empty());
    assert_eq!(found[0]["format"], format);
    assert_eq!(names(&found[0]), want);
    common::assert_valid_json_schema(&stdout(&out));
    for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
        let out = run(&mut fx.s4(["--no-docs", flag, "needle", "m6-media.bin"]));
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        assert!(out.stderr.is_empty());
        let out = if layout == "json" {
            common::mask_json_times(&stdout(&out))
        } else {
            stdout(&out)
        };
        insta::assert_snapshot!(format!("m6_{format}_{layout}"), out);
    }
    let expanded = run(&mut fx.s4(["--no-docs", "--flat", "--expand", "needle", "m6-media.bin"]));
    let flat = run(&mut fx.s4(["--no-docs", "--flat", "needle", "m6-media.bin"]));
    assert_eq!(expanded.stdout, flat.stdout);
    let within = run(&mut fx.s4(["--no-docs", "--json", "--within", want, "needle", "m6-media.bin"]));
    assert!(!rows(&within).is_empty());
}

#[test]
fn m6_tiff_jpeg() {
    binary_case("tiff", &binary::tiff(b"needle-cam 3000"), "TIFF › IFD0 › Model");
    binary_case(
        "jpeg",
        &binary::jpeg(b"needle-cam 3000"),
        "JPEG › APP1 Exif › IFD0 › Model",
    );
}
