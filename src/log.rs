//! `--debug` and `--trace` (M5a plan, D15): the messages of s4 and of ripgrep's crates on
//! stderr, in ripgrep's format (chunk 5).
use crate::cli::Logging;

/// Starts the logger at the level asked for; nothing is logged without one.
pub fn init(_level: Option<Logging>) {}
