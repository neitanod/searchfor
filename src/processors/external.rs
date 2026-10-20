//! Processors served by a Helper Processor (spec §4.4): `HelperProcessor` over a Host (C9),
//! and the run's temporary files.

/// A Processor whose work is done by a Helper Processor (spec §4.4): built-in to s4's
/// pipeline, it only turns the contract into requests.
pub struct HelperProcessor {
    host: crate::helper::host::Host,
    warn: crate::helper::Warn,
    tmp: std::sync::Arc<RunTemp>,
    /// A Grammar Helper's `max_bytes` (spec §5.3), checked before the request.
    limit: Option<super::treesitter::SizeLimit>,
}

impl HelperProcessor {
    pub fn new(
        cfg: crate::helper::HostConfig,
        launcher: Box<dyn crate::helper::Launcher>,
        tmp: std::sync::Arc<RunTemp>,
        warn: crate::helper::Warn,
    ) -> Self {
        Self {
            host: crate::helper::host::Host::new(cfg, launcher, warn.clone()),
            warn,
            tmp,
            limit: None,
        }
    }

    /// A Grammar Helper: a file over the Definition's `max_bytes` is Plain without a request,
    /// with the compiled-in treesitter's one warning per Format (review m2-code-r1, Codex #2).
    pub fn with_size_limit(mut self) -> Self {
        self.limit = Some(super::treesitter::SizeLimit::default());
        self
    }

    /// The error of a failed request. When the failure disabled the Definition, the file's
    /// warning is printed here, followed by the notice, and the pipeline gets `Unavailable`:
    /// the two warnings come out in order (review m2-code-r1, Fable #5).
    fn failure(&self, input: &crate::model::Input, msg: String) -> anyhow::Error {
        match self.host.take_notice() {
            Some(notice) => {
                (self.warn)(format!("s4: {}: {msg}", input.path.display()));
                (self.warn)(notice);
                crate::helper::Unavailable.into()
            }
            None => anyhow::anyhow!(msg),
        }
    }

    /// Err(`Unavailable`) for a file over the limit, after the warning of the first one.
    fn check_size(&self, input: &crate::model::Input, params: &toml::Table) -> anyhow::Result<()> {
        match &self.limit {
            Some(l) if l.over(input, params)? => Err(crate::helper::Unavailable.into()),
            _ => Ok(()),
        }
    }

    pub fn shutdown(&self) {
        self.host.shutdown();
    }

    /// The path the helper reads, and the temporary file to keep alive until it answered: the
    /// file itself when its bytes are the Source's and its path is UTF-8.
    fn path_for(&self, input: &crate::model::Input) -> anyhow::Result<(String, Option<TempFile>)> {
        if let Some(s) = input.disk.and_then(|p| p.to_str()) {
            return Ok((s.to_string(), None));
        }
        let t = self.tmp.write(input.path, input.bytes)?;
        let s = t
            .path()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("the temporary path is not UTF-8"))?
            .to_string();
        Ok((s, Some(t)))
    }
}

