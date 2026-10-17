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

/// JSON messages of the given types, normalized as `json_lines` does.
fn json_of(o: &Output, trail_mode: bool, types: &[&str]) -> Vec<String> {
    stdout(o)
        .lines()
        .filter_map(|l| {
            let mut v: Value = serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}"));
            if !types.iter().any(|t| v["type"] == *t) {
                return None;
            }
            normalize(&mut v, trail_mode);
            Some(v.to_string())
        })
        .collect()
}

/// Code review r2, Codex #2 and Fable #5: with `-m` and an after context, rg 14.1.1 (whose
/// printers count `-m`) prints a line that matches inside the after context of the last
/// Match allowed as a Match, and with `-v` a line that does not; the search stops when that
/// context ends. Patterns with `\A` and `$` take grep-searcher's slow path. With `--within ''`
/// (every Match kept), `-m` counts after the filter and gives the same lines.
#[test]
fn max_count_with_after_context_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("mc")).unwrap();
    std::fs::write(fx.file("mc/m.txt"), "a\nneedle\nb\nneedle\nc\nd\n").unwrap();
    std::fs::write(fx.file("mc/n.txt"), "needle\nneedle\n").unwrap();
    let mut r = Report(Vec::new());
    let flags: Vec<Vec<&str>> = vec![
        vec!["-m1", "-A1", "\\Aneedle|needle"],
        vec!["-m1", "-A2", "\\Aneedle|needle"],
        vec!["-m1", "-A1", "\\Aneedle"],
        vec!["-m2", "-A1", "$"],
        vec!["-m1", "-B1", "-A1", "needle"],
        vec!["-v", "-m1", "-A2", "needle"],
        vec!["-v", "-m1", "-A2", "\\Aneedle|needle"],
        vec!["-v", "-m2", "-A1", "\\Aneedle|needle"],
        vec!["-v", "-m1", "-A2", "x$"],
        vec!["-v", "-m1", "-C1", "^needle"],
        vec!["-v", "-m1", "needle"],
    ];
    let inputs: [(Option<&str>, In); 5] = [
        (None, In::File("mc/m.txt")),
        (None, In::File("mc/n.txt")),
        (Some("mc/m.txt"), In::Null),
        (Some("mc/n.txt"), In::Null),
        (Some("mc"), In::Null),
    ];
    for f in &flags {
        for (path, input) in inputs {
            let mut a = vec!["--sort", "path", "-n", "-I"];
            a.extend_from_slice(f);
            a.extend(path);
            let case = format!("{a:?} {}", if path.is_none() { "stdin" } else { "" });
            let want = rg(&fx, &a, input);
            let mut nt = vec!["--no-trail"];
            nt.extend_from_slice(&a);
            let got = sfor(&fx, &nt, input);
            r.check(
                format!("text --no-trail {case}"),
                (code(&want), stdout(&want)),
                (code(&got), stdout(&got)),
            );
            let w = format!("{:?}", by_line(&stdout(&want), false));
            for within in [&[][..], &["--within", ""]] {
                let mut fl = vec!["--flat"];
                fl.extend_from_slice(within);
                fl.extend_from_slice(&a);
                let got = sfor(&fx, &fl, input);
                let g = format!("{:?}", by_line(&stdout(&got), true));
                r.check(
                    format!("text {within:?} {case}"),
                    (code(&want), w.clone()),
                    (code(&got), g),
                );
            }
            let mut ja = vec!["--json"];
            ja.extend_from_slice(&a);
            let want = rg(&fx, &ja, input);
            let mut nt = vec!["--no-trail"];
            nt.extend_from_slice(&ja);
            let got = sfor(&fx, &nt, input);
            r.check(
                format!("json --no-trail {case}"),
                (code(&want), json_lines(&want, false)),
                (code(&got), json_lines(&got, false)),
            );
            let got = sfor(&fx, &ja, input);
            r.check(
                format!("json {case}"),
                (code(&want), json_lines(&want, true)),
                (code(&got), json_lines(&got, true)),
            );
            // With --within, `end` and `summary` count the whole search (spec §6.4).
            let lines = ["begin", "match", "context"];
            let mut wa = vec!["--within", ""];
            wa.extend_from_slice(&ja);
            let got = sfor(&fx, &wa, input);
            r.check(
                format!("json --within '' {case}"),
                (code(&want), json_of(&want, true, &lines).join("\n")),
                (code(&got), json_of(&got, true, &lines).join("\n")),
            );
        }
    }
    r.assert_empty();
}

