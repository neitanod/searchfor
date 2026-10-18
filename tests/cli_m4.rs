//! End-to-end tests of M4 (plan, Chunk 11): object files, documents with --docs, ripgrep's
//! --pre and -z. The documents and object files are written into the fixture copy by the
//! writers of `searchfor::docs::fixture` and `searchfor::processors::object::fixture`; the
//! SQLite databases are tests/fixtures/docs/*.sqlite (scripts/gen-sqlite-fixtures.py).
mod common;

use common::{Fixture, code, run, stderr, stdout};
use searchfor::docs::fixture as doc;
use searchfor::processors::object::fixture as obj;

/// The fixture copy plus M4's generated files under `m4/`.
fn fixture() -> Fixture {
    let fx = Fixture::new();
    let d = fx.path().join("m4");
    std::fs::create_dir_all(d.join("bin")).unwrap();
    let w = |p: &str, b: &[u8]| std::fs::write(d.join(p), b).unwrap();
    let rodata = b"\0\0\0\0hello\0version=1.2.3-needle\0another needle here\0";
    let spec = obj::Spec::new()
        .section(".text", b"\x55\x48\x89\xe5\xc3\0\0\0")
        .section(".rodata", rodata)
        .sized("main.greeting", 1, 4, 6)
        .sized("main.version", 1, 10, 21);
    w("bin/app", &obj::elf(&spec).file);
    let mut stripped = spec.clone();
    stripped.symbols.clear();
    w("bin/app-stripped", &obj::elf(&stripped).file);
    let pe = obj::Spec::new()
        .section(".text", b"\xc3\0\0\0")
        .section(".rdata", b"\0\0version=1.2.3-needle\0")
        .symbol("version_string", 1, 2);
    w("bin/app.exe", &obj::pe(&pe).file);
    let mo = obj::Spec::new()
        .section("__TEXT,__text", b"\xc3\0\0\0")
        .section("__TEXT,__cstring", b"hi\0version=1.2.3-needle\0")
        .symbol("_version", 1, 3);
    let x86 = obj::macho(&mo).file;
    let arm = obj::macho(&mo.clone().machine(0x0100_000c)).file;
    w(
        "bin/app-mac",
        &obj::fat(&[(0x0100_0007, x86), (0x0100_000c, arm)]).0,
    );
    let report = [
        doc::h(1, "1 Intro"),
        doc::p("The intro needle."),
        doc::h(2, "1.1 Scope"),
        doc::p("Scope needle."),
        doc::h(1, "2 Costs"),
        doc::table(&[&["Item", "Price"], &["Pens", "1.50 needle"]]),
    ];
    w("report.docx", &doc::docx(&report));
    w("report.odt", &doc::odt(&report));
    let sheet: &[&[&str]] = &[&["Item", "Price"], &["Pens", "1.5"], &["needle pins", "2"]];
    w("budget.xlsx", &doc::xlsx(&[("Budget", sheet)]));
    w("budget.ods", &doc::ods(&[("Budget", sheet)]));
    let slides: &[(&[&str], Option<&str>)] = &[
        (
            &["Results", "a needle margin"],
            Some("tell the needle joke"),
        ),
        (&["Next", "find the needle"], None),
    ];
    w("deck.pptx", &doc::pptx(slides));
    w("deck.odp", &doc::odp(slides));
    w(
        "book.epub",
        &doc::epub(
            &[
                (
                    "1. Arrival",
                    "<h1>The Harbour</h1><p>A needle in the water.</p>",
                ),
                ("2. The Storm", "<p>The storm needle.</p>"),
            ],
            true,
        ),
    );
    w(
        "report.pdf",
        &doc::pdf(
            &[
                &["Report"],
                &["1 Intro", "intro needle"],
                &["2 Costs", "costs needle"],
            ],
            &[(1, "1 Intro", 2), (1, "2 Costs", 3)],
        ),
    );
    w(
        "locked.pdf",
        &doc::pdf_encrypted(&[&["locked needle"]], "pw"),
    );
    w("broken.docx", b"PK\x03\x04 not really a zip, needle");
    w(
        "notes.zip",
        &doc::zip(&[("a.txt", b"zipped needle".to_vec())], &[]),
    );
    fx
}

fn ok(fx: &Fixture, args: &[&str]) -> String {
    let o = run(&mut fx.s4(args));
    assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
    assert_eq!(stderr(&o), "", "{args:?}");
    stdout(&o)
}

#[test]
fn m4_01_every_format_in_three_layouts() {
    // Spec §11 #2 for M4's Formats.
    let fx = fixture();
    let cases: &[(&str, &[&str])] = &[
        ("elf", &["needle", "m4/bin/app"]),
        ("pe", &["needle", "m4/bin/app.exe"]),
        ("macho", &["needle", "m4/bin/app-mac"]),
        ("pdf", &["--docs", "needle", "m4/report.pdf"]),
        ("docx", &["--docs", "needle", "m4/report.docx"]),
        ("odt", &["--docs", "needle", "m4/report.odt"]),
        ("xlsx", &["--docs", "needle", "m4/budget.xlsx"]),
        ("ods", &["--docs", "needle", "m4/budget.ods"]),
        ("pptx", &["--docs", "needle", "m4/deck.pptx"]),
        ("odp", &["--docs", "needle", "m4/deck.odp"]),
        ("epub", &["--docs", "needle", "m4/book.epub"]),
        ("sqlite", &["--docs", "needle", "docs/shop.sqlite"]),
    ];
    for (name, args) in cases {
        for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
            let mut a = args.to_vec();
            a.push(flag);
            let mut out = ok(&fx, &a);
            if layout == "json" {
                common::assert_valid_json_schema(&out);
                assert!(
                    out.contains(&format!("\"format\":\"{name}\"")),
                    "{name}: {out}"
                );
                out = common::mask_json_times(&out);
            }
            insta::assert_snapshot!(format!("m4_01_{name}_{layout}"), out);
        }
    }
}

#[test]
fn m4_02_object_rows_with_and_without_symbols() {
    // Spec §11 #6.
    let fx = fixture();
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--sort",
                "path",
                "needle",
                "m4/bin/app",
                "m4/bin/app-stripped"
            ]
        ),
        "m4/bin/app:0x68: ELF x86-64 › .rodata › main.version+0xe │ version=1.2.3-needle\n\
         m4/bin/app:0x77: ELF x86-64 › .rodata │ another needle here\n\
         m4/bin/app-stripped:0x68: ELF x86-64 › .rodata │ version=1.2.3-needle\n\
         m4/bin/app-stripped:0x77: ELF x86-64 › .rodata │ another needle here\n"
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--sort",
                "path",
                "needle",
                "m4/bin/app.exe",
                "m4/bin/app-mac"
            ]
        ),
        "m4/bin/app.exe:0x410: PE x86-64 › .rdata › version_string+0xe │ version=1.2.3-needle\n\
         m4/bin/app-mac:0x1141: Mach-O x86-64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle\n\
         m4/bin/app-mac:0x2141: Mach-O arm64 › __TEXT,__cstring › _version+0xe │ version=1.2.3-needle\n"
    );
}

#[test]
fn m4_03_walked_objects_are_searched_as_rg_searches_them() {
    let fx = fixture();
    // Walked without --binary, rg stops at the first NUL: nothing to describe.
    let o = run(&mut fx.s4(["--flat", "version=", "m4/bin"]));
    assert_eq!(code(&o), 1, "{}", stdout(&o));
    // With --binary every object file with a Match gets its rows.
    let out = ok(
        &fx,
        &["--flat", "--binary", "--sort", "path", "version=", "m4/bin"],
    );
    assert_eq!(out.lines().count(), 5, "{out}");
    assert!(out.lines().all(|l| l.contains(":0x")), "{out}");
    // -v, -c and --no-trail print what rg prints.
    if common::reference_rg() {
        for args in [
            &["--no-trail", "needle", "m4/bin/app"][..],
            &["-c", "needle", "m4/bin/app"],
            &["--flat", "-v", "needle", "m4/bin/app"],
            &[
                "--no-trail",
                "--binary",
                "--sort",
                "path",
                "needle",
                "m4/bin",
            ],
        ] {
            let rg_args: Vec<&str> = args
                .iter()
                .copied()
                .filter(|a| *a != "--flat" && *a != "--no-trail")
                .collect();
            let s = run(&mut fx.s4(args));
            let r = run(&mut fx.rg(&rg_args));
            assert_eq!(stdout(&s), stdout(&r), "{args:?}");
            assert_eq!(code(&s), code(&r), "{args:?}");
        }
    }
}