impl super::Processor for HelperProcessor {
    fn trails(
        &self,
        input: &crate::model::Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<crate::model::Trail>> {
        use crate::helper::{
            HelperError,
            protocol::{MatchPos, Request},
        };
        self.check_size(input, params)?;
        let (path, _tmp) = self.path_for(input)?;
        let lines = crate::trail::LineIndex::new(input.bytes);
        let matches = anchors
            .iter()
            .map(|&b| MatchPos {
                line: lines.line_of(b),
                byte: b as u64,
            })
            .collect();
        let f = self.host.format();
        match self
            .host
            .request(|id| Request::Trails { id, path, matches })
        {
            Ok(r) => match r.trails {
                Some(t) if t.len() == anchors.len() => Ok(t),
                Some(t) => anyhow::bail!(
                    "{f}: the helper returned {} Trails for {} Matches",
                    t.len(),
                    anchors.len()
                ),
                None => anyhow::bail!("{f}: the helper's answer has no `trails`"),
            },
            Err(HelperError::Disabled) => Err(crate::helper::Unavailable.into()),
            Err(e) => Err(self.failure(input, format!("{f}: {e}"))),
        }
    }

    fn names(
        &self,
        input: &crate::model::Input,
        params: &toml::Table,
    ) -> anyhow::Result<Vec<crate::model::Name>> {
        use crate::helper::{HelperError, protocol::Request};
        self.check_size(input, params)?;
        let (path, _tmp) = self.path_for(input)?;
        let f = self.host.format();
        match self.host.request(|id| Request::Names { id, path }) {
            Ok(r) => match r.names {
                Some(n) => Ok(n.into_iter().map(|w| w.into_name()).collect()),
                None => anyhow::bail!("{f}: the helper's answer has no `names`"),
            },
            Err(HelperError::Disabled) => Err(crate::helper::Unavailable.into()),
            Err(e) => Err(self.failure(input, format!("{f}: {e}"))),
        }
    }
}

/// The run's temporary directory (spec §4.4, Paths), created on first use and removed with the
/// last `RunTemp` handle.
pub struct RunTemp {
    dir: std::sync::OnceLock<Result<tempfile::TempDir, String>>,
    next: std::sync::atomic::AtomicU64,
}

/// A temporary copy of a Source, removed when dropped (after the helper answered).
pub struct TempFile(std::path::PathBuf);

impl TempFile {
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        remove_retrying(
            &self.0,
            REMOVE_ATTEMPTS,
            REMOVE_PAUSE,
            |p| std::fs::remove_file(p),
            sharing_violation,
        );
    }
}

/// Tries to remove a temporary file this many times, `REMOVE_PAUSE` apart, while a helper
/// that already answered still holds it open (Windows refuses to remove a file opened without
/// delete sharing, as Python and C open files): a file's end waits a quarter second at most.
const REMOVE_ATTEMPTS: u32 = 6;
const REMOVE_PAUSE: std::time::Duration = std::time::Duration::from_millis(50);

/// Removes `path` with `remove`, again after `pause` while the error is one `busy` accepts, at
/// most `attempts` times in all. True when the file is gone; false leaves it where it is, a
/// uniquely named file in the run's directory under the system's temporary directory, for the
/// system to clean.
fn remove_retrying(
    path: &std::path::Path,
    attempts: u32,
    pause: std::time::Duration,
    mut remove: impl FnMut(&std::path::Path) -> std::io::Result<()>,
    busy: impl Fn(&std::io::Error) -> bool,
) -> bool {
    for attempt in 1..=attempts.max(1) {
        match remove(path) {
            Ok(()) => return true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return true,
            Err(e) if busy(&e) && attempt < attempts => std::thread::sleep(pause),
            Err(_) => return false,
        }
    }
    false
}

/// ERROR_SHARING_VIOLATION or ERROR_LOCK_VIOLATION: another process has the file open without
/// sharing its deletion. Never on Unix, where removing an open file succeeds.
fn sharing_violation(e: &std::io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33))
}

