//! ripgrep compatibility (plan, Task 9.3): `sfor --no-trail` against rg 14.1.1 byte for byte,
//! JSON with the elapsed times masked, and trail-mode JSON and `-M` against rg after removing
//! what sfor adds. Skipped with a note when rg 14.1.1 is not installed.
mod common;

use common::{Fixture, code, reference_rg, run, stdout};
use serde_json::Value;
use std::process::{Command, Output, Stdio};

/// Where a case's stdin comes from: /dev/null, or a fixture file.
#[derive(Clone, Copy)]
enum In {
    Null,
    File(&'static str),
}

fn with_stdin(fx: &Fixture, mut c: Command, input: In) -> Output {
    if let In::File(f) = input {
        c.stdin(Stdio::from(std::fs::File::open(fx.file(f)).unwrap()));
    }
    run(&mut c)
}

fn rg(fx: &Fixture, args: &[&str], input: In) -> Output {
    with_stdin(fx, fx.rg(args), input)
}

fn sfor(fx: &Fixture, args: &[&str], input: In) -> Output {
    with_stdin(fx, fx.sfor(args), input)
}

/// Runs every case and reports all the differences at once.
struct Report(Vec<String>);

impl Report {
    fn check(&mut self, case: String, want: (i32, String), got: (i32, String)) {
        if want != got {
            self.0.push(format!(
                "== {case}\n-- rg (exit {}):\n{}\n-- sfor (exit {}):\n{}",
                want.0, want.1, got.0, got.1
            ));
        }
    }

