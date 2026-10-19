//! `--hyperlink-format` in the Trail layouts (M5a plan, D12): a path, and a line number, as an
//! OSC 8 link on a terminal with colour, the URL built as grep-printer builds it (chunk 4).
use std::path::Path;

/// The run's hyperlink format with the values it reads from the system.
#[derive(Debug)]
pub struct Hyperlinks {}

impl Hyperlinks {
    /// None when the format is empty: no links.
    pub fn new(_format: &str, _hostname_bin: Option<&Path>) -> Option<Hyperlinks> {
        None
    }

    /// The URL of `path` at `line` and `column`, or None when the path cannot be made absolute.
    pub fn url(&self, _path: &Path, _line: Option<u64>, _column: Option<u64>) -> Option<Vec<u8>> {
        None
    }
}
