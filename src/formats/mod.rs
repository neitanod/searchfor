//! Format Definitions: built-in, installed with a Grammar Helper, and the user directory
//! (ADR 0002, spec §4.2).
pub mod builtin;
pub mod definition;
pub mod detector;

use definition::{FormatDefinition, Origin};
use std::path::{Path, PathBuf};

pub struct Registry {
    defs: Vec<FormatDefinition>,
}

/// `SEARCHFOR_FORMATS_DIR` if set (tests use it), else `<config dir>/searchfor/formats`.
pub fn user_formats_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("SEARCHFOR_FORMATS_DIR") {
        return Some(PathBuf::from(d));
    }
    dirs::config_dir().map(|d| d.join("searchfor").join("formats"))
}

impl Registry {
    /// Loads the built-ins, then every `*.toml` in `user_dir` (sorted by file name).
    /// Returns warnings for user files that fail to parse; they are skipped.
    pub fn load(
        user_dir: Option<&Path>,
        known_processor: &dyn Fn(&str) -> bool,
    ) -> (Self, Vec<String>) {
        Self::load_all(user_dir, None, known_processor)
    }

    /// Loads the built-ins, then the Definition of every Grammar Helper installed under
    /// `grammars_dir` (D26), then every `*.toml` in `user_dir` (sorted by file name), which
    /// overrides either. Returns warnings for files that fail to parse; they are skipped.
    pub fn load_all(
        user_dir: Option<&Path>,
        grammars_dir: Option<&Path>,
        known_processor: &dyn Fn(&str) -> bool,
    ) -> (Self, Vec<String>) {
        let mut defs: Vec<FormatDefinition> = builtin::BUILTIN
            .iter()
            .map(|(file, src)| {
                FormatDefinition::parse(src, known_processor)
                    .unwrap_or_else(|e| panic!("built-in {file} is invalid: {e}"))
            })
            .collect();
        let mut warnings = Vec::new();
        for i in grammars_dir
            .map(crate::grammars::installed)
            .unwrap_or_default()
        {
            match read_definition(&i.definition, known_processor) {
                Err(e) => warnings.push(e),
                Ok(mut d) => {
                    d.origin = Origin::Installed(i.definition);
                    match defs.iter().position(|x| x.name == d.name) {
                        Some(at) => defs[at] = d,
                        None => defs.push(d),
                    }
                }
            }
        }
        if let Some(dir) = user_dir {
            let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "toml"))
                .collect();
            files.sort();
            for f in files {
                match read_definition(&f, known_processor) {
                    Err(e) => warnings.push(e),
                    Ok(mut d) => match defs.iter().position(|x| x.name == d.name) {
                        Some(i) => {
                            d.origin = Origin::Overridden(f);
                            defs[i] = d;
                        }
                        None => {
                            d.origin = Origin::User(f);
                            defs.push(d);
                        }
                    },
                }
            }
        }
        (Self { defs }, warnings)
    }

    pub fn get(&self, name: &str) -> Option<&FormatDefinition> {
        self.defs.iter().find(|d| d.name == name)
    }

    pub fn all(&self) -> &[FormatDefinition] {
        &self.defs
    }

    /// Text for `--list-formats`: one line per Format, `name  origin  extensions`.
    pub fn list(&self) -> String {
        self.list_with(&|_| true)
    }

    /// `list`, with `  (unavailable)` after each Definition `available` rejects (spec §7).
    pub fn list_with(&self, available: &dyn Fn(&FormatDefinition) -> bool) -> String {
        let mut out = String::new();
        for d in &self.defs {
            let origin = match &d.origin {
                Origin::BuiltIn => "built-in".to_string(),
                Origin::Installed(p) => format!("installed ({})", p.display()),
                Origin::User(p) => format!("user ({})", p.display()),
                Origin::Overridden(p) => format!("overridden ({})", p.display()),
            };
            out.push_str(&format!(
                "{:<12} {:<40} {}{}\n",
                d.name,
                origin,
                d.extensions.join(","),
                if available(d) { "" } else { "  (unavailable)" }
            ));
        }
        out
    }
}

/// The Definition in `file`, or the warning that names it.
fn read_definition(
    file: &Path,
    known_processor: &dyn Fn(&str) -> bool,
) -> Result<FormatDefinition, String> {
    std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|s| FormatDefinition::parse(&s, known_processor))
        .map_err(|e| format!("s4: ignoring {}: {e}", file.display()))
}