    fn assert_empty(self) {
        assert!(
            self.0.is_empty(),
            "{} case(s) differ:\n{}",
            self.0.len(),
            self.0.join("\n")
        );
    }
}

fn passthrough(fx: &Fixture, r: &mut Report, args: &[&str], input: In) {
    let mut a = vec!["--sort", "path"];
    a.extend_from_slice(args);
    let want = rg(fx, &a, input);
    let mut s = vec!["--no-trail"];
    s.extend_from_slice(&a);
    let got = sfor(fx, &s, input);
    r.check(
        format!("{args:?}"),
        (code(&want), stdout(&want)),
        (code(&got), stdout(&got)),
    );
}

#[test]
fn passthrough_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let empty = tempfile::NamedTempFile::new().unwrap();
    let empty = empty.path().to_str().unwrap().to_string();
    let mut r = Report(Vec::new());
    let n = In::Null;
    let cases: Vec<Vec<&str>> = vec![
        vec!["needle"],
        vec!["needle", "md/guide.md"],
        vec!["-i", "NEEDLE"],
        vec!["-s", "-i", "NEEDLE"],
        vec!["-i", "-s", "NEEDLE"],
        vec!["-w", "needle"],
        vec!["-F", "needle-10"],
        vec!["-v", "needle", "md/guide.md"],
        vec!["-C1", "needle"],
        vec!["-A2", "-B1", "needle"],
        vec!["-c", "needle"],
        vec!["--count-matches", "needle"],
        vec!["-l", "needle"],
        vec!["--files-without-match", "needle"],
        vec!["-m1", "needle"],
        vec!["-n", "needle"],
        vec!["-N", "needle"],
        vec!["-H", "needle", "md/guide.md"],
        vec!["-I", "needle"],
        vec!["-g", "*.md", "needle"],
        vec!["-g", "md/*.md", "needle", "md"],
        vec!["--iglob", "*.MD", "needle"],
        vec!["--iglob", "*.MD", "-g", "!guide.md", "needle"],
        vec!["-g", "!guide.md", "--iglob", "*.MD", "needle"],
        vec!["-t", "markdown", "needle"],
        vec!["--hidden", "needle"],
        vec!["-M", "40", "needle"],
        vec!["absent"],
        vec!["-e", "needle", "-e", "intro"],
        vec!["-e", "--pcre2"],
        vec!["-f", &empty],
        vec!["-v", "-f", &empty, "md/guide.md"],
        vec!["-c", "-v", "-f", &empty, "md/guide.md"],
        vec!["--json", "-v", "-f", &empty, "md/guide.md"],
        vec!["binary", "misc/bin.dat"],
        vec!["binary", "misc"],
        vec!["-uuu", "binary", "misc"],
        vec!["-a", "binary", "misc/bin.dat"],
        vec!["-u", "--ignore", "needle"],
        vec!["--ignore", "-u", "needle"],
        vec!["-uu", "--no-hidden", "needle"],
        vec!["-u", "--ignore", "-u", "needle"],
        vec!["-a", "-uuu", "needle"],
        vec!["-uuu", "-a", "needle"],
        vec!["needle", "misc/late-nul.txt"],
        vec!["needle", "misc"],
        vec!["needle", "misc/bom.md"],
        vec!["needle", "misc/utf16.md"],
        vec!["-q", "needle", "misc/bom.md"],
        vec!["-q", "needle", "misc/utf16.md"],
        vec!["-l", "-c", "needle"],
        vec!["-c", "-l", "needle"],
        // Code review r1, Codex #2 and #12: -m0 and an empty pattern file search nothing and
        // exit 1, after validating what rg validates.
        vec!["-m0", "--files-without-match", "needle", "md/guide.md"],
        vec!["-m0", "needle", "missing.md"],
        vec!["-m0", "--json", "needle", "md"],
        vec!["-m0", "-c", "needle", "md"],
        vec!["-m0", "("],
        vec!["-m0", "-tunknown", "needle"],
        vec!["-m0", "-g", "[", "needle"],
        vec!["-f", &empty, "-tunknown", "md"],
        vec!["-f", &empty, "-g", "[", "md"],
        vec!["-f", &empty, "--json", "md"],
    ];
    for c in &cases {
        passthrough(&fx, &mut r, c, n);
    }
    let stdin_cases: Vec<(Vec<&str>, &'static str)> = vec![
        (vec!["needle", "-"], "md/guide.md"),
        (vec!["binary"], "misc/bin.dat"),
        (vec!["binary", "-"], "misc/bin.dat"),
        (vec!["-a", "--no-binary", "needle"], "misc/bin.dat"),
        (vec!["--no-binary", "-a", "needle"], "misc/bin.dat"),
        (vec!["needle"], "misc/bom.md"),
        (vec!["needle"], "misc/utf16.md"),
        (vec!["-q", "needle"], "misc/bom.md"),
        (vec!["-q", "needle"], "misc/utf16.md"),
    ];
    for (c, f) in &stdin_cases {
        passthrough(&fx, &mut r, c, In::File(f));
    }
    r.assert_empty();
}

