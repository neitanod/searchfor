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
        let _ = std::fs::remove_file(&self.0);
    }
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
                tempfile::Builder::new()
                    .prefix("sfor-")
                    .tempdir()
                    .map_err(|e| e.to_string())
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
        std::fs::write(&p, bytes)?;
        Ok(TempFile(p))
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
}
