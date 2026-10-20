//! M5a (M5a plan, Chunk 9): every flag of ripgrep 14.1.1 that M5a implements, `s4
//! --no-trail` against rg 14.1.1 byte for byte (stdout and exit code), over the fixture copy
//! and the files each case writes into it. Skipped with a note when rg 14.1.1 is not
//! installed; `SEARCHFOR_REQUIRE_REFERENCE=1` makes that a failure.
mod common;

use common::{Fixture, code, reference_rg, run, stdout};
use std::process::Output;

/// Bytes the cases search besides the fixtures: CRLF lines, NUL-separated records, Latin-1,
/// a UTF-16LE file without a BOM, a file with a UTF-8 BOM, runs of lines for
/// `--stop-on-nonmatch`, indented lines for `--trim`, and a long line for `-M`.
fn write_m5_files(fx: &Fixture) {
    let dir = fx.file("m5");
    std::fs::create_dir_all(&dir).unwrap();
    let files: &[(&str, Vec<u8>)] = &[
        ("crlf.txt", b"one needle\r\ntwo\r\nneedle$\r\n".to_vec()),
        (
            "records.dat",
            b"one needle\ntwo\0three\0needle four\n".to_vec(),
        ),
        ("latin1.txt", b"caf\xe9 needle\n".to_vec()),
        (
            "utf16.txt",
            "a needle\nb\n"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ),
        ("bom.txt", b"\xef\xbb\xbfneedle bom\n".to_vec()),
        ("stop.txt", b"a needle\nb needle\nc\nd needle\n".to_vec()),
        ("trim.txt", b"   needle  x\n\tneedle\n".to_vec()),
        (
            "long.txt",
            format!(
                "{} needle {}\nshort needle\n",
                "x".repeat(60),
                "y".repeat(60)
            )
            .into_bytes(),
        ),
        (
            "block.md",
            b"# Install\n\nSome intro needle here.\n\n## Linux\n\nRun the needle command:\n"
                .to_vec(),
        ),
    ];
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}

fn rg(fx: &Fixture, args: &[&str]) -> Output {
    run(&mut fx.rg(args))
}

fn s4(fx: &Fixture, args: &[&str]) -> Output {
    let mut a = vec!["--no-docs", "--no-trail"];
    a.extend_from_slice(args);
    run(&mut fx.s4(&a))
}

