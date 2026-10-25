//! End-to-end tests of M5a (M5a plan, Chunk 9): ripgrep's flags in the Trail layouts, the
//! actions (`--files`, `--generate`, `--pcre2-version`, `--mcp`), the configuration file,
//! `--debug`, `--stats` and rg's message when nothing was searched.
mod common;

use common::{Fixture, code, native, run, stderr, stdout};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

fn ok(fx: &Fixture, args: &[&str]) -> String {
    let o = run(&mut fx.s4(args));
    assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
    assert_eq!(stderr(&o), "", "{args:?}");
    stdout(&o)
}

const PKG: &str = "Deploy guide › Install › Linux › bash";

/// M5a plan, D4: rg's output flags in the one-line layout.
#[test]
fn m5_01_output_flags_in_the_one_line_layout() {
    let fx = Fixture::new();
    let g = "md/guide.md";
    assert_eq!(
        ok(&fx, &["--flat", "--column", "-b", "needle-pkg", g]),
        format!("12:13:96: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(&fx, &["--flat", "-o", "needle-[0-9a-b]+", g]),
        "23: Deploy guide › Install › Plans › Price │ needle-10\n\
         24: Deploy guide › Install › Plans › Plan │ needle-a\n\
         24: Deploy guide › Install › Plans › Price │ needle-b\n\
         33: Deploy guide › Install › Plans │ needle-a\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "--vimgrep", "needle-1", g]),
        "md/guide.md:23:10: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "-r", "X", "needle-pkg", g]),
        format!("12: {PKG} │ apt install X\n")
    );
    assert_eq!(
        ok(&fx, &["--flat", "-0", "-H", "needle-pkg", g]),
        format!("md/guide.md\x0012: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &["--flat", "--path-separator", "\\", "-H", "needle-pkg", g]
        ),
        format!("md\\guide.md:12: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &["--flat", "--field-match-separator", "=", "needle-pkg", g]
        ),
        format!("12= {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "-C1",
                "--context-separator",
                "##",
                "needle-(pkg|10)",
                g
            ]
        ),
        format!(
            "11-```bash\n12: {PKG} │ apt install needle-pkg\n13-```\n##\n\
             22-|------|-------|\n23: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |\n\
             24-| needle-a | needle-b |\n"
        )
    );
}

/// The tree with rg's prelude flags, `-0` after the header's path, and the layout flags of rg:
/// `--heading` and `--pretty` are the tree, `--no-heading` and `--vimgrep` the one-line layout.
#[test]
fn m5_02_the_tree_and_the_layout_flags() {
    let fx = Fixture::new();
    let g = "md/guide.md";
    let tree = "md/guide.md\n  Deploy guide\n    # Install\n      ## Linux\n        ```bash\n";
    assert_eq!(
        ok(&fx, &["--tree", "--column", "-b", "needle-pkg", g]),
        format!("{tree}          12:13:96:apt install needle-pkg\n")
    );
    assert_eq!(
        ok(&fx, &["--tree", "-0", "needle-pkg", g]),
        format!(
            "{}\x00{}          12:apt install needle-pkg\n",
            "md/guide.md",
            &tree["md/guide.md\n".len()..]
        )
    );
    for args in [
        &["--heading"][..],
        &["-p", "--color", "never"],
        &["--flat", "--tree"],
    ] {
        let mut a = args.to_vec();
        a.extend(["needle-pkg", g]);
        assert_eq!(
            ok(&fx, &a),
            format!("{tree}          12:apt install needle-pkg\n"),
            "{args:?}"
        );
    }
    for args in [&["--no-heading"][..], &["--tree", "--no-heading"]] {
        let mut a = args.to_vec();
        a.extend(["needle-pkg", g]);
        assert_eq!(
            ok(&fx, &a),
            format!("12: {PKG} │ apt install needle-pkg\n"),
            "{args:?}"
        );
    }
    // --vimgrep prints one line per Match whatever layout came before it.
    assert_eq!(
        ok(&fx, &["--tree", "--vimgrep", "needle-pkg", g]),
        format!("md/guide.md:12:13: {PKG} │ apt install needle-pkg\n")
    );
}

/// M5a plan, D6: a `-U` Match prints a row per line, `-o` a row per line of its text, a
/// `--null-data` record a row per line with its number, and `--crlf` leaves the `\r` out.
#[test]
fn m5_03_multi_line_records_and_crlf() {
    let fx = Fixture::new();
    std::fs::write(
        fx.file("block.md"),
        "# Install\n\nSome intro needle here.\n\n## Linux\n\nRun the needle command:\n",
    )
    .unwrap();
    std::fs::write(
        fx.file("records.dat"),
        b"one needle\ntwo\0three\0needle four\n",
    )
    .unwrap();
    std::fs::write(fx.file("crlf.txt"), b"one needle\r\ntwo\r\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "-U", r"Linux\n\nRun", "block.md"]),
        "5: Install │ ## Linux\n6: Install │ \n7: Install │ Run the needle command:\n"
    );
    assert_eq!(
        ok(&fx, &["--tree", "-U", r"Linux\n\nRun", "block.md"]),
        "block.md\n  # Install\n    5:## Linux\n    6:\n    7:Run the needle command:\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "-U", "-o", r"Linux\n\nRun", "block.md"]),
        "5: Install │ Linux\n7: Install │ Run\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "--null-data", "needle", "records.dat"]),
        "1: one needle\n1: two\n3: needle four\n"
    );
    assert_eq!(
        ok(&fx, &["--flat", "--crlf", "needle$", "crlf.txt"]),
        "1: one needle\n"
    );
}

/// `--passthru` prints every line, a file without a Match too (whose exit code stays 1).
#[test]
fn m5_04_passthru() {
    let fx = Fixture::new();
    assert_eq!(
        ok(&fx, &["--flat", "--passthru", "Plan C", "md/twice.md"]),
        "1-# Plan of plans\n2-\n3-## Plan B\n4-\n5: Plan of plans │ ## Plan C\n"
    );
    let o = run(&mut fx.s4(["--flat", "--passthru", "absent", "md/twice.md"]));
    assert_eq!(code(&o), 1);
    assert_eq!(
        stdout(&o),
        "1-# Plan of plans\n2-\n3-## Plan B\n4-\n5-## Plan C\n"
    );
    let o = run(&mut fx.s4(["--json", "--passthru", "absent", "md/twice.md"]));
    let types: Vec<String> = stdout(&o)
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        types,
        [
            "begin", "context", "context", "context", "context", "context", "end", "summary"
        ]
    );
}

/// PCRE2 in the Trail layouts, and rg's errors and suggestions.
#[test]
fn m5_05_pcre2_and_the_engines() {
    let fx = Fixture::new();
    assert_eq!(
        ok(&fx, &["--flat", "-P", "needle-(?=pkg)", "md/guide.md"]),
        format!("12: {PKG} │ apt install needle-pkg\n")
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "--flat",
                "--engine",
                "auto",
                "needle-(?=pkg)",
                "md/guide.md"
            ]
        ),
        format!("12: {PKG} │ apt install needle-pkg\n")
    );
    let o = run(&mut fx.s4(["needle-(?=pkg)", "md/guide.md"]));
    assert_eq!(code(&o), 2);
    assert!(stderr(&o).ends_with(
        "Consider enabling PCRE2 with the --pcre2 flag, which can handle backreferences\nand look-around.\n"
    ));
    let o = run(&mut fx.s4(["-P", "(", "md/guide.md"]));
    assert_eq!(
        stderr(&o),
        "s4: PCRE2: error compiling pattern at offset 5: missing closing parenthesis\n"
    );
    // M5b spec §4.4: pcre2-sys builds no JIT on aarch64-apple-darwin, and that system's rg
    // says so too.
    let jit = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "unavailable"
    } else {
        "available"
    };
    let v = ok(&fx, &["--pcre2-version"]);
    assert!(
        v.starts_with("PCRE2 10.") && v.ends_with(&format!(" is available (JIT is {jit})\n")),
        "{v}"
    );
}

/// `-E`: the Trail is computed on the text as decoded, the line printed in UTF-8.
#[test]
fn m5_06_encodings() {
    let fx = Fixture::new();
    std::fs::write(fx.file("latin.md"), b"# Caf\xe9\n\nneedle in the caf\xe9\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "-E", "latin1", "needle", "latin.md"]),
        "3: Café │ needle in the café\n"
    );
    let le: Vec<u8> = "# Notes\nneedle\n"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    std::fs::write(fx.file("utf16.md"), le).unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "-E", "utf-16le", "needle", "utf16.md"]),
        "2: Notes │ needle\n"
    );
}

/// `--files`, `--sort`/`--sortr`, `--type-add` and `--max-filesize` in a search.
#[test]
fn m5_07_files_sorting_and_types() {
    let fx = Fixture::new();
    assert_eq!(
        ok(&fx, &["--files", "--sort", "path", "md"]),
        native("md/guide.md\nmd/twice.md\n")
    );
    assert_eq!(
        ok(&fx, &["--files", "--sortr", "path", "md"]),
        native("md/twice.md\nmd/guide.md\n")
    );
    assert_eq!(
        ok(&fx, &["--files", "-0", "--sort", "path", "md"]),
        native("md/guide.md\0md/twice.md\0")
    );
    let o = run(&mut fx.s4(["--files", "-g", "*.zzz"]));
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
    assert_eq!(
        ok(
            &fx,
            &["--type-add", "notes:twice.*", "-t", "notes", "-l", "Plan"]
        ),
        native("md/twice.md\n")
    );
    let o = run(&mut fx.s4(["--max-filesize", "100", "-l", "needle-pkg", "md"]));
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
}

