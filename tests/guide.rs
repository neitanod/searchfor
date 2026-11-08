//! The External Processor guide runs as documented (M5b spec §10, acceptance criterion 41): the
//! examples of `docs/examples/` behind a real s4, first through `--pre` and then as a Helper
//! Processor, and the protocol transcript the guide shows. Skipped, with the reason, where
//! Python 3 is not installed (`docs/platforms.md`).
mod common;
use common::{Fixture, code, run, stderr, stdout};
use std::path::Path;
use std::process::Command;

fn python() -> Option<String> {
    for name in ["python3", "python"] {
        if Command::new(name)
            .args(["-c", "import sys; assert sys.version_info.major == 3"])
            .status()
            .is_ok_and(|s| s.success())
        {
            return Some(name.into());
        }
    }
    None
}

#[test]
fn m5b_external_processor_guide_examples_run_as_documented() {
    let Some(py) = python() else {
        eprintln!("skipping guide examples: Python 3 is not installed");
        return;
    };
    let fx = Fixture::new();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    std::fs::copy(examples.join("sample.po"), fx.file("sample.po")).unwrap();
    let script = examples.join("po-helper.py");
    let command = vec![py.clone(), script.to_string_lossy().into_owned()];
    let mut def = toml::Table::new();
    def.insert("name".into(), "po-guide".into());
    def.insert("extensions".into(), toml::Value::Array(vec!["po".into()]));
    def.insert("processor".into(), "external".into());
    let mut params = toml::Table::new();
    params.insert(
        "command".into(),
        toml::Value::Array(command.into_iter().map(Into::into).collect()),
    );
    params.insert("mode".into(), "oneshot".into());
    def.insert("params".into(), toml::Value::Table(params));
    std::fs::write(
        fx.formats.path().join("po-guide.toml"),
        toml::to_string(&def).unwrap(),
    )
    .unwrap();
    let out = run(&mut fx.s4(["--flat", "--format", "po-guide", "needle", "sample.po"]));
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
    assert_eq!(stdout(&out), "3: menu › Open │ msgstr \"needle\"\n");
    let pre = Command::new(&py)
        .arg(examples.join("po-pre.py"))
        .arg(fx.file("sample.po"))
        .output()
        .unwrap();
    assert!(pre.status.success());
    assert_eq!(pre.stdout, b"needle\n");
    let launcher = fx.file(&format!("po-pre-launcher{}", std::env::consts::EXE_SUFFIX));
    let compiled = Command::new("rustc")
        .arg("--edition=2024")
        .arg(examples.join("pre-launcher.rs"))
        .arg("-o")
        .arg(&launcher)
        .status()
        .unwrap();
    assert!(compiled.success());
    let converted = run(fx
        .s4(["--no-docs", "--no-trail", "--pre"])
        .arg(&launcher)
        .args(["needle", "sample.po"])
        .env("SEARCHFOR_EXAMPLE_PYTHON", &py)
        .env("SEARCHFOR_EXAMPLE_CONVERTER", examples.join("po-pre.py")));
    assert_eq!(code(&converted), 0, "{}", stderr(&converted));
    assert_eq!(stdout(&converted), "needle\n");
    assert_eq!(stderr(&converted), "");
    for guide in ["external-processors.md", "external-processors.es.md"] {
        let doc = std::fs::read_to_string(examples.parent().unwrap().join(guide)).unwrap();
        assert!(
            doc.contains("3: menu › Open │ msgstr \"needle\""),
            "{guide}"
        );
        assert!(doc.contains("2: menu │ msgid \"Open\""), "{guide}");
        assert!(doc.contains("sample.po"), "{guide}");
        assert!(!doc.contains("sample.ini"), "{guide}");
    }
}