/// Every case, `--sort path` first so both walk in one order; all differences reported at once.
fn compare(fx: &Fixture, cases: &[Vec<&str>]) {
    let mut diffs = Vec::new();
    for case in cases {
        let mut a = vec!["--sort", "path"];
        a.extend_from_slice(case);
        let (want, got) = (rg(fx, &a), s4(fx, &a));
        if (code(&want), stdout(&want)) != (code(&got), stdout(&got)) {
            diffs.push(format!(
                "== {case:?}\n-- rg (exit {}):\n{}\n-- s4 (exit {}):\n{}",
                code(&want),
                stdout(&want),
                code(&got),
                stdout(&got)
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "{} case(s) differ:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}

#[test]
fn m5_output_flags_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    let cases: Vec<Vec<&str>> = vec![
        vec!["--column", "needle"],
        vec!["--column", "-n", "needle", "md"],
        vec!["--no-column", "--vimgrep", "needle", "md"],
        vec!["-b", "needle", "md"],
        vec!["-b", "-o", "needle", "md"],
        // Each negation after its flag (review r1 of the plan, Fable #3; D2).
        vec!["-b", "--no-byte-offset", "needle", "md"],
        vec!["--heading", "needle", "md"],
        vec!["--no-heading", "-n", "needle", "md"],
        vec!["--pretty", "needle", "md"],
        vec!["-p", "--no-heading", "needle", "md"],
        vec!["--vimgrep", "needle"],
        vec!["-o", "needle-\\w+", "md"],
        vec!["-o", "-C1", "needle", "md/guide.md"],
        vec!["-r", "X", "needle", "md"],
        vec!["-r", "<$0>", "-o", "needle", "md"],
        vec!["--replace", "${1}!", "(needle)-(\\w+)", "md"],
        vec!["-0", "needle", "md"],
        vec!["-0", "--heading", "needle", "md"],
        vec!["-0", "-c", "needle", "md"],
        vec!["-0", "-l", "needle"],
        vec!["--path-separator", "\\", "needle", "md"],
        vec!["--path-separator", "", "-l", "needle", "md"],
        vec!["--trim", "needle", "m5/trim.txt"],
        vec!["--trim", "--no-trim", "needle", "m5/trim.txt"],
        vec!["--passthru", "needle", "md/guide.md"],
        vec!["--passthrough", "-n", "needle", "md/twice.md"],
        vec!["--passthru", "-A1", "needle", "md/guide.md"],
        vec!["-C1", "--context-separator", "##", "needle", "md"],
        vec!["-C1", "--no-context-separator", "needle", "md"],
        vec![
            "-C1",
            "--field-context-separator",
            "~",
            "--field-match-separator",
            "=",
            "-n",
            "needle",
            "md",
        ],
        vec!["-M", "20", "--max-columns-preview", "needle", "m5/long.txt"],
        vec![
            "-M",
            "20",
            "--max-columns-preview",
            "--no-max-columns-preview",
            "needle",
            "m5/long.txt",
        ],
        vec![
            "--color",
            "always",
            "-M",
            "20",
            "--max-columns-preview",
            "needle",
            "m5/long.txt",
        ],
        vec![
            "--color",
            "always",
            "--colors",
            "match:fg:blue",
            "--colors",
            "path:none",
            "needle",
            "md",
        ],
        vec![
            "--color",
            "always",
            "--colors",
            "line:style:bold",
            "--colors",
            "column:fg:green",
            "--column",
            "-n",
            "needle",
            "md",
        ],
        vec![
            "--color",
            "always",
            "--hyperlink-format",
            "default",
            "needle",
            "md",
        ],
        vec![
            "--color",
            "always",
            "--heading",
            "--hyperlink-format",
            "file://{path}:{line}:{column}",
            "--column",
            "needle",
            "md",
        ],
        vec!["-c", "--include-zero", "needle", "md", "misc"],
        vec![
            "-c",
            "--include-zero",
            "--no-include-zero",
            "needle",
            "md",
            "misc",
        ],
        vec!["-o", "-c", "needle", "md"],
        vec!["--line-buffered", "needle", "md"],
        vec!["--block-buffered", "needle", "md"],
        vec!["--no-line-buffered", "--no-block-buffered", "needle", "md"],
    ];
    compare(&fx, &cases);
}

#[test]
fn m5_search_flags_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    let cases: Vec<Vec<&str>> = vec![
        vec!["-P", "needle(?=-)", "md"],
        vec!["--pcre2", "(\\w)\\1", "md"],
        vec!["-P", "--no-pcre2", "needle", "md"],
        vec!["--engine", "pcre2", "-i", "-w", "NEEDLE", "md"],
        vec!["--engine", "auto", "needle-(?=pkg)", "md"],
        vec!["--auto-hybrid-regex", "needle-(?=pkg)", "md"],
        vec!["--engine", "default", "needle", "md"],
        vec!["(?=x)", "md"],
        vec!["-P", "("],
        vec!["--engine", "auto", "(?<=a)("],
        vec!["a\\x00b", "md"],
        vec!["-a", "a\\x00b", "md"],
        vec!["--no-unicode", "-o", "\\w+", "m5/latin1.txt"],
        vec!["--no-unicode", "--unicode", "-o", "\\w+", "m5/latin1.txt"],
        vec!["--no-pcre2-unicode", "-P", "-o", "\\w+", "m5/latin1.txt"],
        vec![
            "--no-pcre2-unicode",
            "--pcre2-unicode",
            "-P",
            "-o",
            "\\w+",
            "m5/latin1.txt",
        ],
        vec![
            "--auto-hybrid-regex",
            "--no-auto-hybrid-regex",
            "needle-(?=pkg)",
            "md",
        ],
        vec![
            "-U",
            "--multiline-dotall",
            "--no-multiline-dotall",
            "-n",
            "Linux.*Run",
            "m5/block.md",
        ],
        vec!["--crlf", "needle$", "m5/crlf.txt"],
        vec!["needle$", "m5/crlf.txt"],
        vec!["--crlf", "--no-crlf", "needle$", "m5/crlf.txt"],
        vec!["--null-data", "-n", "needle", "m5/records.dat"],
        vec!["--null-data", "--crlf", "needle$", "m5/crlf.txt"],
        vec!["-U", "-n", "intro needle here\\.\\n\\n##", "m5/block.md"],
        vec![
            "-U",
            "--multiline-dotall",
            "-n",
            "Linux.*Run",
            "m5/block.md",
        ],
        vec!["-U", "--no-multiline", "Linux\\n", "m5/block.md"],
        vec!["-U", "-o", "-n", "Linux\\n\\nRun", "m5/block.md"],
        vec!["-U", "--vimgrep", "Linux\\n\\nRun", "m5/block.md"],
        vec!["-U", "-c", "Linux\\n\\nRun", "m5/block.md"],
        vec!["--stop-on-nonmatch", "-n", "needle", "m5/stop.txt"],
        vec!["--dfa-size-limit", "1K", "needle", "md"],
        vec!["--regex-size-limit", "10", "a{1000}", "md"],
        vec!["-E", "latin1", "café", "m5/latin1.txt"],
        vec!["-E", "utf-16le", "needle", "m5/utf16.txt"],
        vec!["-E", "none", "needle", "m5/bom.txt"],
        vec!["-E", "utf-8", "needle", "m5/bom.txt"],
        vec!["-E", "latin1", "--no-encoding", "needle", "m5/latin1.txt"],
        vec!["--mmap", "-n", "needle", "md"],
        vec!["--no-mmap", "-n", "needle", "md/guide.md"],
    ];
    compare(&fx, &cases);
}

#[test]
fn m5_walk_flags_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    std::fs::write(fx.file("extra.ignore"), "md/twice.md\n").unwrap();
    let cases: Vec<Vec<&str>> = vec![
        vec!["-l", "needle"],
        vec!["--no-ignore-vcs", "-l", "needle"],
        vec!["--no-ignore", "--ignore-vcs", "-l", "needle"],
        // Each negation after its flag (review r1 of the plan, Fable #3; D2).
        vec!["--no-ignore", "--ignore-dot", "-l", "needle"],
        vec!["--no-ignore", "--ignore-exclude", "-l", "needle"],
        vec!["--no-ignore-global", "--ignore-global", "-l", "needle"],
        vec![
            "--ignore-file",
            "extra.ignore",
            "--no-ignore-files",
            "--ignore-files",
            "-l",
            "needle",
        ],
        vec![
            "--ignore-file",
            "missing.ignore",
            "--no-ignore-messages",
            "--ignore-messages",
            "-l",
            "needle",
        ],
        vec![
            "--ignore-file-case-insensitive",
            "--no-ignore-file-case-insensitive",
            "-l",
            "needle",
        ],
        vec!["--one-file-system", "--no-one-file-system", "-l", "needle"],
        vec!["--no-ignore-dot", "-l", "needle"],
        vec!["--no-ignore-exclude", "-l", "needle"],
        vec!["--no-ignore-global", "-l", "needle"],
        vec!["--no-ignore-parent", "-l", "needle"],
        vec!["--ignore-file", "extra.ignore", "-l", "needle"],
        vec![
            "--ignore-file",
            "extra.ignore",
            "--no-ignore-files",
            "-l",
            "needle",
        ],
        vec!["--ignore-file", "missing.ignore", "-l", "needle"],
        vec!["--ignore-file-case-insensitive", "-l", "needle"],
        vec!["--no-require-git", "-l", "needle"],
        vec!["--require-git", "-l", "needle"],
        vec!["--one-file-system", "-l", "needle"],
        vec!["--max-filesize", "100", "-l", "needle"],
        vec!["--max-filesize", "1K", "-l", "needle", "md/guide.md"],
        vec!["--glob-case-insensitive", "-g", "*.MD", "-l", "needle"],
        vec![
            "--glob-case-insensitive",
            "--no-glob-case-insensitive",
            "-g",
            "*.MD",
            "-l",
            "needle",
        ],
        vec!["--type-add", "notes:*.md", "-t", "notes", "-l", "needle"],
        vec![
            "--type-add",
            "notes:*.md",
            "--type-clear",
            "notes",
            "-t",
            "notes",
            "-l",
            "needle",
        ],
        vec!["--type-clear", "markdown", "-T", "markdown", "-l", "needle"],
        vec!["--type-add", "bad", "-l", "needle"],
        vec!["--sortr", "path", "-l", "needle"],
        vec!["--sort-files", "-l", "needle"],
        vec![
            "--sort-files",
            "--no-sort-files",
            "--sort",
            "path",
            "-l",
            "needle",
        ],
        vec!["--files"],
        vec!["--files", "md"],
        vec!["--files", "-0", "md"],
        vec!["--files", "--path-separator", "\\", "-g", "*.md"],
        vec!["--files", "-q"],
        vec!["--files", "-g", "*.zzz"],
        vec!["--files", "--type-list"],
        vec!["--type-list", "--files", "md"],
        // The types as the command line changes them (review r2 of the plan, Codex #3).
        vec!["--type-add", "review:*.review", "--type-list"],
        vec!["--type-clear", "rust", "--type-list"],
        vec![
            "--type-add",
            "rust:*.aa",
            "--type-add",
            "web:include:html,css",
            "--type-list",
        ],
        vec!["--maxdepth", "1", "-l", "needle"],
        vec!["-g", "*.zzz", "needle"],
    ];
    compare(&fx, &cases);
}

/// `--stats`: rg's block, with its times masked.
#[test]
fn m5_stats_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let mask = |o: &Output| {
        let re = regex_lite::Regex::new(r"[0-9]+\.[0-9]{6} seconds").unwrap();
        re.replace_all(&stdout(o), "T seconds").into_owned()
    };
    for case in [
        &["--stats", "needle", "md"][..],
        &["--stats", "-c", "needle", "md", "misc"],
        &["--stats", "-l", "needle", "md", "misc"],
        &["--stats", "-q", "needle", "md", "misc"],
        &["--stats", "--no-stats", "needle", "md"],
    ] {
        let mut a = vec!["--sort", "path"];
        a.extend_from_slice(case);
        let (want, got) = (rg(&fx, &a), s4(&fx, &a));
        assert_eq!(
            (code(&want), mask(&want)),
            (code(&got), mask(&got)),
            "{case:?}"
        );
    }
}

/// `--json` with the search flags of M5a: rg's messages, times masked.
#[test]
fn m5_json_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    for case in [
        &["-U", "Linux\\n\\nRun", "m5/block.md"][..],
        &["--null-data", "needle", "m5/records.dat"],
        &["--crlf", "needle$", "m5/crlf.txt"],
        &["-P", "needle(?=-)", "md"],
        &["-E", "latin1", "café", "m5/latin1.txt"],
        &["--passthru", "needle", "md/twice.md"],
        &[
            "-o",
            "-r",
            "X",
            "--vimgrep",
            "--column",
            "-b",
            "needle",
            "md",
        ],
    ] {
        let mut a = vec!["--sort", "path", "--json"];
        a.extend_from_slice(case);
        let (want, got) = (rg(&fx, &a), s4(&fx, &a));
        assert_eq!(
            (code(&want), common::mask_json_times(&stdout(&want))),
            (code(&got), common::mask_json_times(&stdout(&got))),
            "{case:?}"
        );
    }
}