/// `--export-formats DIR`: writes the embedded Definitions as templates (grill #6 comment).
/// Refuses the user formats directory, so an export never silently overrides the built-ins,
/// and never overwrites or follows an existing file: every target is checked before anything
/// is written, and each file is created with `create_new` (O_EXCL), which also refuses a
/// symlink sitting at the target path.
pub fn export_builtin(dir: &Path, user_dir: Option<&Path>) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(u) = user_dir {
        // Paths that may not exist yet are compared after resolving their longest existing
        // ancestor (symlinks included) and folding `.` and `..` in the rest.
        if resolve(dir)? == resolve(u)? {
            anyhow::bail!(
                "refusing to export into the user formats directory {}: the files would override the built-ins",
                u.display()
            );
        }
    }
    let targets: Vec<PathBuf> = builtin::BUILTIN
        .iter()
        .map(|(file, _)| dir.join(file))
        .collect();
    // The checks above and this one run before anything is created.
    if let Some(t) = targets.iter().find(|t| t.symlink_metadata().is_ok()) {
        anyhow::bail!("refusing to overwrite {}", t.display());
    }
    std::fs::create_dir_all(dir)?;
    for (t, (_, src)) in targets.iter().zip(builtin::BUILTIN) {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(t)?
            .write_all(src.as_bytes())?;
    }
    Ok(targets)
}

