//! pre-launcher.rs: a native `--pre` command that runs a Python converter, for systems where
//! `--pre` cannot name a script (Windows has no shebang lines). Part of the worked example of
//! docs/external-processors.md.
//!
//! It runs `$SEARCHFOR_EXAMPLE_PYTHON $SEARCHFOR_EXAMPLE_CONVERTER PATH`, where PATH is its own
//! only argument (the path s4 passes to every `--pre` command), lets the converter inherit
//! stdin, stdout and stderr, and exits with the converter's exit status.
//!
//! Build it with the Rust compiler alone, no Cargo project needed:
//!
//! ```text
//! rustc docs/examples/pre-launcher.rs -o ini-pre-launcher
//! ```

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let var = |name: &str| match std::env::var_os(name) {
        Some(v) if !v.is_empty() => Ok(v),
        _ => Err(format!("pre-launcher: set {name}")),
    };
    let (python, converter) = match (
        var("SEARCHFOR_EXAMPLE_PYTHON"),
        var("SEARCHFOR_EXAMPLE_CONVERTER"),
    ) {
        (Ok(p), Ok(c)) => (p, c),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let mut args = std::env::args_os().skip(1);
    let (Some(path), None) = (args.next(), args.next()) else {
        eprintln!("pre-launcher: usage: pre-launcher PATH");
        return ExitCode::from(2);
    };
    match Command::new(&python).arg(&converter).arg(&path).status() {
        // The converter's own status, all 32 bits of it on Windows. A converter killed by a
        // signal has no code: report a failure.
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("pre-launcher: cannot run {}: {e}", python.to_string_lossy());
            ExitCode::from(2)
        }
    }
}
