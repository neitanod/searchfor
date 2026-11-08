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
    let within = run(&mut fx.s4([
        "--no-docs",
        "--json",
        "--within",
        want,
        "needle",
        "m6-media.bin",
    ]));
    assert!(!rows(&within).is_empty());
}

#[test]
fn m6_tiff_jpeg() {
    binary_case(
        "tiff",
        &binary::tiff(b"needle-cam 3000"),
        "TIFF › IFD0 › Model",
    );
    binary_case(
        "jpeg",
        &binary::jpeg(b"needle-cam 3000"),
        "JPEG › APP1 Exif › IFD0 › Model",
    );
}

#[test]
fn m6_png_webp_xmp() {
    binary_case("png", &binary::png(b"needle"), "PNG › tEXt Comment");
    binary_case(
        "webp",
        &binary::webp(b"needle"),
        "WebP › EXIF › IFD0 › Model",
    );
    let xmp = b"<x:xmpmeta><rdf:RDF><rdf:Description><dc:title><rdf:Alt><rdf:li>needle</rdf:li></rdf:Alt></dc:title></rdf:Description></rdf:RDF></x:xmpmeta>";
    let fx = Fixture::new();
    let data = binary::jpeg_xmp(xmp);
    std::fs::write(fx.file("xmp.bin"), data).unwrap();
    let out = run(&mut fx.s4(["--no-docs", "--json", "needle", "xmp.bin"]));
    assert_eq!(
        names(&rows(&out)[0]),
        "JPEG › APP1 XMP › dc:title › rdf:Alt › rdf:li"
    );
    let broken = binary::jpeg_xmp(b"<rdf:Description><dc:title>ok</wrong>needle");
    std::fs::write(fx.file("bad-xmp.bin"), broken).unwrap();
    let out = run(&mut fx.s4(["--no-docs", "--json", "needle", "bad-xmp.bin"]));
    assert_eq!(names(&rows(&out)[0]), "JPEG › APP1 XMP");
}

#[test]
fn m6_id3_mp4() {
    binary_case("id3", &binary::id3(b"needle song"), "ID3v2.4 › TIT2");
    binary_case(
        "mp4",
        &binary::mp4(b"needle"),
        "MP4 M4A › moov › udta › meta › ilst › ©nam",
    );
    let fx = Fixture::new();
    let mut bytes = binary::id3(b"song");
    let mut tag = vec![0; 128];
    tag[..3].copy_from_slice(b"TAG");
    tag[3..9].copy_from_slice(b"needle");
    bytes.extend(tag);
    std::fs::write(fx.file("v1.bin"), bytes).unwrap();
    let out = run(&mut fx.s4(["--no-docs", "--json", "needle", "v1.bin"]));
    assert_eq!(names(&rows(&out)[0]), "ID3v1 › title");
    std::fs::write(
        fx.file("only-v1.bin"),
        [b"audio".as_slice(), &[0; 10], b"TAGneedle"].concat(),
    )
    .unwrap();
    let plain = run(&mut fx.s4(["--no-docs", "--json", "needle", "only-v1.bin"]));
    for row in stdout(&plain)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        if row["type"] == "match" {
            assert!(row["data"]["format"].is_null());
        }
    }
}

#[test]
fn m6_wasm_java() {
    binary_case(
        "wasm",
        &binary::wasm(b"needle not found"),
        "WebAssembly › data › data[0]",
    );
    binary_case(
        "java-class",
        &binary::class(b"needle merged"),
        "Java 17 › com.example.Cart › addItem",
    );
    let fx = Fixture::new();
    std::fs::write(fx.file("Cart.bin"), binary::class(b"needle")).unwrap();
    let out = run(&mut fx.s4(["--no-docs", "--json", "addItem", "Cart.bin"]));
    assert_eq!(names(&rows(&out)[0]), "Java 17 › com.example.Cart");
    use searchfor::processors::object::fixture::{Spec, fat, macho};
    let spec = Spec::new().section("__TEXT,__cstring", b"needle\0");
    let thin = macho(&spec);
    let (universal, _starts) = fat(&[(0x01000007, thin.file)]);
    std::fs::write(fx.file("fat.bin"), universal).unwrap();
    let out = run(&mut fx.s4(["--no-docs", "--json", "needle", "fat.bin"]));
    assert_eq!(rows(&out)[0]["format"], "macho");
}

