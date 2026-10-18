//! End-to-end tests of M3's Formats (plan, Chunk 6): fixtures under
//! tests/fixtures/{markup,table,subs}, and Segment names with control characters (D14).
mod common;

use common::{Fixture, code, run, stderr, stdout};

const FORMATS: &[(&str, &str)] = &[
    ("html", "markup/page.html"),
    ("xml", "markup/feed.xml"),
    ("csv", "table/plans.csv"),
    ("tsv", "table/people.tsv"),
    ("srt", "subs/movie.srt"),
    ("vtt", "subs/talk.vtt"),
];

fn flat(fx: &Fixture, args: &[&str]) -> String {
    let o = run(&mut fx.sfor(args.iter().copied().chain(["--flat"])));
    assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
    assert_eq!(stderr(&o), "", "{args:?}");
    stdout(&o)
}

#[test]
fn m3_01_every_format_in_three_layouts() {
    // Spec §11 #2: a fixture and a snapshot of tree, one-line and JSON output per Format.
    let fx = Fixture::new();
    for (name, file) in FORMATS {
        for (layout, flag) in [("tree", "--tree"), ("flat", "--flat"), ("json", "--json")] {
            let o = run(&mut fx.sfor(["needle", file, flag]));
            assert_eq!(code(&o), 0, "{name} {layout}: {}", stderr(&o));
            assert!(stderr(&o).is_empty(), "{name} {layout}: {}", stderr(&o));
            let mut out = stdout(&o);
            if layout == "json" {
                common::assert_valid_json_schema(&out);
                out = common::mask_json_times(&out);
            }
            insta::assert_snapshot!(format!("m3_01_{name}_{layout}"), out);
        }
    }
}

#[test]
fn m3_02_html_element_paths() {
    // Spec §11 #18.
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "markup/page.html"]),
        "4: head › title │   <title>Pricing needle</title>\n\
         5: head › script │   <script>if (a < b) { label = \"</div> needle\"; }</script>\n\
         12: nav.top.menu › ul › li[3] │       <li><a href=\"/needle\">Needle</a>\n\
         19: main#content › section#pricing.card.wide › p.note[2] │       <p class=\"note\">The needle plan is free.\n\
         23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9\n\
         26: main#content › div › div › div │     <div><div><div>deep needle</div></div></div>\n\
         27: main#content │     <img src=\"needle.png\" alt=\"\">\n"
    );
}

#[test]
fn m3_03_xml_paths_and_a_syntax_error() {
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "markup/feed.xml"]),
        "3:   <!ENTITY pub \"needle press\">\n\
         6: catalog │   <!-- needle in a comment -->\n\
         12: catalog › book[2] › title │     <title>The needle</title>\n\
         14: catalog › book[2] › note │     <note><![CDATA[ a < b needle ]]></note>\n\
         17: catalog › magazine › title │   <magazine><title>needle monthly</title></magazine>\n"
    );
    assert_eq!(
        flat(&fx, &["needle", "markup/broken.xml"]),
        "2: root › ok │   <ok>needle before</ok>\n\
         3: root › a › b │   <a><b>needle inside</a>\n\
         4:   <c>needle after</c>\n"
    );
}

#[test]
fn m3_04_csv_and_tsv_rows_and_columns() {
    // Spec §11 #19.
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "table/plans.csv"]),
        "3: row 2 › Plan │ 2,Bea,needle,\"likes \"\"quotes\"\", and commas\"\n\
         5: row 3 › Notes │ lines with needle\"\n\
         7: row 4 › Notes │ 4,Dan,team,needle at the end,extra\n"
    );
    // `;` is sniffed from the first line; the CRLF stays in the printed line, as rg's.
    assert_eq!(
        flat(&fx, &["needle", "table/semi.csv"]),
        "2: row 1 › Plan │ Ana;needle;0\r\n3: row 2 › Precio │ Bea;pro;9,50 needle\r\n"
    );
    assert_eq!(
        flat(&fx, &["needle", "table/people.tsv"]),
        "2: row 1 › Note │ \"Ana\tadmin\tneedle \"quoted\n3: row 2 › Role │ Bea\tneedle\tok\n"
    );
}