#[test]
fn m4_04_object_rows_count_for_m_and_json() {
    let fx = fixture();
    assert_eq!(
        ok(&fx, &["--flat", "-m1", "needle", "m4/bin/app"])
            .lines()
            .count(),
        1
    );
    let json = ok(&fx, &["--json", "needle", "m4/bin/app"]);
    let end: serde_json::Value = json
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["type"] == "end")
        .unwrap();
    let size = std::fs::metadata(fx.file("m4/bin/app")).unwrap().len();
    assert_eq!(end["data"]["stats"]["bytes_searched"], size);
    assert_eq!(end["data"]["stats"]["matched_lines"], 2);
    assert_eq!(end["data"]["binary_offset"], serde_json::Value::Null);
    // --within selects rows by their Trail.
    assert_eq!(
        ok(
            &fx,
            &["--flat", "--within", "main.version", "needle", "m4/bin/app"]
        )
        .lines()
        .count(),
        1
    );
}

#[test]
fn m4_05_pdf_chapters_only_with_docs_and_zips_never() {
    // Spec §11 #7.
    let fx = fixture();
    assert_eq!(
        ok(&fx, &["--docs", "--flat", "needle", "m4/report.pdf"]),
        "3: 1 Intro › page 2 │ intro needle\n5: 2 Costs › page 3 │ costs needle\n"
    );
    if common::reference_rg() {
        // Without --docs the PDF is searched as rg searches it (its streams are compressed).
        let s = run(&mut fx.s4(["--flat", "needle", "m4/report.pdf"]));
        let r = run(&mut fx.rg(["needle", "m4/report.pdf"]));
        assert_eq!((stdout(&s), code(&s)), (stdout(&r), code(&r)));
    }
    // A zip is never opened (8390), --docs or not.
    let o = run(&mut fx.s4(["--docs", "--flat", "zipped", "m4/notes.zip"]));
    assert_eq!(code(&o), 1, "{}", stdout(&o));
}

#[test]
fn m4_06_document_trails() {
    let fx = fixture();
    let flat = |f: &str| ok(&fx, &["--docs", "--flat", "needle", f]);
    assert_eq!(
        flat("m4/report.docx"),
        "2: 1 Intro │ The intro needle.\n\
         4: 1 Intro › 1.1 Scope │ Scope needle.\n\
         7: 2 Costs › table 1 › row 2 › Price │ Pens\t1.50 needle\n"
    );
    assert_eq!(flat("m4/report.odt"), flat("m4/report.docx"));
    assert_eq!(flat("m4/budget.xlsx"), "3: Budget › A3 │ needle pins\t2\n");
    assert_eq!(flat("m4/budget.ods"), flat("m4/budget.xlsx"));
    assert_eq!(
        flat("m4/deck.pptx"),
        "2: slide 1 │ a needle margin\n3: slide 1 › notes │ tell the needle joke\n5: slide 2 │ find the needle\n"
    );
    assert_eq!(flat("m4/deck.odp"), flat("m4/deck.pptx"));
    assert_eq!(
        flat("m4/book.epub"),
        "2: 1. Arrival › The Harbour │ A needle in the water.\n3: 2. The Storm │ The storm needle.\n"
    );
    assert_eq!(
        ok(&fx, &["--docs", "--flat", "needle", "docs/shop.sqlite"]),
        "12: users › row 12 › email │ 12\tUser 12\tana.needle@example.com\t18.0\n\
         42: notes › row 2 › body │ …[+1365] g long long long long long long long long long long long long long long long long long long long long long long long long long long long needle at the end\t\n\
         45: kv › row 3 › v │ gamma\tvalue of gamma needle\n\
         46: late › row 1 › payload │ needle payload\tk1\n\
         48: gaps › row 1 › y │ \tneedle gap\n\
         49: odd › row 1 › payload │ needle odd\t2.0\n\
         50: gen › row 1 › payload │ first\t\tneedle gen\tFIRST\n\
         51: added › row 1 › payload │ old\tneedle added\n\
         52: added › row 2 › payload │ new\tneedle later\n"
    );
}

#[test]
fn m4_07_documents_that_cannot_be_read_warn_and_are_searched_as_rg() {
    let fx = fixture();
    let o = run(&mut fx.s4([
        "--docs",
        "--flat",
        "--sort",
        "path",
        "needle",
        "m4/locked.pdf",
        "m4/broken.docx",
    ]));
    assert_eq!(
        stderr(&o),
        "s4: m4/locked.pdf: pdf: encrypted, and the empty password does not open it\n\
         s4: m4/broken.docx: docx: not a zip file the reader can open (invalid Zip archive: Could not find EOCD)\n"
    );
    // broken.docx is text with a needle: printed as rg prints it, exit 0.
    assert_eq!(code(&o), 0);
    assert!(
        stdout(&o).contains("m4/broken.docx:1: PK"),
        "{}",
        stdout(&o)
    );
    let quiet = run(&mut fx.s4([
        "--docs",
        "--no-messages",
        "--flat",
        "needle",
        "m4/locked.pdf",
    ]));
    assert_eq!(stderr(&quiet), "");
}

