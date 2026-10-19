//! `--generate`: the man page and the shell completions of s4 (M5a plan, D16; chunk 5).
use std::io::Write;

/// Writes what `kind` names (`man`, `complete-bash`, `complete-zsh`, `complete-fish`,
/// `complete-powershell`) to `w`.
pub fn generate(_kind: &str, _w: &mut dyn Write) -> std::io::Result<()> {
    Ok(())
}