/// Code review r2, Codex #3: `-M0` turns the limit off in rg 14.1.1, in every layout.
#[test]
fn max_columns_zero_disables_the_limit() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let mut r = Report(Vec::new());
    for args in [
        &["-n", "-M0", "needle", "md/guide.md"][..],
        &["-n", "-M0", "-C1", "needle", "md/guide.md"],
        &["-n", "--max-columns", "0", "-v", "needle", "md/guide.md"],
    ] {
        let want = rg(&fx, args, In::Null);
        let mut nt = vec!["--no-trail"];
        nt.extend_from_slice(args);
        let got = sfor(&fx, &nt, In::Null);
        r.check(
            format!("--no-trail {args:?}"),
            (code(&want), stdout(&want)),
            (code(&got), stdout(&got)),
        );
        let w = format!("{:?}", by_line(&stdout(&want), false));
        for layout in [&["--flat"][..], &["--flat", "--within", ""]] {
            let mut a = layout.to_vec();
            a.extend_from_slice(args);
            let got = sfor(&fx, &a, In::Null);
            r.check(
                format!("{a:?}"),
                (code(&want), w.clone()),
                (code(&got), format!("{:?}", by_line(&stdout(&got), true))),
            );
        }
        let mut a = vec!["--tree"];
        a.extend_from_slice(args);
        let got = sfor(&fx, &a, In::Null);
        r.check(
            format!("{a:?}"),
            (code(&want), String::new()),
            (code(&got), stdout(&got).matches("[Omitted").collect()),
        );
    }
    r.assert_empty();
}

/// `bytes_searched` of every `end` and `summary` message.
fn bytes_searched(o: &Output) -> Vec<u64> {
    stdout(o)
        .lines()
        .filter_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            let stats = &v["data"]["stats"];
            stats["bytes_searched"].as_u64()
        })
        .collect()
}

/// Code review r2, Codex #5: when `-m` stops a reader search on an after-context line read into
/// a later buffer than its Match, rg 14.1.1 reports the bytes rolled out before that buffer.
#[test]
fn bytes_searched_when_the_after_context_crosses_a_buffer() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("cross")).unwrap();
    // A second line longer than the first buffer (64 KiB).
    let long = format!("needle\n{}\ntail\n", "x".repeat(70_000));
    std::fs::write(fx.file("cross/long.txt"), &long).unwrap();
    std::fs::write(fx.file("cross/long.md"), &long).unwrap();
    let mut r = Report(Vec::new());
    let cases: [(&[&str], In); 3] = [
        (&["-m1", "-A1", "needle"], In::File("cross/long.txt")),
        (&["-m1", "-A1", "needle"], In::File("cross/long.md")),
        (&["-m1", "-A1", "needle", "cross"], In::Null),
    ];
    for (args, input) in cases {
        let mut a = vec!["--sort", "path", "--json"];
        a.extend_from_slice(args);
        let want = rg(&fx, &a, input);
        assert!(bytes_searched(&want).contains(&7), "{}", stdout(&want));
        for mode in [
            &["--no-trail"][..],
            &[],
            &["--expand"],
            &["--format", "markdown"],
        ] {
            let mut s = mode.to_vec();
            s.extend_from_slice(&a);
            let got = sfor(&fx, &s, input);
            r.check(
                format!("{s:?}"),
                (code(&want), json_lines(&want, true)),
                (code(&got), json_lines(&got, true)),
            );
        }
    }
    // Through a pipe whose writer pauses after 8 KiB: the second line comes in a second read.
    let chunked = |mut c: Command| {
        c.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = c.spawn().unwrap();
        let mut w = child.stdin.take().unwrap();
        let line = format!("needle\n{}\ntail\n", "x".repeat(9000));
        std::io::Write::write_all(&mut w, &line.as_bytes()[..8192]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        std::io::Write::write_all(&mut w, &line.as_bytes()[8192..]).unwrap();
        drop(w);
        child.wait_with_output().unwrap()
    };
    let args = ["--json", "-m1", "-A1", "needle", "-"];
    let want = chunked(fx.rg(args));
    assert_eq!(bytes_searched(&want), vec![7, 7], "{}", stdout(&want));
    for mode in [
        &["--no-trail"][..],
        &[],
        &["--expand"],
        &["--format", "markdown"],
    ] {
        let mut s = mode.to_vec();
        s.extend_from_slice(&args);
        let got = chunked(fx.sfor(&s));
        r.check(
            format!("chunked {s:?}"),
            (0, "[7, 7]".into()),
            (code(&got), format!("{:?}", bytes_searched(&got))),
        );
    }
    r.assert_empty();
}