#[test]
fn m4_08_pre_and_z_as_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    let d = fx.path().join("m4/pre");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("a.txt"), "alpha needle\n").unwrap();
    std::fs::write(d.join("b.md"), "# B\ngamma needle\n").unwrap();
    let script = |name: &str, body: &str| {
        let p = d.join(name);
        std::fs::write(&p, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p.to_string_lossy().into_owned()
    };
    let up = script("up.sh", "#!/bin/sh\ntr a-z A-Z < \"$1\"\n");
    let fail = script("fail.sh", "#!/bin/sh\necho oops >&2\nexit 3\n");
    let mut gz = std::process::Command::new("gzip");
    gz.arg("-k").arg(d.join("a.txt"));
    let have_gzip = gz.status().is_ok_and(|s| s.success());
    let mut cases: Vec<Vec<String>> = vec![
        vec![
            "--pre".into(),
            up.clone(),
            "--sort".into(),
            "path".into(),
            "NEEDLE".into(),
            "m4/pre/a.txt".into(),
            "m4/pre/b.md".into(),
        ],
        vec![
            "--pre".into(),
            up.clone(),
            "--pre-glob".into(),
            "*.md".into(),
            "-i".into(),
            "needle".into(),
            "m4/pre".into(),
            "--sort".into(),
            "path".into(),
        ],
        vec![
            "--pre".into(),
            fail.clone(),
            "needle".into(),
            "m4/pre/a.txt".into(),
        ],
        vec![
            "--pre".into(),
            "/nonexistent/pre".into(),
            "needle".into(),
            "m4/pre/a.txt".into(),
        ],
        vec![
            "--json".into(),
            "--pre".into(),
            up.clone(),
            "NEEDLE".into(),
            "m4/pre/a.txt".into(),
        ],
    ];
    // Values that start with a dash, after a space or `=` (code review r2, Codex #3): the
    // glob picks only `-x.txt` for the command, and `-cat` is a command that does not start.
    let dash = fx.path().join("m4/pre-dash");
    std::fs::create_dir_all(&dash).unwrap();
    std::fs::write(dash.join("-x.txt"), "dash needle\n").unwrap();
    std::fs::write(dash.join("y.txt"), "plain needle\n").unwrap();
    for glob in [["--pre-glob", "-*.txt"], ["--pre-glob=-*.txt", "-i"]] {
        let mut v: Vec<String> = vec!["--pre".into(), up.clone()];
        v.extend(glob.map(String::from));
        v.extend(["--sort", "path", "NEEDLE", "m4/pre-dash"].map(String::from));
        cases.push(v);
    }
    for pre in [&["--pre", "-cat"][..], &["--pre=-cat"]] {
        let mut v: Vec<String> = pre.iter().map(|a| a.to_string()).collect();
        v.extend(["needle", "m4/pre/a.txt"].map(String::from));
        cases.push(v);
    }
    if have_gzip {
        cases.push(vec!["-z".into(), "needle".into(), "m4/pre/a.txt.gz".into()]);
        cases.push(vec![
            "--pre".into(),
            up.clone(),
            "-z".into(),
            "needle".into(),
            "m4/pre/a.txt.gz".into(),
        ]);
        cases.push(vec![
            "-z".into(),
            "--pre".into(),
            up.clone(),
            "NEEDLE".into(),
            "m4/pre/a.txt.gz".into(),
        ]);
        // A negation turns off only its own flag: what the other flag turned off stays off
        // (code review r1, Codex #4, Fable #2).
        for (flags, pattern) in [
            (["-z", "--pre", &up, "--no-pre"], "needle"),
            (["--pre", &up, "-z", "--no-search-zip"], "NEEDLE"),
        ] {
            let mut v: Vec<String> = flags.iter().map(|f| f.to_string()).collect();
            v.extend(["--sort", "path", pattern, "m4/pre"].map(String::from));
            cases.push(v);
        }
    }
    for args in cases {
        let s = run(&mut fx.s4(args.iter().map(String::as_str).chain(["--no-trail"])));
        let r = run(&mut fx.rg(&args));
        let (so, ro) = if args.contains(&"--json".to_string()) {
            (
                common::mask_json_times(&stdout(&s)),
                common::mask_json_times(&stdout(&r)),
            )
        } else {
            (stdout(&s), stdout(&r))
        };
        assert_eq!(so, ro, "{args:?}");
        assert_eq!(code(&s), code(&r), "{args:?}");
        assert_eq!(stderr(&s).replace("s4: ", "rg: "), stderr(&r), "{args:?}");
    }
    // Walking the current directory, the command and the decompressor get the path rg prints,
    // without `./`, in their arguments and in rg's errors (code review r1, Fable #1).
    let show = script("show.sh", "#!/bin/sh\nprintf 'ARG=%s\\n' \"$1\"\n");
    let mut walked: Vec<Vec<&str>> = vec![
        vec!["--pre", &show, "--sort", "path", "ARG="],
        vec!["--pre", &fail, "--sort", "path", "needle"],
        vec!["--pre", "/nonexistent/pre", "--sort", "path", "needle"],
    ];
    if have_gzip {
        std::fs::write(d.join("bad.gz"), "not gzip, needle\n").unwrap();
        walked.push(vec!["-z", "--sort", "path", "needle"]);
    }
    for args in walked {
        let s = run(fx.s4(args.iter().chain(&["--no-trail"])).current_dir(&d));
        let r = run(fx.rg(&args).current_dir(&d));
        assert_eq!(stdout(&s), stdout(&r), "{args:?}");
        assert_eq!(code(&s), code(&r), "{args:?}");
        assert_eq!(stderr(&s).replace("s4: ", "rg: "), stderr(&r), "{args:?}");
    }
    // In the Trail layouts the output of --pre gets the Trails of the file's Format.
    assert_eq!(
        ok(&fx, &["--flat", "--pre", &up, "NEEDLE", "m4/pre/b.md"]),
        "2: B │ GAMMA NEEDLE\n"
    );
}

#[test]
fn m4_09_docs_in_the_ripgrep_modes_and_with_z() {
    let fx = fixture();
    assert_eq!(
        ok(&fx, &["--docs", "--no-trail", "needle", "m4/report.docx"]),
        "The intro needle.\nScope needle.\nPens\t1.50 needle\n"
    );
    assert_eq!(
        ok(&fx, &["--docs", "-c", "needle", "m4/report.docx"]),
        "3\n"
    );
    let l = ok(
        &fx,
        &[
            "--docs",
            "-l",
            "--sort",
            "path",
            "needle",
            "m4/report.docx",
            "m4/budget.ods",
            "m4/bin",
        ],
    );
    assert!(
        l.contains("m4/report.docx\n") && l.contains("m4/budget.ods\n"),
        "{l}"
    );
    // A document -z decompresses is searched as text, never extracted (D21).
    let gz = std::process::Command::new("gzip")
        .arg("-k")
        .arg(fx.file("m4/report.docx"))
        .status();
    if gz.is_ok_and(|s| s.success()) {
        let o = run(&mut fx.s4([
            "--docs",
            "-z",
            "--flat",
            "intro needle",
            "m4/report.docx.gz",
        ]));
        assert_eq!(code(&o), 1, "{}", stdout(&o));
    }
}

#[test]
fn m4_10_structural_flags_on_documents_and_objects() {
    let fx = fixture();
    assert_eq!(
        ok(
            &fx,
            &[
                "--docs",
                "--flat",
                "--in-headings",
                "Scope",
                "m4/report.docx"
            ]
        ),
        "3: 1 Intro │ 1.1 Scope\n"
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--docs",
                "--flat",
                "--within",
                "Costs",
                "--sort",
                "path",
                "needle",
                "m4/report.docx",
                "m4/report.pdf"
            ]
        ),
        "m4/report.docx:7: 2 Costs › table 1 › row 2 › Price │ Pens\t1.50 needle\n\
         m4/report.pdf:5: 2 Costs › page 3 │ costs needle\n"
    );
    // --expand: a table cell's Section is its row; an object row is printed as a row (D8).
    assert_eq!(
        ok(
            &fx,
            &["--docs", "--flat", "--expand", "1.50", "m4/report.docx"]
        ),
        "7-7: 2 Costs › table 1 › row 2 › Price\nPens\t1.50 needle\n\n"
    );
    // At its first Match, past the start of its string, as without --expand (M4 plan r1,
    // Fable #4).
    assert_eq!(
        ok(&fx, &["--flat", "--expand", "needle here", "m4/bin/app"]),
        "0x77: ELF x86-64 › .rodata │ another needle here\n"
    );
}

#[test]
fn m4_11_without_docs_documents_are_ripgreps() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    for args in [
        &["--sort", "path", "needle", "m4"][..],
        &["-a", "needle", "m4/report.docx"],
    ] {
        let s = run(&mut fx.s4(args.iter().copied().chain(["--no-trail"])));
        let r = run(&mut fx.rg(args));
        assert_eq!(stdout(&s), stdout(&r), "{args:?}");
        assert_eq!(code(&s), code(&r), "{args:?}");
        let f = run(&mut fx.s4(args.iter().copied().chain(["--flat"])));
        assert_eq!(stderr(&f), "", "{args:?}");
        assert!(!stdout(&f).contains(" › page "), "{args:?}");
    }
}

/// Runs `c` with `input` on its stdin, failing the test past 20 s instead of hanging.
fn run_within(c: &mut std::process::Command, input: &[u8]) -> std::process::Output {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let started = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > std::time::Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("{c:?} still runs after 20 s");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    writer.join().unwrap();
    child.wait_with_output().unwrap()
}

/// Codex r1 #3: with --docs, a path that is not a regular file (stdin by name, a descriptor, a
/// FIFO) is never a document: its bytes are read once, by the search, as rg reads them.
#[cfg(unix)]
#[test]
fn m4_12_docs_reads_streams_once() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    let fifo = fx.path().join("m4/pipe.pdf");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    for path in ["/dev/stdin", "/dev/fd/0", "m4/pipe.pdf"] {
        // The FIFO is named like a document, and its writer writes once.
        let feed = || {
            let fifo = fifo.clone();
            (path == "m4/pipe.pdf")
                .then(|| std::thread::spawn(move || std::fs::write(fifo, b"a fifo needle\n")))
        };
        let writer = feed();
        let s = run_within(
            &mut fx.s4(["--docs", "--no-trail", "needle", path]),
            b"a needle\n",
        );
        if let Some(w) = writer {
            w.join().unwrap().unwrap();
        }
        assert_eq!(stderr(&s), "", "{path}");
        let writer = feed();
        let r = run_within(&mut fx.rg(["needle", path]), b"a needle\n");
        if let Some(w) = writer {
            w.join().unwrap().unwrap();
        }
        assert_eq!(code(&r), 0, "{path}");
        assert_eq!(code(&s), code(&r), "{path}");
        assert_eq!(stdout(&s), stdout(&r), "{path}");
    }
}

