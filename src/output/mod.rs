//! Rendering of Described files (spec §6).
pub mod flat;
pub mod fragment;
pub mod json;
pub mod style;
pub mod summary;
pub mod tree;

use crate::cli::BinaryChoice;
use std::io::Write;
use termcolor::WriteColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Tree,
    Flat,
    Json,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub layout: Layout,
    /// Show line numbers (tree and flat: on unless -N; JSON: `line_number` null with -N).
    pub line_numbers: bool,
    /// Show file names in the flat layout (-H / -I / ripgrep's default).
    pub with_filename: bool,
    /// Total characters available per output line: the terminal width on a TTY,
    /// `--fragment-width` otherwise. Printers subtract their own prefix (design note on C6).
    pub width: usize,
    /// -M: ripgrep's omission messages instead of Fragments.
    pub max_columns: Option<u64>,
    /// Context is on (-A/-B/-C > 0): print `--` between non-adjacent groups, as ripgrep.
    pub context: bool,
    /// --expand
    pub expand: bool,
    /// --binary / --text / ripgrep's default: picks the binary message (design note 3).
    pub binary: BinaryChoice,
}

/// True when -M omits this line: ripgrep compares the whole line, terminator included
/// (grep-printer 0.3.1 `standard.rs:1054` and `1562`).
pub fn omitted(bytes: &[u8], o: &RenderOptions) -> bool {
    o.max_columns.is_some_and(|m| bytes.len() as u64 > m)
}

/// Writes the text of one line for tree and flat: whole, as Fragments (one per output line,
/// each preceded by `prefix`), or ripgrep's omission message with -M.
/// `bytes` is the raw line; `subs` are byte ranges in it; `avail` is the width left after the
/// caller's prefix.
pub fn write_line(
    w: &mut dyn WriteColor,
    prefix: &dyn Fn(&mut dyn WriteColor) -> std::io::Result<()>,
    bytes: &[u8],
    subs: &[(usize, usize)],
    is_match: bool,
    avail: usize,
    o: &RenderOptions,
) -> std::io::Result<()> {
    if omitted(bytes, o) {
        prefix(w)?;
        // grep-printer `standard.rs:1329`: without colour it computes no submatches, so the
        // count only appears when colour is on and the line has submatches.
        if subs.is_empty() || !w.supports_color() {
            if is_match {
                w.write_all(b"[Omitted long matching line]")?;
            } else {
                w.write_all(b"[Omitted long context line]")?;
            }
        } else {
            write!(w, "[Omitted long line with {} matches]", subs.len())?;
        }
        return w.write_all(b"\n");
    }
    let raw = crate::text::strip_terminator(bytes);
    let (text, map) = crate::text::lossy_with_map(raw);
    let subs: Vec<(usize, usize)> = subs
        .iter()
        .map(|&(s, e)| (map[s.min(raw.len())], map[e.min(raw.len())]))
        .collect();
    match fragment::fragments(&text, &subs, avail) {
        None => {
            prefix(w)?;
            style::write_highlighted(w, &text, &subs)?;
            w.write_all(b"\n")
        }
        Some(frags) => {
            for f in frags {
                prefix(w)?;
                if f.omitted_before > 0 {
                    style::write_styled(w, "dim", &format!("…[+{}] ", f.omitted_before))?;
                }
                style::write_highlighted(w, &f.text, &f.submatches)?;
                if f.omitted_after > 0 {
                    style::write_styled(w, "dim", &format!(" […+{}]", f.omitted_after))?;
                }
                w.write_all(b"\n")?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use termcolor::{Ansi, NoColor};

    pub(crate) fn opts() -> RenderOptions {
        RenderOptions {
            layout: Layout::Tree,
            line_numbers: true,
            with_filename: true,
            width: 200,
            max_columns: None,
            context: false,
            expand: false,
            binary: BinaryChoice::Auto,
        }
    }

    fn pre(w: &mut dyn WriteColor) -> std::io::Result<()> {
        w.write_all(b"P:")
    }

    fn line(
        color: bool,
        bytes: &[u8],
        subs: &[(usize, usize)],
        is_match: bool,
        avail: usize,
        o: &RenderOptions,
    ) -> String {
        let out = if color {
            let mut w = Ansi::new(Vec::new());
            write_line(&mut w, &pre, bytes, subs, is_match, avail, o).unwrap();
            w.into_inner()
        } else {
            let mut w = NoColor::new(Vec::new());
            write_line(&mut w, &pre, bytes, subs, is_match, avail, o).unwrap();
            w.into_inner()
        };
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn omitted_counts_the_terminator() {
        let o = RenderOptions {
            max_columns: Some(8),
            ..opts()
        };
        assert!(omitted(b"a needle\n", &o));
        assert!(!omitted(b"a needl\n", &o));
        assert!(!omitted(b"a needle\n", &opts()));
    }

    #[test]
    fn omission_messages_as_ripgrep() {
        let o = RenderOptions {
            max_columns: Some(5),
            ..opts()
        };
        let b = b"c needle needle\n";
        let subs = [(2, 8), (9, 15)];
        // Without colour ripgrep computes no submatches.
        assert_eq!(
            line(false, b, &subs, true, 80, &o),
            "P:[Omitted long matching line]\n"
        );
        assert_eq!(
            line(false, b, &[], false, 80, &o),
            "P:[Omitted long context line]\n"
        );
        // With colour it counts them; an inverted Match or a context line has none.
        assert_eq!(
            line(true, b, &subs, true, 80, &o),
            "P:[Omitted long line with 2 matches]\n"
        );
        assert_eq!(
            line(true, b, &[], true, 80, &o),
            "P:[Omitted long matching line]\n"
        );
        assert_eq!(
            line(true, b, &[], false, 80, &o),
            "P:[Omitted long context line]\n"
        );
        // -v: context lines are the matching ones and carry submatches.
        assert_eq!(
            line(true, b, &subs, false, 80, &o),
            "P:[Omitted long line with 2 matches]\n"
        );
    }

    #[test]
    fn short_line_is_written_whole_without_terminator_doubling() {
        assert_eq!(
            line(false, b"a needle\n", &[(2, 8)], true, 80, &opts()),
            "P:a needle\n"
        );
        assert_eq!(
            line(false, b"no eol", &[], false, 80, &opts()),
            "P:no eol\n"
        );
    }

    #[test]
    fn invalid_utf8_does_not_shift_the_highlight() {
        let out = line(true, b"caf\xe9 NEEDLE x\n", &[(5, 11)], true, 80, &opts());
        assert_eq!(out, "P:caf\u{FFFD} \x1b[0m\x1b[1m\x1b[31mNEEDLE\x1b[0m x\n");
    }

    #[test]
    fn long_line_becomes_fragments_each_with_the_prefix() {
        let mut s = "x".repeat(3000);
        s.replace_range(1000..1001, "A");
        s.replace_range(2500..2501, "C");
        s.push('\n');
        let out = line(
            false,
            s.as_bytes(),
            &[(1000, 1001), (2500, 2501)],
            true,
            60,
            &opts(),
        );
        let rows: Vec<&str> = out.lines().collect();
        assert_eq!(rows.len(), 2, "{out}");
        for r in &rows {
            assert!(r.starts_with("P:…[+"), "{r}");
            assert!(r.ends_with(']'), "{r}");
            assert!(r.chars().count() - 2 <= 60, "{r}");
        }
        assert!(rows[0].contains('A') && rows[1].contains('C'));
    }
}
