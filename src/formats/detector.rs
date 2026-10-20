//! The Detector (spec §4.1): override, exact name, extension, magic bytes, shebang.
use super::{Registry, definition::FormatDefinition};
use globset::{Glob, GlobMatcher};
use std::collections::{HashMap, HashSet};
use std::path::Path;

enum Override {
    All(String),
    Glob(GlobMatcher, String),
}

pub struct Detector<'r> {
    registry: &'r Registry,
    overrides: Vec<Override>,
    /// `--docs`, on by default: with `--no-docs` the document Formats (`processor = "docs"`)
    /// are never detected, so their files are Plain, as ripgrep reads them (spec §5.8).
    docs: bool,
    /// With `--docs`, the exact names and the extensions (lowercase) the document Formats
    /// claim (`may_be_document`).
    doc_names: HashSet<String>,
    doc_extensions: HashSet<String>,
    /// The first active Definition (in the Registry's order) of each exact name and of each
    /// extension (lowercase): `detect` asks them of every file.
    by_name: HashMap<String, usize>,
    by_extension: HashMap<String, usize>,
    /// What `detect` gives a name without an extension and an empty head, past its exact
    /// names: the first active Definition whose magic bytes are empty.
    empty_head: Option<usize>,
    /// An active Definition looks at a file's first bytes (magic bytes or a shebang).
    by_content: bool,
    /// With `--docs`, the document Formats that have magic bytes (`document_magic`).
    doc_magic: Vec<usize>,
    /// With `--docs`, a document Format has a shebang.
    doc_shebang: bool,
}

impl<'r> Detector<'r> {
    /// `--format` values: `NAME` or `GLOB=NAME`. Unknown names are an error.
    pub fn new(registry: &'r Registry, formats: &[String]) -> anyhow::Result<Self> {
        let mut overrides = Vec::new();
        for f in formats {
            let (glob, name) = match f.rsplit_once('=') {
                Some((g, n)) => (Some(g), n),
                None => (None, f.as_str()),
            };
            if registry.get(name).is_none() {
                anyhow::bail!("unknown format `{name}` (see --list-formats)");
            }
            overrides.push(match glob {
                Some(g) => Override::Glob(Glob::new(g)?.compile_matcher(), name.to_string()),
                None => Override::All(name.to_string()),
            });
        }
        let mut d = Self {
            registry,
            overrides,
            docs: false,
            doc_names: HashSet::new(),
            doc_extensions: HashSet::new(),
            by_name: HashMap::new(),
            by_extension: HashMap::new(),
            empty_head: None,
            by_content: false,
            doc_magic: Vec::new(),
            doc_shebang: false,
        };
        d.index();
        Ok(d)
    }

    fn index(&mut self) {
        self.by_name.clear();
        self.by_extension.clear();
        self.empty_head = None;
        self.by_content = false;
        self.doc_magic.clear();
        self.doc_shebang = false;
        for (i, d) in self.registry.all().iter().enumerate() {
            if !self.active(d) {
                continue;
            }
            if self.empty_head.is_none() && d.magic_at(&[]) {
                self.empty_head = Some(i);
            }
            self.by_content |= !d.magic.is_empty() || !d.shebangs.is_empty();
            if d.processor == "docs" && !d.magic.is_empty() {
                self.doc_magic.push(i);
            }
            self.doc_shebang |= d.processor == "docs" && !d.shebangs.is_empty();
            for n in &d.names {
                self.by_name.entry(n.clone()).or_insert(i);
            }
            for e in &d.extensions {
                self.by_extension.entry(e.to_ascii_lowercase()).or_insert(i);
            }
        }
    }

    /// Detects the document Formats too (`--docs`).
    pub fn with_docs(mut self, on: bool) -> Self {
        self.docs = on;
        if on {
            for d in self.registry.all().iter().filter(|d| d.processor == "docs") {
                self.doc_names.extend(d.names.iter().cloned());
                self.doc_extensions
                    .extend(d.extensions.iter().map(|e| e.to_ascii_lowercase()));
            }
        }
        self.index();
        self
    }

    /// False when `path` cannot be a document, which a set lookup tells for most files, before
    /// `detect` walks every Definition: --docs asks it of every file (M4 plan r1, Fable #5).
    /// True with an override, for a name without an extension (its first bytes decide), and
    /// for a name or extension a document Format claims.
    pub fn may_be_document(&self, path: &Path) -> bool {
        if !self.docs {
            return false;
        }
        let (Some(name), Some(ext)) = (
            path.file_name().and_then(|n| n.to_str()),
            path.extension().and_then(|e| e.to_str()),
        ) else {
            return true;
        };
        !self.overrides.is_empty()
            || self.doc_names.contains(name)
            || self
                .doc_extensions
                .contains(normalized_extension(ext).as_ref())
    }