/// Code review r1, Codex #1: stdin redirected from a document, named `/dev/stdin`,
/// `/dev/fd/0`, `/proc/self/fd/0` or through a link to one of them, is a regular file, yet no
/// document: a reader would open it as its own stdin. It is searched as rg searches it.
#[cfg(unix)]
#[test]
fn m4_24_stdin_redirected_from_a_document_is_no_document() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    std::os::unix::fs::symlink("/dev/stdin", fx.path().join("m4/stdin.docx")).unwrap();
    let shop = common::source_dir().join("docs/shop.sqlite");
    let docx = fx.path().join("m4/report.docx");
    for (path, input) in [
        ("/dev/stdin", &shop),
        ("/dev/fd/0", &shop),
        ("/proc/self/fd/0", &shop),
        ("m4/stdin.docx", &docx),
    ] {
        let started = std::time::Instant::now();
        let s = run(fx
            .s4(["--docs", "--no-trail", "needle", path])
            .env("SEARCHFOR_READER_SECONDS", "1")
            .stdin(std::fs::File::open(input).unwrap()));
        let took = started.elapsed();
        let r = run(fx
            .rg(["needle", path])
            .stdin(std::fs::File::open(input).unwrap()));
        assert_eq!(stderr(&s), "", "{path}");
        assert_eq!(stdout(&s), stdout(&r), "{path}");
        assert_eq!(code(&s), code(&r), "{path}");
        assert!(
            took < std::time::Duration::from_millis(900),
            "{path}: {took:?}"
        );
    }
}

/// Code review r2, Codex #2 and Fable #1: a reader opens a document again by its name, in
/// another process, so a name that goes through the process's descriptors (`/proc/self/fd`,
/// `/proc/<pid>/fd`, `/dev/fd`, `/dev/stdin`), in any of its parts, itself or through a link,
/// names no document: here stdin is a directory, and each file under it is searched as rg
/// searches it. A regular file anywhere else, `/dev/shm` included, is a document.
#[cfg(target_os = "linux")]
#[test]
fn m4_27_only_names_through_descriptors_are_no_documents() {
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    let link = |to: &str, name: &str| std::os::unix::fs::symlink(to, fx.path().join(name));
    link("/proc/self/fd/0", "m4/fd0").unwrap();
    link("/proc/self/fd", "m4/fds").unwrap();
    link("/dev/fd/0", "m4/devfd0").unwrap();
    let cases: &[&[&str]] = &[
        &["needle", "/proc/self/fd/0/docs/shop.sqlite"],
        &["needle", "/proc/thread-self/fd/0/docs/shop.sqlite"],
        &["needle", "/dev/fd/0/docs/shop.sqlite"],
        &["needle", "/dev/stdin/docs/shop.sqlite"],
        &["needle", "/proc/self/fd/0/m4/report.docx"],
        &["needle", "m4/fd0/docs/shop.sqlite"],
        &["needle", "m4/fds/0/docs/shop.sqlite"],
        &["needle", "m4/devfd0/m4/report.docx"],
        &["-c", "--sort", "path", "needle", "/proc/self/fd/0/docs"],
        &["-c", "--sort", "path", "needle", "m4/fd0/m4"],
    ];
    for args in cases {
        let stdin = || std::fs::File::open(fx.path()).unwrap();
        let s = run(fx
            .s4(["--docs", "--no-trail"].iter().chain(args.iter()))
            .stdin(stdin()));
        let r = run(fx.rg(*args).stdin(stdin()));
        assert_eq!(stderr(&s), "", "{args:?}");
        assert_eq!(stdout(&s), stdout(&r), "{args:?}");
        assert_eq!(code(&s), code(&r), "{args:?}");
    }
    // The other side: regular files, read by name, under /dev/shm and through /proc links
    // that lead out of /proc.
    let count = ok(&fx, &["--docs", "-c", "needle", "m4/report.docx"]);
    let abs = fx.path().join("m4/report.docx");
    let root = format!("/proc/self/root{}", abs.display());
    let mut named: Vec<(Vec<String>, String)> = vec![
        (vec!["/proc/self/cwd/m4/report.docx".into()], count.clone()),
        (vec![root], count.clone()),
    ];
    let shm = std::path::Path::new("/dev/shm");
    let held = shm
        .is_dir()
        .then(|| tempfile::tempdir_in(shm).ok())
        .flatten();
    if let Some(dir) = &held {
        let d = dir.path();
        std::fs::copy(&abs, d.join("report.docx")).unwrap();
        let at = d.join("report.docx").display().to_string();
        std::os::unix::fs::symlink(&at, fx.path().join("m4/shm.docx")).unwrap();
        std::os::unix::fs::symlink(d, fx.path().join("m4/shmdir")).unwrap();
        named.extend([
            (vec![at.clone()], count.clone()),
            (vec!["m4/shm.docx".into()], count.clone()),
            (vec!["m4/shmdir/report.docx".into()], count.clone()),
            (vec![d.display().to_string()], format!("{at}:{count}")),
        ]);
        let o = run(fx
            .s4(["--docs", "-c", "needle", "report.docx"])
            .current_dir(d));
        assert_eq!((stdout(&o), stderr(&o)), (count.clone(), String::new()));
    }
    for (paths, want) in named {
        let mut args = vec!["--docs", "-c", "needle"];
        args.extend(paths.iter().map(String::as_str));
        assert_eq!(ok(&fx, &args), want, "{paths:?}");
    }
}

/// Codex r1 #7 and #8: --within keeps the Matches of an object row that are within, not the
/// whole row, and -m after it counts rows, which share their line.
#[test]
fn m4_13_within_on_object_rows() {
    let fx = fixture();
    let rodata = b"\0first needle|second needle\0third needle\0fourth needle\0";
    let spec = obj::Spec::new()
        .section(".text", b"\xc3\0\0\0")
        .section(".rodata", rodata)
        .sized("one", 1, 1, 13)
        .sized("two", 1, 14, 13);
    let elf = obj::elf(&spec).file;
    std::fs::write(fx.path().join("m4/bin/two"), &elf).unwrap();
    let at = |s: &[u8]| format!("{:#x}", memchr::memmem::find(&elf, s).unwrap());
    assert_eq!(
        ok(&fx, &["--flat", "--within", "two", "needle", "m4/bin/two"]),
        format!(
            "{}: ELF x86-64 › .rodata › two+0x7 │ first needle|second needle\n",
            at(b"needle\0third")
        )
    );
    let json = ok(&fx, &["--json", "--within", "two", "needle", "m4/bin/two"]);
    let m: Vec<serde_json::Value> = json
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .filter(|v: &serde_json::Value| v["type"] == "match")
        .collect();
    assert_eq!(m.len(), 1, "{json}");
    let subs = m[0]["data"]["submatches"].as_array().unwrap();
    assert_eq!(subs.len(), 1, "{json}");
    assert_eq!(subs[0]["start"], 20, "{json}");
    // Three rows on one line: -m1 keeps the first.
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--within",
                ".rodata",
                "-m1",
                "needle",
                "m4/bin/two"
            ]
        ),
        format!(
            "{}: ELF x86-64 › .rodata › one+0x6 │ first needle|second needle\n",
            at(b"needle|")
        )
    );
}

