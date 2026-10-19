//! The configuration file (M5a plan, D3): `SEARCHFOR_CONFIG_PATH` names a file with one
//! argument per line, read as ripgrep 14.1.1 reads `RIPGREP_CONFIG_PATH` (crates/core/flags/
//! config.rs): each line trimmed, empty lines and lines starting with `#` skipped, the arguments
//! put before the command line's, so the command line wins. `--no-config` skips it. There is
//! no default location: without the variable nothing is read, so a run never reads a file the
//! user did not name.
use bstr::ByteSlice;
use std::ffi::OsString;
use std::io::BufRead;
use std::path::Path;

/// The variable that names the configuration file.
pub const ENV: &str = "SEARCHFOR_CONFIG_PATH";

/// The arguments of the file `SEARCHFOR_CONFIG_PATH` names, and the messages to print: a file
/// that cannot be read gives one and no argument; a line that is not valid UTF-8 (on Windows)
/// gives one and is skipped, as in rg.
pub fn args() -> (Vec<OsString>, Vec<String>) {
    let Some(path) = std::env::var_os(ENV).filter(|p| !p.is_empty()) else {
        return (Vec::new(), Vec::new());
    };
    let path = Path::new(&path);
    match std::fs::File::open(path) {
        Err(e) => (
            Vec::new(),
            vec![format!(
                "failed to read the file specified in {ENV}: {}: {e}",
                path.display()
            )],
        ),
        Ok(f) => {
            let (args, errs) = parse_reader(std::io::BufReader::new(f));
            let msgs = errs
                .into_iter()
                .map(|e| format!("{}:{e}", path.display()))
                .collect();
            (args, msgs)
        }
    }
}

/// One argument per line: trimmed of Unicode white space as rg trims it (bstr's `trim`, so
/// U+00A0 and U+2003 too), with blank lines and `#` comments skipped. Errors are `line: reason`.
pub fn parse_reader(mut r: impl BufRead) -> (Vec<OsString>, Vec<String>) {
    let (mut args, mut errs) = (Vec::new(), Vec::new());
    let mut line = Vec::new();
    let mut n = 0;
    loop {
        line.clear();
        match r.read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                errs.push(format!("{}: {e}", n + 1));
                break;
            }
        }
        n += 1;
        let t = line.trim();
        if t.is_empty() || t[0] == b'#' {
            continue;
        }
        match os_string(t) {
            Some(a) => args.push(a),
            None => errs.push(format!("{n}: invalid UTF-8")),
        }
    }
    (args, errs)
}

#[cfg(unix)]
fn os_string(b: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;
    Some(OsString::from_vec(b.to_vec()))
}

#[cfg(not(unix))]
fn os_string(b: &[u8]) -> Option<OsString> {
    std::str::from_utf8(b).ok().map(OsString::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ripgrep 14.1.1's own test of its reader (config.rs, `basic`).
    #[test]
    fn lines_trimmed_comments_and_blanks_skipped() {
        let (args, errs) = parse_reader(
            &b"\
# Test
--context=0
   --smart-case
-u


   # --bar
--foo
"[..],
        );
        assert!(errs.is_empty());
        let args: Vec<_> = args.iter().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(args, ["--context=0", "--smart-case", "-u", "--foo"]);
    }

    #[test]
    fn a_last_line_without_terminator_counts() {
        let (args, _) = parse_reader(&b"-i\n--hidden"[..]);
        assert_eq!(args, ["-i", "--hidden"]);
    }
}
