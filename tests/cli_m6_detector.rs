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
    ]);
    let d = Detector::new(&r, &[]).unwrap();
    let paths = if cfg!(windows) {
        [
            (r"C:\Users\x\.ssh\config", "ssh-config"),
            (r"C:\etc\nginx\sites-enabled\default", "nginx"),
        ]
    } else {
        [
            ("/home/x/.ssh/config", "ssh-config"),
            ("/etc/nginx/sites-enabled/default", "nginx"),
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