#[test]
fn m5b_guide_names_and_errors_keep_request_ids() {
    use std::io::Write;
    let Some(py) = python() else {
        eprintln!("skipping guide protocol: Python 3 is not installed");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    let mut c = Command::new(py)
        .arg(root.join("po-helper.py"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = c.stdin.take().unwrap();
    for v in [
        serde_json::json!({"id":0,"op":"hello","protocol":1,"format":"po-guide","params":{},"threads":1}),
        serde_json::json!({"id":8,"op":"names","path":root.join("sample.po")}),
        serde_json::json!({"id":9,"op":"unknown"}),
    ] {
        writeln!(input, "{v}").unwrap();
    }
    drop(input);
    let out = c.wait_with_output().unwrap();
    assert!(out.status.success());
    let replies: Vec<serde_json::Value> = out
        .stdout
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["protocol"], 1);
    assert_eq!(replies[1]["id"], 8);
    assert!(
        replies[1]["names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["text"] == "Open")
    );
    assert_eq!(replies[2]["id"], 9);
    assert!(replies[2]["error"].is_string());
}

#[test]
fn m5b_distribution_docs_and_spec_cover_the_v1_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in ["README.md", "README.es.md"] {
        let doc = std::fs::read_to_string(root.join(file)).unwrap();
        for command in [
            "cargo install searchfor",
            "npm install -g searchfor",
            "brew install neitanod/tap/searchfor",
            "gh attestation verify",
            "--install-grammar",
            "--no-docs",
        ] {
            assert!(doc.contains(command), "{file}: {command}");
        }
        for system in ["Windows", "macOS", "Linux"] {
            assert!(doc.contains(system));
        }
    }
    let skill = std::fs::read_to_string(root.join("skills/searchfor/SKILL.md")).unwrap();
    assert!(skill.contains("Windows"));
    assert!(skill.contains("--within"));
    let spec =
        std::fs::read_to_string(root.join("docs/specs/2026-10-16-searchfor-design.md")).unwrap();
    assert!(spec.contains("32 MiB"));
    assert!(spec.contains("grammars-v1"));
    for number in 34..=42 {
        assert!(
            spec.lines().any(|l| l.starts_with(&format!("{number}."))),
            "{number}"
        );
    }
}

#[test]
fn m6_po_guide_runs() {
    let Some(py) = python() else {
        panic!("Python 3 required for M6 guide acceptance")
    };
    let fx = Fixture::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    std::fs::copy(root.join("sample.po"), fx.file("sample.po")).unwrap();
    let plain = run(&mut fx.s4(["--no-docs", "--json", "needle", "sample.po"]));
    assert_eq!(code(&plain), 0);
    for line in stdout(&plain).lines() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        if value["type"] == "match" {
            assert!(value["data"]["format"].is_null());
        }
    }
    let def = toml::toml! {
        name="po-guide"
        extensions=["po"]
        processor="external"
        [params]
        mode="oneshot"
    };
    let mut def = def;
    def.get_mut("params")
        .unwrap()
        .as_table_mut()
        .unwrap()
        .insert(
            "command".into(),
            toml::Value::Array(vec![
                py.clone().into(),
                root.join("po-helper.py")
                    .to_string_lossy()
                    .into_owned()
                    .into(),
            ]),
        );
    std::fs::write(
        fx.formats.path().join("po.toml"),
        toml::to_string(&def).unwrap(),
    )
    .unwrap();
    let described = run(&mut fx.s4(["--flat", "needle", "sample.po"]));
    assert_eq!(code(&described), 0, "{}", stderr(&described));
    assert_eq!(stdout(&described), "3: menu › Open │ msgstr \"needle\"\n");
    // `names` maps each name to its value between the quotes: --in-headings finds
    // the entry's name, with its parent's Trail, and never the keyword.
    let named = run(&mut fx.s4(["--flat", "--in-headings", "Open", "sample.po"]));
    assert_eq!(code(&named), 0, "{}", stderr(&named));
    assert_eq!(stdout(&named), "2: menu │ msgid \"Open\"\n");
    let keyword = run(&mut fx.s4(["--flat", "--in-headings", "msgstr", "sample.po"]));
    assert_eq!(code(&keyword), 1, "{}", stdout(&keyword));
    let pre = Command::new(&py)
        .arg(root.join("po-pre.py"))
        .arg(fx.file("sample.po"))
        .output()
        .unwrap();
    assert!(pre.status.success());
    assert_eq!(pre.stdout, b"needle\n");
    let launcher = fx.file(&format!("po-pre-launcher{}", std::env::consts::EXE_SUFFIX));
    assert!(
        Command::new("rustc")
            .arg("--edition=2024")
            .arg(root.join("pre-launcher.rs"))
            .arg("-o")
            .arg(&launcher)
            .status()
            .unwrap()
            .success()
    );
    let converted = run(fx
        .s4(["--no-docs", "--no-trail", "--pre"])
        .arg(launcher)
        .args(["needle", "sample.po"])
        .env("SEARCHFOR_EXAMPLE_PYTHON", &py)
        .env("SEARCHFOR_EXAMPLE_CONVERTER", root.join("po-pre.py")));
    assert_eq!(code(&converted), 0);
    assert_eq!(stdout(&converted), "needle\n");
}

/// Criterion 55 (M6 spec §14): every built-in Format, by the name `--format` takes, in both
/// READMEs' table and both Format references; the skill names the M6 Formats; HTML's script
/// Trails, the one change since 1.0, are stated wherever HTML's Trails are.
#[test]
fn m6_docs_name_every_format() {
    use searchfor::{formats::Registry, processors::Processors};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (registry, warnings) = Registry::load(None, &Processors::known);
    assert!(warnings.is_empty(), "{warnings:?}");
    let read = |file: &str| std::fs::read_to_string(root.join(file)).unwrap();
    for file in [
        "README.md",
        "README.es.md",
        "docs/formats.md",
        "docs/formats.es.md",
    ] {
        let doc = read(file);
        for def in registry.all() {
            assert!(
                doc.contains(&format!("`{}`", def.name)),
                "{file}: {}",
                def.name
            );
        }
        assert!(doc.contains("head › script[1] › onClick"), "{file}");
        assert!(
            doc.contains("docs/rg-compat.md") || doc.contains("rg-compat.md"),
            "{file}"
        );
    }
    for file in ["README.md", "README.es.md"] {
        let doc = read(file);
        assert!(doc.contains("docs/formats.md"), "{file}");
        assert!(doc.contains("https://searchfor.ip1.cc/"), "{file}");
        assert!(doc.lines().count() < 1200, "{file}");
    }
    let skill = read("skills/searchfor/SKILL.md");
    let description = skill
        .lines()
        .find(|l| l.starts_with("description:"))
        .unwrap();
    for word in [
        "AsciiDoc",
        "INI",
        "nginx",
        "Dockerfile",
        "SQL",
        "diff",
        "Vue",
        "EXIF",
        "mbox",
        "WhatsApp",
    ] {
        assert!(description.contains(word), "skill description: {word}");
    }
    for example in [
        "http › server example.com › location /api › proxy_pass",
        "INSERT INTO users › row 2 › email",
        "src/app.rs › +120 fn describe(",
    ] {
        assert!(skill.contains(example), "skill: {example}");
    }
    let context = read("CONTEXT.md");
    assert!(context.contains(
        "a statement, block, directive, hunk, message part, cell or field of the M6 Formats"
    ));
}

/// Review r1 of M6, Fable #52: the install scripts both READMEs give download the latest
/// release, so a README written for one version installs the next one too.
#[test]
fn m6_readme_installers_follow_the_latest_release() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in ["README.md", "README.es.md"] {
        let doc = std::fs::read_to_string(root.join(file)).unwrap();
        for script in ["searchfor-installer.sh", "searchfor-installer.ps1"] {
            assert!(
                doc.contains(&format!(
                    "https://github.com/neitanod/searchfor/releases/latest/download/{script}"
                )),
                "{file}: {script}"
            );
        }
        assert!(!doc.contains("/releases/download/v"), "{file}");
    }
}

