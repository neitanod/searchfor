//! Shared helpers for the end-to-end and ripgrep compatibility tests: the fixture copy and
//! the launchers (plan, Chunk 9, «The fixture copy»).
#![allow(dead_code)]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A copy of `tests/fixtures/` (dotfiles included) with an empty `.git` at its root, so that
/// `.gitignore` files apply to rg and sfor alike, and an empty formats directory for
/// `SFOR_FORMATS_DIR`.
pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub formats: tempfile::TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        copy_dir(&source_dir(), dir.path());
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        Self {
            dir,
            formats: tempfile::tempdir().unwrap(),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn file(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    /// sfor in the fixture copy, stdin from /dev/null, `--color never` unless `args` sets it.
    pub fn sfor<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut c = Command::new(assert_cmd::cargo::cargo_bin("sfor"));
        c.current_dir(self.path())
            .env("SFOR_FORMATS_DIR", self.formats.path())
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null());
        let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        if !args
            .iter()
            .any(|a| a.to_string_lossy().starts_with("--color"))
        {
            c.arg("--color").arg("never");
        }
        c.args(args);
        c
    }

    /// rg 14.1.1 in the fixture copy, stdin from /dev/null, `--color never` unless set.
    pub fn rg<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut c = Command::new("rg");
        c.current_dir(self.path())
            .env_remove("RIPGREP_CONFIG_PATH")
            .stdin(Stdio::null());
        let args: Vec<_> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
        if !args
            .iter()
            .any(|a| a.to_string_lossy().starts_with("--color"))
        {
            c.arg("--color").arg("never");
        }
        c.args(args);
        c
    }
}

pub fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), &target).unwrap();
        }
    }
}

/// True when the reference rg 14.1.1 is installed; prints a note otherwise.
pub fn reference_rg() -> bool {
    let ok = Command::new("rg")
        .arg("--version")
        .env_remove("RIPGREP_CONFIG_PATH")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).starts_with("ripgrep 14.1.1"));
    if !ok {
        eprintln!("skipping: reference rg 14.1.1 not found");
    }
    ok
}

pub fn run(c: &mut Command) -> Output {
    c.output().unwrap()
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

pub fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}