/// Code review r1 of M5a, Codex #3: `--expand`'s Sections print their lines as the rows do. In
/// the tree each Match line of a Section is `line:column:offset:text` with its Matches
/// replaced, rg's `-n --column -b -r` line, and with `--crlf` no line keeps its `\r`.
#[test]
fn m5_expand_rows_equal_ripgreps_lines() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    std::fs::write(
        fx.file("m5/crlf.md"),
        b"# Install\r\n\r\nRun the needle here\r\nafter\r\n",
    )
    .unwrap();
    // A Match row of the tree: three numbers, each followed by `:`.
    let is_match_row = |l: &&str| {
        let f: Vec<&str> = l.splitn(4, ':').collect();
        f.len() == 4
            && f[..3]
                .iter()
                .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    for (file, crlf) in [("m5/block.md", false), ("m5/crlf.md", true)] {
        let mut a = vec!["--column", "-b", "-r", "X"];
        if crlf {
            a.push("--crlf");
        }
        a.extend(["needle", file]);
        let mut want = vec!["-n", "--no-filename"];
        want.extend(&a);
        let want: std::collections::BTreeSet<String> = stdout(&rg(&fx, &want))
            .lines()
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect();
        let mut got = vec!["--no-docs", "--tree", "--expand"];
        got.extend(&a);
        let out = stdout(&run(&mut fx.s4(&got)));
        assert!(!out.contains('\r'), "{file}: {out:?}");
        let got: std::collections::BTreeSet<String> = out
            .lines()
            .map(str::trim_start)
            .filter(is_match_row)
            .map(String::from)
            .collect();
        assert_eq!(got, want, "{file}:\n{out:?}");
    }
}