/// M5a plan, D3: `SEARCHFOR_CONFIG_PATH`'s arguments go before the command line's; `--no-config`
/// skips them; a file that cannot be read is a message.
#[test]
fn m5_08_the_configuration_file() {
    let fx = Fixture::new();
    let cfg = fx.file("s4rc");
    std::fs::write(&cfg, "# max one per file\n--max-count=1\n\n  -i  \n").unwrap();
    let with = |args: &[&str], path: &std::path::Path| {
        let mut c = fx.s4(args);
        c.env("SEARCHFOR_CONFIG_PATH", path);
        run(&mut c)
    };
    let o = with(&["--flat", "NEEDLE-[0-9a-b]+", "md/guide.md"], &cfg);
    assert_eq!(
        stdout(&o),
        "23: Deploy guide › Install › Plans › Price │ | Pro  | needle-10 |\n"
    );
    let o = with(&["--no-config", "--flat", "NEEDLE", "md/guide.md"], &cfg);
    assert_eq!((code(&o), stdout(&o).as_str()), (1, ""));
    // The command line wins over the file.
    let o = with(&["-s", "--flat", "NEEDLE", "md/guide.md"], &cfg);
    assert_eq!(code(&o), 1);
    let o = with(
        &["--flat", "needle-pkg", "md/guide.md"],
        &fx.file("missing"),
    );
    assert_eq!(code(&o), 0);
    assert!(
        stderr(&o).starts_with("s4: failed to read the file specified in SEARCHFOR_CONFIG_PATH: ")
    );
}

/// M5a plan, D16: `--generate`'s five kinds.
#[test]
fn m5_09_generate() {
    let fx = Fixture::new();
    assert!(ok(&fx, &["--generate", "man"]).contains(".TH s4"));
    assert!(ok(&fx, &["--generate", "complete-bash"]).contains("_s4()"));
    assert!(ok(&fx, &["--generate", "complete-zsh"]).starts_with("#compdef s4"));
    assert!(ok(&fx, &["--generate", "complete-fish"]).contains("complete -c s4"));
    assert!(ok(&fx, &["--generate", "complete-powershell"]).contains("Register-ArgumentCompleter"));
    let o = run(&mut fx.s4(["--generate", "complete-tcsh"]));
    assert_eq!(code(&o), 2);
}

/// M5a plan, D15: `--debug` and `--trace` in rg's format, the ignore crate's lines included.
#[test]
fn m5_10_debug_and_trace() {
    let fx = Fixture::new();
    // An ignore file of the fixture's own, so the walk builds a glob set (and logs it) on any
    // machine, not only on one with a global git ignore file.
    std::fs::write(fx.file("md/.ignore"), "*.never\n").unwrap();
    let o = run(&mut fx.s4(["--debug", "-l", "needle-pkg", "md"]));
    assert_eq!(code(&o), 0);
    let err = stderr(&o);
    assert!(err.lines().all(|l| l.starts_with("s4: DEBUG|")), "{err}");
    assert!(err.contains("|globset|"), "{err}");
    let o = run(&mut fx.s4(["--trace", "-l", "needle-pkg", "md"]));
    assert!(
        stderr(&o).contains("s4: TRACE|"),
        "exit {:?}, stdout {:?}, stderr {:?}",
        o.status,
        String::from_utf8_lossy(&o.stdout),
        stderr(&o)
    );
    let o = run(&mut fx.s4(["-l", "needle-pkg", "md"]));
    assert_eq!(stderr(&o), "");
}