    /// Whether `detect` may find a document Format in `head`, the first bytes of a name
    /// without an extension whose exact name decides nothing: its magic bytes start it, or an
    /// override or a document Format's shebang may decide. False tells that the file is no
    /// document without `detect`'s walk of every Definition (code review r6, Fable #1).
    pub fn document_magic(&self, head: &[u8]) -> bool {
        let defs = self.registry.all();
        !self.overrides.is_empty()
            || self.doc_shebang
            || self.doc_magic.iter().any(|&i| defs[i].magic_at(head))
    }

    /// Whether `d` takes part in detection: a document Format unless `--no-docs`.
    fn active(&self, d: &FormatDefinition) -> bool {
        self.docs || d.processor != "docs"
    }

    /// The object Format whose magic bytes start `head`, whatever the file's name: binaries are
    /// often named without a telling extension (`libc.so.6`, `ls`), so for a binary file with a
    /// Match the magic decides (spec §4.1, M4 plan D5).
    pub fn object(&self, head: &[u8]) -> Option<&'r FormatDefinition> {
        self.registry
            .all()
            .iter()
            .find(|d| d.processor == "object" && d.magic_at(head))
    }

    /// `detect` before reading the file: Some when the path alone decides (an override, an
    /// exact name, an extension, a name with an extension no Format claims, or no Format that
    /// looks at the content), None when the Format may depend on the first bytes (magic bytes
    /// or a shebang), which happens only for a name without extension (M2 plan, D15).
    pub fn detect_by_path(&self, path: &Path) -> Option<Option<&'r FormatDefinition>> {
        // A name without an extension and no override, asked of every such file: `detect` with
        // an empty head would look at its exact names and at the magic bytes that are empty,
        // without walking every Definition (code review r6, Fable #1).
        if self.overrides.is_empty() && path.extension().is_none() {
            let defs = self.registry.all();
            let named = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| self.by_name.get(n))
                .copied();
            if let Some(i) = named.or(self.empty_head) {
                return Some(Some(&defs[i]));
            }
            return (!self.by_content).then_some(None);
        }
        if let Some(d) = self.detect(path, &[]) {
            return Some(Some(d));
        }
        if path.extension().is_some() {
            return Some(None);
        }
        let by_content = self
            .registry
            .all()
            .iter()
            .any(|d| self.active(d) && (!d.magic.is_empty() || !d.shebangs.is_empty()));
        (!by_content).then_some(None)
    }

    /// `head` is the start of the file; 512 bytes are enough for every signal. Magic bytes and
    /// a shebang are read only when the file name has no extension (M2 plan, D15); a dotfile
    /// such as `.bashrc` has none.
    pub fn detect(&self, path: &Path, head: &[u8]) -> Option<&'r FormatDefinition> {
        let defs = self.registry.all();
        for o in &self.overrides {
            match o {
                Override::All(n) => return self.registry.get(n),
                Override::Glob(g, n) if g.is_match(path) => return self.registry.get(n),
                Override::Glob(..) => {}
            }
        }
        // A file name that is not UTF-8 skips the name and extension signals only.
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
            && let Some(&i) = self.by_name.get(file_name)
        {
            return Some(&defs[i]);
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && let Some(&i) = self.by_extension.get(normalized_extension(ext).as_ref())
        {
            return Some(&defs[i]);
        }
        // Magic bytes and shebangs decide only for a name without extension (M2 plan, D15).
        if path.extension().is_some() {
            return None;
        }
        if let Some(d) = defs.iter().find(|d| self.active(d) && d.magic_at(head)) {
            return Some(d);
        }
        let interp = shebang_interpreter(head)?;
        defs.iter().find(|d| d.shebangs.iter().any(|s| s == interp))
    }
}

/// The exact names and the extensions (lowercase) the built-in document Formats claim
/// (formats/docx.toml and the other eight; a unit test keeps them equal).
pub const BUILTIN_DOC_NAMES: &[&str] = &[];
pub const BUILTIN_DOC_EXTENSIONS: &[&str] = &[
    "docx", "docm", "dotx", "dotm", "epub", "xlsx", "xlsm", "xltx", "xltm", "pdf", "pptx", "pptm",
    "potx", "potm", "ppsx", "ppsm", "ods", "ots", "odt", "ott", "sqlite", "sqlite3", "db", "db3",
    "s3db", "sl3", "odp", "otp",
];