/// Codex r1 #11: what a transform outputs gets no object rows and no object Processor, even
/// when its bytes are an object file or --format names one: it prints as Plain (spec §5.9).
#[test]
fn m4_14_transformed_objects_print_as_plain() {
    let fx = fixture();
    let gz = std::process::Command::new("gzip")
        .args(["-k", "m4/bin/app"])
        .current_dir(fx.path())
        .status()
        .unwrap();
    assert!(gz.success());
    for args in [
        &["--pre", "cat", "needle", "m4/bin/app"][..],
        &["--pre", "cat", "-a", "needle", "m4/bin/app"],
        &["--pre", "cat", "--format", "elf", "needle", "m4/bin/app"],
        &["-z", "needle", "m4/bin/app.gz"],
        &["-z", "-a", "--format", "elf", "needle", "m4/bin/app.gz"],
    ] {
        for layout in ["--flat", "--tree", "--json"] {
            let o = run(&mut fx.s4(args.iter().copied().chain([layout])));
            assert_eq!(code(&o), 0, "{args:?} {layout}: {}", stderr(&o));
            let out = stdout(&o);
            if layout == "--json" {
                for v in out
                    .lines()
                    .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
                {
                    if v["type"] == "match" {
                        assert_eq!(v["data"]["trail"], serde_json::json!([]), "{args:?}: {out}");
                        assert_eq!(
                            v["data"]["format"],
                            serde_json::Value::Null,
                            "{args:?}: {out}"
                        );
                    }
                }
            } else {
                assert!(
                    !out.contains("ELF x86-64") && !out.contains(" › ") && !out.contains("0x"),
                    "{args:?} {layout}: {out}"
                );
            }
        }
        if common::reference_rg() && !args.contains(&"--format") {
            let s = run(&mut fx.s4(args.iter().copied().chain(["--no-trail"])));
            let r = run(&mut fx.rg(args));
            assert_eq!(stdout(&s), stdout(&r), "{args:?}");
        }
    }
}

/// Codex r2 #5: one string with two Matches 5 000 bytes apart is one row in every layout, also
/// with -m1.
#[test]
fn m4_15_a_long_string_is_one_row() {
    let fx = fixture();
    let mut rodata = b"\0needle".to_vec();
    rodata.extend(vec![b'a'; 5000]);
    rodata.extend(b"needle\0");
    let spec = obj::Spec::new()
        .section(".text", b"\xc3\0\0\0")
        .section(".rodata", &rodata);
    std::fs::write(fx.path().join("m4/bin/long"), obj::elf(&spec).file).unwrap();
    for m in [&[][..], &["-m1"]] {
        let args = |layout: &'static str| {
            let mut a = vec![layout];
            a.extend_from_slice(m);
            a.extend(["needle", "m4/bin/long"]);
            a
        };
        // One row, at one offset; a long row prints a Fragment around each Match (spec §6.3).
        let offsets = |out: &str| {
            let mut o: Vec<String> = out
                .lines()
                .filter_map(|l| l.trim_start().split_once(':').map(|p| p.0.to_string()))
                .filter(|p| p.starts_with("0x"))
                .collect();
            o.dedup();
            o
        };
        let flat = ok(&fx, &args("--flat"));
        assert_eq!(offsets(&flat), ["0x51"], "{m:?}: {flat}");
        assert_eq!(flat.matches("needle").count(), 2, "{m:?}: {flat}");
        let tree = ok(&fx, &args("--tree"));
        assert_eq!(offsets(&tree), ["0x51"], "{m:?}: {tree}");
        assert_eq!(tree.matches("needle").count(), 2, "{m:?}: {tree}");
        let json = ok(&fx, &args("--json"));
        let rows: Vec<serde_json::Value> = json
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
            .filter(|v| v["type"] == "match")
            .collect();
        assert_eq!(rows.len(), 1, "{m:?}: {json}");
        assert_eq!(
            rows[0]["data"]["submatches"].as_array().unwrap().len(),
            2,
            "{m:?}"
        );
    }
}

/// M4 plan, D30: a document whose reading passes the reader's memory gets one warning, and
/// the search goes on with the other documents. The limit is lowered for the test
/// (`SEARCHFOR_READER_MIB`): the workbook's text, 4 MiB, takes more than 16 MiB to read.
#[test]
fn m4_16_a_reader_past_its_memory_is_one_warning() {
    let fx = fixture();
    let row: &[&str] = &["needle in a big sheet"];
    let rows: Vec<&[&str]> = vec![row; 200_000];
    std::fs::write(fx.path().join("m4/big.xlsx"), doc::xlsx(&[("S", &rows)])).unwrap();
    let o = run(fx
        .s4([
            "--docs",
            "--flat",
            "-c",
            "needle",
            "m4/big.xlsx",
            "m4/report.docx",
        ])
        .env("SEARCHFOR_READER_MIB", "16"));
    assert!(
        stderr(&o)
            .contains("m4/big.xlsx: xlsx: reading it needed more than the 16 MiB a reader may use"),
        "{}",
        stderr(&o)
    );
    assert!(stdout(&o).contains("m4/report.docx:3"), "{}", stdout(&o));
    assert_eq!(code(&o), 0);
    let o = run(&mut fx.s4(["--docs", "-c", "needle", "m4/big.xlsx"]));
    assert_eq!(
        (stdout(&o), stderr(&o)),
        ("200000\n".to_string(), String::new())
    );
}

/// Codex r4 #4: the worker's flag is never a pattern: `__extract-document` as the first word is
/// searched as rg searches it, and the flag without the worker's environment is refused as an
/// unknown flag, as rg refuses one.
#[test]
fn m4_17_the_readers_flag_is_no_pattern() {
    let fx = fixture();
    std::fs::write(
        fx.path().join("m4/words.txt"),
        "a __extract-document here\n",
    )
    .unwrap();
    // As the first argument, which `Fixture::s4` (it adds `--color never` first) would not
    // give.
    let s = run(
        std::process::Command::new(assert_cmd::cargo::cargo_bin("s4"))
            .args(["__extract-document", "m4/words.txt", "--no-trail"])
            .current_dir(fx.path())
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(std::process::Stdio::null()),
    );
    assert_eq!(stdout(&s), "a __extract-document here\n");
    assert_eq!(code(&s), 0);
    let s = run(
        std::process::Command::new(assert_cmd::cargo::cargo_bin("s4"))
            .args(["--__extract-document", "m4/words.txt"])
            .current_dir(fx.path())
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(std::process::Stdio::null()),
    );
    assert_eq!(code(&s), 2, "{}", stdout(&s));
    if common::reference_rg() {
        let r = run(&mut fx.rg(["__extract-document", "m4/words.txt"]));
        assert_eq!(stdout(&r), "a __extract-document here\n");
    }
}

/// Codex r4 #3: a document's path goes to its reader as bytes, whatever they are: not UTF-8,
/// a line break, a carriage return at the end.
#[cfg(target_os = "linux")]
#[test]
fn m4_18_any_path_reaches_the_reader() {
    use std::os::unix::ffi::OsStrExt;
    let fx = fixture();
    let report = std::fs::read(fx.path().join("m4/report.docx")).unwrap();
    // A name that ends in a carriage return has no extension: --format names its Format
    // (Codex r5 #3).
    for (name, format) in [
        (&b"m4/caf\xe9.docx"[..], None),
        (b"m4/two\nlines.docx", None),
        (b"m4/end\r.docx", None),
        (b"m4/end\r", Some("docx")),
    ] {
        let p = std::ffi::OsStr::from_bytes(name);
        std::fs::write(fx.path().join(p), &report).unwrap();
        let mut c = fx.s4(["--docs", "--flat", "-c", "Scope"]);
        if let Some(f) = format {
            c.args(["--format", f]);
        }
        let o = run(c.arg(p));
        assert_eq!(
            (stdout(&o), stderr(&o)),
            ("2\n".to_string(), String::new()),
            "{name:?}"
        );
    }
}