#[test]
fn m3_05_subtitle_cues() {
    // Spec §11 #20: the cue number and its start time; BOM and CRLF in movie.srt.
    let fx = Fixture::new();
    assert_eq!(
        flat(&fx, &["needle", "subs/movie.srt"]),
        "3: cue 1 00:00:01,000 │ Where is the needle?\r\n\
         8: cue 2 00:00:04,000 │ - A needle, in there?\r\n\
         12: cue 3 00:01:02,500 │ <i>needle</i>\r\n"
    );
    assert_eq!(
        flat(&fx, &["needle", "subs/talk.vtt"]),
        "1: WEBVTT - needle talk\n\
         3: NOTE needle in a note\n\
         10: cue 2 00:02.500 │ <v Ana>The needle is here.\n\
         13: cue 3 01:00:00.000 │ needle at one hour\n"
    );
    let o = stdout(&run(&mut fx.sfor(["needle", "subs/movie.srt", "--tree"])));
    assert!(
        o.contains("  cue 2 00:00:04,000 --> 00:00:06,000\n    8:- A needle"),
        "{o}"
    );
}

#[test]
fn m3_06_within_and_expand() {
    let fx = Fixture::new();
    assert_eq!(
        flat(
            &fx,
            &["needle", "markup/page.html", "--within", "pricing › table"]
        ),
        "23: main#content › section#pricing.card.wide › table › tr[3] › td[2] │         <tr><td>Pro<td>needle 9\n"
    );
    // A CSV column's Section is its row (D10): --expand prints the record, both lines.
    let o = stdout(&run(&mut fx.sfor([
        "lines with",
        "table/plans.csv",
        "--expand",
    ])));
    assert_eq!(
        o,
        "4-5: row 3 › Notes\n3,Carla,pro,\"two\nlines with needle\"\n\n"
    );
}

#[test]
fn m3_07_in_headings() {
    // Element names, CSV headers (no Trail: the Match is the column's name) and cue times.
    let fx = Fixture::new();
    let names = |file| flat(&fx, &["--in-headings", "pricing|^Plan$|00:01", file]);
    assert_eq!(
        names("markup/page.html"),
        "16: main#content │     <section id=\"pricing\" class=\"card wide\">\n"
    );
    assert_eq!(names("table/plans.csv"), "1: id,Name,Plan,Notes\n");
    assert_eq!(
        names("subs/movie.srt"),
        "2: cue 1 00:00:01,000 │ 00:00:01,000 --> 00:00:03,500\r\n\
         11: cue 3 00:01:02,500 │ 00:01:02,500 --> 00:01:04,000\r\n"
    );
}

#[test]
fn m3_08_a_name_with_a_decoded_line_break_stays_on_its_row() {
    // The M2 leftover (D14): `"a\nb"` decodes to a line break in every Format with escapes.
    let fx = Fixture::new();
    for (file, body) in [
        ("k.json", "{\"a\\nb\": \"needle\"}\n"),
        ("k.toml", "\"a\\nb\" = \"needle\"\n"),
        ("k.yaml", "\"a\\nb\": needle\n"),
        ("k.js", "const o = {\"a\\nb\": () => \"needle\"};\n"),
    ] {
        std::fs::write(fx.file(file), body).unwrap();
        assert_eq!(
            flat(&fx, &["needle", file]),
            format!("1: a\\nb │ {body}"),
            "{file}"
        );
        let tree = stdout(&run(&mut fx.sfor(["needle", file, "--tree"])));
        assert_eq!(tree.lines().count(), 3, "{file}: {tree}");
    }
    let json = stdout(&run(&mut fx.sfor(["needle", "k.json", "--json"])));
    assert!(
        json.contains(r#""trail":[{"kind":"key","name":"a\nb""#),
        "{json}"
    );
    // An escape sequence in a key cannot drive the terminal.
    std::fs::write(fx.file("esc.json"), "{\"\\u001b[31m\": \"needle\"}\n").unwrap();
    assert_eq!(
        flat(&fx, &["needle", "esc.json"]),
        "1: \\u{1b}[31m │ {\"\\u001b[31m\": \"needle\"}\n"
    );
}

#[test]
fn m3_09_xml_without_an_extension_by_its_declaration() {
    let fx = Fixture::new();
    std::fs::write(
        fx.file("feed"),
        "<?xml version=\"1.0\"?>\n<r><a>needle</a></r>\n",
    )
    .unwrap();
    assert_eq!(
        flat(&fx, &["needle", "feed"]),
        "2: r › a │ <r><a>needle</a></r>\n"
    );
}

#[test]
fn m3_10_no_trail_is_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    for dir in ["markup", "table", "subs"] {
        // Both walk in parallel: compare the lines in order.
        let sorted = |o: std::process::Output| {
            let mut l: Vec<String> = stdout(&o).lines().map(String::from).collect();
            l.sort();
            l
        };
        let ours = sorted(run(&mut fx.sfor(["--no-trail", "-n", "needle", dir])));
        let theirs = sorted(run(&mut fx.rg(["-n", "needle", dir])));
        assert!(!ours.is_empty(), "{dir}");
        assert_eq!(ours, theirs, "{dir}");
    }
}

