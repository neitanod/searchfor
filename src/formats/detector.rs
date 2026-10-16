//! The Detector (spec §4.1): override, exact name, extension, magic bytes, shebang.
use super::{Registry, definition::FormatDefinition};
use globset::{Glob, GlobMatcher};
use std::path::Path;

enum Override {
    All(String),
    Glob(GlobMatcher, String),
}

pub struct Detector<'r> {
    registry: &'r Registry,
    overrides: Vec<Override>,
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
        Ok(Self {
            registry,
            overrides,
        })
    }

    /// `head` is the start of the file; 512 bytes are enough for every signal.
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
            && let Some(d) = defs.iter().find(|d| d.names.iter().any(|n| n == file_name))
        {
            return Some(d);
        }
        if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && let Some(d) = defs
                .iter()
                .find(|d| d.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
        {
            return Some(d);
        }
        if let Some(d) = defs.iter().find(|d| {
            d.magic_bytes()
                .iter()
                .any(|(off, bytes)| head.get(*off..off + bytes.len()) == Some(bytes.as_slice()))
        }) {
            return Some(d);
        }
        let interp = shebang_interpreter(head)?;
        defs.iter().find(|d| d.shebangs.iter().any(|s| s == interp))
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

    fn known(p: &str) -> bool {
        p == "markdown"
    }

    /// The built-ins plus `bashy` (shebang), `elfy` (magic) and `makey` (exact name).
    fn registry() -> Registry {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("bashy.toml"),
            "name='bashy'\nshebangs=['bash']\nprocessor='markdown'",
        )
        .unwrap();
        std::fs::write(
            d.path().join("elfy.toml"),
            "name='elfy'\nmagic=[{hex='7f454c46'}]\nprocessor='markdown'",
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
            det.detect(Path::new("prog"), b"\x7fELF\x02").unwrap().name,
            "elfy"
        );
        assert_eq!(
            det.detect(Path::new("run"), b"#!/usr/bin/env -S bash -e\n")
                .unwrap()
                .name,
            "bashy"
        );
        assert!(det.detect(Path::new("data.bin"), b"\x00\x01").is_none());
    }

    #[test]
    fn earlier_signals_beat_later_ones() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        // Exact name beats extension, extension beats magic, magic beats shebang.
        assert_eq!(
            det.detect(Path::new("Makefile"), b"\x7fELF").unwrap().name,
            "makey"
        );
        assert_eq!(
            det.detect(Path::new("x.md"), b"\x7fELF").unwrap().name,
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
            det.detect(Path::new("data.bin"), b"\x7fELF").unwrap().name,
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
            det.detect(Path::new(name), b"\x7fELF").unwrap().name,
            "elfy"
        );
        assert_eq!(
            det.detect(Path::new(name), b"#!/bin/bash\n").unwrap().name,
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
