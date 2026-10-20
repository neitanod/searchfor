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
    std::fs::copy(examples.join("sample.ini"), fx.file("sample.ini")).unwrap();
    let script = examples.join("ini-helper.py");
    let command = vec![py.clone(), script.to_string_lossy().into_owned()];
    let mut def = toml::Table::new();
    def.insert("name".into(), "ini-guide".into());
    def.insert("extensions".into(), toml::Value::Array(vec!["ini".into()]));
    def.insert("processor".into(), "external".into());
    let mut params = toml::Table::new();
    params.insert(
        "command".into(),
        toml::Value::Array(command.into_iter().map(Into::into).collect()),
    );
    params.insert("mode".into(), "oneshot".into());
    def.insert("params".into(), toml::Value::Table(params));
    std::fs::write(
        fx.formats.path().join("ini-guide.toml"),
        toml::to_string(&def).unwrap(),
    )
    .unwrap();
    let out = run(&mut fx.s4(["--flat", "--format", "ini-guide", "needle", "sample.ini"]));
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(stderr(&out), "");
    assert_eq!(stdout(&out), "2: server › host │ host = needle.example\n");
    let pre = Command::new(&py)
        .arg(examples.join("ini-pre.py"))
        .arg(fx.file("sample.ini"))
        .output()
        .unwrap();
    assert!(pre.status.success());
    assert_eq!(pre.stdout, b"needle.example\n");
    let launcher = fx.file(&format!("ini-pre-launcher{}", std::env::consts::EXE_SUFFIX));
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
        .args(["needle", "sample.ini"])
        .env("SEARCHFOR_EXAMPLE_PYTHON", &py)
        .env("SEARCHFOR_EXAMPLE_CONVERTER", examples.join("ini-pre.py")));
    assert_eq!(code(&converted), 0, "{}", stderr(&converted));
    assert_eq!(stdout(&converted), "needle.example\n");
    assert_eq!(stderr(&converted), "");
    let doc =
        std::fs::read_to_string(examples.parent().unwrap().join("external-processors.md")).unwrap();
    assert!(doc.contains("2: server › host │ host = needle.example"));
    assert!(doc.contains("needle.example"));
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
        .arg(root.join("ini-helper.py"))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = c.stdin.take().unwrap();
    for v in [
        serde_json::json!({"id":0,"op":"hello","protocol":1,"format":"ini-guide","params":{},"threads":1}),
        serde_json::json!({"id":8,"op":"names","path":root.join("sample.ini")}),
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
            .any(|n| n["text"] == "host")
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