#[test]
fn m3_11_list_formats_has_the_six() {
    let fx = Fixture::new();
    let o = stdout(&run(&mut fx.sfor(["--list-formats"])));
    for (name, _) in FORMATS {
        assert!(
            o.lines()
                .any(|l| l.starts_with(&format!("{name} ")) && l.contains("built-in")),
            "{name}: {o}"
        );
    }
}

#[test]
fn m3_12_utf16_subtitles_keep_their_cues() {
    let fx = Fixture::new();
    let text = "1\r\n00:00:01,000 --> 00:00:02,000\r\nañá needle\r\n";
    let mut bytes = vec![0xFF, 0xFE];
    for u in text.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(fx.file("w.srt"), bytes).unwrap();
    assert_eq!(
        flat(&fx, &["needle", "w.srt"]),
        "3: cue 1 00:00:01,000 │ añá needle\r\n"
    );
}

#[test]
fn m3_13_latin1_headers_are_decoded_lossily() {
    let fx = Fixture::new();
    std::fs::write(fx.file("l.csv"), b"A\xf1o,Valor\n1999,needle\n").unwrap();
    let o = run(&mut fx.sfor(["needle", "l.csv", "--flat"]));
    assert_eq!(stdout(&o), "2: row 1 › Valor │ 1999,needle\n");
    std::fs::write(fx.file("m.csv"), b"A\xf1o,Valor\nneedle,1\n").unwrap();
    let o = run(&mut fx.sfor(["needle", "m.csv", "--flat"]));
    assert_eq!(stdout(&o), "2: row 1 › A\u{FFFD}o │ needle,1\n");
}

#[test]
fn m3_14_a_cut_name_stays_searchable_up_to_its_cut() {
    // M3 review r1, Codex #6: the visible part of a cut name is still a name.
    let fx = Fixture::new();
    let a = "A".repeat(50);
    let n = "ñ".repeat(50);
    std::fs::write(fx.file("long.csv"), format!("id,{a}\n1,x\n")).unwrap();
    std::fs::write(fx.file("long.html"), format!("<div id=\"{a}\">x</div>\n")).unwrap();
    std::fs::write(
        fx.file("long.vtt"),
        format!("WEBVTT\n\n{a}\n00:01.000 --> 00:02.000\nx\n"),
    )
    .unwrap();
    std::fs::write(fx.file("longn.csv"), format!("id,{n}\n1,x\n")).unwrap();
    for (file, pattern) in [
        ("long.csv", "AAAA"),
        ("long.html", "AAAA"),
        ("long.vtt", "AAAA"),
        ("longn.csv", "ññññ"),
    ] {
        let o = run(&mut fx.sfor(["--flat", "--in-headings", pattern, file]));
        assert_eq!(code(&o), 0, "{file}: {}", stderr(&o));
        assert_eq!(stdout(&o).lines().count(), 1, "{file}");
    }
    // Latin-1 before the letters: the replaced byte is a piece of its own (M3 review r2,
    // Codex #2).
    let l1 = |file: &str, body: Vec<u8>| {
        std::fs::write(fx.file(file), body).unwrap();
        let o = run(&mut fx.sfor(["--flat", "--in-headings", "AAAA", file]));
        assert_eq!(code(&o), 0, "{file}: {}", stderr(&o));
    };
    l1(
        "l1.csv",
        [&b"id,\xe9"[..], &[b'A'; 40], b"\n1,x\n"].concat(),
    );
    l1(
        "l1.html",
        [&b"<div id=\"\xe9"[..], &[b'A'; 36], b"\">x</div>\n"].concat(),
    );
    l1(
        "l1.vtt",
        [
            &b"WEBVTT\n\n\xe9"[..],
            &[b'A'; 40],
            b"\n00:01.000 --> 00:02.000\nx\n",
        ]
        .concat(),
    );
    // The cut's `…` maps to nothing.
    let o = run(&mut fx.sfor(["--in-headings", "…", "long.csv"]));
    assert_eq!(code(&o), 1);
}

