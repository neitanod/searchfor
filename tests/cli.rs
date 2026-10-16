//! End-to-end tests of the `sfor` binary (plan, Task 9.2). Every case runs in a copy of
//! `tests/fixtures/` with stdin from /dev/null unless it feeds stdin on purpose.
mod common;

use common::{Fixture, code, run, stderr, stdout};
use std::io::Write;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn sfor(args: &[&str]) -> std::process::Output {
    run(&mut Fixture::new().sfor(args))
}

#[test]
fn t01_tree_shows_the_trail_of_a_match() {
    let o = sfor(&["needle-pkg", "md", "--tree"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    for want in [
        "md/guide.md",
        "Deploy guide",
        "# Install",
        "## Linux",
        "```bash",
        "12:apt install needle-pkg",
    ] {
        assert!(out.contains(want), "missing {want:?} in:\n{out}");
    }
    insta::assert_snapshot!("t01_tree", out);
}

#[test]
fn t02_flat_prints_every_match_with_its_trail() {
    let o = sfor(&["needle", "md", "--flat"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("md/guide.md:24:"))
            .count(),
        2,
        "{out}"
    );
    insta::assert_snapshot!("t02_flat", out);
}

#[test]
fn t03_json_lines_parse() {
    let o = sfor(&["needle", "md", "--json"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    for line in stdout(&o).lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["type"] == "match" {
            assert!(v["data"].get("trail").is_some(), "{line}");
            assert!(v["data"].get("format").is_some(), "{line}");
        }
    }
}

#[test]
fn t03_json_matches_validate_against_the_schema() {
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/json-schema.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let o = sfor(&["needle", "md", "--json"]);
    for line in stdout(&o).lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v["type"] == "match" {
            assert!(validator.is_valid(&v["data"]), "{line}");
        }
    }
}

#[test]
fn t04_within_keeps_only_the_linux_matches() {
    let o = sfor(&["needle", "md", "--within", "install › linux", "--flat"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert_eq!(out.lines().count(), 2, "{out}");
    assert!(out.lines().all(|l| l.contains("Linux")), "{out}");
    insta::assert_snapshot!("t04_within", out);
}

#[test]
fn t05_expand_prints_the_table_once_under_plans() {
    let o = sfor(&["needle-10", "md", "--expand", "--tree"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(out.contains("## Plans"), "{out}");
    for n in 21..=24 {
        assert_eq!(
            out.lines()
                .filter(|l| l.trim_start().starts_with(&format!("{n}")))
                .count(),
            1,
            "line {n}:\n{out}"
        );
    }
    insta::assert_snapshot!("t05_expand_tree", out);
}

#[test]
fn t06_expand_prints_a_shared_section_once() {
    let o = sfor(&["needle", "md", "--expand", "--flat"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert_eq!(
        out.lines()
            .filter(|l| l.starts_with("md/guide.md:21-24:"))
            .count(),
        1,
        "{out}"
    );
    insta::assert_snapshot!("t06_expand_flat", out);
}

#[test]
fn t07_in_headings_matches_names_only() {
    let o = sfor(&["--in-headings", "^Linux$", "md", "--flat"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert_eq!(out.lines().count(), 1, "{out}");
    assert!(out.starts_with("md/guide.md:9:"), "{out}");
    let o = sfor(&["--in-headings", "needle", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
}

#[test]
fn t08_long_line_becomes_fragments_with_counts() {
    let o = sfor(&[
        "needle",
        "misc/min.json",
        "--flat",
        "--fragment-width",
        "120",
    ]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    let line_len = std::fs::read_to_string(common::source_dir().join("misc/min.json"))
        .unwrap()
        .trim_end()
        .chars()
        .count();
    let marker = regex_lite::Regex::new(r"^(?:…\[\+(\d+)\] )?(.*?)(?: \[…\+(\d+)\])?$").unwrap();
    let mut shown = 0;
    for l in out.lines() {
        assert!(l.chars().count() <= 120, "{} chars: {l}", l.chars().count());
        let text = l
            .strip_prefix("1: ")
            .unwrap_or_else(|| panic!("prefix: {l}"));
        let c = marker.captures(text).unwrap();
        let n = |i| c.get(i).map_or(0, |m| m.as_str().parse::<usize>().unwrap());
        assert_eq!(n(1) + c[2].chars().count() + n(3), line_len, "{l}");
        shown += c[2].matches("needle").count();
    }
    assert_eq!(shown, 3, "{out}");
    insta::assert_snapshot!("t08_fragments", out);
}

#[test]
fn t09_list_formats_shows_an_override() {
    let fx = Fixture::new();
    std::fs::write(
        fx.formats.path().join("markdown.toml"),
        "name='markdown'\nextensions=['md']\nprocessor='markdown'\n",
    )
    .unwrap();
    let o = run(&mut fx.sfor(["--list-formats"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("overridden ("), "{}", stdout(&o));
}

#[test]
fn t10_export_refuses_the_user_formats_directory() {
    let fx = Fixture::new();
    let o = run(&mut fx.sfor([
        std::ffi::OsStr::new("--export-formats"),
        fx.formats.path().as_os_str(),
    ]));
    assert_eq!(code(&o), 2);
    assert!(stderr(&o).contains("refusing"), "{}", stderr(&o));
}

#[test]
fn t10_export_writes_the_templates() {
    let fx = Fixture::new();
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("t");
    let o = run(&mut fx.sfor([std::ffi::OsStr::new("--export-formats"), target.as_os_str()]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("markdown.toml"), "{}", stdout(&o));
    assert!(target.join("markdown.toml").is_file());
}

#[test]
fn t11_not_yet_flags_fail_loudly() {
    for argv in [&["--pcre2", "x"][..], &["-nP", "x"]] {
        let o = sfor(argv);
        assert_eq!(code(&o), 2, "{argv:?}");
        assert!(
            stderr(&o).contains("not supported yet"),
            "{argv:?}: {}",
            stderr(&o)
        );
    }
}

#[test]
fn t12_the_word_formats_is_searched() {
    let o = sfor(&["formats", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
}

#[test]
fn t13_quiet() {
    let o = sfor(&["-q", "needle", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (0, ""));
    let o = sfor(&["-q", "absent", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
}

#[test]
fn t14_nothing_left_after_filters_exits_1() {
    let o = sfor(&["--within", "nowhere", "needle", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
}

#[test]
fn t15_summary_flags_count_after_filters() {
    let o = sfor(&["-c", "--within", "linux", "needle", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (0, "md/guide.md:2\n"));
    let o = sfor(&["-l", "--within", "linux", "needle", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (0, "md/guide.md\n"));
    let o = sfor(&["--count-matches", "--within", "linux", "needle", "md"]);
    assert_eq!((code(&o), stdout(&o).as_str()), (0, "md/guide.md:2\n"));
}

#[test]
fn t16_binary_file_prints_ripgreps_message() {
    let o = sfor(&["binary", "misc/bin.dat", "--flat"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        "binary file matches (found \"\\0\" byte around offset 17)\n"
    );
}

#[test]
fn t17_in_headings_with_twice() {
    let fx = Fixture::new();
    let o = run(&mut fx.sfor(["--in-headings", "-i", "plan", "md/twice.md", "--json"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let msgs: Vec<serde_json::Value> = stdout(&o)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let matches: Vec<&serde_json::Value> = msgs.iter().filter(|m| m["type"] == "match").collect();
    assert_eq!(matches.len(), 3, "{}", stdout(&o));
    assert_eq!(matches[0]["data"]["lines"]["text"], "# Plan of plans\n");
    assert_eq!(
        matches[0]["data"]["submatches"].as_array().unwrap().len(),
        2
    );

    let o = run(&mut fx.sfor([
        "--in-headings",
        "-m1",
        "-i",
        "plan",
        "md/twice.md",
        "--flat",
    ]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stdout(&o).lines().count(), 1, "{}", stdout(&o));
    assert!(stdout(&o).starts_with("1:"), "{}", stdout(&o));

    let o = run(&mut fx.sfor(["--in-headings", "-m0", "-i", "plan", "md/twice.md"]));
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));

    let o = run(&mut fx.sfor(["--in-headings", "-C1", "^Linux$", "md/guide.md", "--flat"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    let nums: Vec<&str> = out
        .lines()
        .map(|l| &l[..l.find(|c: char| !c.is_ascii_digit()).unwrap() + 1])
        .collect();
    assert_eq!(nums, ["8-", "9:", "10-"], "{out}");

    let o = run(&mut fx.sfor(["--in-headings", "^", "md/twice.md", "--json"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let subs: Vec<serde_json::Value> = stdout(&o)
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|m| m["type"] == "match")
        .map(|m| m["data"]["submatches"].clone())
        .collect();
    assert_eq!(subs.len(), 3, "{}", stdout(&o));
    for s in &subs {
        let s = s.as_array().unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0]["match"]["text"], "");
    }

    let o = run(&mut fx.sfor(["--in-headings", "-v", "x", "md"]));
    assert_eq!(code(&o), 2);
    assert!(stderr(&o).contains("--in-headings"), "{}", stderr(&o));
}

#[test]
fn t18_read_errors() {
    let fx = Fixture::new();
    let o = run(&mut fx.sfor(["-j1", "--sort", "path", "needle", "missing.md", "md"]));
    assert_eq!(code(&o), 2);
    assert!(stderr(&o).contains("missing.md"), "{}", stderr(&o));
    assert!(!stdout(&o).is_empty());

    let o = run(&mut fx.sfor(["-j1", "--sort", "path", "-q", "needle", "missing.md", "md"]));
    assert_eq!((code(&o), stdout(&o).as_str()), (0, ""));

    let o = run(&mut fx.sfor(["-j1", "--sort", "path", "-q", "absent", "missing.md", "md"]));
    assert_eq!((code(&o), stdout(&o).as_str()), (2, ""));

    for (pat, quiet, want) in [
        ("needle", false, 2),
        ("needle", true, 0),
        ("absent", true, 2),
    ] {
        let mut args = vec!["-j1", "--sort", "path", "--no-messages"];
        if quiet {
            args.push("-q");
        }
        args.extend([pat, "missing.md", "md"]);
        let o = run(&mut fx.sfor(&args));
        assert_eq!(code(&o), want, "{args:?}");
        assert_eq!(stderr(&o), "", "{args:?}");
    }
}

#[test]
fn t19_files_without_match_after_filters() {
    let o = sfor(&[
        "--sort",
        "path",
        "--files-without-match",
        "--within",
        "linux",
        "needle",
        "md",
        "misc",
    ]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(!out.contains("md/guide.md"), "{out}");
    assert!(out.contains("md/twice.md"), "{out}");
    assert!(!out.contains("ignored.md"), "{out}");
    assert!(!out.contains(".hidden.md"), "{out}");
    insta::assert_snapshot!("t19_files_without_match", out);
}

#[test]
fn t20_stdin_selection() {
    let fx = Fixture::new();
    // /dev/null: the current directory is searched, as `rg needle < /dev/null`.
    let o = run(&mut fx.sfor(["--sort", "path", "needle", "--flat"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("md/guide.md:"), "{}", stdout(&o));

    // An empty pipe is readable stdin: it is searched, and has nothing.
    let mut c = fx.sfor(["needle"]);
    c.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = c.spawn().unwrap();
    drop(child.stdin.take());
    let o = child.wait_with_output().unwrap();
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));

    // A regular file as stdin.
    let mut c = fx.sfor(["needle-pkg", "--flat"]);
    c.stdin(Stdio::from(
        std::fs::File::open(fx.file("md/guide.md")).unwrap(),
    ));
    let o = run(&mut c);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stdout(&o).contains("apt install needle-pkg"),
        "{}",
        stdout(&o)
    );

    // A pipe with a match, closed.
    let mut c = fx.sfor(["-q", "needle"]);
    c.stdin(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"needle\n").unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(0));

    // An open pipe: -q answers at the first Match, before EOF.
    let mut c = fx.sfor(["-q", "needle"]);
    c.stdin(Stdio::piped());
    let mut child = c.spawn().unwrap();
    let mut writer = child.stdin.take().unwrap();
    writer.write_all(b"needle\n").unwrap();
    writer.flush().unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if started.elapsed() > Duration::from_secs(2) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let elapsed = started.elapsed();
    if status.is_none() {
        child.kill().unwrap();
    }
    drop(writer);
    assert_eq!(
        status.and_then(|s| s.code()),
        Some(0),
        "still running after {elapsed:?}"
    );
    assert!(elapsed < Duration::from_secs(2));
}

#[test]
fn t21_tree_match_on_a_heading_is_not_repeated_as_its_segment() {
    let fx = Fixture::new();
    std::fs::write(
        fx.file("h1.md"),
        "# needle notes\n\none needle\n\n## Sub\n\ntwo needle\n",
    )
    .unwrap();
    let o = run(&mut fx.sfor(["needle", "h1.md", "--tree"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        "h1.md\n  1:# needle notes\n    3:one needle\n    ## Sub\n      7:two needle\n"
    );
    // With context, the lines between the heading and the next Match do not bring it back.
    let o = run(&mut fx.sfor(["-C1", "needle", "h1.md", "--tree"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert_eq!(out.matches("# needle notes").count(), 1, "{out}");
    assert!(out.contains("\n    3:one needle\n"), "{out}");
}

#[test]
fn t22_invalid_utf8_keeps_the_trails_of_the_file() {
    let fx = Fixture::new();
    std::fs::write(
        fx.file("bad.md"),
        b"# Before\n\nneedle-one\n\n## Bad \xff here\n\nneedle-two\n",
    )
    .unwrap();
    let o = run(&mut fx.sfor(["needle", "bad.md", "--flat"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        "3: Before │ needle-one\n7: Before › Bad ? here │ needle-two\n"
    );
}

/// Code review r1, Codex #1: on an open pipe, `-m` answers at the limit, before EOF, as rg.
#[test]
fn t23_max_count_on_stdin_stops_before_eof() {
    let fx = Fixture::new();
    for args in [
        &["-m1", "needle", "-"][..],
        &["-m1", "--format", "markdown", "needle", "-"],
        &["-m1", "--json", "needle", "-"],
        &["-m1", "--tree", "needle", "-"],
    ] {
        let mut c = fx.sfor(args);
        c.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = c.spawn().unwrap();
        let mut writer = child.stdin.take().unwrap();
        writer.write_all(b"# T\nneedle\n").unwrap();
        writer.flush().unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break Some(s);
            }
            if started.elapsed() > Duration::from_secs(2) {
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let elapsed = started.elapsed();
        if status.is_none() {
            child.kill().unwrap();
        }
        drop(writer);
        let o = child.wait_with_output().unwrap();
        assert_eq!(
            status.and_then(|s| s.code()),
            Some(0),
            "{args:?}: still running after {elapsed:?}"
        );
        assert!(elapsed < Duration::from_secs(2), "{args:?}");
        assert!(stdout(&o).contains("needle"), "{args:?}: {}", stdout(&o));
    }
}

/// Code review r1, Codex #2 and #12: -m0 and an empty pattern file exit 1 without printing in
/// every layout, after validating the file types and globs.
#[test]
fn t24_max_count_zero_and_no_patterns_search_nothing() {
    let fx = Fixture::new();
    let empty = fx.file("empty-patterns");
    std::fs::write(&empty, "").unwrap();
    let empty = empty.to_str().unwrap();
    for args in [
        &["-m0", "--flat", "needle", "md"][..],
        &["-m0", "--tree", "needle", "md"],
        &["-m0", "--json", "needle", "md"],
        &["-m0", "--within", "linux", "needle", "md"],
        &[
            "-m0",
            "--files-without-match",
            "--within",
            "x",
            "needle",
            "md",
        ],
        &["-m0", "needle", "missing.md"],
        &["-f", empty, "--json", "md"],
    ] {
        let o = run(&mut fx.sfor(args));
        assert_eq!((code(&o), stdout(&o).as_str()), (1, ""), "{args:?}");
    }
    for args in [
        &["-m0", "-tunknown", "needle", "md"][..],
        &["-f", empty, "-tunknown", "md"],
        &["-f", empty, "-g", "[", "md"],
        &["-m0", "--format", "nope", "needle", "md"],
    ] {
        let o = run(&mut fx.sfor(args));
        assert_eq!(code(&o), 2, "{args:?}");
        assert!(!stderr(&o).is_empty(), "{args:?}");
    }
}

/// Code review r1, Codex #5: the lines of an expanded Section follow -M and the Fragments
/// like every other line.
#[test]
fn t25_expanded_lines_follow_max_columns_and_fragments() {
    let fx = Fixture::new();
    let long = format!("{}needle{}", "a".repeat(80), "b".repeat(80));
    std::fs::write(fx.file("long.md"), format!("# H\n\n{long}\n")).unwrap();
    for layout in ["--flat", "--tree"] {
        let o = run(&mut fx.sfor(["--expand", layout, "-M10", "needle", "long.md"]));
        assert_eq!(code(&o), 0, "{}", stderr(&o));
        let out = stdout(&o);
        assert!(!out.contains(&long), "{layout}: {out}");
        assert!(
            out.contains("[Omitted long matching line]"),
            "{layout}: {out}"
        );

        let o = run(&mut fx.sfor([
            "--expand",
            layout,
            "--fragment-width",
            "50",
            "needle",
            "long.md",
        ]));
        let out = stdout(&o);
        assert!(!out.contains(&long), "{layout}: {out}");
        let row = out.lines().find(|l| l.contains("needle")).unwrap();
        assert!(
            row.contains("…[+") && row.contains("[…+"),
            "{layout}: {out}"
        );
        assert!(row.chars().count() <= 50, "{layout}: {row}");
    }
}

/// Code review r1, Codex #9: the frontmatter title is YAML, and --in-headings searches it.
#[test]
fn t26_frontmatter_title_is_read_as_yaml() {
    let fx = Fixture::new();
    std::fs::write(
        fx.file("fm.md"),
        "---\ntitle: |\n  Hello\n  World\n---\n\n# A\n\nneedle\n",
    )
    .unwrap();
    let o = run(&mut fx.sfor(["--in-headings", "World", "fm.md", "--flat"]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert_eq!(stdout(&o), "4:   World\n");
    let o = run(&mut fx.sfor(["needle", "fm.md", "--flat"]));
    assert_eq!(stdout(&o), "9: Hello World › A │ needle\n");
    std::fs::write(
        fx.file("c.md"),
        "---\ntitle: Hello # comment\n---\nneedle\n",
    )
    .unwrap();
    let o = run(&mut fx.sfor(["needle", "c.md", "--flat"]));
    assert_eq!(stdout(&o), "4: Hello │ needle\n");
}

/// Code review r1, Codex #10: the space between the lines of a setext heading can be matched.
#[test]
fn t27_in_headings_matches_the_space_of_a_setext_heading() {
    let fx = Fixture::new();
    let mut c = fx.sfor(["--in-headings", "--format", "markdown", "--flat", " ", "-"]);
    c.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"foo\nbar\n===\n")
        .unwrap();
    let o = child.wait_with_output().unwrap();
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    // The Match is the `\n` that the space stands for, at the end of line 1.
    assert_eq!(stdout(&o), "1: foo\n");
}

/// Code review r1, Codex #11: a huge context with --in-headings is clamped to the file.
#[test]
fn t28_in_headings_with_a_huge_context() {
    let fx = Fixture::new();
    let all = std::fs::read_to_string(fx.file("md/twice.md")).unwrap();
    for flag in [
        "-A18446744073709551615",
        "-B18446744073709551615",
        "-C18446744073709551615",
    ] {
        let o = run(&mut fx.sfor(["--in-headings", flag, "--flat", "-N", "^", "md/twice.md"]));
        assert_eq!(code(&o), 0, "{flag}: {}", stderr(&o));
        let out = stdout(&o);
        assert_eq!(out.lines().count(), all.lines().count(), "{flag}: {out}");
    }
}

/// Code review r1, Fable #3: with --within, -m counts the lines the filter keeps.
#[test]
fn t29_max_count_counts_the_lines_within_keeps() {
    let fx = Fixture::new();
    let numbers = |args: &[&str]| -> Vec<String> {
        let o = run(&mut fx.sfor(args));
        assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
        let mut v: Vec<String> = stdout(&o)
            .lines()
            .map(|l| l.split(':').next().unwrap().to_string())
            .collect();
        v.dedup();
        v
    };
    assert_eq!(
        numbers(&[
            "--flat",
            "-m2",
            "--within",
            "plans",
            "needle",
            "md/guide.md"
        ]),
        ["23", "24"]
    );
    assert_eq!(
        numbers(&["-c", "-m2", "--within", "plans", "needle", "md/guide.md"]),
        ["2"]
    );
    // --in-headings keeps the first -m names that --within keeps: `# Plan of plans` has no
    // Segment above it, so the first kept name is `## Plan B`.
    assert_eq!(
        numbers(&[
            "--flat",
            "-m1",
            "--in-headings",
            "--within",
            "plans",
            "-i",
            "plan",
            "md/twice.md"
        ]),
        ["3"]
    );
    assert_eq!(
        numbers(&[
            "--flat",
            "-m2",
            "--in-headings",
            "--within",
            "plans",
            "-i",
            "plan",
            "md/twice.md"
        ]),
        ["3", "5"]
    );
}

/// Code review r1, Fable #4: a Match in a name has the Trail a Match on that line has (a
/// Segment whose label holds it is left out), and --expand prints the named Segment's Section.
#[test]
fn t30_in_headings_trails_and_sections() {
    let fx = Fixture::new();
    std::fs::write(fx.file("li.md"), "# A\n\n- # Head in item\n  needle\n").unwrap();
    let plain = run(&mut fx.sfor(["--flat", "Head", "li.md"]));
    let named = run(&mut fx.sfor(["--in-headings", "--flat", "Head", "li.md"]));
    assert_eq!(stdout(&plain), "3: A │ - # Head in item\n");
    assert_eq!(stdout(&named), stdout(&plain));
    let o = run(&mut fx.sfor([
        "--in-headings",
        "--expand",
        "--flat",
        "^Linux$",
        "md/guide.md",
    ]));
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let out = stdout(&o);
    assert!(
        out.starts_with("9-18: Deploy guide › Install\n## Linux\n"),
        "{out}"
    );
    let o = run(&mut fx.sfor([
        "--in-headings",
        "--expand",
        "--json",
        "^Linux$",
        "md/guide.md",
    ]));
    let m: serde_json::Value = stdout(&o)
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|m| m["type"] == "match")
        .unwrap();
    assert_eq!(m["data"]["section"]["start_line"], 9);
    assert_eq!(m["data"]["section"]["end_line"], 18);
}