/// M5a plan, D17; spec §11 #14: `s4 --mcp` over stdin and stdout. The server and its
/// searches read no configuration file, whose `--pre` would otherwise run on every call and
/// whose `--generate` would replace the server (review r1 of the plan, Fable #1, Codex #1); a
/// value that starts with `-` is a value; a search with an error keeps its Matches; long lines,
/// bytes that are not UTF-8 and a repeated Section come within limits (Codex #2, #3, #7;
/// Fable #2).
#[test]
fn m5_11_mcp_server() {
    let fx = Fixture::new();
    // A configuration file the server and its searches never read (M5b plan, Task 1.2): read
    // by the server, `--generate` would replace it; read by a search, its pattern would find
    // the sentinel's line. No executable script, so the trap is the same on every system.
    let cfg = fx.file("s4rc");
    std::fs::write(&cfg, "--generate=man\n--regexp=ABSENT_CONFIG_SENTINEL\n").unwrap();
    let dir = fx.file("mcp");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("sentinel.txt"), "ABSENT_CONFIG_SENTINEL\n").unwrap();
    let long = format!("{} needle {}\n", "x".repeat(3000), "y".repeat(3000));
    std::fs::write(dir.join("long.txt"), long).unwrap();
    // A line longer than the 4 MiB a message of s4 may be (review r2 of the plan, Fable #1).
    let huge = format!("{} needle\n", "x".repeat(5 << 20));
    std::fs::write(dir.join("huge.txt"), huge).unwrap();
    std::fs::write(dir.join("lossy.txt"), b"needle\xff\n").unwrap();
    std::fs::write(dir.join("two.md"), "# Part\n\nneedle one\n\nneedle two\n").unwrap();
    let mut child = fx
        .s4(["--mcp"])
        .env("SEARCHFOR_CONFIG_PATH", &cfg)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut ask = |msg: Value| -> Option<Value> {
        writeln!(input, "{msg}").unwrap();
        input.flush().unwrap();
        msg.get("id")?;
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        Some(serde_json::from_str(&line).unwrap())
    };
    let init = ask(json!({"jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}))
    .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(ask(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    let tools = ask(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).unwrap();
    assert_eq!(tools["result"]["tools"][0]["name"], "search");
    let r = ask(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"needle","paths":["md"],"within":"Install › Linux","glob":["guide.md"]}}}))
    .unwrap();
    let matches = &r["result"]["structuredContent"]["matches"];
    assert_eq!(r["result"]["isError"], false);
    assert_eq!(matches[0]["path"], native("md/guide.md"));
    assert_eq!(matches[0]["line"], 12);
    assert_eq!(
        matches[0]["trail"],
        json!(["Deploy guide", "Install", "Linux", "bash"])
    );
    assert_eq!(matches[0]["text"], "apt install needle-pkg");
    let r = ask(
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"needle","paths":["md/guide.md"],"max_results":2}}}),
    )
    .unwrap();
    assert_eq!(
        r["result"]["structuredContent"]["matches"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(r["result"]["structuredContent"]["truncated"], true);
    assert_eq!(
        r["result"]["structuredContent"]["truncated_by"],
        "max_results"
    );
    let r = ask(
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"(","paths":["md"]}}}),
    )
    .unwrap();
    assert_eq!(r["result"]["isError"], true);
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("regex parse error")
    );
    let mut search = |arguments: Value| -> Value {
        let r = ask(json!({"jsonrpc":"2.0","id":6,"method":"tools/call",
            "params":{"name":"search","arguments":arguments}}))
        .unwrap();
        r["result"].clone()
    };
    let r = search(json!({"pattern":"needle","paths":["mcp/long.txt"]}));
    let m = &r["structuredContent"]["matches"][0];
    assert!(m["text"].as_str().unwrap().len() < 1000, "{m}");
    assert!(m["omitted_before"].as_u64().unwrap() > 2000, "{m}");
    let r = search(json!({"pattern":"needle","paths":["mcp/lossy.txt"]}));
    let m = &r["structuredContent"]["matches"][0];
    assert_eq!(
        (&m["text"], &m["lossy"]),
        (&json!("needle\u{FFFD}"), &json!(true))
    );
    let r = search(json!({"pattern":"needle","paths":["mcp/two.md"],"expand":true}));
    let m = &r["structuredContent"]["matches"];
    assert_eq!(
        m[0]["section"]["text"],
        "# Part\n\nneedle one\n\nneedle two\n"
    );
    assert_eq!(m[1]["section"]["same_as"], 0);
    // Review r2 of the plan, Fable #1: a Match on a line past 4 MiB is left out, and said.
    let r = search(json!({"pattern":"needle","paths":["mcp/huge.txt"]}));
    let r = &r["structuredContent"];
    assert_eq!(r["matches"], json!([]));
    assert_eq!(
        (&r["truncated"], &r["truncated_by"]),
        (&json!(true), &json!("long_line"))
    );
    assert!(
        r["errors"].as_str().unwrap().contains("`mcp/huge.txt`"),
        "{r}"
    );
    // Review r2 of the plan, Fable #2: line 24 holds Matches under two cells, two Matches.
    let r = search(json!({"pattern":"needle-[ab]","paths":["md/guide.md"]}));
    let trails: Vec<(Value, Value)> = r["structuredContent"]["matches"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["line"] == 24)
        .map(|m| (m["trail"][3].clone(), m["text"].clone()))
        .collect();
    let row = json!("| needle-a | needle-b |");
    assert_eq!(
        trails,
        [(json!("Plan"), row.clone()), (json!("Price"), row)]
    );
    let r = search(json!({"pattern":"needle","paths":["md"],"within":"-x"}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["matches"], json!([]));
    let r = search(json!({"pattern":"needle-pkg","paths":["md/guide.md","missing"]}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["structuredContent"]["matches"][0]["line"], 12);
    assert!(
        r["structuredContent"]["errors"]
            .as_str()
            .unwrap()
            .contains("missing"),
        "{r}"
    );
    let r = search(json!({"pattern":"needle","paths":["mcp/sentinel.txt"]}));
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(
        r["structuredContent"]["matches"],
        json!([]),
        "the configuration's pattern reached a search"
    );
    drop(input);
    assert!(child.wait().unwrap().success());
    // A `--generate` in the configuration file does not replace the server.
    std::fs::write(&cfg, "--generate=man\n").unwrap();
    let mut c = fx.s4(["--mcp"]);
    c.env("SEARCHFOR_CONFIG_PATH", &cfg)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut server = c.spawn().unwrap();
    let mut input = server.stdin.take().unwrap();
    writeln!(input, "{}", json!({"jsonrpc":"2.0","id":1,"method":"ping"})).unwrap();
    drop(input);
    let out = std::io::read_to_string(server.stdout.take().unwrap()).unwrap();
    assert_eq!(out, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n");
    assert!(server.wait().unwrap().success());
}

/// rg's message when the current directory, searched implicitly, gave no file.
#[test]
fn m5_12_nothing_searched() {
    let fx = Fixture::new();
    let o = run(&mut fx.s4(["-g", "*.zzz", "needle"]));
    assert_eq!(code(&o), 2);
    assert_eq!(
        stderr(&o),
        "s4: No files were searched, which means s4 probably applied a filter you didn't expect.\n\
         Running with --debug will show why files are being skipped.\n"
    );
    let o = run(&mut fx.s4(["-g", "*.zzz", "needle", "md"]));
    assert_eq!((code(&o), stderr(&o).as_str()), (1, ""));
}

/// M5a plan, D14: `--stats` after the Trail layouts' output, every file searched counted.
#[test]
fn m5_13_stats_in_the_trail_layouts() {
    let fx = Fixture::new();
    let out = ok(
        &fx,
        &["--flat", "--stats", "--sort", "path", "needle-pkg", "md"],
    );
    let re = regex_lite::Regex::new(r"[0-9]+\.[0-9]{6} seconds").unwrap();
    let out = re.replace_all(&out, "T seconds");
    let row = format!(
        "{}:12: {PKG} │ apt install needle-pkg\n",
        native("md/guide.md")
    );
    assert_eq!(
        out,
        format!(
            "{row}\n1 matches\n1 matched lines\n1 files contained matches\n2 files searched\n\
             {} bytes printed\n{} bytes searched\nT seconds spent searching\nT seconds\n",
            row.len(),
            std::fs::metadata(fx.file("md/guide.md")).unwrap().len()
                + std::fs::metadata(fx.file("md/twice.md")).unwrap().len()
        )
    );
}

/// `--colors` with the `trail` type, and `--hyperlink-format`, in the one-line layout. Each
/// link's target is the one rg gives with the same format (M5b plan, Task 1.3): its own path
/// rules on each system (`/C:/…` on Windows), and its percent-encoding of a space, `%` and `#`.
#[test]
fn m5_14_colours_and_links() {
    let fx = Fixture::new();
    let format = "file://{path}:{line}";
    let out = ok(
        &fx,
        &[
            "--flat",
            "--color",
            "ansi",
            "--colors",
            "trail:fg:yellow",
            "--colors",
            "line:fg:blue",
            "--hyperlink-format",
            format,
            "-H",
            "needle-pkg",
            "md/guide.md",
        ],
    );
    assert!(out.starts_with("\x1b]8;;file://"), "{out:?}");
    assert!(out.contains("\x1b[34m12\x1b[0m"), "{out:?}");
    assert!(out.contains("\x1b[33mDeploy guide"), "{out:?}");
    if !common::reference_rg() {
        return;
    }
    let odd = "x y 100%#.md";
    std::fs::write(fx.file(odd), "# Head needle\nneedle\n").unwrap();
    let rg = |args: &[&str]| {
        let mut all = vec!["--color", "ansi", "--hyperlink-format", format, "-H", "-n"];
        all.extend_from_slice(args);
        stdout(&run(&mut fx.rg(&all)))
    };
    let want = rg(&["needle-pkg", "md/guide.md"]);
    assert_eq!(link_targets(&out), link_targets(&want), "{out:?}\n{want:?}");
    let want = link_targets(&rg(&["needle", odd]));
    assert_eq!(want.len(), 2, "{want:?}");
    assert!(want[0].ends_with("/x%20y%20100%25%23.md:1"), "{want:?}");
    let flat = ok(
        &fx,
        &[
            "--flat",
            "--color",
            "ansi",
            "--hyperlink-format",
            format,
            "-H",
            "needle",
            odd,
        ],
    );
    assert_eq!(link_targets(&flat), want, "{flat:?}");
    // The tree's header links the file at line 1, and each row its line.
    let tree = ok(
        &fx,
        &[
            "--tree",
            "--color",
            "ansi",
            "--hyperlink-format",
            format,
            "needle",
            odd,
        ],
    );
    assert_eq!(link_targets(&tree), want, "{tree:?}");
    assert!(
        tree.starts_with(&format!("\x1b]8;;{}\x1b\\", want[0])),
        "{tree:?}"
    );
}

/// The targets of the OSC 8 links of `out`, in order, each once where links that follow each
/// other open the same target; a link's end (an empty target) left out.
fn link_targets(out: &str) -> Vec<String> {
    let mut targets: Vec<String> = Vec::new();
    for piece in out.split("\x1b]8;;").skip(1) {
        let target = &piece[..piece.find("\x1b\\").unwrap_or(piece.len())];
        if !target.is_empty() && targets.last().is_none_or(|t| t != target) {
            targets.push(target.to_string());
        }
    }
    targets
}

/// `-c`, `-0` and `--include-zero` with a structural flag, through s4's own summaries.
#[test]
fn m5_15_summaries_with_trails() {
    let fx = Fixture::new();
    assert_eq!(
        ok(
            &fx,
            &[
                "-c", "-0", "--sort", "path", "--within", "Linux", "needle", "md"
            ]
        ),
        native("md/guide.md\x002\n")
    );
    assert_eq!(
        ok(
            &fx,
            &[
                "-c",
                "--include-zero",
                "--sort",
                "path",
                "--within",
                "Linux",
                "needle",
                "md"
            ]
        ),
        native("md/guide.md:2\nmd/twice.md:0\n")
    );
}

/// M5a plan, D13: `--line-buffered` prints each line as it is found: the first line of a slow
/// stdin comes out before the input ends, as rg's. The line is read in a thread and waited for
/// at most `WAIT`, so a line kept in a buffer fails the test instead of hanging it (code review
/// r1 of M5a, Fable #5).
#[test]
fn m5_16_line_buffered() {
    const WAIT: std::time::Duration = std::time::Duration::from_secs(10);
    let fx = Fixture::new();
    let mut child = fx
        .s4(["--no-trail", "--line-buffered", "needle"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    writeln!(input, "a needle").unwrap();
    input.flush().unwrap();
    let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let _ = tx.send(out.read_line(&mut line).map(|_| line));
    });
    let line = rx.recv_timeout(WAIT);
    // Ends s4 either way: its stdin closes, and the reader thread gets the end of its output.
    drop(input);
    let status = child.wait().unwrap();
    reader.join().unwrap();
    let line = line.unwrap_or_else(|_| panic!("no line within {WAIT:?} of its input"));
    assert_eq!(line.unwrap(), "a needle\n");
    assert!(status.success());
}

/// Review r1 of the M5a plan, Codex #4: an object file's string takes `-o`, `--vimgrep` and
/// `-r` (spec §5.7, §8): a row per Match at its offset under its Trail, the string with its
/// Matches replaced; in ELF, PE and Mach-O.
#[test]
fn m5_17_object_rows_take_o_vimgrep_and_r() {
    use searchfor::processors::object::fixture as obj;
    let fx = Fixture::new();
    let strings = b"\0first needle|second needle\0";
    let elf = obj::elf(
        &obj::Spec::new()
            .section(".text", b"\xc3\0\0\0")
            .section(".rodata", strings)
            .sized("one", 1, 1, 13)
            .sized("two", 1, 14, 13),
    )
    .file;
    let pe = obj::pe(
        &obj::Spec::new()
            .section(".text", b"\xc3\0\0\0")
            .section(".rdata", strings)
            .symbol("first_string", 1, 1),
    )
    .file;
    let macho = obj::macho(
        &obj::Spec::new()
            .section("__TEXT,__text", b"\xc3\0\0\0")
            .section("__TEXT,__cstring", strings)
            .symbol("_first", 1, 1),
    )
    .file;
    let cases = [
        ("app", &elf, "ELF x86-64 › .rodata", ["one+0x6", "two+0x7"]),
        (
            "app.exe",
            &pe,
            "PE x86-64 › .rdata",
            ["first_string+0x6", "first_string+0x14"],
        ),
        (
            "app-mac",
            &macho,
            "Mach-O x86-64 › __TEXT,__cstring",
            ["_first+0x6", "_first+0x14"],
        ),
    ];
    for (name, bytes, trail, [sym1, sym2]) in cases {
        std::fs::write(fx.file(name), bytes).unwrap();
        // The string's first byte, and its two Matches at 6 and 20.
        let s = memchr::memmem::find(bytes, b"first needle").unwrap();
        let (a, b) = (format!("{:#x}", s + 6), format!("{:#x}", s + 20));
        assert_eq!(
            ok(&fx, &["--flat", "-o", "needle", name]),
            format!("{a}: {trail} › {sym1} │ needle\n{b}: {trail} › {sym2} │ needle\n")
        );
        assert_eq!(
            ok(&fx, &["--flat", "--vimgrep", "needle", name]),
            format!(
                "{name}:{a}:7: {trail} › {sym1} │ first needle|second needle\n\
                 {name}:{b}:21: {trail} › {sym2} │ first needle|second needle\n"
            )
        );
        assert_eq!(
            ok(&fx, &["--flat", "-r", "X", "needle", name]),
            format!("{a}: {trail} › {sym1} │ first X|second X\n")
        );
    }
    // Review r2 of the plan, Codex #2: `-r` replaces each Match where the search found it in
    // the file, so `^` does not match after a NUL and a group reads the file's bytes, and only
    // then is the string made printable.
    let strings = b"\0needle target\0zz ab\x01cd yy\0";
    let files = [
        (
            "anchors",
            obj::elf(
                &obj::Spec::new()
                    .section(".text", b"\xc3\0\0\0")
                    .section(".rodata", strings),
            )
            .file,
        ),
        (
            "anchors.exe",
            obj::pe(
                &obj::Spec::new()
                    .section(".text", b"\xc3\0\0\0")
                    .section(".rdata", strings),
            )
            .file,
        ),
        (
            "anchors-mac",
            obj::macho(
                &obj::Spec::new()
                    .section("__TEXT,__text", b"\xc3\0\0\0")
                    .section("__TEXT,__cstring", strings),
            )
            .file,
        ),
    ];
    let texts = |args: &[&str], name: &str| -> Vec<String> {
        let mut a = vec!["--flat"];
        a.extend_from_slice(args);
        a.push(name);
        ok(&fx, &a)
            .lines()
            .map(|l| l.split_once(" │ ").unwrap().1.to_string())
            .collect()
    };
    for (name, bytes) in &files {
        std::fs::write(fx.file(name), bytes).unwrap();
        assert_eq!(texts(&["-r", "X", "^needle|target"], name), ["needle X"]);
        assert_eq!(texts(&["-r", "X", "ab\\x01cd"], name), ["zz X yy"]);
        assert_eq!(texts(&["-r", "<$1>", "a(b\\x01c)d"], name), ["zz <b.c> yy"]);
        assert_eq!(
            texts(&["-o", "-r", "[$1]", "(tar)get|a(b\\x01c)d"], name),
            ["[tar]", "[]"]
        );
        // Review r3 of the plan, Codex #3: a `\K` Match, whose pattern reads the bytes before
        // it, is found again from its line's start, its groups before `\K` too, as `rg -a`.
        let k = ["-P", "-r", "X", "needle \\Ktarget"];
        assert_eq!(texts(&k, name), ["needle X"]);
        let k = ["-P", "-r", "<$1>", "(nee)dle \\Ktarget"];
        assert_eq!(texts(&k, name), ["needle <nee>"]);
        let k = ["-P", "-o", "-r", "X", "needle \\Ktarget"];
        assert_eq!(texts(&k, name), ["X"]);
    }
    // Review r4 of the plan, Codex #2: a Match after `\K` keeps the groups the search found.
    // From the line's start the first alternative matches, its group before `\K`; from the
    // string `target`, the second would match the same bytes with the other group.
    let strings = b"\0needle\0target\0";
    let alternatives = [
        obj::elf(
            &obj::Spec::new()
                .section(".text", b"\xc3\0\0\0")
                .section(".rodata", strings),
        )
        .file,
        obj::pe(
            &obj::Spec::new()
                .section(".text", b"\xc3\0\0\0")
                .section(".rdata", strings),
        )
        .file,
        obj::macho(
            &obj::Spec::new()
                .section("__TEXT,__text", b"\xc3\0\0\0")
                .section("__TEXT,__cstring", strings),
        )
        .file,
    ];
    let k = [
        "-a",
        "-P",
        "-o",
        "-r",
        "<$1|$2>",
        "(needle)\\x00\\Ktarget|(target)",
    ];
    for (name, bytes) in ["alt", "alt.exe", "alt-mac"].iter().zip(&alternatives) {
        std::fs::write(fx.file(name), bytes).unwrap();
        assert_eq!(texts(&k, name), ["<needle|>"]);
        if common::reference_rg() {
            let mut rg = k.to_vec();
            rg.push(name);
            assert_eq!(stdout(&run(&mut fx.rg(&rg))), "<needle|>\n");
        }
    }
    // Review r5 of the plan, Codex #2: a `\K` Match whose pattern reads the record before its
    // own is reported on its record, where the search of the record's bytes finds no Match.
    // Its row is the record's string, without a submatch, and s4 exits with 0, as `rg -a`
    // prints the record; with `-r` and `-o` too, as rg.
    let strings = b"\0A\0B\0";
    let records = [
        (
            "k",
            obj::elf(
                &obj::Spec::new()
                    .section(".text", b"\xc3\0\0\0")
                    .section(".rodata", strings),
            )
            .file,
        ),
        (
            "k.exe",
            obj::pe(
                &obj::Spec::new()
                    .section(".text", b"\xc3\0\0\0")
                    .section(".rdata", strings),
            )
            .file,
        ),
        (
            "k-mac",
            obj::macho(
                &obj::Spec::new()
                    .section("__TEXT,__text", b"\xc3\0\0\0")
                    .section("__TEXT,__cstring", strings),
            )
            .file,
        ),
    ];
    for (name, bytes) in &records {
        std::fs::write(fx.file(name), bytes).unwrap();
        for flags in [&[][..], &["-r", "X"], &["-o"], &["-o", "-r", "X"]] {
            let mut k = vec!["-a", "-P", "-U", "--null-data"];
            k.extend_from_slice(flags);
            k.push("A\\x00\\KB");
            assert_eq!(texts(&k, name), ["B"], "{k:?}");
            if common::reference_rg() {
                let mut rg = k.clone();
                rg.push(name);
                let o = run(&mut fx.rg(&rg));
                assert_eq!((code(&o), stdout(&o)), (0, "B\0".into()), "{k:?}");
            }
        }
    }
}

/// Review r5 of the plan, Codex #1 and the audit that followed it: what s4 does with the
/// Matches of a line or of a file costs time that grows with them, not with their square, in
/// every mode that prints them. Each case runs over N Matches and over 4 N, and the second
/// must cost less than six times the first (CPU time of s4 and its children, best of three):
/// one line of JSON with a Match under each of its N keys; a line of text (Codex's `-o -r`,
/// over 4 000 and 16 000 Matches); a `--null-data` record and a `-U` block of 2 N lines; a
/// Markdown file of N Sections; an object file's string of N Matches, and its N strings `B`
/// each after a NUL (`\x00\KB`, review r4, Codex #1); the MCP server over the line and the
/// Sections. Each file's rows are wider than a Fragment's index block (4 KiB), so a row costs
/// the same at N and at 4 N.
#[cfg(target_os = "linux")]
#[test]
fn m5_22_the_matches_of_a_line_or_a_file_cost_linear_time() {
    use searchfor::processors::object::fixture as obj;
    let fx = Fixture::new();
    let elf = |rodata: &[u8]| {
        obj::elf(
            &obj::Spec::new()
                .section(".text", b"\xc3\0\0\0")
                .section(".rodata", rodata),
        )
        .file
    };
    for n in [1_000, 4_000, 16_000] {
        let keys: Vec<String> = (0..n).map(|i| format!("\"k{i}\":\"B\"")).collect();
        let files = [
            (
                "line.json",
                format!("{{{}}}\n", keys.join(",")).into_bytes(),
            ),
            ("text.txt", [b"B ".repeat(n), b"\n".to_vec()].concat()),
            (
                "record.txt",
                [b"B\nx\n".repeat(n), b"\0y\0".to_vec()].concat(),
            ),
            ("block.txt", b"B\nx\n".repeat(n)),
            (
                "doc.md",
                (0..n)
                    .map(|i| format!("# H{i}\n\nB\n\n"))
                    .collect::<String>()
                    .into_bytes(),
            ),
            (
                "string.o",
                elf(&[b"\0".to_vec(), b"B       ".repeat(n), b"\0".to_vec()].concat()),
            ),
            (
                "strings.o",
                elf(&[b"\0B".repeat(n), b"\0".to_vec()].concat()),
            ),
        ];
        for (name, bytes) in files {
            std::fs::write(fx.file(&name.replace('.', &format!("{n}."))), bytes).unwrap();
        }
    }
    let cases: &[(usize, &[&str])] = &[
        (1_000, &["--flat", "B", "line@.json"]),
        (1_000, &["--tree", "B", "line@.json"]),
        (1_000, &["--json", "B", "line@.json"]),
        (1_000, &["--json", "--expand", "B", "line@.json"]),
        (1_000, &["--flat", "-o", "B", "line@.json"]),
        (1_000, &["--flat", "-r", "X", "B", "line@.json"]),
        (1_000, &["--tree", "-o", "-r", "X", "B", "line@.json"]),
        (1_000, &["--vimgrep", "B", "line@.json"]),
        (
            1_000,
            &["--flat", "--trim", "--column", "-b", "B", "line@.json"],
        ),
        (4_000, &["--flat", "-o", "-r", "X", "B", "text@.txt"]),
        (1_000, &["--flat", "--null-data", "B", "record@.txt"]),
        (1_000, &["--tree", "--null-data", "-o", "B", "record@.txt"]),
        (
            1_000,
            &["--flat", "--null-data", "-r", "X", "B", "record@.txt"],
        ),
        (1_000, &["--vimgrep", "--null-data", "B", "record@.txt"]),
        (
            1_000,
            &["--flat", "--null-data", "--passthru", "B", "record@.txt"],
        ),
        (
            1_000,
            &["--flat", "--null-data", "-v", "-C1", "B", "record@.txt"],
        ),
        (1_000, &["--flat", "-U", "B\\nx", "block@.txt"]),
        (1_000, &["--flat", "-U", "-o", "B\\nx", "block@.txt"]),
        (1_000, &["--vimgrep", "-U", "B\\nx", "block@.txt"]),
        (1_000, &["--flat", "-C1", "B", "doc@.md"]),
        (1_000, &["--flat", "--expand", "B", "doc@.md"]),
        (1_000, &["--tree", "--expand", "B", "doc@.md"]),
        (1_000, &["--flat", "-a", "B", "string@.o"]),
        (1_000, &["--flat", "-a", "-o", "-r", "X", "B", "string@.o"]),
        (1_000, &["--vimgrep", "-a", "B", "string@.o"]),
        (
            1_000,
            &["--flat", "-a", "-P", "-r", "X", "\\x00\\KB", "strings@.o"],
        ),
        (1_000, &["--mcp", "line@.json"]),
        (1_000, &["--mcp", "doc@.md"]),
    ];
    let mut slow = Vec::new();
    for &(n, case) in cases {
        let time = |n: usize| -> std::time::Duration {
            let args: Vec<String> = case
                .iter()
                .map(|a| a.replace('@', &n.to_string()))
                .collect();
            (0..3).map(|_| cpu_time(&fx, &args)).min().unwrap()
        };
        let (small, large) = (time(n), time(4 * n));
        eprintln!("{case:?}: {small:?} and {large:?}");
        if large >= small * 6 {
            slow.push(format!("{case:?}: {small:?} and {large:?}"));
        }
    }
    assert!(
        slow.is_empty(),
        "4 N Matches cost six times N or more:\n{}",
        slow.join("\n")
    );
}

/// N Sections, each with a few Matches, under a Segment open to the end of the file: a
/// Markdown `# Top`, HTML's `html` and `body`, XML's root element and YAML's top-level key
/// over its hidden document.
#[cfg(target_os = "linux")]
fn sections_under_a_root(ext: &str, n: usize) -> String {
    let each = |f: &dyn Fn(usize) -> String| (0..n).map(f).collect::<String>();
    match ext {
        "md" => format!(
            "# Top\n{}",
            each(&|i| format!("## S{i}\nneedle a\nneedle b\nneedle c\n"))
        ),
        "html" => format!(
            "<html><body>\n{}</body></html>\n",
            each(&|i| format!("<div id=\"d{i}\">\n<p>needle a</p>\n<p>needle b</p>\n</div>\n"))
        ),
        "xml" => format!(
            "<root>\n{}</root>\n",
            each(&|i| format!("<rec id=\"{i}\">\n<a>needle</a>\n<b>needle</b>\n</rec>\n"))
        ),
        "yaml" => format!(
            "items:\n{}",
            each(&|i| format!("  - name: n{i}\n    a: needle\n    b: needle\n"))
        ),
        _ => unreachable!(),
    }
}

/// Code review r2, Fable #1 and Codex #2: --expand under a Segment that stays open costs each
/// Section its own rows. Every Section printed made the rows of every Match held since the root
/// opened, so 4 N Sections cost 16 times N (Markdown with 5 000 Sections: 26.6 s, 0.07 s in
/// 0.5.0). As `m5_22`, 4 N must cost less than six times N (CPU time, best of three).
#[cfg(target_os = "linux")]
#[test]
fn m5b_expand_under_an_open_root_costs_linear_time() {
    let fx = Fixture::new();
    let n = 200;
    let formats = ["md", "html", "xml", "yaml"];
    for ext in formats {
        for k in [n, 4 * n] {
            let name = format!("root{k}.{ext}");
            std::fs::write(fx.file(&name), sections_under_a_root(ext, k)).unwrap();
        }
    }
    let mut slow = Vec::new();
    for ext in formats {
        for layout in ["--flat", "--tree"] {
            let time = |k: usize| -> std::time::Duration {
                let args: Vec<String> = [layout, "--expand", "-j1", "needle"]
                    .iter()
                    .map(|a| a.to_string())
                    .chain([format!("root{k}.{ext}")])
                    .collect();
                (0..3).map(|_| cpu_time(&fx, &args)).min().unwrap()
            };
            let (small, large) = (time(n), time(4 * n));
            eprintln!("{ext} {layout}: {small:?} and {large:?}");
            if large >= small * 6 {
                slow.push(format!("{ext} {layout}: {small:?} and {large:?}"));
            }
        }
    }
    assert!(
        slow.is_empty(),
        "4 N Sections cost six times N or more:\n{}",
        slow.join("\n")
    );
}

/// The CPU time of `s4 args` and of its children, its stdout read to its end; it must exit
/// with 0 and print something. `--mcp PATH` asks the MCP server for every Match of PATH, with
/// its Section.
#[cfg(target_os = "linux")]
#[allow(clippy::zombie_processes)] // `wait4` reaps it, and gives its CPU time.
fn cpu_time(fx: &Fixture, args: &[String]) -> std::time::Duration {
    use std::io::Read;
    let mcp = args[0] == "--mcp";
    let mut child = fx
        .s4(if mcp { &args[..1] } else { args })
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    if mcp {
        let call = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search",
            "arguments":{"pattern":"B","paths":[args[1]],"expand":true,"max_results":1000}}});
        writeln!(input, "{call}").unwrap();
    }
    drop(input);
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let pid = child.id() as libc::pid_t;
    let mut status = 0;
    // SAFETY: an all-zero `rusage` is valid, and `wait4` writes it for a child of this process.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::wait4(pid, &mut status, 0, &mut usage) }, pid);
    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "{args:?}: {status}"
    );
    assert!(!out.is_empty(), "{args:?}");
    if mcp {
        let r: Value = serde_json::from_slice(&out).unwrap();
        assert!(
            !r["result"]["structuredContent"]["matches"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{args:?}: {r}"
        );
    }
    let time = |t: libc::timeval| {
        std::time::Duration::from_secs(t.tv_sec as u64)
            + std::time::Duration::from_micros(t.tv_usec as u64)
    };
    time(usage.ru_utime) + time(usage.ru_stime)
}