/// Replaces every `elapsed`/`elapsed_total` object by a placeholder; with `trail_mode`, also
/// removes the keys sfor adds and masks `bytes_printed`.
fn normalize(v: &mut Value, trail_mode: bool) {
    match v {
        Value::Object(m) => {
            for k in ["elapsed", "elapsed_total"] {
                if let Some(e) = m.get_mut(k) {
                    *e = Value::String("<elapsed>".into());
                }
            }
            if trail_mode {
                for k in ["format", "trail", "fragment", "section"] {
                    m.shift_remove(k);
                }
                if let Some(b) = m.get_mut("bytes_printed") {
                    *b = Value::from(0);
                }
            }
            for (_, x) in m.iter_mut() {
                normalize(x, trail_mode);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| normalize(x, trail_mode)),
        _ => {}
    }
}

fn json_lines(o: &Output, trail_mode: bool) -> String {
    stdout(o)
        .lines()
        .map(|l| {
            let mut v: Value = serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}"));
            normalize(&mut v, trail_mode);
            v.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn json_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let mut r = Report(Vec::new());
    let n = In::Null;
    let cases: Vec<(Vec<&str>, In)> = vec![
        (vec!["needle"], n),
        (vec!["-C1", "needle"], n),
        (vec!["-N", "needle"], n),
        (vec!["\\r", "misc/crlf.txt"], n),
        (vec!["$", "misc/crlf.txt"], n),
        (vec!["$", "misc/noeol.txt"], n),
        (vec!["-v", "needle", "md/guide.md"], n),
        (vec!["-v", "-C1", "needle", "md/guide.md"], n),
        (vec!["binary"], In::File("misc/bin.dat")),
        (vec!["needle"], In::File("misc/bin.dat")),
        (vec!["\\Afoo|needle", "misc/anchor.txt"], n),
        (vec!["^needle", "misc/anchor.txt"], n),
        (vec!["needle", "misc/bin.dat"], n),
        (vec!["needle", "misc"], n),
        (vec!["needle", "misc/bom-bin.dat"], n),
        (vec!["needle", "misc/bom.md"], n),
        (vec!["needle", "misc/utf16.md"], n),
        (vec!["needle"], In::File("misc/bom.md")),
        (vec!["needle"], In::File("misc/utf16.md")),
        // Code review r1, Codex #8: `bytes_searched` when -m stops a reader search (stdin, a
        // walked directory) and a search in memory (one explicit file).
        (vec!["-m1", "needle"], In::File("md/guide.md")),
        (vec!["-m1", "-C1", "needle"], In::File("md/guide.md")),
        (vec!["-m2", "-A1", "needle", "md"], n),
        (vec!["-m1", "-v", "needle", "md"], n),
        (vec!["-m1", "needle", "md/guide.md"], n),
        (vec!["-m2", "-A1", "needle", "md/guide.md"], n),
        (vec!["-m1", "needle"], In::File("misc/utf16.md")),
    ];
    let eleven: Vec<&str> = std::iter::once("needle")
        .chain(std::iter::repeat_n("misc/bin.dat", 11))
        .collect();
    let mut eleven_m: Vec<&str> = vec!["-m1"];
    eleven_m.extend(&eleven);
    let mut all = cases;
    all.push((eleven, n));
    all.push((eleven_m, n));
    for (args, input) in &all {
        let mut a = vec!["--sort", "path", "--json"];
        a.extend_from_slice(args);
        let want = rg(&fx, &a, *input);
        let want = (code(&want), json_lines(&want, false));
        let mut nt = vec!["--no-trail"];
        nt.extend_from_slice(&a);
        let got = sfor(&fx, &nt, *input);
        r.check(
            format!("--no-trail {a:?}"),
            want.clone(),
            (code(&got), json_lines(&got, false)),
        );
        let got = sfor(&fx, &a, *input);
        let masked = (want.0, {
            let w = rg(&fx, &a, *input);
            json_lines(&w, true)
        });
        r.check(
            format!("trail {a:?}"),
            masked,
            (code(&got), json_lines(&got, true)),
        );
    }
    // The last output mode wins: `-c --json` prints JSON, `--json -c` counts.
    for args in [&["-c", "--json", "needle"][..], &["--json", "-c", "needle"]] {
        let mut a = vec!["--sort", "path"];
        a.extend_from_slice(args);
        let want = rg(&fx, &a, n);
        let is_json = stdout(&want).starts_with('{');
        let want = (
            code(&want),
            if is_json {
                json_lines(&want, false)
            } else {
                stdout(&want)
            },
        );
        let mut nt = vec!["--no-trail"];
        nt.extend_from_slice(&a);
        let got = sfor(&fx, &nt, n);
        let got = (
            code(&got),
            if is_json {
                json_lines(&got, false)
            } else {
                stdout(&got)
            },
        );
        r.check(format!("{a:?}"), want, got);
    }
    r.assert_empty();
}

/// The binary message lines of rg and of sfor's flat layout agree.
#[test]
fn binary_messages_in_the_flat_layout() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let mut r = Report(Vec::new());
    for args in [
        &["needle", "misc/late-nul.txt"][..],
        &["needle", "misc"],
        &["binary", "misc/bin.dat"],
    ] {
        let mut a = vec!["--sort", "path"];
        a.extend_from_slice(args);
        let want = rg(&fx, &a, In::Null);
        let mut f = vec!["--flat"];
        f.extend_from_slice(&a);
        let got = sfor(&fx, &f, In::Null);
        let msgs = |o: &Output| -> String {
            stdout(o)
                .lines()
                .filter(|l| l.contains("binary file"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert!(
            !msgs(&want).is_empty(),
            "{args:?}: rg printed no binary message"
        );
        r.check(
            format!("{a:?}"),
            (code(&want), msgs(&want)),
            (code(&got), msgs(&got)),
        );
    }
    r.assert_empty();
}

fn strip_ansi(s: &str) -> String {
    regex_lite::Regex::new("\x1b\\[[0-9;]*m")
        .unwrap()
        .replace_all(s, "")
        .into_owned()
}

/// (line number, `:` or `-`, message) of each line, `--` separators left out.
fn by_line(out: &str, sfor: bool) -> Vec<(u64, char, String)> {
    let re = regex_lite::Regex::new(r"^(\d+)([:-])(.*)$").unwrap();
    let mut v: Vec<(u64, char, String)> = Vec::new();
    for l in out.lines().map(strip_ansi) {
        if l == "--" {
            continue;
        }
        let c = re
            .captures(&l)
            .unwrap_or_else(|| panic!("unexpected line: {l:?}"));
        let sep = c[2].chars().next().unwrap();
        let mut msg = c[3].to_string();
        if sfor && sep == ':' {
            msg = msg.strip_prefix(' ').unwrap_or(&msg).to_string();
            if let Some((_, text)) = msg.split_once(" │ ") {
                msg = text.to_string();
            }
        }
        let e = (c[1].parse().unwrap(), sep, msg);
        if v.last() != Some(&e) {
            v.push(e);
        }
    }
    v
}

#[test]
fn max_columns_in_the_flat_layout() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    for color in ["never", "always"] {
        let want = rg(
            &fx,
            &[
                "--color",
                color,
                "-n",
                "-M",
                "15",
                "-C1",
                "needle",
                "md/guide.md",
            ],
            In::Null,
        );
        let got = sfor(
            &fx,
            &[
                "--color",
                color,
                "--flat",
                "-M",
                "15",
                "-C1",
                "needle",
                "md/guide.md",
            ],
            In::Null,
        );
        assert_eq!((code(&want), code(&got)), (0, 0));
        let (w, g) = (by_line(&stdout(&want), false), by_line(&stdout(&got), true));
        assert_eq!(w, g, "--color {color}");
        assert!(
            w.iter()
                .any(|(_, s, m)| *s == ':' && m.starts_with("[Omitted long")),
            "{w:?}"
        );
        assert!(
            w.iter()
                .any(|(_, s, m)| *s == '-' && m == "[Omitted long context line]"),
            "{w:?}"
        );
        let row24: Vec<_> = g.iter().filter(|(n, _, _)| *n == 24).collect();
        assert_eq!(row24.len(), 1, "{g:?}");
        if color == "always" {
            assert_eq!(row24[0].2, "[Omitted long line with 2 matches]");
        }
    }
}

/// rg 14.1.1 run with `args` and `RIPGREP_CONFIG_PATH` unset, as the reference.
fn reference(args: &[&str]) -> Output {
    run(Command::new("rg")
        .args(args)
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(Stdio::null()))
}

#[test]
fn type_list_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let want = reference(&["--type-list"]);
    let got = run(&mut fx.sfor(["--type-list"]));
    assert_eq!((code(&got), stdout(&got)), (code(&want), stdout(&want)));
}

#[test]
fn type_selection_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    // `*.env` is `sh` in the ignore crate's table and nothing in rg 14.1.1's.
    for (f, body) in [
        ("types/run.sh", "needle sh\n"),
        ("types/.zshrc", "needle zshrc\n"),
        ("types/prod.env", "needle env\n"),
        ("types/notes.md", "needle md\n"),
        ("types/PKGBUILD", "needle pkgbuild\n"),
    ] {
        let p = fx.file(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    let mut r = Report(Vec::new());
    for args in [
        vec!["-t", "sh", "needle", "types"],
        vec!["--hidden", "-t", "sh", "needle", "types"],
        vec!["-T", "md", "needle", "types"],
        vec!["--hidden", "-T", "md", "needle", "types"],
        // A type the ignore crate knows and rg 14.1.1 does not: an error in both.
        vec!["-t", "pkgbuild", "needle", "types"],
        // Code review r1, Codex #3: selections apply in command-line order.
        vec!["-T", "md", "-t", "md", "needle", "types"],
        vec!["-t", "md", "-T", "md", "needle", "types"],
        vec!["-t", "sh", "-T", "sh", "-t", "md", "needle", "types"],
    ] {
        passthrough(&fx, &mut r, &args, In::Null);
        // Trail mode walks the same files.
        let mut a = vec!["--sort", "path"];
        a.extend_from_slice(&args);
        let want = rg(&fx, &a, In::Null);
        a.push("--flat");
        let got = sfor(&fx, &a, In::Null);
        let files = |o: &Output| {
            let mut v: Vec<String> = stdout(o)
                .lines()
                .map(|l| l.split(':').next().unwrap().to_string())
                .collect();
            v.dedup();
            v
        };
        r.check(
            format!("files of {args:?}"),
            (code(&want), files(&want).join("\n")),
            (code(&got), files(&got).join("\n")),
        );
    }
    r.assert_empty();
}

/// Code review r1, Codex #4: -w and -x override each other, the last one wins.
#[test]
fn word_and_line_regexp_override_each_other() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::write(fx.file("wx.txt"), "needle needle\nneedle\nneedles\n").unwrap();
    let mut r = Report(Vec::new());
    for args in [
        &["-x", "-w", "needle", "wx.txt"][..],
        &["-w", "-x", "needle", "wx.txt"],
        &["-x", "-w", "-x", "needle", "wx.txt"],
    ] {
        passthrough(&fx, &mut r, args, In::Null);
        let want = rg(&fx, args, In::Null);
        let mut a = vec!["--flat", "-N"];
        a.extend_from_slice(args);
        let got = sfor(&fx, &a, In::Null);
        let text = |o: &Output, sfor: bool| -> String {
            stdout(o)
                .lines()
                .map(|l| {
                    if sfor {
                        l.strip_prefix(' ').unwrap_or(l)
                    } else {
                        l
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        r.check(
            format!("flat {args:?}"),
            (code(&want), text(&want, false)),
            (code(&got), text(&got, true)),
        );
    }
    r.assert_empty();
}

/// Code review r1, Codex #7: with --within, `end` and `summary` count what the search found,
/// before the filter (spec §6.4), even when no Match survives: ripgrep's stats without it.
#[test]
fn within_keeps_the_search_stats_in_json() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let stats = |o: &Output, kind: &str| -> Vec<Value> {
        stdout(o)
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|m| m["type"] == kind)
            .map(|m| {
                let mut s = m["data"]["stats"].clone();
                let s = s.as_object_mut().unwrap();
                for k in ["elapsed", "bytes_printed"] {
                    s.remove(k);
                }
                Value::Object(s.clone())
            })
            .collect()
    };
    let want = rg(&fx, &["--json", "needle", "md/guide.md"], In::Null);
    for within in ["linux", "nowhere"] {
        let got = sfor(
            &fx,
            &["--json", "--within", within, "needle", "md/guide.md"],
            In::Null,
        );
        assert_eq!(stats(&got, "summary"), stats(&want, "summary"), "{within}");
        if within == "linux" {
            assert_eq!(stats(&got, "end"), stats(&want, "end"), "{within}");
        } else {
            assert_eq!(code(&got), 1);
            assert_eq!(stdout(&got).lines().count(), 1, "{}", stdout(&got));
        }
    }
}
