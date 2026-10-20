//! searchfor: ripgrep-compatible search that reports the Trail of every Match.
pub mod app;
pub mod cli;
pub mod docs;
pub mod filter;
pub mod formats;
pub mod grammars;
pub mod helper;
pub mod log;
pub mod matcher;
pub mod mcp;
pub mod model;
pub mod output;
pub mod platform;
pub mod preprocess;
pub mod processors;
pub mod search;
pub mod source;
pub mod text;
pub mod trail;

/// Entry point used by `main`. Returns the process exit code (0, 1 or 2, as ripgrep).
pub fn run(args: Vec<std::ffi::OsString>) -> i32 {
    // The child that extracts a document for --docs (M4 plan, D30).
    if docs::isolated::is_worker(&args) {
        return docs::isolated::child(&args);
    }
    // An MCP search ends with the s4 that started it (M5b spec §4.2), before any other work.
    if let Err(e) = platform::initialize_parent_watch() {
        warn(format!("s4: cannot watch the parent process: {e}"));
        return 2;
    }
    let code = app::run(args);
    docs::isolated::finish();
    code
}

/// Writes one diagnostic line on stderr. A stderr that is closed, or a pipe nobody reads, loses
/// the line and nothing else: `eprintln!` would panic there, and change the exit code or hang
/// the walk (code review r3, Codex #4).
pub(crate) fn warn(msg: impl std::fmt::Display) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{msg}");
}
