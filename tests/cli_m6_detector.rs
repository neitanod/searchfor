mod common;
use searchfor::formats::{Registry, definition::FormatDefinition, detector::Detector};
use std::path::Path;
fn registry(defs: &[(&str, &str)]) -> (tempfile::TempDir, Registry) {
    let dir = tempfile::tempdir().unwrap();
    for (name, signals) in defs {
        std::fs::write(
            dir.path().join(format!("{name}.toml")),
            format!("name='{name}'\nprocessor='markdown'\n{signals}\n"),
        )
        .unwrap();
    }
    let (r, warnings) = Registry::load(Some(dir.path()), &|_| true);
    assert!(warnings.is_empty(), "{warnings:?}");
    (dir, r)
}
fn selected(d: &Detector<'_>, path: &str) -> Option<String> {
    let got = d.detect(Path::new(path), b"").map(|x| x.name.clone());
    if let Some(early) = d.detect_by_path(Path::new(path)) {
        assert_eq!(early.map(|x| x.name.clone()), got, "{path}");
    }
    got
}
#[test]
fn m6_compound() {
    let parsed =
        FormatDefinition::parse("name='x'\nprocessor='markdown'\npaths=['X.*']", |_| true).unwrap();
    assert_eq!(parsed.paths, ["X.*"]);
    let (_dir, r) = registry(&[
        ("blade", "extensions=['blade.php']"),
        ("special", "names=['special.blade.php']"),
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    for (path, want) in [
        ("cart.blade.php", "blade"),
        ("cart.BLADE.PHP", "blade"),
        ("a.b.cart.blade.php", "blade"),
        ("cart.php", "php"),
        ("special.blade.php", "special"),
    ] {
        assert_eq!(selected(&d, path).as_deref(), Some(want), "{path}");
    }
    let d = Detector::new(&r, &["*.php=markdown".into()]).unwrap();
    assert_eq!(selected(&d, "cart.blade.php").as_deref(), Some("markdown"));
    assert!(FormatDefinition::parse("name='x'\nprocessor='markdown'\npaths=3", |_| true).is_err());
    let old = FormatDefinition::parse("name='x'\nprocessor='markdown'", |_| true).unwrap();
    assert!(old.paths.is_empty());
}
#[test]
fn m6_paths() {
    let (_dir, r) = registry(&[
        (
            "env",
            "names=['.env']\nextensions=['env']\npaths=['.env.*']",
        ),
        (
            "dockerfile",
            "names=['Dockerfile']\nextensions=['dockerfile']\npaths=['Dockerfile.*']",
        ),
        (
            "ssh-config",
            "paths=['**/.ssh/config','**/.ssh/config.d/*']",
        ),
        (
            "nginx",
            "names=['nginx.conf']\npaths=['**/nginx/**/*.conf','**/nginx/sites-enabled/*']",
        ),
        (
            "make",
            "names=['Makefile','makefile']\npaths=['Makefile.*']",
        ),
        ("systemd", "paths=['**/systemd/**/*.{path,target,conf}']"),
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    for (path, want) in [
        (".env.production", "env"),
        ("Dockerfile.dev", "dockerfile"),
        ("prod.Dockerfile", "dockerfile"),
        ("/home/x/.ssh/config", "ssh-config"),
        ("/home/x/.ssh/config.d/local", "ssh-config"),
        ("/etc/nginx/sites-enabled/default", "nginx"),
        ("/etc/nginx/a/b.conf", "nginx"),
        ("nginx.conf", "nginx"),
        ("Makefile.debug", "make"),
        ("Makefile.toml", "toml"),
        ("/etc/systemd/system/demo.path", "systemd"),
    ] {
        assert_eq!(selected(&d, path).as_deref(), Some(want), "{path}");
    }
    for path in ["other.conf", "MAKEFILE", "other.asc", "Otherfile.toml.bak"] {
        assert!(selected(&d, path).is_none(), "{path}");
    }
    assert!(d.detect_by_path(Path::new("other.conf")).unwrap().is_none());
    // A user glob with no mandatory literal still follows full globset semantics.
    let (_any, any) = registry(&[("any", "paths=['**/*']")]);
    assert_eq!(
        selected(&Detector::new(&any, &[]).unwrap(), "a/b.unknown").as_deref(),
        Some("any")
    );
    // A user document Definition's `paths` reach the pre-search document gate,
    // which answers only with --docs (`may_be_document` is false without it).
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(
        docs.path().join("doc-path.toml"),
        "name='doc-path'\nprocessor='docs'\npaths=['Book.*']\n[params]\nextractor='docx'\n",
    )
    .unwrap();
    let (r, warnings) = Registry::load(Some(docs.path()), &|_| true);
    assert!(warnings.is_empty(), "{warnings:?}");
    let d = Detector::new(&r, &[]).unwrap().with_docs(true);
    assert!(d.may_be_document(Path::new("Book.raw")));
    assert!(!d.may_be_document(Path::new("Other.raw")));
    assert!(
        !Detector::new(&r, &[])
            .unwrap()
            .may_be_document(Path::new("Book.raw"))
    );
}
#[test]
fn m6_path_separators_are_native() {
    let (_dir, r) = registry(&[
        ("ssh-config", "paths=['**/.ssh/config']"),
        ("nginx", "paths=['**/nginx/sites-enabled/*']"),
        ("telegram", "paths=['**/ChatExport_*/result.json']"),
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    // Both arrays have one type, so the `/` branch carries Telegram's case too.
    let paths = if cfg!(windows) {
        [
            (r"C:\Users\x\.ssh\config", "ssh-config"),
            (r"C:\etc\nginx\sites-enabled\default", "nginx"),
            (r"C:\x\ChatExport_2024\result.json", "telegram"),
        ]
    } else {
        [
            ("/home/x/.ssh/config", "ssh-config"),
            ("/etc/nginx/sites-enabled/default", "nginx"),
            ("/x/ChatExport_2024/result.json", "telegram"),
        ]
    };
    for (path, want) in paths {
        assert_eq!(selected(&d, path).as_deref(), Some(want));
    }
}
#[test]
fn m6_invalid_user_glob_warns() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("broken.toml"),
        "name='broken'\nprocessor='markdown'\npaths=['a[']\n",
    )
    .unwrap();
    let (r, warnings) = Registry::load(Some(dir.path()), &|_| true);
    assert!(r.get("broken").is_none());
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("broken.toml") && warnings[0].contains("`paths`"),
        "{warnings:?}"
    );
}
#[test]
fn m6_detection_table() {
    let definitions = [
        ("asciidoc", "extensions=['adoc','asciidoc']"),
        ("rst", "extensions=['rst','rest']"),
        ("org", "extensions=['org']"),
        ("latex", "extensions=['tex','ltx','latex']"),
        ("jupyter", "extensions=['ipynb']"),
        (
            "ini",
            "names=['.gitconfig','.gitmodules','.editorconfig','.npmrc','.pypirc','.flake8','.pylintrc','.coveragerc','.hgrc']\nextensions=['ini','cfg','cnf','inf','desktop']\npaths=['**/.git/config','**/git/config']",
        ),
        (
            "systemd",
            "extensions=['service','socket','timer','mount','automount']\npaths=['**/systemd/**/*.{path,target,slice,scope,swap,network,netdev,link,conf}']",
        ),
        ("properties", "extensions=['properties']"),
        (
            "env",
            "names=['.env']\nextensions=['env']\npaths=['.env.*']",
        ),
        (
            "ssh-config",
            "names=['ssh_config','sshd_config']\npaths=['**/.ssh/config','**/.ssh/config.d/*','**/ssh/ssh_config.d/*.conf','**/ssh/sshd_config.d/*.conf']",
        ),
        (
            "nginx",
            "names=['nginx.conf']\npaths=['**/nginx/**/*.conf','**/nginx/sites-available/*','**/nginx/sites-enabled/*']",
        ),
        (
            "apache",
            "names=['httpd.conf','apache2.conf','.htaccess']\npaths=['**/apache2/**/*.conf','**/httpd/**/*.conf','**/apache2/sites-available/*','**/apache2/sites-enabled/*']",
        ),
        ("hcl", "extensions=['tf','tfvars','hcl','nomad']"),
        (
            "dockerfile",
            "names=['Dockerfile','Containerfile','dockerfile']\nextensions=['dockerfile','containerfile']\npaths=['Dockerfile.*','Containerfile.*']",
        ),
        (
            "make",
            "names=['Makefile','makefile','GNUmakefile','BSDmakefile']\nextensions=['mk','mak','make']\npaths=['Makefile.*']",
        ),
        (
            "just",
            "names=['justfile','Justfile','.justfile']\nextensions=['just']",
        ),
        ("sql", "extensions=['sql','ddl','dml']"),
        ("diff", "extensions=['diff','patch','rej']"),
        ("vue", "extensions=['vue']"),
        ("svelte", "extensions=['svelte']"),
        ("blade", "extensions=['blade.php']"),
        ("graphql", "extensions=['graphql','gql','graphqls']"),
        ("protobuf", "extensions=['proto']"),
        ("har", "extensions=['har']"),
        ("eml", "extensions=['eml']"),
        (
            "mbox",
            "names=['mbox']\nextensions=['mbox','mbx']\nmagic=[{hex='46726f6d20'}]",
        ),
        ("icalendar", "extensions=['ics','ical','ifb']"),
        ("vcard", "extensions=['vcf','vcard']"),
        ("bibtex", "extensions=['bib']"),
        ("whatsapp", "names=['_chat.txt']\npaths=['*WhatsApp*.txt']"),
        (
            "telegram",
            "paths=['**/ChatExport_*/result.json','**/DataExport_*/result.json']",
        ),
        ("telegram-html", "paths=['**/ChatExport_*/messages*.html']"),
    ];
    let (_dir, r) = registry(&definitions);
    let d = Detector::new(&r, &[]).unwrap();
    for (name, _) in definitions {
        let def = r.get(name).unwrap();
        for exact in &def.names {
            assert_eq!(selected(&d, exact).as_deref(), Some(name), "{exact}");
        }
        for extension in &def.extensions {
            assert_eq!(
                selected(&d, &format!("sample.{extension}")).as_deref(),
                Some(name)
            );
        }
    }
    assert_eq!(
        d.detect(Path::new("Inbox"), b"From sender\n").unwrap().name,
        "mbox"
    );
    assert!(d.detect(Path::new("Inbox.txt"), b"From sender\n").is_none());
}
#[test]
fn m6_telegram_precedence() {
    let (_dir, r) = registry(&[
        (
            "telegram",
            "paths=['**/ChatExport_*/result.json','**/DataExport_*/result.json']",
        ),
        ("telegram-html", "paths=['**/ChatExport_*/messages*.html']"),
        ("conf-json", "paths=['**/conf/*.json']"),
        ("whatsapp", "names=['_chat.txt']\npaths=['*WhatsApp*.txt']"),
        ("make", "names=['Makefile']\npaths=['Makefile.*']"),
        ("env", "names=['.env']\npaths=['.env.*']"),
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    // Spec §4.1 signal 4: a glob that writes the extension wins over the extension.
    for (path, want) in [
        ("x/ChatExport_2024/result.json", "telegram"),
        ("x/DataExport_2024/result.json", "telegram"),
        ("x/ChatExport_2024/messages.html", "telegram-html"),
        ("x/ChatExport_2024/messages2.html", "telegram-html"),
        ("x/conf/app.json", "conf-json"),
        // Outside the folders the claimed extension keeps the file.
        ("x/result.json", "json"),
        ("x/messages.html", "html"),
        ("x/ChatExport_2024/other.json", "json"),
        ("x/ChatExport_2024/index.html", "html"),
        // WhatsApp's one glob: the app's name in the export's name, any language.
        ("x/WhatsApp Chat with Ana.txt", "whatsapp"),
        ("x/WhatsApp Chat - Ana.txt", "whatsapp"),
        ("x/Chat de WhatsApp con Ana.txt", "whatsapp"),
        ("x/_chat.txt", "whatsapp"),
        // Signal 6: a glob that leaves the extension open comes after it.
        ("Makefile.toml", "toml"),
        (".env.json", "json"),
        ("Makefile.debug", "make"),
    ] {
        assert_eq!(selected(&d, path).as_deref(), Some(want), "{path}");
    }
    for path in [
        "x/notes.txt",
        "x/WhatsApp/notes.txt",
        "x/WhatsApp Chat with Ana.log",
    ] {
        assert!(selected(&d, path).is_none(), "{path}");
    }
    // An override still wins over signal 4.
    let d = Detector::new(&r, &["*.json=json".into()]).unwrap();
    assert_eq!(
        selected(&d, "x/ChatExport_2024/result.json").as_deref(),
        Some("json")
    );
}