/// A Java 17 class `C` of `methods` methods that share the name `name` and the descriptor
/// `()V`, none with attributes, and an unused Utf8 `needle` (code review M6 r3, Codex #1).
fn shared_name_class(name: &[u8], methods: u16) -> Vec<u8> {
    let mut b = b"\xca\xfe\xba\xbe\0\0\0\x3d\0\x06".to_vec();
    for (i, text) in [&b"C"[..], name, b"()V", b"needle"].iter().enumerate() {
        b.push(1);
        b.extend((text.len() as u16).to_be_bytes());
        b.extend(*text);
        if i == 0 {
            b.extend([7, 0, 1]);
        }
    }
    // Access, this, super, no interfaces, no fields, the methods.
    b.extend([0, 0x21, 0, 2, 0, 0, 0, 0, 0, 0]);
    b.extend(methods.to_be_bytes());
    for _ in 0..methods {
        b.extend([0, 1, 0, 3, 0, 4, 0, 0]);
    }
    b.extend([0, 0]);
    b
}

/// Code review M6 r3, Codex #1: methods that share one long name are each named by it, cut at
/// 40 characters, decoded from modified UTF-8 once for all of them; a Match in the shared
/// descriptor belongs to the first method, and one in an unused constant to the pool.
#[test]
fn m6_java_members_share_a_long_name() {
    let fx = Fixture::new();
    let tail = [0xff; 59_999];
    let cases: [(&[u8], String); 2] = [
        (&tail, format!("m{}…", "\u{fffd}".repeat(38))),
        (&[b'x'; 59_999], format!("m{}…", "x".repeat(38))),
    ];
    for (tail, label) in cases {
        let name = [b"m".as_slice(), tail].concat();
        std::fs::write(fx.file("Shared.bin"), shared_name_class(&name, 3)).unwrap();
        let out = run(&mut fx.s4(["--no-docs", "--json", "needle", "Shared.bin"]));
        let found = rows(&out);
        assert_eq!(found[0]["format"], "java-class");
        assert_eq!(names(&found[0]), "Java 17 › C › constant pool");
        let out = run(&mut fx.s4(["--no-docs", "--json", "-F", "()V", "Shared.bin"]));
        let found = rows(&out);
        assert_eq!(names(&found[0]), format!("Java 17 › C › {label}"));
        assert_eq!(found[0]["trail"][2]["kind"], "method");
    }
}

fn binaries() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("jpeg", binary::jpeg(b"needle")),
        ("png", binary::png(b"needle")),
        ("webp", binary::webp(b"needle")),
        ("tiff", binary::tiff(b"needle")),
        ("id3", binary::id3(b"needle")),
        ("mp4", binary::mp4(b"needle")),
        ("wasm", binary::wasm(b"needle")),
        ("class", binary::class(b"needle")),
    ]
}

#[test]
fn m6_binary_modes() {
    // As rg_compat.rs: without rg 14.1.1 return, or panic under SEARCHFOR_REQUIRE_REFERENCE=1.
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("m6-binaries")).unwrap();
    for (layout, b) in binaries() {
        let file = format!("m6-binaries/{layout}.bin");
        std::fs::write(fx.file(&file), b).unwrap();
        for flags in [
            vec!["-v"],
            vec!["-c"],
            vec!["-l"],
            vec!["-q"],
            vec!["--no-trail"],
        ] {
            let s = run(fx.s4(["--no-docs"]).args(&flags).args(["needle", &file]));
            let mut args = flags.clone();
            args.retain(|s| *s != "--no-trail");
            let r = run(&mut fx.rg(args.iter().copied().chain(["needle", file.as_str()])));
            assert_eq!(code(&s), code(&r), "{layout} {flags:?}");
            assert_eq!(s.stdout, r.stdout, "{layout} {flags:?}");
            assert_eq!(s.stderr, r.stderr, "{layout} {flags:?}");
        }
    }
    let s = run(&mut fx.s4(["--no-docs", "--flat", "needle", "m6-binaries"]));
    let r = run(&mut fx.rg(["needle", "m6-binaries"]));
    assert_eq!(code(&s), code(&r));
    assert_eq!(s.stdout, r.stdout);
    for binary_flag in ["--binary", "-a"] {
        let out = run(&mut fx.s4(["--no-docs", "--json", binary_flag, "needle", "m6-binaries"]));
        let found = rows(&out);
        assert_eq!(found.len(), binaries().len(), "{binary_flag}");
        assert!(
            found.iter().all(|r| r["format"].is_string()),
            "{binary_flag}"
        );
    }
}