/// A PDF of `pages` pages sharing one content stream of `ops` operations: slow to read, so a
/// signal arrives while its reader works. The stream is compressed, so the file is small and
/// searching its bytes is quick.
fn slow_pdf(pages: usize, ops: usize) -> Vec<u8> {
    use lopdf::{Dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let mut stream = Stream::new(Dictionary::new(), b"h ".repeat(ops));
    stream.compress().unwrap();
    let content = doc.add_object(stream);
    let pages_id = doc.new_object_id();
    let kids: Vec<Object> = (0..pages)
        .map(|_| {
            let mut p = Dictionary::new();
            p.set("Type", Object::Name(b"Page".to_vec()));
            p.set("Parent", Object::Reference(pages_id));
            p.set("Contents", Object::Reference(content));
            Object::Reference(doc.add_object(p))
        })
        .collect();
    let mut pages_d = Dictionary::new();
    pages_d.set("Type", Object::Name(b"Pages".to_vec()));
    pages_d.set("Count", Object::Integer(pages as i64));
    pages_d.set("Kids", Object::Array(kids));
    doc.objects.insert(pages_id, Object::Dictionary(pages_d));
    let mut cat = Dictionary::new();
    cat.set("Type", Object::Name(b"Catalog".to_vec()));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = doc.add_object(cat);
    doc.trailer.set("Root", Object::Reference(cat_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// The pids whose parent is `parent`.
#[cfg(target_os = "linux")]
fn children_of(parent: u32) -> Vec<u32> {
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|st| {
                st.rsplit_once(") ")
                    .and_then(|(_, rest)| rest.split(' ').nth(1)?.parse::<u32>().ok())
                    == Some(parent)
            })
        })
        .collect()
}

/// Codex r4 #2: a SIGTERM to s4 alone kills and reaps its readers, and s4 dies by it; a
/// SIGKILL, which s4 cannot see, ends them too (the kernel's parent-death signal). Fable r6
/// #1: nothing is printed after the signal, neither for the documents whose readers it killed
/// nor for those still waiting for one: eight slow PDFs for the four readers. Code review r2,
/// Codex #5: the pattern is in each PDF's bytes and in none of their texts, so a document
/// searched as it is once its reader was killed, or refused one, would print its count; and
/// a SIGINT is sent to s4 alone and to its group, as the terminal sends Ctrl-C (pending r1,
/// lote A: s4 printed those counts before the signal ended it).
#[cfg(target_os = "linux")]
#[test]
fn m4_19_a_signal_to_sfor_ends_its_readers() {
    use std::io::Read;
    let fx = fixture();
    // Four pages of two million operations: under the reader's limit of operations.
    let slow = slow_pdf(4, 2_000_000);
    assert!(slow.starts_with(b"%PDF-") && slow.len() < 64 * 1024);
    let mut args = vec!["--docs".to_string(), "-c".into(), "^%PDF-".into()];
    for i in 0..8 {
        std::fs::write(fx.path().join(format!("m4/slow{i}.pdf")), &slow).unwrap();
        args.push(format!("m4/slow{i}.pdf"));
    }
    // The terminal's Ctrl-C reaches the whole group, readers included, whose id is s4's pid.
    for (sig, group) in [
        (libc::SIGINT, true),
        (libc::SIGINT, false),
        (libc::SIGTERM, false),
        (libc::SIGKILL, false),
    ] {
        let mut c = fx.s4(&args);
        if group {
            std::os::unix::process::CommandExt::process_group(&mut c, 0);
        }
        // A test run started with the signals ignored (a background job) would pass that on.
        // SAFETY: signal() is async-signal-safe, as pre_exec asks.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(&mut c, || {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                libc::signal(libc::SIGTERM, libc::SIG_DFL);
                Ok(())
            });
        }
        let mut child = c
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let started = std::time::Instant::now();
        let readers = loop {
            let r = children_of(pid);
            if !r.is_empty() || started.elapsed() > std::time::Duration::from_secs(10) {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(!readers.is_empty(), "no reader started");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let to = if group { -(pid as i32) } else { pid as i32 };
        // SAFETY: a signal to our own child, or to the group it leads.
        unsafe { libc::kill(to, sig) };
        let status = child.wait().unwrap();
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(sig)
        );
        let mut errors = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut errors)
            .unwrap();
        assert_eq!(errors, "", "after signal {sig}");
        let mut printed = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut printed)
            .unwrap();
        assert_eq!(printed, "", "after signal {sig}");
        let gone = std::time::Instant::now();
        while readers
            .iter()
            .any(|r| std::path::Path::new(&format!("/proc/{r}")).exists())
        {
            assert!(
                gone.elapsed() < std::time::Duration::from_secs(5),
                "{readers:?} still run after signal {sig}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

/// Code review r2, Fable #2: a signal s4 was started ignoring (`trap '' INT`, nohup, a
/// background job) stays ignored while its readers work, as rg leaves it: the search goes on
/// and ends as it would have without the signal.
#[cfg(target_os = "linux")]
#[test]
fn m4_26_a_signal_ignored_by_its_parent_is_ignored() {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    let fx = fixture();
    let slow = slow_pdf(4, 2_000_000);
    for i in 0..2 {
        std::fs::write(fx.path().join(format!("m4/slow{i}.pdf")), &slow).unwrap();
    }
    let count = ok(&fx, &["--docs", "-c", "needle", "m4/report.docx"]);
    let args = [
        "--docs",
        "-c",
        "needle",
        "m4/slow0.pdf",
        "m4/slow1.pdf",
        "m4/report.docx",
    ];
    for sig in [libc::SIGINT, libc::SIGTERM] {
        let mut c = fx.s4(args);
        // SAFETY: signal() is async-signal-safe, as pre_exec asks.
        unsafe {
            c.pre_exec(move || {
                libc::signal(sig, libc::SIG_IGN);
                Ok(())
            });
        }
        let mut child = c
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let started = std::time::Instant::now();
        while children_of(pid).is_empty() {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "no reader started"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        // SAFETY: a signal to our own child.
        unsafe { libc::kill(pid as libc::pid_t, sig) };
        let status = child.wait().unwrap();
        let (mut out, mut err) = (String::new(), String::new());
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        assert_eq!(status.code(), Some(0), "signal {sig}: {status:?}: {err}");
        assert_eq!(err, "", "signal {sig}");
        assert_eq!(out, format!("m4/report.docx:{count}"), "signal {sig}");
    }
}

/// Code review r3, Codex #1: a SIGINT or SIGTERM that s4's parent blocked, and that is
/// pending as s4 starts, never reaches it: a reader that fails, and a path that does not
/// exist, are reported as without the signal, with the same output and exit code.
#[cfg(target_os = "linux")]
#[test]
fn m4_28_a_signal_blocked_by_its_parent_is_no_stop() {
    use std::os::unix::process::CommandExt;
    let fx = fixture();
    let args = [
        "--docs",
        "--no-trail",
        "-j1",
        "-c",
        "needle",
        "m4/broken.docx",
        "m4/nonexistent",
    ];
    let free = run(&mut fx.s4(args));
    assert_eq!(code(&free), 2, "{}", stderr(&free));
    assert_eq!(stdout(&free), "m4/broken.docx:1\n");
    assert_eq!(stderr(&free).lines().count(), 2, "{}", stderr(&free));
    for sig in [libc::SIGINT, libc::SIGTERM] {
        let mut c = fx.s4(args);
        // A signal to the process, as the terminal's Ctrl-C, and not to the thread (raise).
        // SAFETY: sigemptyset, sigaddset, sigprocmask, getpid and kill are async-signal-safe,
        // as pre_exec asks; the mask and the pending signal survive the exec.
        unsafe {
            c.pre_exec(move || {
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigaddset(&mut set, sig);
                libc::sigprocmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
                libc::kill(libc::getpid(), sig);
                Ok(())
            });
        }
        let o = run(&mut c);
        assert_eq!(code(&o), 2, "signal {sig}: {}", stderr(&o));
        assert_eq!(stdout(&o), stdout(&free), "signal {sig}");
        assert_eq!(stderr(&o), stderr(&free), "signal {sig}");
    }
}

/// Fable r5 #1: a reader's own time limit holds for each document, never for its life: one
/// reader (`-j1`) reads workbooks that take less than the limit each and more together.
/// Fable r1 #8 (M4 code): the limit (`SEARCHFOR_READER_SECONDS`) is six times what one
/// workbook takes on this machine now, so a busy machine keeps its margin, and the workbooks
/// double until together they take longer than it, so a fast one still passes it.
#[test]
fn m4_20_the_readers_time_limit_is_per_document() {
    let fx = fixture();
    let row: &[&str] = &["needle in a sheet"];
    let rows: Vec<&[&str]> = vec![row; 3_500];
    let book = doc::xlsx(&[("S", &rows)]);
    let path = |i: usize| format!("m4/w{i:03}.xlsx");
    std::fs::write(fx.path().join(path(0)), &book).unwrap();
    let started = std::time::Instant::now();
    let o = run(&mut fx.s4(["--docs", "-c", "needle", &path(0)]));
    let one = started.elapsed();
    assert_eq!((stdout(&o), stderr(&o)), ("3500\n".into(), String::new()));
    let limit = std::time::Duration::from_secs((one.as_secs_f64() * 6.0).ceil().max(1.0) as u64);
    let mut books = 24;
    loop {
        let mut args = ["--docs", "-j1", "-c", "--sort", "path", "needle"]
            .map(String::from)
            .to_vec();
        let mut expected = String::new();
        for i in 0..books {
            let p = path(i);
            std::fs::write(fx.path().join(&p), &book).unwrap();
            expected.push_str(&format!("{p}:3500\n"));
            args.push(p);
        }
        let started = std::time::Instant::now();
        let o = run(fx
            .s4(&args)
            .env("SEARCHFOR_READER_SECONDS", limit.as_secs().to_string()));
        let took = started.elapsed();
        eprintln!(
            "{books} workbooks in {took:?}, {:?} each, one alone in {one:?}, against a limit of \
             {limit:?}",
            took / books as u32
        );
        assert_eq!(stderr(&o), "");
        assert_eq!(stdout(&o), expected);
        if took > limit {
            break;
        }
        assert!(books < 400, "{books} workbooks took less than {limit:?}");
        books *= 2;
    }
}

/// Codex r6 #2: the reader's own limit ends the document that passes it, counted from that
/// document's start on a monotonic clock: a reader idle for longer than the limit reads on,
/// and one whose document never ends (a FIFO nobody writes) exits `LATE` (3) about the limit
/// later.
#[cfg(target_os = "linux")]
#[test]
fn m4_21_a_document_past_the_readers_limit_ends_its_reader() {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    let fx = fixture();
    let fifo = fx.path().join("m4/never.docx");
    let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: mkfifo reads the path it is given.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    let mut reader = std::process::Command::new(assert_cmd::cargo::cargo_bin("s4"))
        .args(["--__extract-document", "1024"])
        .env("SEARCHFOR_READER_OF", std::process::id().to_string())
        .env("SEARCHFOR_READER_SECONDS", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(reader.try_wait().unwrap().is_none(), "an idle reader ended");
    let mut stdin = reader.stdin.take().unwrap();
    let path = fifo.as_os_str().as_bytes();
    writeln!(stdin, "docx 1024 {}", path.len()).unwrap();
    stdin.write_all(path).unwrap();
    stdin.flush().unwrap();
    let asked = std::time::Instant::now();
    // A limit of the test's own (M4 plan r7, Codex #2): a reader whose watchdog fails is killed
    // and reaped, and the test fails, instead of waiting on the FIFO for ever.
    let status = loop {
        if let Some(s) = reader.try_wait().unwrap() {
            break s;
        }
        if asked.elapsed() > std::time::Duration::from_secs(10) {
            let _ = reader.kill();
            let _ = reader.wait();
            panic!("the reader still read the FIFO 10 s after its 1 s limit");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let took = asked.elapsed();
    drop(stdin);
    assert_eq!(status.code(), Some(3), "{status:?}");
    assert!(
        took >= std::time::Duration::from_millis(900) && took < std::time::Duration::from_secs(10),
        "{took:?}"
    );
}

/// M4 plan, D31: the reader's own count of its memory ends it at its limit, the same on every
/// system. With `RLIMIT_DATA` far above the count (`SEARCHFOR_READER_RLIMIT_MIB`, 4 096 MiB), as on
/// macOS and Windows, where no `RLIMIT_DATA` holds, the workbook of `m4_16` still gives the
/// warning at 16 MiB, and the other document is read.
#[test]
fn m4_22_the_readers_own_count_ends_it_at_its_limit() {
    let fx = fixture();
    let row: &[&str] = &["needle in a big sheet"];
    let rows: Vec<&[&str]> = vec![row; 200_000];
    std::fs::write(fx.path().join("m4/big.xlsx"), doc::xlsx(&[("S", &rows)])).unwrap();
    let o = run(fx
        .s4([
            "--docs",
            "--flat",
            "-c",
            "needle",
            "m4/big.xlsx",
            "m4/report.docx",
        ])
        .env("SEARCHFOR_READER_MIB", "16")
        .env("SEARCHFOR_READER_RLIMIT_MIB", "4096"));
    assert!(
        stderr(&o)
            .contains("m4/big.xlsx: xlsx: reading it needed more than the 16 MiB a reader may use"),
        "{}",
        stderr(&o)
    );
    assert!(stdout(&o).contains("m4/report.docx:3"), "{}", stdout(&o));
    assert_eq!(code(&o), 0);
}

/// Bits of a deflate stream, the first in the lowest bit of each byte (RFC 1951).
#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    n: u32,
}

impl Bits {
    /// `v`'s `n` low bits, the lowest first: a block header or an extra field.
    fn put(&mut self, v: u32, n: u32) {
        for i in 0..n {
            if self.n.is_multiple_of(8) {
                self.out.push(0);
            }
            *self.out.last_mut().unwrap() |= (((v >> i) & 1) as u8) << (self.n % 8);
            self.n += 1;
        }
    }

    /// A Huffman code of `n` bits, its highest bit first.
    fn code(&mut self, c: u32, n: u32) {
        for i in (0..n).rev() {
            self.put(c >> i, 1);
        }
    }
}

/// A PDF whose one page draws a string of `1 + 258 × runs` letters `A`, its content stream
/// deflated with the fixed codes as one `A` and `runs` copies of the 258 bytes before, 13 bits
/// each: a stream about 160 times smaller than what it inflates to (M4 plan r8, Codex #1).
fn pdf_of_a_long_run(runs: usize) -> Vec<u8> {
    let (head, tail) = (&b"BT /F1 12 Tf 72 700 Td ("[..], &b") Tj ET"[..]);
    let mut bits = Bits::default();
    // The last block, with the fixed codes.
    bits.put(0b011, 3);
    // A literal below 144 is 8 bits, 0x30 above its byte.
    let literal = |b: &mut Bits, v: u8| b.code(0x30 + u32::from(v), 8);
    head.iter().for_each(|&v| literal(&mut bits, v));
    literal(&mut bits, b'A');
    for _ in 0..runs {
        // Length 258 is symbol 285, 8 bits; distance 1 is code 0, 5 bits.
        bits.code(0xc5, 8);
        bits.code(0, 5);
    }
    tail.iter().for_each(|&v| literal(&mut bits, v));
    // The end of the block, symbol 256: 7 bits of 0.
    bits.code(0, 7);
    let (mut a, mut b) = (1u32, 0u32);
    let n = 1 + 258 * runs;
    let text = head.iter().chain(std::iter::repeat_n(&b'A', n)).chain(tail);
    for &v in text {
        a = (a + u32::from(v)) % 65521;
        b = (b + a) % 65521;
    }
    let mut z = vec![0x78, 0x01];
    z.extend_from_slice(&bits.out);
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut stream =
        format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", z.len()).into_bytes();
    stream.extend_from_slice(&z);
    stream.extend_from_slice(b"\nendstream");
    let objects: [&[u8]; 5] = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 612 792] >>",
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
        &stream,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ];
    let mut pdf = b"%PDF-1.5\n".to_vec();
    let mut at = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        at.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(o);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for o in at {
        pdf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer << /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    pdf
}

/// Codex r8 #1: a reader refused memory inside lopdf, which ends the stream where its
/// `read_to_end` failed and goes on, gives the memory warning, its partial text dropped, as
/// one the refusal aborted. The page's 16 MiB stream, refused at 16 MiB, gave an empty text
/// without a warning; and at the reader's own limit the same PDF is read whole.
#[test]
fn m4_23_a_reader_refused_memory_it_went_on_without_warns() {
    let fx = fixture();
    std::fs::write(fx.path().join("m4/long.pdf"), pdf_of_a_long_run(16 << 12)).unwrap();
    let o = run(fx
        .s4([
            "--docs",
            "--flat",
            "-c",
            "needle",
            "m4/long.pdf",
            "m4/report.docx",
        ])
        .env("SEARCHFOR_READER_MIB", "16")
        .env("SEARCHFOR_READER_RLIMIT_MIB", "4096"));
    assert!(
        stderr(&o)
            .contains("m4/long.pdf: pdf: reading it needed more than the 16 MiB a reader may use"),
        "{}",
        stderr(&o)
    );
    assert!(stdout(&o).contains("m4/report.docx:3"), "{}", stdout(&o));
    assert_eq!(code(&o), 0);
    let o = run(&mut fx.s4(["--docs", "-c", "AAAAAAAA", "m4/long.pdf"]));
    assert_eq!((stdout(&o), stderr(&o)), ("1\n".to_string(), String::new()));
}

/// Code review r1, Codex #2 and #3: a `--pre` command that prints its output and then fails is
/// an error, exit 2, also where the search stops before the end of its output (-q, -m, -l); the
/// lines it found before are printed as rg 14.1.1 prints them: with one thread, as they come,
/// and with more, never (rg drops the file's buffer). The Trail layouts print them too.
#[cfg(unix)]
#[test]
fn m4_25_a_command_that_fails_late_is_an_error_after_its_lines() {
    use std::os::unix::fs::PermissionsExt;
    if !common::reference_rg() {
        return;
    }
    let fx = fixture();
    let d = fx.path().join("m4/late");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("a.txt"), "needle one\nother\n").unwrap();
    std::fs::write(d.join("b.txt"), "needle two\n").unwrap();
    let late = d.join("late.sh");
    std::fs::write(&late, "#!/bin/sh\ncat \"$1\"\necho boom >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&late, std::fs::Permissions::from_mode(0o755)).unwrap();
    let late = late.to_string_lossy().into_owned();
    let pre = |rest: &[&str]| -> Vec<String> {
        ["--pre", late.as_str()]
            .iter()
            .chain(rest)
            .map(|s| s.to_string())
            .collect()
    };
    let a = "m4/late/a.txt";
    let b = "m4/late/b.txt";
    for args in [
        pre(&["needle", a]),
        pre(&["-q", "needle", a]),
        pre(&["-m1", "needle", a]),
        pre(&["-c", "needle", a]),
        pre(&["-l", "needle", a]),
        pre(&["-v", "needle", a]),
        pre(&["--json", "needle", a]),
        pre(&["-m1", "--json", "needle", a]),
        pre(&["-j1", "needle", a, b]),
        pre(&["-q", "-j1", "needle", a, b]),
        pre(&["-l", "-j1", "needle", a, b]),
    ] {
        let s = run(&mut fx.s4(args.iter().map(String::as_str).chain(["--no-trail"])));
        let r = run(&mut fx.rg(&args));
        let json = args.contains(&"--json".to_string());
        let mask = |o: &std::process::Output| match json {
            true => common::mask_json_times(&stdout(o)),
            false => stdout(o),
        };
        assert_eq!(mask(&s), mask(&r), "{args:?}");
        assert_eq!(code(&s), 2, "{args:?}");
        assert_eq!(code(&s), code(&r), "{args:?}");
        assert_eq!(stderr(&s).replace("s4: ", "rg: "), stderr(&r), "{args:?}");
    }
    // With threads rg prints nothing of a file whose search failed; its two errors come in
    // either order.
    let args = pre(&["-j4", "needle", a, b]);
    let s = run(&mut fx.s4(args.iter().map(String::as_str).chain(["--no-trail"])));
    let r = run(&mut fx.rg(&args));
    assert_eq!((stdout(&s), code(&s)), (stdout(&r), code(&r)), "{args:?}");
    assert_eq!(stdout(&s), "");
    assert_eq!(stderr(&s).matches("boom").count(), 2, "{}", stderr(&s));
    // The Trail layouts: the rows found, then the error.
    let err = |path: &str, prefixed: bool| {
        let cmd = if prefixed {
            format!("preprocessor command failed: '\"{late}\" \"{path}\"': ")
        } else {
            String::new()
        };
        format!("s4: {path}: {cmd}\n{0}\nboom\n{0}\n", "-".repeat(79))
    };
    for (args, out, prefixed) in [
        (pre(&["--flat", "needle", a]), "1: needle one\n", true),
        (
            pre(&["--flat", "-m1", "needle", a]),
            "1: needle one\n",
            false,
        ),
        (
            pre(&["--flat", "-j1", "needle", a, b]),
            "m4/late/a.txt:1: needle one\nm4/late/b.txt:1: needle two\n",
            true,
        ),
        (pre(&["--flat", "-j4", "needle", a, b]), "", true),
    ] {
        let s = run(&mut fx.s4(&args));
        assert_eq!(stdout(&s), out, "{args:?}");
        assert_eq!(code(&s), 2, "{args:?}");
        if args.contains(&"-j4".to_string()) {
            assert_eq!(stderr(&s).matches("boom").count(), 2, "{}", stderr(&s));
        } else if args.contains(&"-j1".to_string()) {
            assert_eq!(stderr(&s), err(a, prefixed) + &err(b, prefixed), "{args:?}");
        } else {
            assert_eq!(stderr(&s), err(a, prefixed), "{args:?}");
        }
    }
    // --json: the file's begin and match, without its end, and nothing in the summary, as rg.
    let s = run(&mut fx.s4(pre(&["--json", "needle", a])));
    assert_eq!(code(&s), 2);
    assert_eq!(stderr(&s), err(a, true));
    let types: Vec<String> = stdout(&s)
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["type"].to_string())
        .collect();
    assert_eq!(types, ["\"begin\"", "\"match\"", "\"summary\""]);
    let summary: serde_json::Value =
        serde_json::from_str(stdout(&s).lines().last().unwrap()).unwrap();
    assert_eq!(summary["data"]["stats"]["matches"], 0);
    assert_eq!(summary["data"]["stats"]["searches"], 0);
    // A truncated gzip file: its decompressor fails after the lines it gave.
    let text: String = (0..50_000).map(|i| format!("needle {i}\n")).collect();
    std::fs::write(d.join("big.txt"), text).unwrap();
    let zipped = std::process::Command::new("gzip")
        .arg("-k")
        .arg(d.join("big.txt"))
        .status();
    if !zipped.is_ok_and(|s| s.success()) {
        return;
    }
    let gz = std::fs::read(d.join("big.txt.gz")).unwrap();
    std::fs::write(d.join("cut.gz"), &gz[..gz.len() / 2]).unwrap();
    let args = ["-z", "needle", "m4/late/cut.gz"];
    let s = run(&mut fx.s4(args.iter().chain(&["--no-trail"])));
    let r = run(&mut fx.rg(args));
    assert_eq!(code(&r), 2);
    assert!(stdout(&r).lines().count() > 1000);
    assert_eq!((stdout(&s), code(&s)), (stdout(&r), code(&r)));
    assert_eq!(stderr(&s).replace("s4: ", "rg: "), stderr(&r));
    let s = run(&mut fx.s4(args.iter().chain(&["--flat"])));
    assert_eq!(stdout(&s).lines().count(), stdout(&r).lines().count());
    assert_eq!(
        (code(&s), stderr(&s).replace("s4: ", "rg: ")),
        (2, stderr(&r))
    );
}