/// Review r1 of the M5a plan, Codex #8: `--trim` in the Trail layouts leaves out the white
/// space grep-printer leaves out (`\r` and `\x0B` too), never the line terminator's bytes.
#[test]
fn m5_18_trim_as_ripgrep() {
    let fx = Fixture::new();
    std::fs::write(fx.file("t.txt"), b"\r needle\n\x0b needle\n\x0c\tneedle\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "--trim", "needle", "t.txt"]),
        "1: needle\n2: needle\n3: needle\n"
    );
    std::fs::write(fx.file("crlf.txt"), b"\r\tneedle\r\n").unwrap();
    assert_eq!(
        ok(&fx, &["--flat", "--trim", "--crlf", "needle", "crlf.txt"]),
        "1: \r\tneedle\n"
    );
}

/// Review r2 of the plan, Codex #1: `--no-mmap` maps no file, however large, as rg; without it
/// s4 maps a file of 8 MiB or more. Seen in `/proc/PID/maps` while s4 prints: the test reads
/// the first line and no more, so s4 waits on a full pipe with the file still open. Linux
/// alone has `/proc/PID/maps`; the output `--no-mmap` keeps is checked on every system by
/// `m5b_no_mmap_keeps_output_portable`.
#[cfg(target_os = "linux")]
#[test]
fn m5_19_no_mmap_maps_no_file() {
    let fx = Fixture::new();
    let line = format!("needle {}\n", "x".repeat(90));
    let lines = line.repeat(9 * 1024 * 1024 / line.len() + 1);
    std::fs::write(fx.file("big.md"), format!("# Big\n{lines}")).unwrap();
    let big = fx.file("big.md").canonicalize().unwrap();
    let mapped = |flags: &[&str]| -> bool {
        let mut args = flags.to_vec();
        args.extend(["--flat", "needle", "big.md"]);
        let mut child = fx
            .s4(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        out.read_line(&mut first).unwrap();
        assert!(first.contains("needle"), "{flags:?}: {first}");
        let maps = std::fs::read_to_string(format!("/proc/{}/maps", child.id())).unwrap();
        child.kill().unwrap();
        child.wait().unwrap();
        maps.contains(big.to_str().unwrap())
    };
    assert!(mapped(&[]), "a file of 8 MiB is mapped by default");
    assert!(!mapped(&["--no-mmap"]));
    assert!(!mapped(&["--mmap", "--no-mmap"]));
}

/// Review r2 of the plan, Codex #4: `-o` in the Trail layouts prints the rows rg prints, an
/// empty Match or an empty replacement as an empty row, and a `\r` Match without `--crlf`.
/// A Plain file's row is `N: text`, rg's `N:text`, and ends with `\n`, with `--crlf` and
/// `--null-data` too.
#[test]
fn m5_20_only_matching_rows_as_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::write(fx.file("n.txt"), b"needle\nnext\n").unwrap();
    std::fs::write(fx.file("cr.txt"), b"needle\r\n").unwrap();
    // Not `nul.txt`: on Windows that name, whatever its extension, is the null device, which
    // takes every write and reads empty.
    std::fs::write(fx.file("records.txt"), b"a needle\0b\0c needle\0").unwrap();
    let cases: [&[&str]; 17] = [
        &["-o", "-r", "", "needle", "n.txt"],
        &["-o", "-r", "", "e", "n.txt"],
        &["-o", "x*", "n.txt"],
        &["-o", "\\r", "cr.txt"],
        // With -U, a pattern that cannot match a line break prints as without it; one that
        // can, as rg's printer of several lines, which skips what is left empty.
        &["-U", "-o", "x*", "n.txt"],
        &["-U", "-o", "-r", "", "needle", "n.txt"],
        &["-U", "-o", "-r", "Z", "e\\nn", "n.txt"],
        // Review r3 of the plan, Codex #4: `--vimgrep` in a search of several lines prints no
        // row for an empty Match at a line's start, as rg's printer of several lines.
        &["-U", "--vimgrep", "^", "n.txt"],
        &["-U", "--vimgrep", "^|e", "n.txt"],
        &["-U", "--vimgrep", "-P", "e?", "n.txt"],
        // Review r4 of the plan, Fable #1: with `--crlf` rg ends each line it prints with
        // `\r\n`; the Trail layouts end every row with `\n` (rg-compat's deviations).
        &["--crlf", "needle", "cr.txt"],
        &["--crlf", "-o", "needle", "cr.txt"],
        &["-U", "--vimgrep", "--crlf", "$", "cr.txt"],
        // With `--null-data` rg ends each record with its NUL; a row leaves it out (D6), a
        // context row too (review r5 of the plan, Codex #3).
        &["--null-data", "needle", "records.txt"],
        &["--null-data", "-C1", "needle", "records.txt"],
        &["--null-data", "--passthru", "needle", "records.txt"],
        &["--null-data", "-v", "-C1", "needle", "records.txt"],
    ];
    for case in cases {
        let mut rg = vec!["-n"];
        rg.extend_from_slice(case);
        let mut want = stdout(&run(&mut fx.rg(&rg)));
        if case.contains(&"--crlf") {
            assert!(want.ends_with("\r\n"), "{case:?}: {want:?}");
            want = want.replace("\r\n", "\n");
        }
        if case.contains(&"--null-data") {
            assert!(want.ends_with('\0'), "{case:?}: {want:?}");
            want = want.replace('\0', "\n");
        }
        let mut s4 = vec!["--flat"];
        s4.extend_from_slice(case);
        let got: String = ok(&fx, &s4)
            .split_inclusive('\n')
            .map(|l| l.replacen(": ", ":", 1))
            .collect();
        assert_eq!(got, want, "{case:?}");
        assert!(!want.is_empty() || case[2] == "^", "{case:?}");
    }
}

