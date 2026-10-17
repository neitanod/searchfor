//! Processors served by a Helper Processor (spec §4.4): `HelperProcessor` over a Host (C9),
//! and the run's temporary files.

/// A Processor that asks a Helper Processor through a Host (spec §4.4). C9 fills it.
pub struct HelperProcessor {}

impl HelperProcessor {
    pub fn new(
        _cfg: crate::helper::HostConfig,
        _launcher: Box<dyn crate::helper::Launcher>,
        _tmp: std::sync::Arc<RunTemp>,
        _warn: crate::helper::Warn,
    ) -> Self {
        unimplemented!("C9")
    }

    pub fn shutdown(&self) {}
}

impl super::Processor for HelperProcessor {
    fn trails(
        &self,
        _: &crate::model::Input,
        _: &toml::Table,
        _: &[usize],
    ) -> anyhow::Result<Vec<crate::model::Trail>> {
        anyhow::bail!("Helper Processors are not implemented yet (C9)")
    }

    fn names(
        &self,
        _: &crate::model::Input,
        _: &toml::Table,
    ) -> anyhow::Result<Vec<crate::model::Name>> {
        anyhow::bail!("Helper Processors are not implemented yet (C9)")
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
