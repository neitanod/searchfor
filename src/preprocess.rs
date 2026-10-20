//! What a file is searched through (M4 plan, design note 3): its own bytes, the output of
//! ripgrep's `--pre` command, its decompressor's output with `-z`, or the text an Extractor
//! takes from it with `--docs`. `--pre` and `-z` behave as in ripgrep 14.1.1 (crates/core/
//! search.rs): `--pre` first, when its globs allow it, then `-z`; stdin is never transformed.
//! `--docs` comes last, so a file `--pre` or `-z` takes is never extracted (M4 plan, D21).
//!
//! Every command starts through `platform::spawn` (M5b plan, D21): grep-cli's readers spawn
//! inside the crate, where s4 cannot put the child in its Job Object before it runs, so
//! `ManagedCommandReader` and `Decompressors` reproduce grep-cli 0.1.11's `CommandReader` and
//! `DecompressionMatcher` around s4's own launcher.
use crate::platform::{self, Input, Limits, Output, PipeReader, SpawnSpec};
use std::ffi::OsString;
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
    /// `--pre`'s program, resolved as rg resolves it (`grep_cli::resolve_binary`): on Windows
    /// it is searched in `PATH` only, never in the current directory (M5b spec §4.2).
    pre: Option<PathBuf>,
    /// `--pre-glob`: None when no glob was given (every file goes through `--pre`).
    globs: Option<ignore::overrides::Override>,
    zip: Option<Decompressors>,
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
        let pre = match &args.pre {
            Some(p) => Some(grep_cli::resolve_binary(Path::new(p))?),
            None => None,
        };
        Ok(Preprocess {
            pre,
            globs,
            zip: args.search_zip.then(Decompressors::new),
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
            && z.has_command(path)
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
                let file = std::fs::File::open(path)?;
                // Built only for the text rg prints, `{cmd:?}`; never spawned.
                let mut cmd = std::process::Command::new(bin);
                cmd.arg(path);
                let shown = format!("{cmd:?}");
                let mut spec = SpawnSpec::new(bin.as_os_str());
                spec.args = vec![path.as_os_str().to_owned()];
                spec.stdin = Input::File(file);
                let reader = ManagedCommandReader::start(spec, true).map_err(|e| {
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
                    inner: Inner::Zip(z.open(path)?),
                    cmd: None,
                })
            }
            Through::Docs(..) => unreachable!("documents are extracted, not read"),
        }
    }
}

/// grep-cli 0.1.11's `CommandError`: an I/O error, or the command's stderr after it failed.
#[derive(Debug)]
enum CommandError {
    Io(std::io::Error),
    Stderr(Vec<u8>),
}

impl CommandError {
    fn is_empty(&self) -> bool {
        matches!(self, CommandError::Stderr(b) if b.is_empty())
    }
}

impl std::error::Error for CommandError {}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Io(e) => e.fmt(f),
            CommandError::Stderr(bytes) => {
                let msg = String::from_utf8_lossy(bytes);
                if msg.trim().is_empty() {
                    write!(f, "<stderr is empty>")
                } else {
                    let div = "-".repeat(79);
                    write!(f, "\n{div}\n{}\n{div}", msg.trim())
                }
            }
        }
    }
}

impl From<CommandError> for std::io::Error {
    fn from(e: CommandError) -> Self {
        match e {
            CommandError::Io(e) => e,
            e @ CommandError::Stderr(_) => std::io::Error::other(e),
        }
    }
}

fn stderr_to_command_error(stderr: &mut PipeReader) -> CommandError {
    let mut bytes = Vec::new();
    match stderr.read_to_end(&mut bytes) {
        Ok(_) => CommandError::Stderr(bytes),
        Err(e) => CommandError::Io(e),
    }
}

/// The command's stderr, read by a thread of its own (`async_stderr`) or only once stdout is
/// done.
enum StderrReader {
    Async(Option<std::thread::JoinHandle<CommandError>>),
    Sync(PipeReader),
}

impl StderrReader {
    fn read_to_end(&mut self) -> CommandError {
        match self {
            StderrReader::Async(handle) => handle
                .take()
                .expect("stderr is read once")
                .join()
                .expect("the stderr thread does not panic"),
            StderrReader::Sync(stderr) => stderr_to_command_error(stderr),
        }
    }
}