/// Review r3 of the plan, Codex #1: the MCP server reads the Trails of a line from its message,
/// so a line under 240 Trails asked for one Match takes little memory (over 200 MiB when
/// each Trail copied the message). Seen in the server's `VmHWM` before it exits.
#[cfg(target_os = "linux")]
#[test]
fn m5_21_mcp_memory_of_a_line_with_many_trails() {
    let fx = Fixture::new();
    let elements: String = (0..240).map(|i| format!("<k{i}>needle</k{i}>")).collect();
    let xml = format!("<?xml version=\"1.0\"?><root>{elements}</root>\n");
    std::fs::write(fx.file("many.xml"), xml).unwrap();
    let mut server = fx
        .s4(["--mcp"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = server.stdin.take().unwrap();
    let call = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search",
        "arguments":{"pattern":"needle","paths":["many.xml"],"max_results":1}}});
    writeln!(input, "{call}").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    std::io::BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let status = std::fs::read_to_string(format!("/proc/{}/status", server.id())).unwrap();
    drop(input);
    assert!(server.wait().unwrap().success());
    let r: Value = serde_json::from_str(&line).unwrap();
    let r = &r["result"]["structuredContent"];
    assert_eq!(r["matches"][0]["trail"], json!(["root", "k0"]), "{r}");
    assert_eq!(r["truncated_by"], "max_results");
    let peak: u64 = status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().strip_suffix(" kB"))
        .unwrap()
        .parse()
        .unwrap();
    assert!(peak < 64 * 1024, "{peak} kB");
}