/// `Detector::may_be_document` with `--docs`, no override and only the built-in Definitions,
/// before the Registry is loaded: a search with no candidate never loads it (code review r7,
/// Codex #3).
pub fn builtin_may_be_document(path: &Path) -> bool {
    let (Some(name), Some(ext)) = (
        path.file_name().and_then(|n| n.to_str()),
        path.extension().and_then(|e| e.to_str()),
    ) else {
        return true;
    };
    BUILTIN_DOC_NAMES.contains(&name)
        || BUILTIN_DOC_EXTENSIONS.contains(&normalized_extension(ext).as_ref())
}

/// `ext` in lowercase, as the Detector keys extensions: borrowed unless it holds an ASCII
/// uppercase letter, so the common lowercase extension costs no copy per file (M5b plan,
/// Task 3.2).
fn normalized_extension(ext: &str) -> std::borrow::Cow<'_, str> {
    if ext.bytes().any(|b| b.is_ascii_uppercase()) {
        std::borrow::Cow::Owned(ext.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Borrowed(ext)
    }
}

/// `#!/usr/bin/env bash -e` → "bash"; `#!/bin/sh` → "sh".
fn shebang_interpreter(head: &[u8]) -> Option<&str> {
    let line = head.strip_prefix(b"#!")?;
    let line = &line[..memchr::memchr(b'\n', line).unwrap_or(line.len())];
    let line = std::str::from_utf8(line).ok()?.trim();
    let mut words = line.split_whitespace();
    let first = words.next()?;
    let prog = first.rsplit('/').next()?;
    if prog == "env" {
        words.find(|w| !w.starts_with('-'))
    } else {
        Some(prog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_extensions_borrow_the_existing_bytes() {
        use std::borrow::Cow;
        assert!(matches!(normalized_extension("txt"), Cow::Borrowed("txt")));
        assert!(matches!(normalized_extension(""), Cow::Borrowed("")));
        assert_eq!(normalized_extension("TXT"), "txt");
        assert_eq!(normalized_extension("DocX"), "docx");
    }

    /// Code review r7, Codex #3: before the Registry is loaded, the built-in document Formats'
    /// exact names and extensions tell which names may be documents, as the Detector of the
    /// built-in Definitions does.
    #[test]
    fn the_built_in_document_names_are_the_document_formats() {
        let r = registry();
        let docs = r.all().iter().filter(|d| d.processor == "docs");
        let names: HashSet<String> = docs.clone().flat_map(|d| d.names.clone()).collect();
        let extensions: HashSet<String> = docs
            .flat_map(|d| d.extensions.iter().map(|e| e.to_ascii_lowercase()))
            .collect();
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        assert_eq!(names, set(BUILTIN_DOC_NAMES));
        assert_eq!(extensions, set(BUILTIN_DOC_EXTENSIONS));
        let d = Detector::new(&r, &[]).unwrap().with_docs(true);
        for path in [
            "a.txt",
            "r.pdf",
            "R.PDF",
            "app.db",
            "x.Sqlite3",
            "Makefile",
            "bin/ls",
            ".bashrc",
            "a.b.docx",
            "dir.pdf/a.md",
            "noext/",
        ] {
            let p = Path::new(path);
            assert_eq!(builtin_may_be_document(p), d.may_be_document(p), "{path}");
        }
    }

    #[test]
    fn only_some_names_may_be_documents() {
        let r = registry();
        let d = Detector::new(&r, &[]).unwrap().with_docs(true);
        let p = Path::new;
        for (path, may) in [
            ("a.txt", false),
            ("src/main.rs", false),
            ("r.pdf", true),
            ("R.PDF", true),
            ("app.db", true),
            ("Makefile", true),
            ("bin/ls", true),
        ] {
            assert_eq!(d.may_be_document(p(path)), may, "{path}");
        }
        assert!(!Detector::new(&r, &[]).unwrap().may_be_document(p("r.pdf")));
        let forced = Detector::new(&r, &["*.txt=pdf".into()])
            .unwrap()
            .with_docs(true);
        assert!(forced.may_be_document(p("a.txt")));
    }

    /// `detect`'s name and extension signals give the first active Definition, in the
    /// Registry's order, whose names or extensions (in any case) match, with `--docs` off and on.
    #[test]
    fn names_and_extensions_pick_the_first_active_definition() {
        let r = registry();
        let scan = |d: &Detector, path: &str| {
            let p = Path::new(path);
            let file = p.file_name().and_then(|n| n.to_str()).unwrap();
            let ext = p.extension().and_then(|e| e.to_str());
            r.all()
                .iter()
                .find(|f| d.active(f) && f.names.iter().any(|n| n == file))
                .or_else(|| {
                    ext.and_then(|ext| {
                        r.all().iter().find(|f| {
                            d.active(f) && f.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
                        })
                    })
                })
                .map(|f| f.name.clone())
        };
        let paths = [
            "Makefile",
            "a.md",
            "README.MD",
            "x.Rs",
            "r.pdf",
            "R.PDF",
            "t.docx",
            "s.XLSX",
            "app.db",
            "b.epub",
            "c.odt",
            "d.json",
            "e.unknown",
            "f.HtMl",
        ];
        for docs in [false, true] {
            let d = Detector::new(&r, &[]).unwrap().with_docs(docs);
            for path in paths {
                let got = d.detect(Path::new(path), b"").map(|f| f.name.clone());
                if path.contains('.') || scan(&d, path).is_some() {
                    assert_eq!(got, scan(&d, path), "{path} with --docs {docs}");
                }
            }
        }
    }

    /// Code review r6, Fable #1: `detect_by_path` of a name without an extension answers
    /// without walking every Definition, and as `detect` with an empty head would; and
    /// `document_magic` is false only for first bytes no document Format's `detect` finds.
    #[test]
    fn names_without_an_extension_are_decided_without_walking_every_definition() {
        let r = registry();
        let p = Path::new;
        for formats in [
            &[][..],
            &["*.txt=pdf".to_string()],
            &["markdown".to_string()],
        ] {
            for docs in [true, false] {
                let d = Detector::new(&r, formats).unwrap().with_docs(docs);
                for path in [
                    "Makefile", "LICENSE", ".bashrc", "bin/ls", "a.txt", "r.pdf", "x.",
                ] {
                    let slow = match d.detect(p(path), &[]) {
                        Some(def) => Some(Some(def.name.clone())),
                        None if p(path).extension().is_some() => Some(None),
                        None => None,
                    };
                    let fast = d
                        .detect_by_path(p(path))
                        .map(|o| o.map(|def| def.name.clone()));
                    assert_eq!(fast, slow, "{formats:?} {docs} {path}");
                }
            }
        }
        let d = Detector::new(&r, &[]).unwrap().with_docs(true);
        for (head, doc) in [
            (&b"%PDF-1.7\n"[..], true),
            (b"SQLite format 3\0", true),
            (b"\x7fELF\x02\x01", false),
            (b"#!/bin/sh\n", false),
            (b"plain text\n", false),
            (b"", false),
        ] {
            assert_eq!(d.document_magic(head), doc, "{head:?}");
            let found = d.detect(p("noext"), head);
            assert_eq!(
                found.is_some_and(|f| f.processor == "docs"),
                doc,
                "{head:?}"
            );
        }
        let forced = Detector::new(&r, &["pdf".into()]).unwrap().with_docs(true);
        assert!(forced.document_magic(b"plain text\n"));
    }

    fn known(p: &str) -> bool {
        crate::processors::Processors::known(p)
    }

    /// The built-ins plus `bashy` (shebang), `myfy` (magic) and `makey` (exact name). `myfy`'s
    /// magic is not ELF's, which the built-in `elf` claims since M4.
    fn registry() -> Registry {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("bashy.toml"),
            "name='bashy'\nshebangs=['fish']\nprocessor='markdown'",
        )
        .unwrap();
        std::fs::write(
            d.path().join("myfy.toml"),
            "name='myfy'\nmagic=[{hex='7f4d5946'}]\nprocessor='markdown'",
        )
        .unwrap();
        std::fs::write(
            d.path().join("makey.toml"),
            "name='makey'\nnames=['Makefile']\nprocessor='markdown'",
        )
        .unwrap();
        let (reg, warnings) = Registry::load(Some(d.path()), &known);
        assert!(warnings.is_empty(), "{warnings:?}");
        reg
    }

    #[test]
    fn signals_in_order() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        assert_eq!(
            det.detect(Path::new("a/README.MD"), b"").unwrap().name,
            "markdown"
        );
        assert_eq!(
            det.detect(Path::new("Makefile"), b"").unwrap().name,
            "makey"
        );
        assert_eq!(
            det.detect(Path::new("prog"), b"\x7fMYF\x02").unwrap().name,
            "myfy"
        );
        assert_eq!(
            det.detect(Path::new("run"), b"#!/usr/bin/env -S fish -e\n")
                .unwrap()
                .name,
            "bashy"
        );
        assert!(det.detect(Path::new("data.bin"), b"\x00\x01").is_none());
    }

    #[test]
    fn the_path_alone_decides_unless_a_format_reads_the_content() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        let name = |p: &str| det.detect_by_path(Path::new(p)).map(|d| d.map(|d| &d.name));
        assert_eq!(name("a.md"), Some(Some(&"markdown".to_string())));
        assert_eq!(name("Makefile"), Some(Some(&"makey".to_string())));
        // myfy (magic) and bashy (shebang) need the first bytes of a name without extension.
        assert_eq!(name("data"), None);
        let (builtin, _) = Registry::load(None, &known);
        let det = Detector::new(&builtin, &[]).unwrap();
        assert_eq!(det.detect_by_path(Path::new("data.txt")), Some(None));
        let det = Detector::new(&builtin, &["markdown".into()]).unwrap();
        assert!(det.detect_by_path(Path::new("data.txt")).unwrap().is_some());
    }

    #[test]
    fn content_signals_need_a_file_without_extension() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        // A shebang or magic bytes decide only when the name has no extension (M2 plan, D15).
        assert_eq!(
            det.detect(Path::new("run"), b"#!/usr/bin/env fish\n")
                .unwrap()
                .name,
            "bashy"
        );
        assert!(
            det.detect(Path::new("run.txt"), b"#!/usr/bin/env fish\n")
                .is_none()
        );
        assert!(det.detect(Path::new("prog.bin"), b"\x7fMYF\x02").is_none());
        assert_eq!(
            det.detect(Path::new(".bashrc_like"), b"\x7fMYF")
                .unwrap()
                .name,
            "myfy"
        );
        // So a file with an unclaimed extension is Plain from its path alone.
        assert_eq!(det.detect_by_path(Path::new("data.txt")), Some(None));
        assert_eq!(det.detect_by_path(Path::new("data")), None);
    }

    #[test]
    fn earlier_signals_beat_later_ones() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        // Exact name beats extension, extension beats magic, magic beats shebang.
        assert_eq!(
            det.detect(Path::new("Makefile"), b"\x7fMYF").unwrap().name,
            "makey"
        );
        assert_eq!(
            det.detect(Path::new("x.md"), b"\x7fMYF").unwrap().name,
            "markdown"
        );
        assert_eq!(
            det.detect(Path::new("x.md"), b"#!/bin/bash\n")
                .unwrap()
                .name,
            "markdown"
        );
    }

    #[test]
    fn overrides_win_and_unknown_is_an_error() {
        let reg = registry();
        let det = Detector::new(&reg, &["*.txt=markdown".into()]).unwrap();
        assert_eq!(
            det.detect(Path::new("notes.txt"), b"").unwrap().name,
            "markdown"
        );
        assert!(det.detect(Path::new("notes.log"), b"").is_none());
        assert!(Detector::new(&reg, &["nope".into()]).is_err());
        assert!(Detector::new(&reg, &["*.txt=nope".into()]).is_err());
    }

    #[test]
    fn a_bare_name_applies_to_every_file() {
        let reg = registry();
        let det = Detector::new(&reg, &["makey".into()]).unwrap();
        assert_eq!(det.detect(Path::new("x.md"), b"").unwrap().name, "makey");
        assert_eq!(
            det.detect(Path::new("data.bin"), b"\x7fMYF").unwrap().name,
            "makey"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_file_name_still_reaches_magic_and_shebang() {
        use std::os::unix::ffi::OsStrExt;
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        let name = std::ffi::OsStr::from_bytes(b"\xff\xfe");
        assert_eq!(
            det.detect(Path::new(name), b"\x7fMYF").unwrap().name,
            "myfy"
        );
        assert_eq!(
            det.detect(Path::new(name), b"#!/usr/bin/fish\n")
                .unwrap()
                .name,
            "bashy"
        );
    }

    #[test]
    fn shebang_interpreters() {
        assert_eq!(shebang_interpreter(b"#!/bin/sh\necho"), Some("sh"));
        assert_eq!(
            shebang_interpreter(b"#!/usr/bin/env bash -e\n"),
            Some("bash")
        );
        assert_eq!(shebang_interpreter(b"#! /usr/bin/python3"), Some("python3"));
        assert_eq!(shebang_interpreter(b"#!/usr/bin/env\n"), None);
        assert_eq!(shebang_interpreter(b"# not a shebang"), None);
        assert_eq!(shebang_interpreter(b""), None);
    }
}