/// grep-cli 0.1.11's `CommandReader` over a child started by `platform::spawn`: its stdout
/// is read as a stream, and a failing status becomes an error holding its stderr.
pub(crate) struct ManagedCommandReader {
    child: platform::Child,
    stdout: Option<PipeReader>,
    stderr: StderrReader,
    /// Set once `read` returned zero bytes. A reader closed before that expects the broken pipe
    /// it caused, and an empty stderr then means success.
    eof: bool,
}

impl ManagedCommandReader {
    /// Sets stdout and stderr to Output::Piped; stdin stays as the caller set it.
    pub(crate) fn start(mut spec: SpawnSpec, async_stderr: bool) -> std::io::Result<Self> {
        spec.stdout = Output::Piped;
        spec.stderr = Output::Piped;
        let mut child = platform::spawn(spec, Limits::default())?;
        let stdout = child.stdout.take();
        let mut err = child.stderr.take().expect("stderr is piped");
        let stderr = if async_stderr {
            StderrReader::Async(Some(std::thread::spawn(move || {
                stderr_to_command_error(&mut err)
            })))
        } else {
            StderrReader::Sync(err)
        };
        Ok(ManagedCommandReader {
            child,
            stdout,
            stderr,
            eof: false,
        })
    }

    /// grep-cli 0.1.11 `CommandReader::close`; idempotent.
    pub(crate) fn close(&mut self) -> std::io::Result<()> {
        // Dropping stdout makes a well-behaved child end; None means already closed.
        let Some(stdout) = self.stdout.take() else {
            return Ok(());
        };
        drop(stdout);
        if self.child.wait()?.success() {
            return Ok(());
        }
        let err = self.stderr.read_to_end();
        if !self.eof && err.is_empty() {
            return Ok(());
        }
        Err(err.into())
    }
}

impl Drop for ManagedCommandReader {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::warn!("{e}");
        }
    }
}

impl Read for ManagedCommandReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(stdout) = self.stdout.as_mut() else {
            return Ok(0);
        };
        let n = stdout.read(buf)?;
        if n == 0 {
            self.eof = true;
            self.close().map(|_| 0)
        } else {
            Ok(n)
        }
    }
}

/// The default rules of grep-cli 0.1.11 (`default_decompression_commands`), in its order: a
/// glob, then the program and its arguments.
const DEFAULT_RULES: [(&str, &[&str]); 12] = [
    ("*.gz", &["gzip", "-d", "-c"]),
    ("*.tgz", &["gzip", "-d", "-c"]),
    ("*.bz2", &["bzip2", "-d", "-c"]),
    ("*.tbz2", &["bzip2", "-d", "-c"]),
    ("*.xz", &["xz", "-d", "-c"]),
    ("*.txz", &["xz", "-d", "-c"]),
    ("*.lz4", &["lz4", "-d", "-c"]),
    ("*.lzma", &["xz", "--format=lzma", "-d", "-c"]),
    ("*.br", &["brotli", "-d", "-c"]),
    ("*.zst", &["zstd", "-q", "-d", "-c"]),
    ("*.zstd", &["zstd", "-q", "-d", "-c"]),
    ("*.Z", &["uncompress", "-c"]),
];

/// -z: the choice of command, replicated from grep-cli 0.1.11 `DecompressionMatcher`.
pub(crate) struct Decompressors {
    globs: globset::GlobSet,
    // Same index as `globs`: the program resolved with grep_cli::resolve_binary, and its arguments.
    commands: Vec<(OsString, Vec<OsString>)>,
}

impl Decompressors {
    /// The twelve default rules of grep-cli 0.1.11, in its order; a rule whose program
    /// does not resolve is left out, as `add` does there.
    pub(crate) fn new() -> Self {
        Self::build(
            DEFAULT_RULES.iter().filter_map(|(glob, argv)| {
                match grep_cli::resolve_binary(Path::new(argv[0])) {
                    Ok(bin) => Some((
                        *glob,
                        bin.into_os_string(),
                        argv[1..].iter().map(OsString::from).collect(),
                    )),
                    Err(e) => {
                        log::debug!("{e}");
                        None
                    }
                }
            }),
        )
    }