/// The supervised child of `m6_binary_mutations`: reads one case's bytes with one layout.
/// Without its variables it does nothing and counts as no fuzz run.
#[test]
fn m6_binary_mutation_case() {
    let Some(path) = std::env::var_os("SEARCHFOR_M6_BINARY_CASE") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let layout = std::env::var("SEARCHFOR_M6_BINARY_LAYOUT").unwrap();
    if let Some(map) =
        searchfor::processors::object::layouts::BinaryMap::parse_loudly(&bytes, &layout)
    {
        for range in map.ranges() {
            assert!(range.start <= range.end && range.end <= bytes.len());
        }
        let lines = searchfor::trail::LineIndex::new(&bytes);
        // A Trail changes only where a range starts or ends: one byte of each run between
        // those offsets asks every Trail the map gives, instead of every byte (code round 2,
        // Fable #21).
        let mut at: Vec<usize> = std::iter::once(0)
            .chain(map.ranges().flat_map(|r| [r.start, r.end]))
            .filter(|&a| a < bytes.len())
            .collect();
        at.sort_unstable();
        at.dedup();
        for at in at {
            for s in map.trail(at, &lines) {
                assert!(s.line > 0 && s.end_line >= s.line);
            }
        }
    }
}

/// Declared-length bombs (M6 plan, Task 10.5): each length or count field of a layout's seed
/// replaced by its largest value, and pointers that repeat their target. (layout, offset,
/// bytes written there.)
const BOMBS: &[(&str, usize, &[u8])] = &[
    ("jpeg", 4, b"\xff\xff"),
    ("jpeg", 20, b"\xff\xff"),
    ("jpeg", 22, b"\xff\xff\xff\xff"),
    ("png", 33, b"\x7f\xff\xff\xff"),
    ("png", 8, b"\x7f\xff\xff\xff"),
    ("webp", 4, b"\xff\xff\xff\xff"),
    ("webp", 16, b"\xff\xff\xff\xff"),
    ("webp", 28, b"\xff\xff"),
    ("tiff", 4, b"\xff\xff\xff\xff"),
    ("tiff", 8, b"\xff\xff"),
    ("tiff", 14, b"\xff\xff\xff\xff"),
    ("tiff", 18, b"\xff\xff\xff\xff"),
    // IFD0's next IFD is IFD0 itself.
    ("tiff", 22, b"\x08\0\0\0"),
    ("id3", 6, b"\x7f\x7f\x7f\x7f"),
    ("id3", 14, b"\x7f\x7f\x7f\x7f"),
    ("id3", 14, b"\xff\xff\xff\xff"),
    // A v2.3 extended header of length u32::MAX (code round 2, Codex #9).
    ("id3", 3, b"\x03\0\x40\0\0\x01\0\xff\xff\xff\xff"),
    ("mp4", 0, b"\xff\xff\xff\xff"),
    ("mp4", 20, b"\xff\xff\xff\xff"),
    ("mp4", 20, b"\0\0\0\x01"),
    ("mp4", 28, b"\0\0\0\0"),
    ("mp4", 36, b"\0\0\0\x01"),
    ("wasm", 9, b"\xff\xff\xff\xff\x0f"),
    ("wasm", 9, b"\xff\xff\xff\xff\xff"),
    ("wasm", 10, b"\xff\xff\xff\xff\x0f"),
    ("class", 8, b"\xff\xff"),
    ("class", 11, b"\xff\xff"),
    ("class", 4, b"\xff\xff\xff\xff"),
];

