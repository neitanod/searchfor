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