#[test]
fn m3_15_a_name_match_across_a_replaced_byte_is_one_match() {
    // M3 review r3, Codex #1: `A\xe9B` is three pieces (D31); `A.*B` is one Match over 0..3.
    let fx = Fixture::new();
    std::fs::write(fx.file("l.csv"), b"A\xe9B\nvalue\n").unwrap();
    let out = stdout(&run(&mut fx.sfor([
        "--json",
        "--in-headings",
        "A.*B",
        "l.csv",
    ])));
    assert!(
        out.contains(r#""submatches":[{"match":{"bytes":"QelC"},"start":0,"end":3}]"#),
        "{out}"
    );
    assert!(out.contains(r#""matches":1,"#), "{out}");
}

#[test]
fn m3_16_a_quoted_header_cell_with_commas_keeps_its_semicolons() {
    // M3 code review r2, Codex #2: the commas inside a quoted field after a `;` are not
    // delimiters, so the sniff picks `;`.
    let fx = Fixture::new();
    std::fs::write(
        fx.file("people.csv"),
        "ID;\"Family, Given, Middle, Suffix\";Plan\n1;\"Doe, John, Paul, Jr\";needle\n",
    )
    .unwrap();
    assert_eq!(
        flat(&fx, &["--format", "csv", "needle", "people.csv"]),
        "2: row 1 › Plan │ 1;\"Doe, John, Paul, Jr\";needle\n"
    );
    let o = run(&mut fx.sfor(["--flat", "--within", "Suffix", "needle", "people.csv"]));
    assert_eq!(code(&o), 1, "{}", stdout(&o));
}

#[test]
fn m3_17_an_invalid_xml_declaration_ends_the_structure() {
    // M3 code review r2, Codex #3: the structure ends at `<?xml?>`, so no Match is inside `a`.
    let fx = Fixture::new();
    for (file, body) in [
        ("empty.xml", "<?xml?><r><a>needle</a></r>\n"),
        ("upper.xml", "<?XML version=\"1.0\"?><r><a>needle</a></r>\n"),
        (
            "subset.xml",
            "<!DOCTYPE r [<?xml version=\"1.0\"?>]><r><a>needle</a></r>\n",
        ),
    ] {
        std::fs::write(fx.file(file), body).unwrap();
        let o = run(&mut fx.sfor(["--format", "xml", "--flat", "--within", "a", "needle", file]));
        assert_eq!(code(&o), 1, "{file}: {}", stdout(&o));
    }
}

#[test]
fn m3_18_a_doctype_without_a_name_ends_the_structure() {
    // M3 code review r3, Codex #1: `<!DOCTYPEjunk>` and `<!DOCTYPE>` are not doctypes, so no
    // Match is inside `a`, read from a file or from stdin.
    let fx = Fixture::new();
    for (file, body) in [
        ("junk.xml", "<!DOCTYPEjunk><r><a>needle</a></r>\n"),
        ("bare.xml", "<!DOCTYPE><r><a>needle</a></r>\n"),
        // M3 code review r4, Codex #1: a valid prefix of the name is not enough.
        ("copy.xml", "<!DOCTYPE r©><r><a>needle</a></r>\n"),
        ("equals.xml", "<!DOCTYPE r=><r><a>needle</a></r>\n"),
    ] {
        std::fs::write(fx.file(file), body).unwrap();
        let o = run(&mut fx.sfor(["--format", "xml", "--flat", "--within", "a", "needle", file]));
        assert_eq!(code(&o), 1, "{file}: {}", stdout(&o));
        let input = std::fs::File::open(fx.file(file)).unwrap();
        let o = run(fx
            .sfor(["--format", "xml", "--flat", "--within", "a", "needle"])
            .stdin(input));
        assert_eq!(code(&o), 1, "{file} on stdin: {}", stdout(&o));
    }
    // A doctype with its name keeps the structure.
    std::fs::write(fx.file("named.xml"), "<!DOCTYPE r><r><a>needle</a></r>\n").unwrap();
    let o = run(&mut fx.sfor(["--flat", "--within", "a", "needle", "named.xml"]));
    assert_eq!(code(&o), 0);
    assert_eq!(stdout(&o), "1: r › a │ <!DOCTYPE r><r><a>needle</a></r>\n");
}

#[test]
fn m3_19_the_csv_delimiter_comes_from_the_first_record() {
    // M3 code review r3, Fable #2: a blank line before the header, or a header whose first cell
    // spans two lines, still gives `;`.
    let fx = Fixture::new();
    for (file, body, row) in [
        (
            "blank.csv",
            "\nID;Plan;Note\n1;needle;ok\n",
            "3: row 1 › Plan │ 1;needle;ok\n",
        ),
        (
            "multi.csv",
            "\"Name\nfull\";Plan;Note\nx;needle;y\n",
            "3: row 1 › Plan │ x;needle;y\n",
        ),
    ] {
        std::fs::write(fx.file(file), body).unwrap();
        let o = run(&mut fx.sfor(["--flat", "needle", file]));
        assert_eq!(code(&o), 0, "{file}");
        assert_eq!(stdout(&o), row, "{file}");
    }
}
