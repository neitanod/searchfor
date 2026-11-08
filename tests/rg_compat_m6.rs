//! M6 acceptance against rg 14.1.1 and the real Definitions (M6 plan Task 12.2; spec
//! criteria 43, 44 and 46): `--no-docs --no-trail` prints rg's output over every M6 fixture and
//! binary layout, in each output mode, and the forty built-in Definitions load, export and
//! yield to a user Definition of the same name.
mod common;
use common::{Fixture, code, run, stdout};
use std::path::Path;
fn files(root: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir() {
            files(&entry.path(), out)
        } else {
            out.push(entry.path());
        }
    }
}
#[test]
fn m6_rg_over_every_fixture() {
    if !common::reference_rg() {
        return;
    }
    let fx = Fixture::new();
    let mut paths = Vec::new();
    for group in ["docs-a", "config", "code-c", "mail", "chat"] {
        files(&fx.file(group), &mut paths);
    }
    use searchfor::processors::object::fixture::m6 as b;
    for (name, bytes) in [
        ("jpeg", b::jpeg(b"needle")),
        ("png", b::png(b"needle")),
        ("webp", b::webp(b"needle")),
        ("tiff", b::tiff(b"needle")),
        ("id3", b::id3(b"needle")),
        ("mp4", b::mp4(b"needle")),
        ("wasm", b::wasm(b"needle")),
        ("class", b::class(b"needle")),
    ] {
        let path = fx.file(&format!("{name}.bin"));
        std::fs::write(&path, bytes).unwrap();
        paths.push(path);
    }
    assert!(paths.len() >= 40);
    paths.sort();
    for path in paths {
        for flags in [
            vec![],
            vec!["--json"],
            vec!["-n"],
            vec!["-o"],
            vec!["-v"],
            vec!["-c"],
            vec!["-l"],
            vec!["-q"],
            vec!["-U"],
            vec!["--crlf"],
        ] {
            let mut s = fx.s4(["--no-docs", "--no-trail"]);
            s.args(&flags).arg("needle").arg(&path);
            let mut r = fx.rg(&flags);
            r.arg("needle").arg(&path);
            let (s, r) = (run(&mut s), run(&mut r));
            assert_eq!(code(&s), code(&r), "{path:?}/{flags:?}");
            assert_eq!(s.stderr, r.stderr, "{path:?}/{flags:?}");
            if flags.contains(&"--json") {
                assert_eq!(
                    common::mask_json_times(&stdout(&s)),
                    common::mask_json_times(&stdout(&r))
                );
            } else {
                assert_eq!(s.stdout, r.stdout, "{path:?}/{flags:?}");
            }
        }
    }
}
#[test]
fn m6_real_definitions_export_and_override() {
    use searchfor::{formats::Registry, processors::Processors};
    let (r, w) = Registry::load(None, &Processors::known);
    assert!(w.is_empty());
    let names = [
        "asciidoc",
        "rst",
        "org",
        "latex",
        "jupyter",
        "ini",
        "systemd",
        "properties",
        "env",
        "ssh-config",
        "nginx",
        "apache",
        "hcl",
        "dockerfile",
        "make",
        "just",
        "sql",
        "diff",
        "vue",
        "svelte",
        "blade",
        "graphql",
        "protobuf",
        "har",
        "jpeg",
        "png",
        "webp",
        "tiff",
        "id3",
        "mp4",
        "wasm",
        "java-class",
        "eml",
        "mbox",
        "icalendar",
        "vcard",
        "bibtex",
        "whatsapp",
        "telegram",
        "telegram-html",
    ];
    for name in names {
        assert!(r.get(name).is_some(), "{name}");
    }
    let fx = Fixture::new();
    let export = fx.file("exported");
    let out = run(fx.s4(["--export-formats"]).arg(&export));
    assert_eq!(code(&out), 0);
    for name in names {
        assert!(export.join(format!("{name}.toml")).is_file());
    }
    std::fs::write(
        fx.formats.path().join("ini.toml"),
        "name='ini'\nextensions=['ini']\nprocessor='markdown'\n",
    )
    .unwrap();
    let out = run(&mut fx.s4(["--list-formats"]));
    assert_eq!(code(&out), 0);
    assert!(
        stdout(&out)
            .lines()
            .any(|l| l.starts_with("ini ") && l.contains("overridden"))
    );
}