/// The CI's Windows run of the two guide tests above read `needle\r\n`: Python's text stdout
/// writes CRLF there. Each example runs here under the stdout Windows gives it (CRLF, the
/// ANSI code page) and must still write LF, in UTF-8.
#[test]
fn m6_examples_write_lf_under_a_windows_stdout() {
    let Some(py) = python() else {
        eprintln!("skipping guide examples: Python 3 is not installed");
        return;
    };
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/examples");
    let windows = "import io, runpy, sys\n\
        sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='cp1252', errors='replace', newline='\\r\\n')\n\
        script = sys.argv[1]\n\
        sys.argv = sys.argv[1:]\n\
        runpy.run_path(script, run_name='__main__')\n\
        sys.stdout.flush()\n";
    let pre = Command::new(&py)
        .args(["-c", windows])
        .arg(examples.join("po-pre.py"))
        .arg(examples.join("sample.po"))
        .output()
        .unwrap();
    assert!(
        pre.status.success(),
        "{}",
        String::from_utf8_lossy(&pre.stderr)
    );
    assert!(!pre.stdout.is_empty());
    assert!(
        !pre.stdout.contains(&b'\r'),
        "{:?}",
        String::from_utf8_lossy(&pre.stdout)
    );
    assert!(std::str::from_utf8(&pre.stdout).is_ok());
    let mut helper = Command::new(&py)
        .args(["-c", windows])
        .arg(examples.join("po-helper.py"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let mut stdin = helper.stdin.take().unwrap();
        stdin
            .write_all(b"{\"id\":0,\"op\":\"hello\",\"protocol\":1}\n")
            .unwrap();
    }
    let out = helper.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"{\"id\": 0, \"protocol\": 1}\n");
}