    #[cfg(test)]
    pub(crate) fn from_rules(rules: &[(&str, PathBuf, Vec<OsString>)]) -> Self {
        Self::build(
            rules
                .iter()
                .map(|(g, bin, args)| (*g, bin.clone().into_os_string(), args.clone())),
        )
    }

    fn build<'a>(rules: impl Iterator<Item = (&'a str, OsString, Vec<OsString>)>) -> Self {
        let mut b = globset::GlobSetBuilder::new();
        let mut commands = Vec::new();
        for (glob, bin, args) in rules {
            b.add(globset::Glob::new(glob).expect("the decompression globs compile"));
            commands.push((bin, args));
        }
        Decompressors {
            globs: b.build().expect("the decompression globs compile"),
            commands,
        }
    }

    pub(crate) fn has_command(&self, path: &Path) -> bool {
        self.globs.is_match(path)
    }

    /// The last matching glob wins. argv is the rule's arguments plus `path`; stdin is
    /// `Input::Inherit`, as grep-cli leaves it.
    pub(crate) fn spec_for(&self, path: &Path) -> Option<SpawnSpec> {
        let i = *self.globs.matches(path).last()?;
        let (bin, args) = &self.commands[i];
        let mut spec = SpawnSpec::new(bin.clone());
        spec.args = args.clone();
        spec.args.push(path.as_os_str().to_owned());
        spec.stdin = Input::Inherit;
        Some(spec)
    }

    /// grep-cli's `DecompressionReaderBuilder::build`: no rule, or a spawn that fails,
    /// reads the raw file (`File::open`'s error if that fails).
    pub(crate) fn open(&self, path: &Path) -> std::io::Result<Decompression> {
        if let Some(spec) = self.spec_for(path) {
            let shown = format!("{:?} {:?}", spec.program, spec.args);
            match ManagedCommandReader::start(spec, true) {
                Ok(r) => return Ok(Decompression::Command(r)),
                Err(e) => log::debug!(
                    "{}: error spawning command '{shown}': {e} \
                     (falling back to uncompressed reader)",
                    path.display(),
                ),
            }
        }
        Ok(Decompression::Raw(std::fs::File::open(path)?))
    }
}

/// A file read with `-z`: its decompressor's output, or its own bytes.
pub(crate) enum Decompression {
    Command(ManagedCommandReader),
    Raw(std::fs::File),
}

impl Decompression {
    fn close(&mut self) -> std::io::Result<()> {
        match self {
            Decompression::Command(r) => r.close(),
            Decompression::Raw(_) => Ok(()),
        }
    }
}

impl Read for Decompression {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Decompression::Command(r) => r.read(buf),
            Decompression::Raw(f) => f.read(buf),
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
    Pre(ManagedCommandReader),
    Zip(Decompression),
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