/// What docs/rg-compat.md documented as a deviation until code review r4, Codex #2: with an
/// after context longer than the before context, a reader search that `-m` or binary data
/// stops past the first buffer. With rg 14.1.1's searcher (grep-searcher 0.1.14), which rolls
/// its buffers keeping `max(-A, -B)` lines, `bytes_searched` is rg's in every layout.
#[test]
fn bytes_searched_with_a_longer_after_context_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("deep")).unwrap();
    let line = format!("{}\n", "x".repeat(99));
    std::fs::write(
        fx.file("deep/deep.txt"),
        format!("{}needle\n{}", line.repeat(1500), line.repeat(10)),
    )
    .unwrap();
    let mut r = Report(Vec::new());
    let cases: [(&[&str], &str); 5] = [
        (&["-m1", "-A1"], "deep"),
        (&["-m1", "-A3"], "deep"),
        (&["-m1", "-C3"], "deep"),
        (&["-g", "late-nul*", "-A2", "-B1"], "misc"),
        (&["-g", "late-nul*", "-A1"], "misc"),
    ];
    for (flags, dir) in cases {
        let mut a = vec!["--json"];
        a.extend_from_slice(flags);
        a.extend(["needle", dir]);
        let want = rg(&fx, &a, In::Null);
        // Past the first buffer: rg counts the bytes rolled out of it (65213 for late-nul).
        assert!(
            bytes_searched(&want).iter().all(|&b| b > 65_000),
            "{a:?} {}",
            stdout(&want)
        );
        for mode in [&["--no-trail"][..], &[], &["--format", "markdown"]] {
            let mut s = mode.to_vec();
            s.extend_from_slice(&a);
            let got = sfor(&fx, &s, In::Null);
            r.check(
                format!("{s:?}"),
                (code(&want), format!("{:?}", bytes_searched(&want))),
                (code(&got), format!("{:?}", bytes_searched(&got))),
            );
        }
    }
    r.assert_empty();
}

/// Code review r2, Codex #7: `--json -q` prints no message per file and ends with rg 14.1.1's
/// `summary`, which counts every file searched; the search does not stop at the first Match.
#[test]
fn json_quiet_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let mut r = Report(Vec::new());
    let n = In::Null;
    let cases: Vec<(Vec<&str>, In)> = vec![
        (vec!["needle"], In::File("md/guide.md")),
        (vec!["absent"], In::File("md/guide.md")),
        (vec!["needle", "md/guide.md"], n),
        (vec!["absent", "md/guide.md"], n),
        (vec!["needle", "md"], n),
        (vec!["needle"], n),
        (vec!["-m1", "needle", "md"], n),
        (vec!["-m2", "-A1", "needle", "md/guide.md"], n),
        (vec!["-v", "needle", "md/guide.md"], n),
        (vec!["-g", "late-nul*", "needle", "misc"], n),
        (vec!["binary", "misc/bin.dat"], n),
    ];
    for (args, input) in &cases {
        let mut a = vec!["--sort", "path", "--json", "-q"];
        a.extend_from_slice(args);
        let want = rg(&fx, &a, *input);
        let want = (code(&want), json_lines(&want, false));
        for mode in [&["--no-trail"][..], &[]] {
            let mut s = mode.to_vec();
            s.extend_from_slice(&a);
            let got = sfor(&fx, &s, *input);
            r.check(
                format!("{s:?}"),
                want.clone(),
                (code(&got), json_lines(&got, false)),
            );
        }
    }
    // With --within the summary counts the search (spec §6.4); the exit code, the filter.
    let want = rg(&fx, &["--json", "-q", "needle", "md/guide.md"], n);
    for (within, exit) in [("linux", 0), ("nowhere", 1)] {
        let a = ["--json", "-q", "--within", within, "needle", "md/guide.md"];
        let got = sfor(&fx, &a, n);
        r.check(
            format!("{a:?}"),
            (exit, json_lines(&want, false)),
            (code(&got), json_lines(&got, false)),
        );
    }
    r.assert_empty();
}