/// s4 with `input` on stdin.
fn piped(fx: &Fixture, args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = fx
        .s4(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

/// Code review r1, Codex #2: `--passthru` prints an input without a Match, its lines as
/// context, as rg does, whether it is read with a reader (stdin, a walked file) or in memory
/// (an explicit file); and it is no Match: exit 1, and `--stats` counts no file with Matches.
#[test]
fn m5_23_passthru_prints_an_input_without_a_match() {
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("pt")).unwrap();
    std::fs::write(fx.file("pt/c.txt"), "nothing\n").unwrap();
    // The tree heads stdin and indents its lines, with a Match or without.
    for (layout, want) in [
        ("--flat", "1-nothing\n"),
        ("--tree", "<stdin>\n  1-nothing\n"),
    ] {
        let o = piped(&fx, &[layout, "--passthru", "needle"], b"nothing\n");
        assert_eq!((code(&o), stdout(&o).as_str()), (1, want), "{layout}");
    }
    let o = run(&mut fx.s4(["--flat", "--passthru", "needle", "pt"]));
    assert_eq!((code(&o), stdout(&o)), (1, native("pt/c.txt-1-nothing\n")));
    let o = run(&mut fx.s4(["--tree", "--passthru", "needle", "pt"]));
    assert_eq!(
        (code(&o), stdout(&o)),
        (1, native("pt/c.txt\n  1-nothing\n"))
    );
    for path in ["pt", "pt/c.txt"] {
        let o = run(&mut fx.s4(["--flat", "--stats", "--passthru", "needle", path]));
        assert_eq!(code(&o), 1, "{path}");
        assert!(
            stdout(&o).contains("\n0 files contained matches\n"),
            "{path}: {}",
            stdout(&o)
        );
    }
}

/// Code review r2, Fable #1: in s4's `--json`, a file `--passthru` prints without a Match has
/// `searches_with_match` 0 in its `end` and adds 0 to the `summary`; with `-v`, a file whose
/// inverted lines matched counts 1 although its `matches` is 0. Against the reference rg.
#[test]
fn m5_23b_json_counts_a_search_with_a_match_as_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    std::fs::create_dir(fx.file("pt")).unwrap();
    std::fs::write(fx.file("pt/c.txt"), "nothing\n").unwrap();
    std::fs::write(fx.file("pt/all.txt"), "needle\n").unwrap();
    // Per `end`, its path and its matched_lines, matches and searches_with_match; then the
    // summary's searches and searches_with_match.
    let counts = |o: &std::process::Output| -> Vec<Value> {
        stdout(o)
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter_map(|m| {
                let st = &m["data"]["stats"];
                match m["type"].as_str()? {
                    "end" => Some(json!([
                        m["data"]["path"],
                        st["matched_lines"],
                        st["matches"],
                        st["searches_with_match"],
                    ])),
                    "summary" => Some(json!([st["searches"], st["searches_with_match"]])),
                    _ => None,
                }
            })
            .collect()
    };
    for flags in [&["--passthru"][..], &["--passthru", "-v"], &["-v"], &[]] {
        let mut args = vec!["--json", "--sort", "path"];
        args.extend(flags);
        args.push("needle");
        let s = piped(&fx, &args, b"nothing\n");
        let r = run(fx
            .rg(&args)
            .stdin(std::fs::File::open(fx.file("pt/c.txt")).unwrap()));
        assert_eq!(counts(&s), counts(&r), "{args:?} on stdin: {}", stderr(&s));
        args.extend(["pt/c.txt", "pt/all.txt", "md/guide.md"]);
        let (s, r) = (run(&mut fx.s4(&args)), run(&mut fx.rg(&args)));
        assert_eq!(counts(&s), counts(&r), "{args:?}: {}", stderr(&s));
    }
}