/// Absolute path with every existing prefix canonicalized, re-checked after each component:
/// `..` and `.` are folded lexically, and whenever the result exists again (a `..` that comes
/// back into an existing directory, a symlink after it) it is canonicalized again. So
/// `nuevo/../link/formats` with `link → base` resolves to `base/formats`.
fn resolve(p: &Path) -> std::io::Result<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in std::path::absolute(p)?.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
        if std::fs::symlink_metadata(&out).is_ok() {
            out = std::fs::canonicalize(&out)?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn known(p: &str) -> bool {
        crate::processors::Processors::known(p)
    }

    /// M4 plan, D5 and D10: document Formats only with --docs; object Formats by their magic
    /// whatever the name.
    #[test]
    fn the_m4_definitions_load_and_detect() {
        let (r, warnings) = Registry::load(None, &known);
        assert!(warnings.is_empty(), "{warnings:?}");
        let plain = detector::Detector::new(&r, &[]).unwrap();
        let docs = detector::Detector::new(&r, &[]).unwrap().with_docs(true);
        let p = std::path::Path::new;
        for (path, format) in [
            ("r.pdf", "pdf"),
            ("r.DOCX", "docx"),
            ("b.xlsm", "xlsx"),
            ("d.ppsx", "pptx"),
            ("t.ott", "odt"),
            ("s.ods", "ods"),
            ("p.odp", "odp"),
            ("b.epub", "epub"),
            ("x.sqlite3", "sqlite"),
            ("app.db", "sqlite"),
        ] {
            assert_eq!(
                docs.detect(p(path), b"").map(|d| d.name.as_str()),
                Some(format),
                "{path}"
            );
            assert!(
                plain.detect(p(path), b"").is_none(),
                "{path} without --docs"
            );
        }
        let mut odt = b"PK\x03\x04".to_vec();
        odt.resize(30, 0);
        odt.extend(b"mimetypeapplication/vnd.oasis.opendocument.text");
        for (head, format) in [
            (&b"%PDF-1.7\n"[..], "pdf"),
            (b"SQLite format 3\0", "sqlite"),
            (&odt, "odt"),
        ] {
            assert_eq!(
                docs.detect(p("noext"), head).map(|d| d.name.as_str()),
                Some(format)
            );
            assert!(plain.detect(p("noext"), head).is_none());
        }
        for (head, format) in [
            (&b"\x7fELF\x02\x01"[..], "elf"),
            (b"MZ\x90\0", "pe"),
            (b"\xcf\xfa\xed\xfe", "macho"),
            (b"\xca\xfe\xba\xbe", "macho"),
        ] {
            assert_eq!(plain.object(head).map(|d| d.name.as_str()), Some(format));
            // Without an extension the magic decides as for any Format.
            assert_eq!(
                plain.detect(p("ls"), head).map(|d| d.name.as_str()),
                Some(format)
            );
        }
        assert!(plain.object(b"#!/bin/sh\n").is_none());
        // `libc.so.6`: its extension decides nothing, so only `object` tells it.
        assert!(plain.detect(p("libc.so.6"), b"\x7fELF").is_none());
        assert_eq!(
            plain.object(b"\x7fELF").map(|d| d.name.as_str()),
            Some("elf")
        );
    }

    #[test]
    fn the_m3_definitions_load_and_detect() {
        let (r, warnings) = Registry::load(None, &known);
        assert!(warnings.is_empty(), "{warnings:?}");
        let det = detector::Detector::new(&r, &[]).unwrap();
        for (path, head, format) in [
            ("a/page.HTML", &b""[..], "html"),
            ("p.xhtml", b"", "html"),
            ("feed.xml", b"", "xml"),
            ("icon.svg", b"", "xml"),
            ("feed", b"<?xml version=\"1.0\"?>\n<r/>", "xml"),
            ("t.csv", b"", "csv"),
            ("t.tsv", b"", "tsv"),
            ("m.srt", b"", "srt"),
            ("m.vtt", b"", "vtt"),
        ] {
            let got = det
                .detect(std::path::Path::new(path), head)
                .map(|d| d.name.as_str());
            assert_eq!(got, Some(format), "{path}");
        }
    }

    #[test]
    fn user_file_overrides_builtin_and_adds_new() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("markdown.toml"),
            "name='markdown'\nextensions=['txt']\nprocessor='markdown'",
        )
        .unwrap();
        std::fs::write(
            d.path().join("notes.toml"),
            "name='notes'\nextensions=['note']\nprocessor='markdown'",
        )
        .unwrap();
        std::fs::write(d.path().join("broken.toml"), "name=").unwrap();
        let (r, warnings) = Registry::load(Some(d.path()), &known);
        assert_eq!(r.get("markdown").unwrap().extensions, ["txt"]);
        assert!(matches!(
            r.get("markdown").unwrap().origin,
            Origin::Overridden(_)
        ));
        assert!(matches!(r.get("notes").unwrap().origin, Origin::User(_)));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("broken.toml"));
    }

    #[test]
    fn list_shows_name_origin_and_extensions() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("notes.toml"),
            "name='notes'\nextensions=['note','nt']\nprocessor='markdown'",
        )
        .unwrap();
        let (r, _) = Registry::load(Some(d.path()), &known);
        let text = r.list();
        let lines: Vec<&str> = text.lines().collect();
        let line = |name: &str| {
            lines
                .iter()
                .find(|l| l.starts_with(&format!("{name} ")))
                .copied()
                .unwrap()
        };
        assert!(line("markdown").contains("built-in"));
        assert!(line("markdown").ends_with("md,markdown,mdx,mkd,mdown"));
        assert!(
            line("notes").contains(&format!("user ({})", d.path().join("notes.toml").display()))
        );
        assert!(line("notes").ends_with("note,nt"));
        assert_eq!(lines.len(), builtin::BUILTIN.len() + 1);
    }

    #[test]
    fn missing_user_dir_loads_only_builtins() {
        let base = tempfile::tempdir().unwrap();
        let (r, warnings) = Registry::load(Some(&base.path().join("absent")), &known);
        assert_eq!(r.all().len(), builtin::BUILTIN.len());
        assert!(warnings.is_empty());
    }

    #[test]
    fn installed_definitions_load_between_built_ins_and_user_files() {
        let grammars = tempfile::tempdir().unwrap();
        let lua = grammars.path().join("lua");
        std::fs::create_dir(&lua).unwrap();
        std::fs::write(lua.join("lua.toml"), "name='lua'\nextensions=['lua']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'").unwrap();
        std::fs::write(lua.join(crate::grammars::binary_name("lua")), "").unwrap();
        std::fs::write(lua.join("installed.json"), r#"{"version":"0.2.0"}"#).unwrap();
        let user = tempfile::tempdir().unwrap();
        let (r, w) = Registry::load_all(
            Some(user.path()),
            Some(grammars.path()),
            &crate::processors::Processors::known,
        );
        assert!(w.is_empty(), "{w:?}");
        assert!(matches!(r.get("lua").unwrap().origin, Origin::Installed(_)));
        assert!(r.list().contains("installed ("));
        // A user file still wins (#6).
        std::fs::write(user.path().join("lua.toml"), "name='lua'\nextensions=['luau']\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery='(x) @segment'").unwrap();
        let (r, _) = Registry::load_all(
            Some(user.path()),
            Some(grammars.path()),
            &crate::processors::Processors::known,
        );
        assert_eq!(r.get("lua").unwrap().extensions, ["luau"]);
        assert!(matches!(
            r.get("lua").unwrap().origin,
            Origin::Overridden(_)
        ));
        assert!(r.list().contains("overridden ("));
    }

    #[test]
    fn an_installed_definition_that_does_not_parse_is_a_warning_naming_it() {
        let grammars = tempfile::tempdir().unwrap();
        let lua = grammars.path().join("lua");
        std::fs::create_dir(&lua).unwrap();
        std::fs::write(lua.join("lua.toml"), "name=").unwrap();
        std::fs::write(lua.join(crate::grammars::binary_name("lua")), "").unwrap();
        std::fs::write(lua.join("installed.json"), r#"{"version":"0.2.0"}"#).unwrap();
        let (r, w) = Registry::load_all(None, Some(grammars.path()), &known);
        assert!(r.get("lua").is_none());
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(
            w[0].contains(&lua.join("lua.toml").display().to_string()),
            "{w:?}"
        );
    }

    #[test]
    fn list_marks_unavailable_formats() {
        let (r, _) = Registry::load(None, &crate::processors::Processors::known);
        let text = r.list_with(&|d| d.name != "php");
        let php = text.lines().find(|l| l.starts_with("php ")).unwrap();
        assert!(php.ends_with("(unavailable)"), "{php}");
        assert!(
            !text
                .lines()
                .find(|l| l.starts_with("rust "))
                .unwrap()
                .contains("unavailable")
        );
    }

    #[test]
    fn export_writes_files_and_refuses_user_dir() {
        let user = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let written = export_builtin(&out.path().join("new"), Some(user.path())).unwrap();
        assert_eq!(written.len(), builtin::BUILTIN.len());
        assert!(
            export_builtin(user.path(), Some(user.path()))
                .unwrap_err()
                .to_string()
                .contains("refusing")
        );
    }

    #[test]
    fn export_never_overwrites_or_follows_symlinks() {
        let out = tempfile::tempdir().unwrap();
        let victim = tempfile::tempdir().unwrap();
        let victim_file = victim.path().join("markdown.toml");
        std::fs::write(&victim_file, "precious").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim_file, out.path().join("markdown.toml")).unwrap();
        #[cfg(not(unix))]
        std::fs::write(out.path().join("markdown.toml"), "existing").unwrap();
        assert!(
            export_builtin(out.path(), None)
                .unwrap_err()
                .to_string()
                .contains("refusing to overwrite")
        );
        assert_eq!(std::fs::read_to_string(&victim_file).unwrap(), "precious");
    }

    #[test]
    fn refusing_a_missing_user_dir_creates_nothing() {
        let base = tempfile::tempdir().unwrap();
        let missing = base.path().join("formats");
        assert!(
            export_builtin(&missing, Some(&missing))
                .unwrap_err()
                .to_string()
                .contains("refusing")
        );
        assert!(!missing.exists());
    }

    #[test]
    fn refusing_through_dotdot_and_symlinked_ancestors() {
        let base = tempfile::tempdir().unwrap();
        std::fs::create_dir(base.path().join("aux")).unwrap();
        let user = base.path().join("formats");
        let sneaky = base.path().join("aux/../formats");
        assert!(
            export_builtin(&sneaky, Some(&user))
                .unwrap_err()
                .to_string()
                .contains("refusing")
        );
        assert!(!user.exists());
        let missing_parent = base.path().join("nuevo/../formats");
        assert!(
            export_builtin(&missing_parent, Some(&user))
                .unwrap_err()
                .to_string()
                .contains("refusing")
        );
        assert!(!user.exists() && !base.path().join("nuevo").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.path(), base.path().join("link2")).unwrap();
            let combined = base.path().join("nuevo/../link2/formats");
            assert!(
                export_builtin(&combined, Some(&user))
                    .unwrap_err()
                    .to_string()
                    .contains("refusing")
            );
            assert!(!user.exists() && !base.path().join("nuevo").exists());
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.path(), base.path().join("link")).unwrap();
            let via_link = base.path().join("link/formats");
            assert!(
                export_builtin(&via_link, Some(&user))
                    .unwrap_err()
                    .to_string()
                    .contains("refusing")
            );
            assert!(!user.exists());
        }
    }
}