/// Code review r3, Codex #1: with a filter, `-c` and `--count-matches` stop at `-m` as rg
/// 14.1.1's summary printer does, without the Matches of the after context that the printing
/// modes keep. With `--within` the filter keeps every Match; with `--in-headings` every line
/// is a heading, so the Matches are rg's.
#[test]
fn filtered_counts_stop_at_max_count() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("cnt")).unwrap();
    std::fs::write(fx.file("cnt/w.md"), "# H\nneedle needle\nneedle needle\n").unwrap();
    std::fs::write(fx.file("cnt/h.md"), "# Hello\n# Hello Hello\n# Hello\n").unwrap();
    let mut r = Report(Vec::new());
    let filters: [(&[&str], &str, &str); 3] = [
        (&["--within", ""], "needle", "cnt/w.md"),
        (&["--within", "H"], "needle", "cnt/w.md"),
        (&["--in-headings"], "Hello", "cnt/h.md"),
    ];
    for (filter, pattern, path) in filters {
        for count in ["-c", "--count-matches"] {
            for limit in [
                &["-m1", "-A1"][..],
                &["-m1", "-C2"],
                &["-m2", "-A1"],
                &["-m1"],
            ] {
                let mut a = vec![count];
                a.extend_from_slice(limit);
                a.extend([pattern, path]);
                let want = rg(&fx, &a, In::Null);
                let mut s = filter.to_vec();
                s.extend_from_slice(&a);
                let got = sfor(&fx, &s, In::Null);
                r.check(
                    format!("{s:?}"),
                    (code(&want), stdout(&want)),
                    (code(&got), stdout(&got)),
                );
            }
        }
    }
    r.assert_empty();
}

/// Code review r3, Codex #2: an after context that runs through several buffers, with a
/// before context as long (`-C`), so that both searchers roll their buffers alike: rg 14.1.1
/// reports the start of the buffer `-m` stopped in, however many it crossed.
#[test]
fn bytes_searched_when_the_context_crosses_several_buffers() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("several")).unwrap();
    let long = format!("{}\n", "x".repeat(9000));
    let first = format!("needle\n{}tail\n", long.repeat(30));
    std::fs::write(fx.file("several/first.txt"), &first).unwrap();
    std::fs::write(fx.file("several/first.md"), &first).unwrap();
    // The Match in the second buffer, its context through the fourth.
    let later = format!("{}needle\n{}tail\n", long.repeat(10), long.repeat(30));
    std::fs::write(fx.file("several/later.txt"), &later).unwrap();
    let mut r = Report(Vec::new());
    let mut cases: Vec<(Vec<&str>, In)> = Vec::new();
    for flags in [
        &["-m1", "-C20"][..],
        &["-m1", "-A20", "-B20"],
        &["-m1", "-C16"],
    ] {
        for input in ["several/first.txt", "several/first.md", "several/later.txt"] {
            let mut a = flags.to_vec();
            a.push("needle");
            cases.push((a, In::File(input)));
        }
        let mut a = flags.to_vec();
        a.extend(["needle", "several"]);
        cases.push((a, In::Null));
    }
    for (args, input) in &cases {
        let mut a = vec!["--sort", "path", "--json"];
        a.extend_from_slice(args);
        let want = rg(&fx, &a, *input);
        assert!(
            bytes_searched(&want).iter().all(|&b| b > 65536),
            "{args:?} {}",
            stdout(&want)
        );
        for mode in [&["--no-trail"][..], &[], &["--format", "markdown"]] {
            let mut s = mode.to_vec();
            s.extend_from_slice(&a);
            let got = sfor(&fx, &s, *input);
            r.check(
                format!("{s:?}"),
                (code(&want), format!("{:?}", bytes_searched(&want))),
                (code(&got), format!("{:?}", bytes_searched(&got))),
            );
        }
    }
    r.assert_empty();
}