    #[test]
    fn pre_resolution_matches_the_library_reference() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing preprocessor");
        let argv = vec![
            "s4".into(),
            "--pre".into(),
            missing.clone().into_os_string(),
            "needle".into(),
        ];
        let args = crate::cli::parse(argv).unwrap();
        let expected = grep_cli::resolve_binary(&missing).unwrap();
        let got = Preprocess::new(&args, dir.path()).unwrap();
        assert_eq!(got.pre.as_deref(), Some(expected.as_path()));
        let input = dir.path().join("input");
        std::fs::write(&input, "needle").unwrap();
        assert!(got.open(&Through::Pre, &input).is_err());
    }

    #[test]
    fn zip_choice_matches_grep_cli_for_every_default_rule() {
        let reference = grep_cli::DecompressionMatcher::new();
        let ours = Decompressors::new();
        for name in [
            "a.gz",
            "a.tgz",
            "a.bz2",
            "a.tbz2",
            "a.xz",
            "a.txz",
            "a.lz4",
            "a.lzma",
            "a.br",
            "a.zst",
            "a.zstd",
            "a.Z",
            "a.z",
            "a.GZ",
            "a.txt",
            "a.gz.txt",
            "dir.gz/a.txt",
            "gz",
        ] {
            let p = Path::new(name);
            assert_eq!(ours.has_command(p), reference.has_command(p), "{name}");
            let want = reference.command(p).map(|c| {
                (
                    c.get_program().to_owned(),
                    c.get_args().map(|a| a.to_owned()).collect::<Vec<_>>(),
                )
            });
            let got = ours.spec_for(p).map(|mut s| {
                // The last argument is the path itself.
                assert_eq!(s.args.pop().as_deref(), Some(p.as_os_str()), "{name}");
                (s.program, s.args)
            });
            assert_eq!(got, want, "{name}");
        }
    }

    #[test]
    fn the_last_matching_zip_rule_wins() {
        let z = Decompressors::from_rules(&[
            ("*.gz", "first".into(), vec![]),
            ("a.*", "second".into(), vec!["-x".into()]),
        ]);
        let s = z.spec_for(Path::new("a.gz")).unwrap();
        assert_eq!(s.program, std::ffi::OsString::from("second"));
        assert_eq!(s.args, vec![std::ffi::OsString::from("-x"), "a.gz".into()]);
    }

    #[cfg(unix)]
    #[test]
    fn zip_falls_back_to_the_raw_file_when_the_decompressor_cannot_start() {
        let d = tempfile::tempdir().unwrap();
        // Present but not executable: the rule exists, the spawn fails.
        let broken = d.path().join("gzip");
        std::fs::write(&broken, "#!/bin/sh\n").unwrap();
        let f = d.path().join("a.txt.gz");
        std::fs::write(&f, "plain bytes\n").unwrap();
        let z = Decompressors::from_rules(&[("*.gz", broken, vec!["-d".into(), "-c".into()])]);
        let mut out = String::new();
        z.open(&f).unwrap().read_to_string(&mut out).unwrap();
        assert_eq!(out, "plain bytes\n");
        // When the raw file cannot be opened either, File::open's error is the result.
        assert!(z.open(&d.path().join("missing.gz")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn managed_reader_reports_stderr_as_grep_cli_does() {
        let d = tempfile::tempdir().unwrap();
        let fail = script(d.path(), "fail.sh", "echo out\necho boom >&2\nexit 3");
        let mut r = start_busy(&fail);
        let mut out = Vec::new();
        let e = r.read_to_end(&mut out).unwrap_err().to_string();
        assert_eq!(out, b"out\n");
        // CommandError's text: the trimmed stderr between two 79-dash lines.
        let rule = "-".repeat(79);
        assert_eq!(e, format!("\n{rule}\nboom\n{rule}"));
        // An empty stderr with a non-zero status, read to the end, is "<stderr is empty>".
        let quiet = script(d.path(), "quiet.sh", "exit 4");
        let mut r = start_busy(&quiet);
        let e = r.read_to_end(&mut Vec::new()).unwrap_err().to_string();
        assert_eq!(e, "<stderr is empty>");
    }

    /// `ManagedCommandReader::start` of `program`, tried again while it is busy (ETXTBSY), as
    /// `open_busy`.
    #[cfg(unix)]
    fn start_busy(program: &Path) -> ManagedCommandReader {
        for _ in 0..50 {
            let spec = platform::SpawnSpec::new(program.as_os_str());
            match ManagedCommandReader::start(spec, true) {
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                r => return r.unwrap(),
            }
        }
        panic!("the script stayed busy for a second");
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "isolated child for Windows pre resolution"]
    fn windows_pre_resolution_probe() {
        let args = crate::cli::parse(vec![
            "s4".into(),
            "--pre".into(),
            "s4-fake-pre.exe".into(),
            "needle".into(),
        ])
        .unwrap();
        assert!(Preprocess::new(&args, &std::env::current_dir().unwrap()).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn windows_pre_bare_name_does_not_search_cwd() {
        let d = tempfile::tempdir().unwrap();
        let exe = std::env::current_exe().unwrap();
        std::fs::copy(&exe, d.path().join("s4-fake-pre.exe")).unwrap();
        let status = std::process::Command::new(exe)
            .args([
                "--exact",
                "preprocess::tests::windows_pre_resolution_probe",
                "--ignored",
            ])
            .current_dir(d.path())
            .env("PATH", d.path().join("empty-path"))
            .status()
            .unwrap();
        assert!(status.success());
    }
}
