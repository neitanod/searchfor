//! What a file is searched through (M4 plan, design note 3). Stub of C1; Chunk 3 writes it.
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Through {
    Pre,
    Zip,
    Docs(String, String),
}

pub struct Preprocess;

impl Preprocess {
    pub fn new(_args: &crate::cli::Args, _cwd: &Path) -> anyhow::Result<Self> {
        Ok(Preprocess)
    }

    pub fn through(&self, _path: &Path) -> Option<Through> {
        None
    }

    pub fn open(&self, _how: &Through, _path: &Path) -> std::io::Result<Box<dyn Read + Send>> {
        Err(std::io::Error::other("--pre and -z are written in Chunk 3"))
    }
}