/// Code review r3, Codex #3 and Fable #2: `-v -m` on an explicit file with a NUL, searched in
/// memory: rg 14.1.1 reports where its search stopped, but no further than the binary data.
#[test]
fn inverted_max_count_in_memory_stops_at_the_binary_data() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("vbin")).unwrap();
    std::fs::write(
        fx.file("vbin/one.dat"),
        "needle first\nmore\0binary needle\nneedle again\n",
    )
    .unwrap();
    std::fs::write(fx.file("vbin/two.dat"), "a\nneedle\nb\nneedle\nc\0d\ne\n").unwrap();
    let mut r = Report(Vec::new());
    let cases: [&[&str]; 6] = [
        &["-v", "-m1", "absent", "misc/bin.dat"],
        &["-v", "-m1", "first", "vbin/one.dat"],
        &["-v", "-m1", "a", "vbin/two.dat"],
        &["-v", "-m3", "needle", "vbin/two.dat"],
        &["-v", "-m1", "needle", "vbin/two.dat"],
        &["-v", "-m2", "needle", "vbin/two.dat"],
    ];
    for args in cases {
        for quiet in [&[][..], &["-q"]] {
            let mut a = vec!["--json"];
            a.extend_from_slice(quiet);
            a.extend_from_slice(args);
            let want = rg(&fx, &a, In::Null);
            for mode in [&["--no-trail"][..], &[]] {
                let mut s = mode.to_vec();
                s.extend_from_slice(&a);
                let got = sfor(&fx, &s, In::Null);
                r.check(
                    format!("{s:?}"),
                    (code(&want), format!("{:?}", bytes_searched(&want))),
                    (code(&got), format!("{:?}", bytes_searched(&got))),
                );
            }
        }
    }
    r.assert_empty();
}

/// Code review r3, Fable #3: a reader that closes the pipe after the first line. rg 14.1.1
/// prints while it searches, stops at the closed pipe and exits 1; so does sfor through
/// grep-printer's printers (`--no-trail`), which print one file's output as it grows.
#[test]
fn a_closed_pipe_exits_as_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::write(
        fx.file("big100k.txt"),
        "line of text here\n".repeat(100_000),
    )
    .unwrap();
    let first_line = |mut c: Command| {
        c.stdout(Stdio::piped());
        let mut child = c.spawn().unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        std::io::BufRead::read_line(&mut out, &mut line).unwrap();
        drop(out);
        (child.wait().unwrap().code().unwrap_or(-1), line.is_empty())
    };
    let mut r = Report(Vec::new());
    // With a context, a separator goes between files; one file has none, and prints as it
    // grows too (code review r4 replay). The tree, which rg does not have, also stops at the
    // closed pipe and exits 1.
    for args in [
        &["text", "big100k.txt"][..],
        &["--json", "text", "big100k.txt"],
        &["-C1", "text", "big100k.txt"],
        &["--json", "-C1", "text", "big100k.txt"],
    ] {
        let want = first_line(fx.rg(args));
        for mode in [&["--no-trail"][..], &["--flat"], &["--tree"]] {
            let mut s = mode.to_vec();
            s.extend_from_slice(args);
            let got = first_line(fx.sfor(&s));
            r.check(
                format!("{s:?}"),
                (want.0, format!("{:?}", want.1)),
                (got.0, format!("{:?}", got.1)),
            );
        }
    }
    r.assert_empty();
}

/// Code review r4, Codex #2: `bytes_searched` with a BOM (UTF-8 and UTF-16LE), on stdin and on
/// an explicit file, which the searcher reads decoded: rg 14.1.1's count, with and without
/// Trails, past the first buffer and with any context.
#[test]
fn bytes_searched_with_a_bom_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("bom")).unwrap();
    let text = format!(
        "needle\n{}tail\n",
        format!("{}\n", "x".repeat(9000)).repeat(30)
    );
    let mut utf8 = b"\xef\xbb\xbf".to_vec();
    utf8.extend_from_slice(text.as_bytes());
    let mut utf16 = b"\xff\xfe".to_vec();
    utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    for (name, bytes) in [("utf8", &utf8), ("utf16", &utf16)] {
        for ext in ["txt", "md"] {
            std::fs::write(fx.file(&format!("bom/{name}.{ext}")), bytes).unwrap();
        }
    }
    let inputs = [
        "bom/utf8.txt",
        "bom/utf8.md",
        "bom/utf16.txt",
        "bom/utf16.md",
    ];
    let mut r = Report(Vec::new());
    for flags in [
        &["-m1"][..],
        &["-m1", "-C8"],
        &["-m1", "-C20"],
        &["-m1", "-A2"],
        &["-m1", "-A20", "-B1"],
        &["-m2", "-A3", "-B1"],
        &[],
    ] {
        for input in inputs {
            for stdin in [true, false] {
                let mut a = vec!["--json"];
                a.extend_from_slice(flags);
                a.push("needle");
                let src = if stdin {
                    a.push("-");
                    In::File(input)
                } else {
                    a.push(input);
                    In::Null
                };
                let want = rg(&fx, &a, src);
                for mode in [&["--no-trail"][..], &[], &["--expand"]] {
                    let mut s = mode.to_vec();
                    s.extend_from_slice(&a);
                    let got = sfor(&fx, &s, src);
                    r.check(
                        format!("{s:?} < {}", if stdin { input } else { "null" }),
                        (code(&want), format!("{:?}", bytes_searched(&want))),
                        (code(&got), format!("{:?}", bytes_searched(&got))),
                    );
                }
            }
        }
    }
    r.assert_empty();
}

