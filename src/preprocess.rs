//! What a file is searched through (M4 plan, design note 3): its own bytes, the output of
//! ripgrep's `--pre` command, its decompressor's output with `-z`, or the text an Extractor
//! takes from it with `--docs`. `--pre` and `-z` behave as in ripgrep 14.1.1 (crates/core/
//! search.rs): `--pre` first, when its globs allow it, then `-z`; stdin is never transformed.
//! `--docs` comes last, so a file `--pre` or `-z` takes is never extracted (M4 plan, D21).
use grep_cli::{
    CommandReader, CommandReaderBuilder, DecompressionReader, DecompressionReaderBuilder,
};
use std::io::Read;
use std::path::{Path, PathBuf};

/// How one file is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Through {
    /// The output of `--pre COMMAND PATH`.
    Pre,
    /// The output of the decompressor `-z` picks for the file's extension.
    Zip,
    /// The text of an Extractor: the document Format's name and its Extractor's (`extractor` in
    /// its Definition, a `docs::Kind` name).
    Docs(String, String),
}

/// The run's `--pre` and `-z`.
pub struct Preprocess {
    pre: Option<PathBuf>,
    /// `--pre-glob`: None when no glob was given (every file goes through `--pre`).
    globs: Option<ignore::overrides::Override>,
    zip: Option<DecompressionReaderBuilder>,
    command: CommandReaderBuilder,
}

impl Preprocess {
    pub fn new(args: &crate::cli::Args, cwd: &Path) -> anyhow::Result<Self> {
        let globs = if args.pre_globs.is_empty() {
            None
        } else {
            let mut b = ignore::overrides::OverrideBuilder::new(cwd);
            for g in &args.pre_globs {
                b.add(g)?;
            }
            Some(b.build()?)
        };
        let mut command = CommandReaderBuilder::new();
        command.async_stderr(true);
        let zip = args.search_zip.then(|| {
            let mut z = DecompressionReaderBuilder::new();
            z.async_stderr(true);
            z
        });
        Ok(Preprocess {
            pre: args.pre.clone().map(PathBuf::from),
            globs,
            zip,
            command,
        })
    }

    /// How the file at `path` is read, when not as it is. Never for stdin.
    pub fn through(&self, path: &Path) -> Option<Through> {
        if self.pre.is_some()
            && self
                .globs
                .as_ref()
                .is_none_or(|g| !g.matched(path, false).is_ignore())
        {
            return Some(Through::Pre);
        }
        if let Some(z) = &self.zip
            && z.get_matcher().has_command(path)
        {
            return Some(Through::Zip);
        }
        None
    }

    /// The reader of a file `through` says to transform (`Pre` or `Zip`). Its errors are
    /// ripgrep's: `preprocessor command could not start: '<command>': <error>` and, while it is
    /// read, `preprocessor command failed: '<command>': <error and stderr>`.
    pub fn open(&self, how: &Through, path: &Path) -> std::io::Result<Transformed> {
        match how {
            Through::Pre => {
                let bin = self.pre.as_deref().expect("Through::Pre needs --pre");
                let mut cmd = std::process::Command::new(bin);
                cmd.arg(path)
                    .stdin(std::process::Stdio::from(std::fs::File::open(path)?));
                let shown = format!("{cmd:?}");
                let reader = self.command.build(&mut cmd).map_err(|e| {
                    std::io::Error::other(format!(
                        "preprocessor command could not start: '{shown}': {e}"
                    ))
                })?;
                Ok(Transformed {
                    inner: Inner::Pre(reader),
                    cmd: Some(shown),
                })
            }
            Through::Zip => {
                let z = self.zip.as_ref().expect("Through::Zip needs -z");
                Ok(Transformed {
                    inner: Inner::Zip(z.build(path).map_err(std::io::Error::from)?),
                    cmd: None,
                })
            }
            Through::Docs(..) => unreachable!("documents are extracted, not read"),
        }
    }
}

/// The output of `--pre` or of a decompressor. A command's failure comes when its output is
/// closed: at its end, as an error of the read that reaches it, or from `close`, which the
/// search calls after it stops, as ripgrep 14.1.1 (crates/core/search.rs, `search_preprocessor`
/// and `search_decompress`; code review r1, Codex #2).
pub struct Transformed {
    inner: Inner,
    /// `--pre`'s command as its errors name it while it is read; None for a decompressor, whose
    /// errors rg gives as they are, and once the search is done (`rest`).
    cmd: Option<String>,
}

enum Inner {
    Pre(CommandReader),
    Zip(DecompressionReader),
}

impl Transformed {
    /// Waits for the command, and returns its failure, as ripgrep's `close`: its stderr, without
    /// naming the command. A command whose output was not read to its end and that wrote
    /// nothing on stderr succeeded (grep-cli's rule for the broken pipe it was given).
    pub fn close(&mut self) -> std::io::Result<()> {
        match &mut self.inner {
            Inner::Pre(r) => r.close(),
            Inner::Zip(r) => r.close(),
        }
    }

    /// From now on, what is read is past the search, which ripgrep would have closed here: a
    /// failure at its end is `close`'s, without naming the command.
    pub fn rest(&mut self) {
        self.cmd = None;
    }
}