#[test]
fn m6_binary_mutations() {
    let fx = Fixture::new();
    let inflight = common::FuzzInFlight::new("m6_binary_mutations");
    let mut cases: Vec<(&str, String, Vec<u8>)> = Vec::new();
    for (layout, seed) in binaries() {
        for n in 0..seed.len() + 4 {
            let mut b = seed.clone();
            if n < seed.len() {
                b.truncate(n)
            } else {
                for x in b.iter_mut().skip(n - seed.len()).step_by(7) {
                    *x = 0xff;
                }
            }
            cases.push((layout, format!("{n}"), b));
        }
        // 0x00 every 7 bytes, and seeded random bytes (code round 1, Fable #37): a zero length
        // or count, and values no pass above writes.
        for n in 0..4 {
            let mut b = seed.clone();
            for x in b.iter_mut().skip(n).step_by(7) {
                *x = 0;
            }
            cases.push((layout, format!("zero {n}"), b));
        }
        let mut state = 0x9e37_79b9_7f4a_7c15u64 ^ seed.len() as u64;
        let mut next = || {
            // xorshift64: reproducible without a crate.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for n in 0..8 {
            let mut b = seed.clone();
            for _ in 0..b.len().div_ceil(16) {
                let r = next();
                let at = (r >> 8) as usize % b.len();
                b[at] = r as u8;
            }
            cases.push((layout, format!("random {n}"), b));
        }
        let bombs: Vec<_> = BOMBS.iter().filter(|(l, ..)| *l == layout).collect();
        assert!(!bombs.is_empty(), "{layout} has no bomb");
        for (i, (_, at, bytes)) in bombs.into_iter().enumerate() {
            let mut b = seed.clone();
            assert!(at + bytes.len() <= b.len(), "{layout} bomb {i}");
            b[*at..at + bytes.len()].copy_from_slice(bytes);
            cases.push((layout, format!("bomb {i}"), b));
        }
    }
    let layouts: std::collections::HashSet<&str> = cases.iter().map(|c| c.0).collect();
    assert_eq!(layouts.len(), 8);
    for (layout, case, b) in &cases {
        inflight.at(|| format!("{layout}/{case}: {b:?}"));
        std::fs::write(fx.file("case.bin"), b).unwrap();
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args(["--exact", "m6_binary_mutation_case", "--nocapture"])
            .env("SEARCHFOR_M6_BINARY_CASE", fx.file("case.bin"))
            .env("SEARCHFOR_M6_BINARY_LAYOUT", layout);
        let (status, err) = common::run_limited(&mut cmd, std::time::Duration::from_secs(2));
        assert!(
            status.is_some_and(|s| s.success()),
            "{layout}/{case} {b:?}: {}",
            String::from_utf8_lossy(&err)
        );
        let (status, err) = common::run_limited(
            &mut fx.s4(["--no-docs", "--json", "needle", "case.bin"]),
            std::time::Duration::from_secs(2),
        );
        assert!(
            status
                .and_then(|s| s.code())
                .is_some_and(|c| c == 0 || c == 1),
            "{layout}/{case} {b:?}: {}",
            String::from_utf8_lossy(&err)
        );
    }
    inflight.finish();
}

#[test]
#[ignore = "local binary-layout rate; selected explicitly"]
fn m6_binary_rate() {
    use searchfor::processors::object::layouts::BinaryMap;
    // The small seeds, and large ones whose metadata holds a megabyte of text, so that the
    // tiny files' hot-cache speed alone is not the evidence.
    let large = vec![b'x'; 1 << 20];
    let sets = [
        ("small", binaries()),
        (
            "large",
            vec![
                ("jpeg", binary::jpeg_xmp(&large[..60_000])),
                ("png", binary::png(&large)),
                ("webp", binary::webp(&large)),
                ("tiff", binary::tiff(&large)),
                ("id3", binary::id3(&large)),
                ("mp4", binary::mp4(&large)),
                ("wasm", binary::wasm(&large)),
                ("class", binary::class(&large[..60_000])),
            ],
        ),
    ];
    for (size, set) in sets {
        for (layout, mut bytes) in set {
            // Repeat a valid file for bounded parser overhead; bytes/sec is measured on bytes
            // actually supplied, not padding a single tiny unit.
            let unit = bytes.clone();
            let repeats = (100_000_000 / unit.len()).max(1);
            let started = std::time::Instant::now();
            for _ in 0..repeats {
                assert!(
                    BinaryMap::parse_loudly(&bytes, layout).is_some(),
                    "{layout}"
                );
                std::hint::black_box(&mut bytes);
            }
            let rate = (unit.len() * repeats) as f64 / started.elapsed().as_secs_f64();
            eprintln!("{size} {layout}: {rate:.0} bytes/s");
            if std::env::var_os("SEARCHFOR_PERF_TIMING").is_some_and(|s| s == "1") {
                assert!(rate >= 100_000_000.0, "{size} {layout}: {rate:.0} bytes/s");
            }
        }
    }
}