/// Code review r5, Codex #1: the matcher's Unicode tables are rg 14.1.1's (Unicode 15), so
/// letters, digits and case pairs that Unicode 16 added match as in rg: `\w`, `\d`, `\b`, `-w`,
/// `-i`, general categories and scripts, on files and stdin, and the submatches in JSON with and
/// without trails. Each character new in Unicode 16 sits alone on its line, so a line matches
/// only when the class includes it.
#[test]
fn unicode_classes_equal_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("uni")).unwrap();
    // U+105C0 Todhri letter, U+10D40 Garay digit zero, U+10D50/U+10D70 Garay capital and small
    // letters, U+A7CB/U+0264 and U+A7DC/U+019B case pairs: all new in Unicode 16.
    let text = "needle\u{105C0}\n\
                \u{105C0}\n\
                \u{10D40}\n\
                \u{10D50}\u{10D70}\n\
                \u{A7CB} \u{A7DC}\n\
                needle plain\n";
    std::fs::write(fx.file("uni/u.txt"), text).unwrap();
    std::fs::write(fx.file("uni/u.md"), format!("# Title\n\n{text}")).unwrap();
    let pats = [
        &["-w", "needle"][..],
        &[r"\w"],
        &[r"^\w+$"],
        &[r"\d"],
        &[r"\bneedle\b"],
        &[r"needle\B"],
        &[r"\b\w"],
        &["-i", "\u{0264}"],
        &["-i", "\u{019B}"],
        &["-i", "\u{10D70}"],
        &[r"\p{Lu}"],
        &[r"\p{Ll}"],
        &[r"^\pL+$"],
        &[r"\p{Nd}"],
        &[r"\p{Garay}"],
        &[r"[^\W\d]"],
    ];
    let mut r = Report(Vec::new());
    for pat in pats {
        for file in ["uni/u.txt", "uni/u.md"] {
            let mut a = pat.to_vec();
            a.push(file);
            passthrough(&fx, &mut r, &a, In::Null);
        }
        let mut a = pat.to_vec();
        a.push("-");
        passthrough(&fx, &mut r, &a, In::File("uni/u.txt"));
        let mut a = vec!["--json"];
        a.extend_from_slice(pat);
        a.push("uni/u.md");
        let want = rg(&fx, &a, In::Null);
        for (mode, trail) in [(&["--no-trail"][..], false), (&[], true)] {
            let mut s = mode.to_vec();
            s.extend_from_slice(&a);
            let got = sfor(&fx, &s, In::Null);
            r.check(
                format!("{s:?}"),
                (code(&want), json_lines(&want, trail)),
                (code(&got), json_lines(&got, trail)),
            );
        }
    }
    r.assert_empty();
}

/// Runs `cmd` in `sh` on a pseudo-terminal from util-linux `script`, in the fixture copy, with
/// `rg` and `sfor` naming the reference and our binary. None, with a note, without `script`.
fn on_a_tty(fx: &Fixture, cmd: &str) -> Option<String> {
    if Command::new("script").arg("--version").output().is_err() {
        eprintln!("skipping: util-linux script not found");
        return None;
    }
    let sfor = assert_cmd::cargo::cargo_bin("sfor");
    let cmd = cmd.replace("sfor ", &format!("{} --no-trail ", sfor.display()));
    let o = Command::new("script")
        .args(["-qec", &cmd, "/dev/null"])
        .current_dir(fx.path())
        .env("SFOR_FORMATS_DIR", fx.formats.path())
        .env_remove("RIPGREP_CONFIG_PATH")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Some(format!("exit {}\n{}", code(&o), stdout(&o)))
}