impl Read for Transformed {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let r = match &mut self.inner {
            Inner::Pre(r) => r.read(buf),
            Inner::Zip(r) => r.read(buf),
        };
        match &self.cmd {
            Some(cmd) => r.map_err(|e| {
                std::io::Error::other(format!("preprocessor command failed: '{cmd}': {e}"))
            }),
            None => r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn pre(args: &[&str]) -> Preprocess {
        let mut v = vec![OsString::from("s4"), OsString::from("x")];
        v.extend(args.iter().map(OsString::from));
        Preprocess::new(&crate::cli::parse(v).unwrap(), Path::new("/")).unwrap()
    }

    #[test]
    fn pre_globs_choose_the_files_and_z_takes_the_rest() {
        let p = pre(&["--pre", "cat", "--pre-glob", "*.pdf"]);
        assert_eq!(p.through(Path::new("/a/b.pdf")), Some(Through::Pre));
        assert_eq!(p.through(Path::new("/a/b.md")), None);
        let p = pre(&["--pre", "cat", "--pre-glob", "!*.pdf"]);
        assert_eq!(p.through(Path::new("/a/b.pdf")), None);
        assert_eq!(p.through(Path::new("/a/b.md")), Some(Through::Pre));
        let p = pre(&["-z"]);
        assert_eq!(p.through(Path::new("/a/b.txt.gz")), Some(Through::Zip));
        assert_eq!(p.through(Path::new("/a/b.txt")), None);
        // The last of -z and --pre wins.
        assert_eq!(
            pre(&["-z", "--pre", "cat"]).through(Path::new("/a.gz")),
            Some(Through::Pre)
        );
        assert_eq!(
            pre(&["--pre", "cat", "-z"]).through(Path::new("/a.gz")),
            Some(Through::Zip)
        );
        assert_eq!(pre(&["--pre", ""]).through(Path::new("/a")), None);
        assert_eq!(
            pre(&["--pre", "cat", "--no-pre"]).through(Path::new("/a")),
            None
        );
    }

    /// An executable `/bin/sh` script with `body`, in the test's own directory.
    #[cfg(unix)]
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// `--pre`'s reader of `file`. Another test thread that forks while a script is open for
    /// writing keeps it busy (ETXTBSY) until its child execs: that alone is tried again.
    #[cfg(unix)]
    fn open_busy(p: &Preprocess, file: &Path) -> Transformed {
        for _ in 0..50 {
            match p.open(&Through::Pre, file) {
                Err(e) if e.to_string().contains("os error 26") => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                r => return r.unwrap(),
            }
        }
        panic!("the script stayed busy for a second");
    }

    #[cfg(unix)]
    #[test]
    fn pre_runs_the_command_with_the_path_and_the_file_on_stdin() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.txt");
        std::fs::write(&f, "abc\n").unwrap();
        // The command is a script of the test's own: it prints its argument, then its stdin.
        let script = script(
            d.path(),
            "show.sh",
            "printf '%s\\n' \"$1\"\nwhile IFS= read -r l; do printf '<%s>\\n' \"$l\"; done",
        );
        let p = pre(&["--pre", script.to_str().unwrap()]);
        let mut out = String::new();
        open_busy(&p, &f).read_to_string(&mut out).unwrap();
        assert_eq!(out, format!("{}\n<abc>\n", f.display()));
        let p = pre(&["--pre", "/nonexistent/cmd"]);
        let e = p.open(&Through::Pre, &f).err().unwrap().to_string();
        assert!(
            e.starts_with("preprocessor command could not start: '\"/nonexistent/cmd\" "),
            "{e}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_pre_command_names_itself_and_its_stderr() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.txt");
        std::fs::write(&f, "abc\n").unwrap();
        let script = script(d.path(), "fail.sh", "echo out\necho boom >&2\nexit 3");
        let p = pre(&["--pre", script.to_str().unwrap()]);
        let mut out = Vec::new();
        let e = open_busy(&p, &f)
            .read_to_end(&mut out)
            .unwrap_err()
            .to_string();
        let shown = format!("preprocessor command failed: '\"{}\" ", script.display());
        assert!(e.starts_with(&shown), "{e}");
        assert!(e.contains("boom"), "{e}");
        assert_eq!(out, b"out\n");
    }

    /// A command read only in part, as by `-q` or `-m`, fails at `close`, with its stderr and
    /// without naming the command, as ripgrep's close (code review r1, Codex #2).
    #[cfg(unix)]
    #[test]
    fn closing_a_command_read_in_part_gives_its_failure() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.txt");
        std::fs::write(&f, "abc\n").unwrap();
        let script = script(d.path(), "late.sh", "cat \"$1\"\necho boom >&2\nexit 3");
        let p = pre(&["--pre", script.to_str().unwrap()]);
        let mut r = open_busy(&p, &f);
        let mut first = [0u8; 2];
        r.read_exact(&mut first).unwrap();
        let e = r.close().unwrap_err().to_string();
        assert!(e.contains("boom"), "{e}");
        assert!(!e.contains("preprocessor command failed"), "{e}");
        // Read past the search (`rest`), its end fails the same way.
        let mut r = open_busy(&p, &f);
        r.rest();
        let e = r.read_to_end(&mut Vec::new()).unwrap_err().to_string();
        assert!(e.contains("boom") && !e.contains("preprocessor"), "{e}");
    }
}
