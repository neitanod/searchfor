//! searchfor: ripgrep-compatible search that reports the Trail of every Match.
pub mod app;
pub mod cli;
pub mod docs;
pub mod filter;
pub mod formats;
pub mod grammars;
pub mod helper;
pub mod model;
pub mod output;
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
    let code = app::run(args);
    docs::isolated::finish();
    code
}