/// Code review r5, Fable #1: on a tty, stdin alone (piped, or `-`) is printed without line
/// numbers, as rg 14.1.1; `/dev/stdin`, stdin with a file and `-c` as rg too.
#[test]
fn stdin_alone_on_a_tty_equals_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::write(fx.file("tty.txt"), "first\nneedle 1\nthird\nneedle 2\n").unwrap();
    let mut r = Report(Vec::new());
    for args in [
        "cat tty.txt | {} --color never needle",
        "{} --color never needle - < tty.txt",
        "{} --color never needle /dev/stdin < tty.txt",
        "{} --color never needle - tty.txt < tty.txt",
        "{} --color never -c needle - < tty.txt",
    ] {
        let Some(want) = on_a_tty(&fx, &args.replace("{}", "rg")) else {
            return;
        };
        let got = on_a_tty(&fx, &args.replace("{}", "sfor")).unwrap();
        r.check(args.to_string(), (0, want), (0, got));
    }
    r.assert_empty();
}

/// Code review r5, Fable #2: with one thread (`--sort path`, `-j1`, one file), rg 14.1.1's
/// printer writes the separator between files before a file's first line (hiargs.rs:625-634),
/// so none goes before a file whose output is only the binary message, and the flat layout
/// does the same; a single missing path is walked with one thread, as rg's.
#[test]
fn the_separator_with_one_thread_waits_for_a_line() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("seq")).unwrap();
    let plain = "first needle líne\nsecond\nthird needle\n\n# Head\n\nfourth needle ß\nfifth\n";
    let mut nul8 = b"\xef\xbb\xbf".to_vec();
    nul8.extend_from_slice(plain.as_bytes());
    nul8.extend_from_slice(b"bin\0ary needle\nlast needle\n");
    let mut odd = b"\xff\xfe".to_vec();
    odd.extend(plain.encode_utf16().flat_map(u16::to_le_bytes));
    odd.push(b'A');
    std::fs::write(fx.file("seq/plain.txt"), plain).unwrap();
    std::fs::write(fx.file("seq/nul8.txt"), &nul8).unwrap();
    std::fs::write(fx.file("seq/odd.txt"), &odd).unwrap();
    let files = ["seq/plain.txt", "seq/nul8.txt", "seq/odd.txt"];
    // The flat layout's Match rows put a space after the line number.
    let flat = regex_lite::Regex::new(r"(?m)^([^:\n]*:\d+:) ").unwrap();
    let mut r = Report(Vec::new());
    for threads in [&["--sort", "path"][..], &["-j1"]] {
        for order in [&files[..], &[files[1], files[0], files[2]]] {
            let mut a = threads.to_vec();
            a.extend_from_slice(&["-C1", "needle"]);
            a.extend_from_slice(order);
            let want = rg(&fx, &a, In::Null);
            let mut s = vec!["--no-trail"];
            s.extend_from_slice(&a);
            let got = sfor(&fx, &s, In::Null);
            r.check(
                format!("{s:?}"),
                (code(&want), stdout(&want)),
                (code(&got), stdout(&got)),
            );
            let mut n = vec!["-n"];
            n.extend_from_slice(&a);
            let want = rg(&fx, &n, In::Null);
            let mut f = vec!["--flat"];
            f.extend_from_slice(&a);
            let got = sfor(&fx, &f, In::Null);
            r.check(
                format!("{f:?}"),
                (code(&want), stdout(&want)),
                (
                    code(&got),
                    flat.replace_all(&stdout(&got), "$1").into_owned(),
                ),
            );
        }
    }
    // On a tty the separator is an empty line before the path heading.
    let cmd = "{} --color never --sort path -C1 needle seq/plain.txt seq/nul8.txt seq/odd.txt";
    if let Some(want) = on_a_tty(&fx, &cmd.replace("{}", "rg")) {
        let got = on_a_tty(&fx, &cmd.replace("{}", "sfor")).unwrap();
        r.check(cmd.to_string(), (0, want), (0, got));
    }
    for mode in [&["--no-trail"][..], &[], &["--json"]] {
        let want = rg(&fx, &["needle", "zzz"], In::Null);
        let mut s = mode.to_vec();
        s.extend_from_slice(&["needle", "zzz"]);
        let got = sfor(&fx, &s, In::Null);
        r.check(
            format!("{s:?} (stderr)"),
            (
                code(&want),
                common::stderr(&want).replacen("rg:", "sfor:", 1),
            ),
            (code(&got), common::stderr(&got)),
        );
    }
    r.assert_empty();
}