impl RunTemp {
    pub fn new() -> Self {
        Self {
            dir: std::sync::OnceLock::new(),
            next: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// A new file holding `bytes`, named `<n>-<file name of name>` so the helper still sees the
    /// extension.
    pub fn write(&self, name: &std::path::Path, bytes: &[u8]) -> std::io::Result<TempFile> {
        let dir = self
            .dir
            .get_or_init(|| {
                let mut b = tempfile::Builder::new();
                b.prefix("searchfor-");
                // What a search reads from stdin or gets from `--pre` lands here: on Unix the
                // directory is created 0700 by mkdir itself, never widened and then narrowed. On
                // Windows the system's temporary directory is already the user's own
                // (%LOCALAPPDATA%\Temp), and what is created in it inherits that.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    b.permissions(std::fs::Permissions::from_mode(0o700));
                }
                b.tempdir().map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|e| std::io::Error::other(e.clone()))?;
        let n = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // `<stdin>` and other names with characters Windows refuses become `_` (M5's Windows).
        let file: String = name
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stdin".into())
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let p = dir.path().join(format!("{n}-{file}"));
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create_new(true);
        // 0600 from open(2) itself, as the directory.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
        let mut f = o.open(&p)?;
        let t = TempFile(p);
        std::io::Write::write_all(&mut f, bytes)?;
        Ok(t)
    }
}

impl Default for RunTemp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helper::host::testing::{Fake, helper};
    use crate::helper::host::{Host, cancel};
    use crate::helper::protocol::{Reply, Request, WireName};
    use crate::helper::{Mode, Unavailable};
    use crate::model::{Input, Segment};
    use crate::processors::Processor;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// A HelperProcessor over a fake helper (`fail`: a command that cannot start), with a
    /// Supervisor of its own, and the warnings it printed.
    fn processor(
        body: crate::helper::host::testing::Body,
        fail: bool,
    ) -> (HelperProcessor, Arc<Mutex<Vec<String>>>) {
        let warnings = Arc::new(Mutex::new(Vec::new()));
        let w = warnings.clone();
        let fake = Fake {
            body,
            launches: Arc::new(AtomicUsize::new(0)),
            reaps: Arc::new(AtomicUsize::new(0)),
            fail,
            hook: None,
            slow_kill: Duration::ZERO,
        };
        let sup: &'static cancel::Supervisor = Box::leak(Box::new(cancel::Supervisor::new(false)));
        let warn: crate::helper::Warn = Arc::new(move |m| w.lock().unwrap().push(m));
        let host = Host::with_supervisor(
            crate::helper::host::testing::cfg(Mode::Pool, 2, 2000),
            Box::new(fake),
            warn.clone(),
            sup,
        );
        let p = HelperProcessor {
            host,
            warn,
            tmp: Arc::new(RunTemp::new()),
            limit: None,
        };
        (p, warnings)
    }

    /// One Trail per Match, with one Segment named `name(request)`.
    fn named(
        name: impl Fn(&str, &[crate::helper::protocol::MatchPos]) -> String + Send + Sync + 'static,
    ) -> crate::helper::host::testing::Body {
        helper(move |r| {
            let Request::Trails { id, path, matches } = r else {
                return None;
            };
            let seg = Segment {
                kind: "key".into(),
                name: name(path, matches),
                line: 1,
                end_line: 1,
                level: None,
                signature: None,
            };
            Some(Reply {
                id: *id,
                trails: Some(vec![vec![seg]; matches.len()]),
                ..Default::default()
            })
        })
    }

    fn input<'a>(bytes: &'a [u8], disk: Option<&'a Path>) -> Input<'a> {
        Input {
            bytes,
            path: Path::new("dir/a.ini"),
            format: "ini",
            disk,
        }
    }

    fn first_name(t: &[crate::model::Trail]) -> &str {
        &t[0][0].name
    }

    #[test]
    fn a_file_on_disk_is_sent_by_its_path() {
        let (p, _) = processor(named(|path, _| path.to_string()), false);
        let src = b"[s]\nk = needle\n";
        let t = p
            .trails(
                &input(src, Some(Path::new("dir/a.ini"))),
                &toml::Table::new(),
                &[8],
            )
            .unwrap();
        assert_eq!(first_name(&t), "dir/a.ini");
        p.shutdown();
    }

    #[test]
    fn bytes_that_differ_from_the_file_go_through_a_temporary_file() {
        let seen: Arc<Mutex<Option<PathBuf>>> = Arc::default();
        let s = seen.clone();
        let (p, _) = processor(
            named(move |path, _| {
                *s.lock().unwrap() = Some(PathBuf::from(path));
                String::from_utf8(std::fs::read(path).unwrap()).unwrap()
            }),
            false,
        );
        let src = b"[s]\nk = needle\n";
        let t = p
            .trails(&input(src, None), &toml::Table::new(), &[8])
            .unwrap();
        assert_eq!(first_name(&t).as_bytes(), src);
        let tmp = seen.lock().unwrap().clone().unwrap();
        assert!(
            tmp.to_string_lossy().ends_with("-a.ini"),
            "{}",
            tmp.display()
        );
        assert!(!tmp.exists(), "{} still exists", tmp.display());
        p.shutdown();
    }

    #[test]
    fn lines_are_computed_from_the_bytes() {
        let (p, _) = processor(
            named(|_, m| {
                m.iter()
                    .map(|m| format!("{}@{}", m.line, m.byte))
                    .collect::<Vec<_>>()
                    .join(",")
            }),
            false,
        );
        let src = b"[s]\nk=1\nneedle\n";
        let t = p
            .trails(
                &input(src, Some(Path::new("dir/a.ini"))),
                &toml::Table::new(),
                &[0, 4, 9],
            )
            .unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(first_name(&t), "1@0,2@4,3@9");
        p.shutdown();
    }

    #[test]
    fn a_wrong_number_of_trails_is_an_error() {
        let (p, _) = processor(
            helper(|r| {
                let Request::Trails { id, .. } = r else {
                    return None;
                };
                Some(Reply {
                    id: *id,
                    trails: Some(vec![Vec::new()]),
                    ..Default::default()
                })
            }),
            false,
        );
        let e = p
            .trails(
                &input(b"a\nb\n", Some(Path::new("dir/a.ini"))),
                &toml::Table::new(),
                &[0, 2],
            )
            .unwrap_err()
            .to_string();
        assert!(e.contains("ini") && e.contains("2 Matches"), "{e}");
        p.shutdown();
    }

    #[test]
    fn a_disabled_helper_is_unavailable() {
        let (p, warnings) = processor(helper(|_| None), true);
        let i = input(b"a\n", Some(Path::new("dir/a.ini")));
        for _ in 0..2 {
            let e = p.trails(&i, &toml::Table::new(), &[0]).unwrap_err();
            assert!(e.downcast_ref::<Unavailable>().is_some(), "{e}");
            let e = p.names(&i, &toml::Table::new()).unwrap_err();
            assert!(e.downcast_ref::<Unavailable>().is_some(), "{e}");
        }
        let w = warnings.lock().unwrap();
        assert_eq!(w.len(), 1, "{w:?}");
        p.shutdown();
    }

    #[test]
    fn names_come_back_as_model_names() {
        let seg = Segment {
            kind: "section".into(),
            name: "s".into(),
            line: 1,
            end_line: 2,
            level: None,
            signature: None,
        };
        let wire = WireName {
            text: "s".into(),
            pieces: vec![[0, 1, 1, 2]],
            anchor: 1,
            trail: vec![seg.clone()],
        };
        let w = wire.clone();
        let (p, _) = processor(
            helper(move |r| {
                let Request::Names { id, .. } = r else {
                    return None;
                };
                Some(Reply {
                    id: *id,
                    names: Some(vec![w.clone()]),
                    ..Default::default()
                })
            }),
            false,
        );
        let n = p
            .names(
                &input(b"[s]\nk=1\n", Some(Path::new("dir/a.ini"))),
                &toml::Table::new(),
            )
            .unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].text, "s");
        assert_eq!(n[0].pieces, vec![(0..1, 1..2)]);
        assert_eq!(n[0].anchor, 1);
        assert_eq!(n[0].trail, vec![seg]);
        p.shutdown();
    }

    /// A remover that fails with a sharing violation `busy` times, then removes, counting calls.
    fn flaky(
        busy: usize,
        calls: &std::cell::Cell<usize>,
    ) -> impl FnMut(&Path) -> std::io::Result<()> + '_ {
        move |_| {
            calls.set(calls.get() + 1);
            if calls.get() <= busy {
                Err(std::io::Error::other("sharing violation"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn a_busy_temp_file_is_removed_once_its_helper_lets_go() {
        let calls = std::cell::Cell::new(0);
        let pause = Duration::from_millis(10);
        let started = std::time::Instant::now();
        let gone = remove_retrying(Path::new("x"), 8, pause, flaky(3, &calls), |_| true);
        let took = started.elapsed();
        assert!(gone);
        assert_eq!(calls.get(), 4);
        assert!(took >= pause * 3, "{took:?}");
        assert!(took < Duration::from_secs(2), "{took:?}");
    }

    #[test]
    fn a_temp_file_still_busy_is_left_after_bounded_attempts() {
        let calls = std::cell::Cell::new(0);
        let pause = Duration::from_millis(10);
        let started = std::time::Instant::now();
        let gone = remove_retrying(Path::new("x"), 5, pause, flaky(usize::MAX, &calls), |_| {
            true
        });
        let took = started.elapsed();
        assert!(!gone);
        assert_eq!(calls.get(), 5);
        assert!(took >= pause * 4, "{took:?}");
        assert!(took < Duration::from_secs(2), "{took:?}");
        // The real bound: a file a helper keeps open delays its file's end by a quarter second
        // at most.
        assert!(REMOVE_PAUSE * (REMOVE_ATTEMPTS - 1) <= Duration::from_millis(250));
    }

    #[test]
    fn only_a_sharing_violation_is_retried() {
        let calls = std::cell::Cell::new(0);
        let started = std::time::Instant::now();
        let gone = remove_retrying(
            Path::new("x"),
            8,
            Duration::from_secs(1),
            flaky(usize::MAX, &calls),
            |_| false,
        );
        assert!(!gone);
        assert_eq!(calls.get(), 1);
        assert!(started.elapsed() < Duration::from_millis(500));
        // A file already gone is gone.
        let missing = |_: &Path| Err(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(remove_retrying(
            Path::new("x"),
            8,
            Duration::ZERO,
            missing,
            |_| false
        ));
        #[cfg(not(windows))]
        assert!(!sharing_violation(&std::io::Error::from_raw_os_error(32)));
        #[cfg(windows)]
        assert!(sharing_violation(&std::io::Error::from_raw_os_error(32)));
    }

    /// What a search reads from stdin or gets from `--pre` stays the user's own: the run's
    /// directory is 0700 and its files 0600, whatever the umask lets through.
    #[cfg(unix)]
    #[test]
    fn the_run_directory_and_its_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let t = RunTemp::new();
        let f = t.write(Path::new("<stdin>"), b"secret").unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let dir = f.path().parent().unwrap();
        assert_eq!(mode(dir), 0o700, "{}", dir.display());
        assert_eq!(mode(f.path()), 0o600, "{}", f.path().display());
        assert_eq!(std::fs::read(f.path()).unwrap(), b"secret");
        // Each file is new: a name already there is never reused.
        let g = t.write(Path::new("<stdin>"), b"other").unwrap();
        assert_ne!(f.path(), g.path());
        assert_eq!(mode(g.path()), 0o600);
    }

    /// The Windows case itself: a file open without delete sharing, as a helper in Python or C
    /// opens it, is removed once the helper closes it within the retries, and left in the
    /// run's temporary directory when it does not.
    #[cfg(windows)]
    #[test]
    fn a_file_a_helper_holds_open_is_removed_when_it_closes() {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        const FILE_SHARE_WRITE: u32 = 2;
        let t = RunTemp::new();
        let hold = |p: &Path| {
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .open(p)
                .unwrap()
        };
        let f = t.write(Path::new("a.ini"), b"x").unwrap();
        let p = f.path().to_path_buf();
        let held = hold(&p);
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(held);
        });
        let started = std::time::Instant::now();
        drop(f);
        let took = started.elapsed();
        release.join().unwrap();
        assert!(!p.exists());
        assert!(took >= Duration::from_millis(40), "{took:?}");
        assert!(took < Duration::from_secs(2), "{took:?}");
        // Held past the retries: left for the system, after a bounded wait.
        let f = t.write(Path::new("b.ini"), b"x").unwrap();
        let p = f.path().to_path_buf();
        let held = hold(&p);
        let started = std::time::Instant::now();
        drop(f);
        let took = started.elapsed();
        assert!(p.exists());
        assert!(took < Duration::from_secs(2), "{took:?}");
        drop(held);
        std::fs::remove_file(&p).unwrap();
    }
}