/// Code review r2 of M5a, Codex #3: `--vimgrep -b` in the one-line layout prints the column and
/// offset rg prints, chosen by the search's mode and not by how many lines a Match holds: a
/// search that can match a line break (`-U` with `-P`, or with a pattern of `\n`) prints the
/// offset of the Match's line (of its record with `--null-data`), one that cannot the offset of
/// the Match. Each row is compared by its `path:line:column:offset:` and, without
/// `--null-data`, its text: s4 prints a space before the text, and of a record the line that
/// holds the Match where rg prints the record (spec §8).
#[test]
fn m5_vimgrep_offsets_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    write_m5_files(&fx);
    std::fs::write(
        fx.file("m5/vim.txt"),
        b"xx needle\none\nxx needle two\nthree\n",
    )
    .unwrap();
    std::fs::write(fx.file("m5/vim.dat"), b"a\nb needle\0c\0").unwrap();
    let mut cases: Vec<(Vec<&str>, bool)> = Vec::new();
    for engine in [&[][..], &["-P"][..]] {
        for multi in [&[][..], &["-U"][..]] {
            let mut a = engine.to_vec();
            a.extend(multi);
            a.extend(["needle", "m5/vim.txt"]);
            cases.push((a, false));
            for file in ["m5/records.dat", "m5/vim.dat"] {
                let mut a = engine.to_vec();
                a.extend(multi);
                a.extend(["--null-data", "-a", "needle", file]);
                cases.push((a, true));
            }
        }
        for pattern in ["needle|two\\n", "needle two\\nthree"] {
            let mut a = engine.to_vec();
            a.extend(["-U", pattern, "m5/vim.txt"]);
            cases.push((a, false));
        }
    }
    // Each row's four fields and, without `--null-data`, its text; rg ends a record with its
    // NUL.
    let rows = |out: &str, null: bool, space: bool| -> Vec<String> {
        let term = if null && !space { '\0' } else { '\n' };
        out.split_terminator(term)
            .map(|l| {
                let f: Vec<&str> = l.splitn(5, ':').collect();
                let text = f.last().copied().unwrap_or_default();
                let text = if null { "" } else { text };
                let text = if space {
                    text.strip_prefix(' ').unwrap_or(text)
                } else {
                    text
                };
                format!("{}:{}", f[..f.len() - 1].join(":"), text)
            })
            .collect()
    };
    let mut diffs = Vec::new();
    for (case, null) in &cases {
        let mut want = vec!["--vimgrep", "-b"];
        want.extend(case);
        let mut got = vec!["--no-docs", "--flat", "--vimgrep", "-b"];
        got.extend(case);
        let (want, got) = (rg(&fx, &want), run(&mut fx.s4(&got)));
        let (want, got) = (
            rows(&stdout(&want), *null, false),
            rows(&stdout(&got), *null, true),
        );
        if want != got {
            diffs.push(format!("== {case:?}\n-- rg:\n{want:?}\n-- s4:\n{got:?}"));
        }
    }
    assert!(
        diffs.is_empty(),
        "{} case(s) differ:\n{}",
        diffs.len(),
        diffs.join("\n")
    );
}