/// The deviation «A binary file or stdin with a context» of docs/rg-compat.md (code review r5,
/// Fable #3): when the first line after the NUL is a context line, rg 14.1.1 and `--no-trail`
/// print nothing and exit 1, and the Trail layouts print the binary message, as `rg --json`
/// reports the Match; when it is a Match line, all print the message.
#[test]
fn a_binary_file_with_a_context_deviates_as_documented() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::write(fx.file("nul.txt"), "one\nneedle\n\0\n").unwrap();
    let msg = "binary file matches (found \"\\0\" byte around offset 11)\n";
    let mut r = Report(Vec::new());
    for (args, input, rg_prints) in [
        (&["-B1", "needle", "nul.txt"][..], In::Null, false),
        (&["-B1", "needle"], In::File("nul.txt"), false),
        (&["-v", "-C1", "one"], In::File("nul.txt"), false),
        (&["-A1", "needle", "nul.txt"], In::Null, true),
    ] {
        let want = if rg_prints {
            (0, msg.to_string())
        } else {
            (1, String::new())
        };
        let o = rg(&fx, args, input);
        r.check(format!("rg {args:?}"), want.clone(), (code(&o), stdout(&o)));
        let mut j = vec!["--json"];
        j.extend_from_slice(args);
        let o = rg(&fx, &j, input);
        assert!(stdout(&o).contains("\"type\":\"match\""), "rg {j:?}");
        for mode in ["--no-trail", "--flat", "--tree"] {
            let mut s = vec![mode];
            s.extend_from_slice(args);
            let o = sfor(&fx, &s, input);
            let got = (
                code(&o),
                stdout(&o)
                    .replace("<stdin>\n  ", "")
                    .replace("nul.txt\n  ", ""),
            );
            let want = if mode == "--no-trail" {
                want.clone()
            } else {
                (0, msg.to_string())
            };
            r.check(format!("{s:?}"), want, got);
        }
    }
    r.assert_empty();
}

/// Code review r6, Codex #1: ripgrep 14.1.1 runs `-v --count-matches` as `-v --count`
/// (hiargs.rs:129), so it counts the inverted lines, `-m` included; `--json` placed before
/// `--count-matches` gives way to it. With and without `--no-trail`, and with `--within`, whose
/// filter keeps every Match of `cnt/w.md`.
#[test]
fn inverted_count_matches_counts_lines_as_ripgrep() {
    if !reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("cnt")).unwrap();
    std::fs::write(fx.file("cnt/w.md"), "# H\nneedle needle\nplain\nother\nneedle\n").unwrap();
    std::fs::write(fx.file("cnt/in.txt"), "needle\na\nb\nneedle needle\n").unwrap();
    let mut r = Report(Vec::new());
    for variant in [&["--no-trail"][..], &[], &["--within", ""]] {
        for count in [&["--count-matches"][..], &["--json", "--count-matches"]] {
            for limit in [&[][..], &["-m1"], &["-m2"]] {
                let tail: [(&[&str], In); 4] = [
                    (&["needle", "-"], In::File("cnt/in.txt")),
                    (&["needle", "cnt/in.txt"], In::Null),
                    (&["needle", "cnt/w.md"], In::Null),
                    (&["--sort", "path", "needle", "cnt"], In::Null),
                ];
                for (rest, input) in tail {
                    let mut a = vec!["-v"];
                    a.extend_from_slice(count);
                    a.extend_from_slice(limit);
                    a.extend_from_slice(rest);
                    let want = rg(&fx, &a, input);
                    let mut s = variant.to_vec();
                    s.extend_from_slice(&a);
                    let got = sfor(&fx, &s, input);
                    r.check(
                        format!("{s:?}"),
                        (code(&want), stdout(&want)),
                        (code(&got), stdout(&got)),
                    );
                }
            }
        }
    }
    r.assert_empty();
}
