//! -c, --count-matches, -l and --files-without-match computed after filters (design note 9).
use super::Look;
use crate::model::Described;
use std::path::Path;
use termcolor::WriteColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// -c: kept lines.
    Count,
    /// --count-matches: kept submatches.
    CountMatches,
    /// -l
    FilesWithMatches,
    /// --files-without-match
    FilesWithoutMatch,
}

/// What `mode` counts in a file after filters; `None` is a file the search found nothing in.
pub fn count(mode: Mode, d: Option<&Described>) -> u64 {
    let Some(d) = d else { return 0 };
    let lines = d.file.matches();
    match mode {
        // An inverted Match has no submatches and counts as one, as ripgrep counts it.
        Mode::CountMatches => lines.map(|m| m.submatches.len().max(1) as u64).sum(),
        _ => lines.count() as u64,
    }
}

/// Writes the file's line in ripgrep's format, if the mode prints one for it, and returns
/// whether it did: `path:N` (`N` alone without file names) for the counts of files with a
/// kept Match (with `--include-zero`, of every file), `path` for -l, and for
/// --files-without-match `path` when nothing was kept. The path is printed as the Trail
/// layouts print it (`--path-separator`, `--colors`, `--hyperlink-format`), and `-0` ends it
/// with a NUL, in place of the `:` of a count or the line break of a path alone, as rg's
/// summary printer (M5a plan, D4).
pub fn render(
    mode: Mode,
    path: &Path,
    d: Option<&Described>,
    with_filename: bool,
    look: &Look,
    include_zero: bool,
    w: &mut dyn WriteColor,
) -> std::io::Result<bool> {
    let n = count(mode, d);
    let write_path = |w: &mut dyn WriteColor| -> std::io::Result<()> {
        let link = super::link_start(w, look, path, None, None)?;
        super::style::write_in(w, &look.palette, "path", &super::shown_path(path, look))?;
        super::link_end(w, link)
    };
    match mode {
        Mode::Count | Mode::CountMatches if n > 0 || include_zero => {
            if with_filename {
                write_path(w)?;
                w.write_all(&[look.path_terminator.unwrap_or(b':')])?;
            }
            w.write_all(format!("{n}\n").as_bytes())?;
        }
        Mode::FilesWithMatches if n > 0 => {
            write_path(w)?;
            w.write_all(&[look.path_terminator.unwrap_or(b'\n')])?;
        }
        Mode::FilesWithoutMatch if n == 0 => {
            write_path(w)?;
            w.write_all(&[look.path_terminator.unwrap_or(b'\n')])?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::fixture::*;
    use termcolor::{Ansi, NoColor};

    fn out(mode: Mode, d: Option<&Described>, with_filename: bool) -> String {
        let mut w = NoColor::new(Vec::new());
        let wrote = render(
            mode,
            Path::new("docs/a.md"),
            d,
            with_filename,
            &Look::default(),
            false,
            &mut w,
        )
        .unwrap();
        let s = String::from_utf8(w.into_inner()).unwrap();
        assert_eq!(wrote, !s.is_empty());
        s
    }

    fn sample() -> Described {
        let s = src(&["one needle needle", "two", "three needle"]);
        described(
            &s,
            vec![mat(&s, 1), ctx(&s, 2), mat(&s, 3)],
            vec![vec![vec![]]; 2],
        )
    }

    #[test]
    fn counts_with_and_without_file_names() {
        let d = sample();
        assert_eq!(out(Mode::Count, Some(&d), true), "docs/a.md:2\n");
        assert_eq!(out(Mode::Count, Some(&d), false), "2\n");
        assert_eq!(out(Mode::CountMatches, Some(&d), true), "docs/a.md:3\n");
        assert_eq!(out(Mode::CountMatches, Some(&d), false), "3\n");
        // Files without a kept Match print nothing, as ripgrep.
        assert_eq!(out(Mode::Count, None, true), "");
        assert_eq!(out(Mode::CountMatches, None, false), "");
    }

    #[test]
    fn file_lists() {
        let d = sample();
        for named in [true, false] {
            assert_eq!(out(Mode::FilesWithMatches, Some(&d), named), "docs/a.md\n");
            assert_eq!(out(Mode::FilesWithMatches, None, named), "");
            assert_eq!(out(Mode::FilesWithoutMatch, Some(&d), named), "");
            assert_eq!(out(Mode::FilesWithoutMatch, None, named), "docs/a.md\n");
        }
        // Every Match filtered out: the file has no Match left.
        let mut empty = sample();
        empty
            .file
            .items
            .retain(|i| matches!(i, crate::model::Item::Context(_)));
        assert_eq!(
            out(Mode::FilesWithoutMatch, Some(&empty), true),
            "docs/a.md\n"
        );
        assert_eq!(out(Mode::FilesWithMatches, Some(&empty), true), "");
    }

    #[test]
    fn path_in_ripgreps_colour() {
        let mut w = Ansi::new(Vec::new());
        render(
            Mode::Count,
            Path::new("a.md"),
            Some(&sample()),
            true,
            &Look::default(),
            false,
            &mut w,
        )
        .unwrap();
        // rg 14.1.1 --color always -c: reset, magenta, path, reset, `:N`.
        assert_eq!(w.into_inner(), b"\x1b[0m\x1b[35ma.md\x1b[0m:2\n");
    }

    /// M5a plan, D4: `-0`, `--path-separator` and `--include-zero`, as rg's summary printer.
    #[test]
    fn nul_separator_and_zero_counts() {
        let look = Look {
            path_terminator: Some(0),
            path_separator: Some(b'\\'),
            ..Look::default()
        };
        let run = |mode, d: Option<&Described>, zero| {
            let mut w = NoColor::new(Vec::new());
            render(mode, Path::new("docs/a.md"), d, true, &look, zero, &mut w).unwrap();
            w.into_inner()
        };
        assert_eq!(
            run(Mode::Count, Some(&sample()), false),
            b"docs\\a.md\x002\n"
        );
        assert_eq!(
            run(Mode::FilesWithMatches, Some(&sample()), false),
            b"docs\\a.md\x00"
        );
        assert_eq!(run(Mode::Count, None, false), b"");
        assert_eq!(run(Mode::Count, None, true), b"docs\\a.md\x000\n");
        assert_eq!(run(Mode::FilesWithMatches, None, true), b"");
    }

    #[test]
    fn count_matches_with_inverted_search_counts_lines() {
        // rg --count-matches -v counts the selected lines: inverted Matches have no submatches.
        let s = src(&["a", "b", "c", "d"]);
        let all = described(
            &s,
            (1..=4).map(|n| inverted(&s, n)).collect(),
            vec![vec![vec![]]; 4],
        );
        assert_eq!(count(Mode::CountMatches, Some(&all)), 4);
        assert_eq!(count(Mode::Count, Some(&all)), 4);
        let some = described(
            &s,
            vec![inverted(&s, 1), inverted(&s, 3)],
            vec![vec![vec![]]; 2],
        );
        assert_eq!(count(Mode::CountMatches, Some(&some)), 2);
    }
}