/// Code review r1, Codex #4: the last mode flag wins, as rg 14.1.1's `Mode::update`, where a
/// search mode (-c, --count-matches, -l, --files-without-match, --json, and --no-json after
/// --json) given after `--files`, `--type-list` or `--generate` searches again; on the command
/// line and from the configuration file, against the reference rg.
#[test]
fn m5_24_the_last_mode_flag_wins_as_ripgrep() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let both = |args: &[&str], config: Option<&str>| {
        let (mut s, mut r) = (fx.s4(["--sort", "path"]), fx.rg(["--sort", "path"]));
        s.args(args);
        r.args(args);
        if let Some(c) = config {
            let path = fx.file("modes-rc");
            std::fs::write(&path, c).unwrap();
            s.env("SEARCHFOR_CONFIG_PATH", &path);
            r.env("RIPGREP_CONFIG_PATH", &path);
        }
        let (s, r) = (run(&mut s), run(&mut r));
        assert_eq!(
            (code(&s), stdout(&s)),
            (code(&r), stdout(&r)),
            "{args:?} with {config:?}: {}",
            stderr(&s)
        );
        assert_eq!(code(&s), 0, "{args:?} with {config:?}");
    };
    for args in [
        &["--files", "-l", "^#", "md"][..],
        &["-l", "--files", "md"],
        &["--files", "-c", "Plan", "md"],
        &["--files", "--count-matches", "Plan", "md"],
        &["--files", "--files-without-match", "needle-pkg", "md"],
        &["--type-list", "-l", "Plan", "md"],
        &["--generate", "man", "-l", "Plan", "md"],
        &["--generate", "man", "--files", "md"],
        &["--type-list", "--files", "md"],
        &["--files", "--type-list"],
        &["-c", "--type-list"],
        &["--json", "--files", "--no-json", "md"],
        &["--files", "--no-json", "md"],
        &["--json", "-l", "--no-json", "Plan", "md"],
    ] {
        both(args, None);
    }
    both(&["-l", "Plan", "md"], Some("--files\n"));
    both(&["--files", "md"], Some("-l\n"));
    both(&["--type-list"], Some("--files\n"));
    both(&["-c", "Plan", "md"], Some("--generate=man\n"));
    // `--no-json` after `--json` searches again, in s4's own layout.
    // In path order: two parallel walks print their files in any order (code review r3,
    // Fable #2).
    let plain = ok(&fx, &["--sort", "path", "Plan", "md"]);
    let again = [
        "--sort",
        "path",
        "--files",
        "--json",
        "--no-json",
        "Plan",
        "md",
    ];
    assert_eq!(ok(&fx, &again), plain);
    assert!(ok(&fx, &["-l", "--generate", "man"]).contains(".TH s4"));
}

/// Code review r1, Fable #1: a reader that closes stdout before the end ends `--type-list`,
/// `--generate` and the other actions with exit 0 and no message, as rg and s4's searches; and
/// an error is still exit 2 with stderr closed, without a panic. Code review r2, Codex #4:
/// `--pcre2-version`, `--list-formats` and `--export-formats` too.
#[test]
fn m5_25_a_closed_stdout_ends_the_actions_quietly() {
    let fx = Fixture::new();
    let closed = || {
        let (r, w) = std::io::pipe().unwrap();
        drop(r);
        w
    };
    let export = tempfile::tempdir().unwrap();
    let (out1, out2) = (export.path().join("1"), export.path().join("2"));
    for (args, again) in [
        (&["--type-list"][..], None),
        (&["--generate", "man"], None),
        (&["--generate", "complete-bash"], None),
        (&["--pcre2-version"], None),
        (&["--list-formats"], None),
        (&["--help"], None),
        (&["--version"], None),
        (
            &["--export-formats", out1.to_str().unwrap()],
            Some(&["--export-formats", out2.to_str().unwrap()][..]),
        ),
    ] {
        let o = fx
            .s4(args)
            .stdout(closed())
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        assert_eq!((code(&o), stderr(&o).as_str()), (0, ""), "{args:?}");
        let o = fx
            .s4(again.unwrap_or(args))
            .stdout(closed())
            .stderr(closed())
            .output()
            .unwrap();
        assert_eq!(code(&o), 0, "{args:?} with stderr closed");
    }
    // `--list-grammars` warns that the index is not available, and prints the list all the same.
    let o = fx
        .s4(["--list-grammars"])
        .stdout(closed())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(!stderr(&o).contains("panicked"), "{}", stderr(&o));
    // `--install-grammar` installs, and its closing line meets a closed stdout. The payload is
    // no Grammar Helper: the install does not run it.
    let payload = export.path().join("payload");
    std::fs::write(&payload, b"not a helper").unwrap();
    let sum: String = {
        use sha2::Digest;
        sha2::Sha256::digest(b"not a helper")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    let definition = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/helpers/grammar/definitions/lua.toml"
    ))
    .unwrap();
    let index = export.path().join("index.json");
    std::fs::write(
        &index,
        json!({"version": 1, "grammars": [{
            "name": "lua", "version": "0.2.0", "source": "test", "license": "MIT",
            "definition": definition,
            "binaries": {(searchfor::grammars::index::system()): {
                "url": format!("file://{}", payload.display()), "sha256": sum,
            }},
        }]})
        .to_string(),
    )
    .unwrap();
    let o = fx
        .s4(["--install-grammar", "lua"])
        .env(
            "SEARCHFOR_GRAMMAR_INDEX",
            format!("file://{}", index.display()),
        )
        .stdout(closed())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert_eq!((code(&o), stderr(&o).as_str()), (0, ""));
    let file = fx.file("md/guide.md");
    for args in [
        &["--type-list", "-t", "zzz"][..],
        &["--install-grammar", "zzz"],
        &["--export-formats", file.to_str().unwrap()],
    ] {
        let o = fx.s4(args).stderr(closed()).output().unwrap();
        assert_eq!(code(&o), 2, "{args:?}");
    }
}

/// Code review r1, Codex #6 and Fable #2: the version is the milestone's. M5b is v1.0 (M5b
/// spec §11), the version every release package prints.
#[test]
fn m5_26_the_version_is_1_0() {
    let fx = Fixture::new();
    let v = ok(&fx, &["--version"]);
    assert_eq!(v.trim_end(), "s4 1.0.0", "{v}");
}

/// Code review r3, Codex #4: every diagnostic of a search or an action is written without a
/// panic when stderr is a pipe without a reader, or closed: the exit code is the one s4 gives
/// with stderr open, and the run ends (a panic in a walker thread used to hang it). The pipe
/// without a reader exists on every system; a closed descriptor 2 in
/// `m5_27_unix_a_closed_descriptor_2_keeps_the_exit_code_of_every_diagnostic`.
#[test]
fn m5_27_a_closed_stderr_keeps_the_exit_code_of_every_diagnostic() {
    each_diagnostic_with_stderr("a pipe without a reader", |mut c| {
        let (r, w) = std::io::pipe().unwrap();
        drop(r);
        c.stderr(w).spawn().unwrap()
    });
}

/// `m5_27`'s cases with descriptor 2 closed in the child, which only Unix can start: Windows
/// has no `pre_exec`, and its child gets a null handle instead (M5b plan, Task 1.2).
#[cfg(unix)]
#[test]
fn m5_27_unix_a_closed_descriptor_2_keeps_the_exit_code_of_every_diagnostic() {
    use std::os::unix::process::CommandExt;
    each_diagnostic_with_stderr("closed", |mut c| {
        c.stderr(std::process::Stdio::inherit());
        // SAFETY: close(2) is async-signal-safe.
        unsafe {
            c.pre_exec(|| {
                libc::close(2);
                Ok(())
            });
        }
        c.spawn().unwrap()
    });
}