/// Task 12.2, Step 3: the forty real Definitions carry the signals and parameters of spec
/// §4.2 and §5.1 (Task 1.3's table), and the real Detector sorts Telegram's two globs and
/// WhatsApp's `*WhatsApp*.txt` into signal 4 (a real-Definition copy of C1.3's precedence).
#[test]
fn m6_real_definitions_carry_their_signals() {
    use searchfor::formats::{Registry, detector::Detector};
    use searchfor::processors::Processors;
    let (r, w) = Registry::load(None, &Processors::known);
    assert!(w.is_empty(), "{w:?}");
    // name, names, extensions, paths, magic (offset:hex), processor and its selecting params.
    type Row<'a> = (
        &'a str,
        &'a [&'a str],
        &'a [&'a str],
        &'a [&'a str],
        &'a [&'a str],
        &'a str,
        &'a [(&'a str, &'a str)],
    );
    let table: &[Row] = &[
        (
            "asciidoc",
            &[],
            &["adoc", "asciidoc"],
            &[],
            &[],
            "headings",
            &[("syntax", "asciidoc")],
        ),
        (
            "rst",
            &[],
            &["rst", "rest"],
            &[],
            &[],
            "headings",
            &[("syntax", "rst")],
        ),
        (
            "org",
            &[],
            &["org"],
            &[],
            &[],
            "headings",
            &[("syntax", "org")],
        ),
        (
            "latex",
            &[],
            &["tex", "ltx", "latex"],
            &[],
            &[],
            "headings",
            &[("syntax", "latex")],
        ),
        (
            "jupyter",
            &[],
            &["ipynb"],
            &[],
            &[],
            "keypath",
            &[("syntax", "json"), ("view", "jupyter")],
        ),
        (
            "ini",
            &[
                ".gitconfig",
                ".gitmodules",
                ".editorconfig",
                ".npmrc",
                ".pypirc",
                ".flake8",
                ".pylintrc",
                ".coveragerc",
                ".hgrc",
            ],
            &["ini", "cfg", "cnf", "inf", "desktop"],
            &["**/.git/config", "**/git/config"],
            &[],
            "keypath",
            &[("syntax", "ini")],
        ),
        (
            "systemd",
            &[],
            &["service", "socket", "timer", "mount", "automount"],
            &["**/systemd/**/*.{path,target,slice,scope,swap,network,netdev,link,conf}"],
            &[],
            "keypath",
            &[("syntax", "ini")],
        ),
        (
            "properties",
            &[],
            &["properties"],
            &[],
            &[],
            "keypath",
            &[("syntax", "properties")],
        ),
        (
            "env",
            &[".env"],
            &["env"],
            &[".env.*"],
            &[],
            "keypath",
            &[("syntax", "env")],
        ),
        (
            "ssh-config",
            &["ssh_config", "sshd_config"],
            &[],
            &[
                "**/.ssh/config",
                "**/.ssh/config.d/*",
                "**/ssh/ssh_config.d/*.conf",
                "**/ssh/sshd_config.d/*.conf",
            ],
            &[],
            "keypath",
            &[("syntax", "ssh")],
        ),
        (
            "nginx",
            &["nginx.conf"],
            &[],
            &[
                "**/nginx/**/*.conf",
                "**/nginx/sites-available/*",
                "**/nginx/sites-enabled/*",
            ],
            &[],
            "blocks",
            &[("syntax", "nginx")],
        ),
        (
            "apache",
            &["httpd.conf", "apache2.conf", ".htaccess"],
            &[],
            &[
                "**/apache2/**/*.conf",
                "**/httpd/**/*.conf",
                "**/apache2/sites-available/*",
                "**/apache2/sites-enabled/*",
            ],
            &[],
            "blocks",
            &[("syntax", "apache")],
        ),
        (
            "hcl",
            &[],
            &["tf", "tfvars", "hcl", "nomad"],
            &[],
            &[],
            "blocks",
            &[("syntax", "hcl")],
        ),
        (
            "dockerfile",
            &["Dockerfile", "Containerfile", "dockerfile"],
            &["dockerfile", "containerfile"],
            &["Dockerfile.*", "Containerfile.*"],
            &[],
            "buildfile",
            &[("syntax", "dockerfile")],
        ),
        (
            "make",
            &["Makefile", "makefile", "GNUmakefile", "BSDmakefile"],
            &["mk", "mak", "make"],
            &["Makefile.*"],
            &[],
            "buildfile",
            &[("syntax", "make")],
        ),
        (
            "just",
            &["justfile", "Justfile", ".justfile"],
            &["just"],
            &[],
            &[],
            "buildfile",
            &[("syntax", "just")],
        ),
        ("sql", &[], &["sql", "ddl", "dml"], &[], &[], "sql", &[]),
        (
            "diff",
            &[],
            &["diff", "patch", "rej"],
            &[],
            &[],
            "diff",
            &[],
        ),
        (
            "vue",
            &[],
            &["vue"],
            &[],
            &[],
            "markup",
            &[("syntax", "html")],
        ),
        (
            "svelte",
            &[],
            &["svelte"],
            &[],
            &[],
            "markup",
            &[("syntax", "html"), ("templates", "svelte")],
        ),
        (
            "blade",
            &[],
            &["blade.php"],
            &[],
            &[],
            "markup",
            &[("syntax", "html"), ("templates", "blade")],
        ),
        (
            "graphql",
            &[],
            &["graphql", "gql", "graphqls"],
            &[],
            &[],
            "blocks",
            &[("syntax", "graphql")],
        ),
        (
            "protobuf",
            &[],
            &["proto"],
            &[],
            &[],
            "blocks",
            &[("syntax", "protobuf")],
        ),
        (
            "har",
            &[],
            &["har"],
            &[],
            &[],
            "keypath",
            &[("syntax", "json"), ("view", "har")],
        ),
        (
            "jpeg",
            &[],
            &[],
            &[],
            &["0:ffd8ff"],
            "object",
            &[("layout", "jpeg")],
        ),
        (
            "png",
            &[],
            &[],
            &[],
            &["0:89504e470d0a1a0a"],
            "object",
            &[("layout", "png")],
        ),
        (
            "webp",
            &[],
            &[],
            &[],
            &["8:57454250"],
            "object",
            &[("layout", "webp")],
        ),
        (
            "tiff",
            &[],
            &[],
            &[],
            &["0:49492a00", "0:4d4d002a"],
            "object",
            &[("layout", "tiff")],
        ),
        (
            "id3",
            &[],
            &[],
            &[],
            &["0:494433"],
            "object",
            &[("layout", "id3")],
        ),
        (
            "mp4",
            &[],
            &[],
            &[],
            &["4:66747970"],
            "object",
            &[("layout", "mp4")],
        ),
        (
            "wasm",
            &[],
            &[],
            &[],
            &["0:0061736d"],
            "object",
            &[("layout", "wasm")],
        ),
        (
            "java-class",
            &[],
            &[],
            &[],
            &["0:cafebabe"],
            "object",
            &[("layout", "class")],
        ),
        ("eml", &[], &["eml"], &[], &[], "mail", &[("syntax", "eml")]),
        (
            "mbox",
            &["mbox"],
            &["mbox", "mbx"],
            &[],
            &["0:46726f6d20"],
            "mail",
            &[("syntax", "mbox")],
        ),
        (
            "icalendar",
            &[],
            &["ics", "ical", "ifb"],
            &[],
            &[],
            "keypath",
            &[("syntax", "ical")],
        ),
        (
            "vcard",
            &[],
            &["vcf", "vcard"],
            &[],
            &[],
            "keypath",
            &[("syntax", "ical")],
        ),
        (
            "bibtex",
            &[],
            &["bib"],
            &[],
            &[],
            "keypath",
            &[("syntax", "bibtex")],
        ),
        (
            "whatsapp",
            &["_chat.txt"],
            &[],
            &["*WhatsApp*.txt"],
            &[],
            "chat",
            &[],
        ),
        (
            "telegram",
            &[],
            &[],
            &["**/ChatExport_*/result.json", "**/DataExport_*/result.json"],
            &[],
            "keypath",
            &[("syntax", "json"), ("view", "telegram")],
        ),
        (
            "telegram-html",
            &[],
            &[],
            &[
                "**/ChatExport_*/messages*.html",
                "**/DataExport_*/chats/*/messages*.html",
            ],
            &[],
            "markup",
            &[("syntax", "html"), ("view", "telegram")],
        ),
    ];
    assert_eq!(table.len(), 40);
    let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    for (name, names, extensions, paths, magic, processor, params) in table {
        let def = r.get(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(def.names, strings(names), "{name}: names");
        assert_eq!(def.extensions, strings(extensions), "{name}: extensions");
        assert_eq!(def.paths, strings(paths), "{name}: paths");
        let got: Vec<String> = def
            .magic
            .iter()
            .map(|m| format!("{}:{}", m.offset, m.hex))
            .collect();
        assert_eq!(got, strings(magic), "{name}: magic");
        assert_eq!(&def.processor, processor, "{name}: processor");
        for (key, value) in *params {
            assert_eq!(
                def.params.get(*key).and_then(|v| v.as_str()),
                Some(*value),
                "{name}: {key}"
            );
        }
    }
    // Spec §8.3, D24: HTML's scripts and their three limits.
    let html = r.get("html").unwrap();
    assert_eq!(
        html.params.get("scripts").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        html.params
            .get("script_max_bytes")
            .and_then(|v| v.as_integer()),
        Some(1_048_576)
    );
    assert_eq!(
        html.params
            .get("script_max_line")
            .and_then(|v| v.as_integer()),
        Some(1000)
    );
    let d = Detector::new(&r, &[]).unwrap();
    let selected = |path: &str| {
        let got = d.detect(Path::new(path), b"").map(|x| x.name.clone());
        if let Some(early) = d.detect_by_path(Path::new(path)) {
            assert_eq!(early.map(|x| x.name.clone()), got, "{path}");
        }
        got
    };
    for (name, names, extensions, _, _, _, _) in table {
        for exact in *names {
            assert_eq!(selected(exact).as_deref(), Some(*name), "{exact}");
        }
        for extension in *extensions {
            assert_eq!(
                selected(&format!("sample.{extension}")).as_deref(),
                Some(*name),
                "{extension}"
            );
        }
    }
    for (path, want) in [
        // Signal 4 with the real Definitions: Telegram's globs win over json and html.
        ("x/ChatExport_2024/result.json", Some("telegram")),
        ("x/DataExport_2024/result.json", Some("telegram")),
        ("x/ChatExport_2024/messages.html", Some("telegram-html")),
        ("x/ChatExport_2024/messages2.html", Some("telegram-html")),
        (
            "x/DataExport_2024/chats/chat_001/messages.html",
            Some("telegram-html"),
        ),
        ("x/result.json", Some("json")),
        ("x/messages.html", Some("html")),
        ("x/ChatExport_2024/other.json", Some("json")),
        // WhatsApp's one glob writes `.txt`, which no Definition claims.
        ("x/WhatsApp Chat with Ana.txt", Some("whatsapp")),
        ("x/Chat de WhatsApp con Ana.txt", Some("whatsapp")),
        ("x/_chat.txt", Some("whatsapp")),
        ("x/notes.txt", None),
        // Signal 3: the compound extension over PHP.
        ("cart.blade.php", Some("blade")),
        ("cart.php", Some("php")),
        // Signal 6 after the extension.
        ("Makefile.toml", Some("toml")),
        (".env.json", Some("json")),
        ("Makefile.debug", Some("make")),
        ("Dockerfile.dev", Some("dockerfile")),
        (".env.production", Some("env")),
        ("prod.Dockerfile", Some("dockerfile")),
        ("home/ana/.ssh/config", Some("ssh-config")),
        ("etc/nginx/sites-enabled/default", Some("nginx")),
        ("etc/apache2/sites-enabled/000-default.conf", Some("apache")),
        ("etc/systemd/system/a.target", Some("systemd")),
        ("repo/.git/config", Some("ini")),
        ("x/app.conf", None),
        ("x/config", None),
    ] {
        assert_eq!(selected(path).as_deref(), want, "{path}");
    }
}
