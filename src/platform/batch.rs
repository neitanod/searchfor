//! Batch files on Windows (plan D22): `std::process::Command` runs a `.bat` or `.cmd` through
//! `cmd.exe`, and so does the platform layer, with the command line std builds. A port of
//! `make_bat_command_line` and `append_bat_arg` (Rust 1.90, `library/std/src/sys/args/windows.rs`)
//! and of the suffix test of `library/std/src/sys/process/windows.rs`. UTF-16 code units in and
//! out; the Windows launcher converts each `OsStr` with `encode_wide`. Pure, so it is tested on
//! every system.
// Only the Windows launcher calls it outside the tests.
#![cfg_attr(not(windows), allow(dead_code))]

use std::io;

const fn u(c: char) -> u16 {
    c as u16
}

/// The command line `cmd.exe` gets to run `script` with `args`, as std builds it: refused when
/// the script's path holds a `"` or ends in `\`, or an argument holds a NUL, `\r` or `\n`.
pub(crate) fn make_bat_command_line(script: &[u16], args: &[Vec<u16>]) -> io::Result<Vec<u16>> {
    // `/e:ON` enables the command extensions the `%` escape below needs; the line is wrapped
    // in an extra pair of quotes, closed after the last argument.
    let mut cmd: Vec<u16> = "cmd.exe /e:ON /v:OFF /d /c \"".encode_utf16().collect();
    cmd.push(u('"'));
    // Windows file names cannot contain a `"` or end with `\`.
    if script.contains(&u('"')) || script.last() == Some(&u('\\')) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows file names may not contain `\"` or end with `\\`",
        ));
    }
    cmd.extend_from_slice(script.strip_suffix(&[0]).unwrap_or(script));
    cmd.push(u('"'));
    for arg in args {
        cmd.push(u(' '));
        // `\r` and `\n` would truncate the arguments.
        if arg.iter().any(|&c| c == u('\r') || c == u('\n')) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "batch file arguments are invalid",
            ));
        }
        append_bat_arg(&mut cmd, arg)?;
    }
    cmd.push(u('"'));
    Ok(cmd)
}

fn append_bat_arg(cmd: &mut Vec<u16>, arg: &[u16]) -> io::Result<()> {
    if arg.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nul byte found in provided data",
        ));
    }
    // An empty argument would vanish; one ending in `\` would escape the closing quote of a
    // script's `"%~2"`.
    let mut quote = arg.is_empty() || arg.last() == Some(&u('\\'));
    // Every ASCII symbol is quoted unless known to be good, and so is a control character.
    // An unquoted `\` is fine as long as the argument is not otherwise quoted.
    const UNQUOTED: &str = r"#$*+-./:?@\_";
    for c in char::decode_utf16(arg.iter().copied()).flatten() {
        let ascii_needs_quotes =
            c.is_ascii() && !(c.is_ascii_alphanumeric() || UNQUOTED.contains(c));
        if ascii_needs_quotes || c.is_control() {
            quote = true;
        }
    }
    if quote {
        cmd.push(u('"'));
    }
    // `\` is escaped only before a `"`, and a `"` is escaped by doubling it.
    let mut backslashes = 0usize;
    for &x in arg {
        if x == u('\\') {
            backslashes += 1;
        } else {
            if x == u('"') {
                // n more backslashes make 2n before the inner `"`.
                cmd.extend(std::iter::repeat_n(u('\\'), backslashes));
                cmd.push(u('"'));
            } else if x == u('%') || x == u('\r') {
                // `%%cd:~,%` expands to nothing, and keeps cmd.exe from expanding `%VAR%`.
                cmd.extend("%%cd:~,".encode_utf16());
            }
            backslashes = 0;
        }
        cmd.push(x);
    }
    if quote {
        // n more backslashes make 2n before the closing `"`.
        cmd.extend(std::iter::repeat_n(u('\\'), backslashes));
        cmd.push(u('"'));
    }
    Ok(())
}

/// True when the path ends in .bat or .cmd, ASCII case-insensitive.
pub(crate) fn is_batch_path(full_path: &[u16]) -> bool {
    let path = full_path.strip_suffix(&[0]).unwrap_or(full_path);
    let Some(tail) = path.len().checked_sub(4).map(|i| &path[i..]) else {
        return false;
    };
    let lower: Vec<u16> = tail
        .iter()
        .map(|&c| {
            if (u('A')..=u('Z')).contains(&c) {
                c + 32
            } else {
                c
            }
        })
        .collect();
    lower == ".bat".encode_utf16().collect::<Vec<_>>()
        || lower == ".cmd".encode_utf16().collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn line(script: &str, args: &[&str]) -> std::io::Result<String> {
        let args: Vec<Vec<u16>> = args.iter().map(|a| w(a)).collect();
        make_bat_command_line(&w(script), &args).map(|l| String::from_utf16(&l).unwrap())
    }

    #[test]
    fn batch_command_line_matches_std_for_the_arguments_that_matter() {
        const P: &str = "cmd.exe /e:ON /v:OFF /d /c \"";
        let script = "C:\\t m\\a.cmd";
        assert_eq!(line(script, &[]).unwrap(), format!("{P}\"{script}\"\""));
        assert_eq!(
            line(script, &["a", "b c"]).unwrap(),
            format!("{P}\"{script}\" a \"b c\"\"")
        );
        for (arg, want) in [
            ("plain", "plain"),
            ("-x", "-x"),
            ("./a/b.txt", "./a/b.txt"),
            ("é", "é"),
            ("", "\"\""),
            ("a b", "\"a b\""),
            ("&", "\"&\""),
            ("a|b", "\"a|b\""),
            ("^", "\"^\""),
            ("(x)", "\"(x)\""),
            ("%", "\"%%cd:~,%\""),
            ("50%off", "\"50%%cd:~,%off\""),
            ("say \"hi\"", "\"say \"\"hi\"\"\""),
            ("C:\\dir\\", "\"C:\\dir\\\\\""),
            ("a\\\"b", "\"a\\\\\"\"b\""),
        ] {
            assert_eq!(
                line(script, &[arg]).unwrap(),
                format!("{P}\"{script}\" {want}\""),
                "{arg:?}"
            );
        }
    }

    #[test]
    fn batch_command_line_refuses_what_std_refuses() {
        for bad in ["a\nb", "a\rb"] {
            assert!(line("C:\\a.cmd", &[bad]).is_err(), "{bad:?}");
        }
        assert!(line("C:\\a\".cmd", &[]).is_err());
        assert!(line("C:\\dir\\", &[]).is_err());
    }

    #[test]
    fn only_bat_and_cmd_suffixes_are_batch_files() {
        for yes in ["a.cmd", "a.BAT", "C:\\x y\\a.Cmd"] {
            assert!(is_batch_path(&w(yes)), "{yes}");
        }
        for no in ["a.exe", "a.cmd.exe", "cmd", "a.batx", "bat"] {
            assert!(!is_batch_path(&w(no)), "{no}");
        }
    }
}