/// Each diagnostic of `m5_27`: with stderr open it prints its message and gives its exit code;
/// started by `spawn` with stderr `closed`, it gives the same exit code within 30 s.
fn each_diagnostic_with_stderr(
    closed: &str,
    spawn: impl Fn(std::process::Command) -> std::process::Child,
) {
    use std::time::{Duration, Instant};
    let fx = Fixture::new();
    // A Format whose Grammar Helper cannot start, in a fixture of its own.
    let helperless = Fixture::new();
    std::fs::write(
        helperless.formats.path().join("ini.toml"),
        "name = 'ini'\nextensions = ['ini']\nprocessor = 'external'\n[params]\n\
         command = ['/nonexistent-helper']\nmode = 'pool'\ntimeout_ms = 2000\n",
    )
    .unwrap();
    // A user Definition that does not parse, in another.
    let broken = Fixture::new();
    std::fs::write(broken.formats.path().join("bad.toml"), "garbage = [\n").unwrap();
    let missing = "/nonexistent-s4-test";
    let cases = vec![
        (
            &fx,
            vec!["--list-grammars"],
            Some(("SEARCHFOR_GRAMMAR_INDEX", "file:///nonexistent")),
            0,
        ),
        (
            &fx,
            vec!["--ignore-file", missing, "needle-pkg", "md"],
            None,
            0,
        ),
        (&fx, vec![], None, 2),
        (&fx, vec!["needle-pkg", "md", missing], None, 2),
        (
            &fx,
            vec!["needle-pkg", "md"],
            Some(("SEARCHFOR_CONFIG_PATH", missing)),
            0,
        ),
        (&broken, vec!["needle-pkg", "md"], None, 0),
        (&helperless, vec!["needle", "ini"], None, 0),
    ];
    let limit = Duration::from_secs(30);
    for (fx, args, env, want) in cases {
        let command = || {
            let mut c = fx.s4(&args);
            if let Some((k, v)) = env {
                c.env(k, v);
            }
            c.stdout(std::process::Stdio::null());
            c
        };
        // With stderr open the case prints its diagnostic, and that is the exit code to keep.
        let o = command()
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(code(&o), want, "{args:?}: {}", stderr(&o));
        assert!(!stderr(&o).is_empty(), "{args:?} prints no diagnostic");
        let start = Instant::now();
        let mut child = spawn(command());
        let status = loop {
            if let Some(s) = child.try_wait().unwrap() {
                break s;
            }
            if start.elapsed() > limit {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{args:?} with stderr {closed}: still running after {limit:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!(
            status.code(),
            Some(want),
            "{args:?} with stderr {closed}, after {:?}",
            start.elapsed()
        );
    }
}

/// M5b plan, Task 1.2: the MCP server's trap needs no executable script, so it runs on every
/// system.
#[test]
fn m5b_mcp_config_trap_needs_no_executable_script() {
    let fx = Fixture::new();
    let config = fx.file("config");
    std::fs::write(&config, "--generate=man\n--regexp=ABSENT_CONFIG_SENTINEL\n").unwrap();
    let mut cmd = fx.s4(["--mcp"]);
    cmd.env("SEARCHFOR_CONFIG_PATH", config)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let call = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"search","arguments":{
            "pattern":"needle-pkg","paths":["md/guide.md"]}}});
    writeln!(child.stdin.take().unwrap(), "{call}").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let reply: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(reply["id"], 1);
    assert_eq!(
        reply["result"]["structuredContent"]["matches"][0]["line"],
        12
    );
}

/// M5b plan, Task 1.2: `--no-mmap` prints what a mapped search prints, on every system; that
/// it maps no file is seen in Linux's `/proc` alone (`m5_19`).
#[test]
fn m5b_no_mmap_keeps_output_portable() {
    let fx = Fixture::new();
    let bytes = format!("# Head\n{}needle\n", "x\n".repeat(4_200_000));
    std::fs::write(fx.file("large.md"), bytes).unwrap();
    for mode in ["--flat", "--json"] {
        let a = run(&mut fx.s4([mode, "needle", "large.md"]));
        let b = run(&mut fx.s4([mode, "--no-mmap", "needle", "large.md"]));
        assert_eq!(code(&a), code(&b));
        if mode == "--json" {
            assert_eq!(
                common::mask_json_times(&stdout(&a)),
                common::mask_json_times(&stdout(&b))
            );
        } else {
            assert_eq!(a.stdout, b.stdout);
        }
    }
}

/// M5b spec §3, item 1: a row belongs to the Section where its Match starts, so a replaced
/// Match's continuation is printed once, under that Section.
#[test]
fn m5b_expand_replacement_continuations_belong_to_start_section() {
    let fx = Fixture::new();
    std::fs::write(fx.file("a.md"), "# A\nneedle\n# B\nlast\nneedle\n").unwrap();
    let out = ok(
        &fx,
        &[
            "--tree",
            "--expand",
            "-U",
            "-r",
            "X",
            "needle\\n# B\\nlast|needle",
            "a.md",
        ],
    );
    assert_eq!(out.matches("X").count(), 2, "{out}");
    assert_eq!(out.matches("last").count(), 0, "{out}");
    // The replacement consumes the continuation; Section B must not print it again.
    assert_eq!(
        out.lines().filter(|l| l.ends_with(":X")).count(),
        2,
        "{out}"
    );
}

/// M5b spec §3, item 2: the row of a replaced Match covers the whole item, so no piece of the
/// Source line comes back as a context row.
#[test]
fn m5b_expand_vimgrep_newline_replacement_covers_the_source_item() {
    let fx = Fixture::new();
    std::fs::write(fx.file("a.md"), "# A\nprefix needle suffix\n").unwrap();
    let out = ok(
        &fx,
        &[
            "--flat",
            "--expand",
            "--vimgrep",
            "-r",
            "X\nY",
            "needle",
            "a.md",
        ],
    );
    assert_eq!(out.matches("prefix").count(), 1, "{out}");
    // --vimgrep keeps the first replacement line; the rest of the original
    // item is covered, so suffix must not reappear as source context.
    assert_eq!(out.matches("suffix").count(), 0, "{out}");
    assert!(out.contains("prefix X"), "{out}");
    assert!(!out.contains("Y"), "{out}");
    assert!(!out.contains("needle"), "{out}");
}

/// M5b spec §3, item 3: a row of the same Match after an empty line past the Section's end
/// prints, with the gap as context, as it does inside the Section.
#[test]
fn m5b_expand_only_matching_keeps_a_row_after_a_gap_outside_section() {
    let fx = Fixture::new();
    std::fs::write(fx.file("a.md"), "# A\nneedle\n# B\n\nlast\n").unwrap();
    let out = ok(
        &fx,
        &[
            "--tree",
            "--expand",
            "-U",
            "-o",
            "needle\\n# B\\n\\nlast",
            "a.md",
        ],
    );
    assert_eq!(out.matches("needle").count(), 1, "{out}");
    assert_eq!(out.matches("last").count(), 1, "{out}");
    assert!(out.lines().any(|l| l.ends_with("5:last")), "{out}");
    assert!(out.lines().any(|l| l.ends_with("4-")), "{out}");
}

/// Code review r1 (Codex #3, Fable #1): files over a compiled-in Grammar's `max_bytes` give one
/// warning for the run, also when they take the streaming path; the files after the first are
/// `helper::Unavailable`, which has no warning of its own (D33).
#[test]
fn m5b_max_bytes_warns_once_on_the_streaming_path() {
    let fx = Fixture::new();
    let mut rust: toml::Table = toml::from_str(include_str!("../formats/rust.toml")).unwrap();
    rust["params"]
        .as_table_mut()
        .unwrap()
        .insert("max_bytes".into(), toml::Value::Integer(64));
    std::fs::write(
        fx.formats.path().join("rust.toml"),
        toml::to_string(&rust).unwrap(),
    )
    .unwrap();
    let body = format!(
        "fn a() {{\n    let needle = 1;\n}}\n// {}\n",
        "x".repeat(80)
    );
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(fx.file(name), &body).unwrap();
    }
    for mode in [
        &["--flat", "-j1"][..],
        &["--json", "-j1"],
        &["--flat", "-j2"],
        &["--json", "-j2"],
    ] {
        let mut args = mode.to_vec();
        args.extend(["--no-docs", "needle", "a.rs", "b.rs", "c.rs"]);
        let o = run(&mut fx.s4(&args));
        assert_eq!(code(&o), 0, "{args:?}: {}", stderr(&o));
        let err = stderr(&o);
        assert_eq!(err.lines().count(), 1, "{args:?}: {err}");
        assert!(err.contains("max_bytes"), "{args:?}: {err}");
        let rows = if args.contains(&"--json") {
            stdout(&o).matches(r#""type":"match""#).count()
        } else {
            stdout(&o).matches("needle").count()
        };
        assert_eq!(rows, 3, "{args:?}");
    }
}

/// Code review r3, Codex #1: -q decides over a summary on the streaming path, as in rg: nothing
/// is printed, and the exit code says whether a Match the filters kept was found, also when
/// the filters drop every Match.
#[test]
fn m5b_quiet_prints_no_summary_under_the_filters() {
    let fx = Fixture::new();
    let g = "md/guide.md";
    let cases: [(&[&str], i32); 8] = [
        (&["-q", "-c", "--within", ""], 0),
        (&["-q", "-c", "--within", "MISSING"], 1),
        (&["-q", "-c", "--include-zero", "--within", "MISSING"], 1),
        (&["-q", "--count-matches", "--within", "Install"], 0),
        (&["-q", "-l", "--within", "MISSING"], 1),
        (&["-q", "--files-without-match", "--within", "MISSING"], 1),
        (&["-q", "--files-without-match", "--within", ""], 0),
        (
            &[
                "-q",
                "-c",
                "--include-zero",
                "--in-headings",
                "--within",
                "MISSING",
            ],
            1,
        ),
    ];
    for (mode, want) in cases {
        let mut args = vec!["--no-docs"];
        args.extend_from_slice(mode);
        args.extend(["needle", g]);
        let o = run(&mut fx.s4(&args));
        assert_eq!((code(&o), stdout(&o)), (want, String::new()), "{args:?}");
        assert_eq!(stderr(&o), "", "{args:?}");
    }
}
