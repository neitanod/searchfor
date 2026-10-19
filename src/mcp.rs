//! `s4 --mcp`: an MCP server over stdin and stdout (spec §9; M5a plan, D17; chunk 6).
use std::io::{BufRead, Write};
use std::path::Path;

/// Serves MCP requests from stdin until it closes; `s4` is the binary each search runs.
pub fn serve(_s4: &Path) -> anyhow::Result<()> {
    anyhow::bail!("--mcp is not ready")
}

/// `serve` over any reader and writer, for the tests.
pub fn serve_io(_input: impl BufRead, _output: impl Write, _s4: &Path) -> anyhow::Result<()> {
    anyhow::bail!("--mcp is not ready")
}
