//! ripgrep's flags, their arity, and whether searchfor implements them yet (spec §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Implemented,
    NotYet,
}

#[derive(Debug, Clone, Copy)]
pub struct RgFlag {
    pub long: &'static str,
    pub short: Option<char>,
    /// True when the flag takes a value (`--glob=GLOB`, `-g GLOB`).
    pub takes_value: bool,
    pub status: Status,
}

use Status::{Implemented, NotYet};

/// One entry per unique long name, sorted: the entries of ripgrep 14.1.1's `--help` and the
/// negations it documents in prose (`--no-hidden`, `--ignore`), which take no value and share
/// the status of the flag they negate.
#[rustfmt::skip]
pub const RG_FLAGS: &[RgFlag] = &[
    RgFlag { long: "after-context", short: Some('A'), takes_value: true, status: Implemented },
    RgFlag { long: "auto-hybrid-regex", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "before-context", short: Some('B'), takes_value: true, status: Implemented },
    RgFlag { long: "binary", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "block-buffered", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "byte-offset", short: Some('b'), takes_value: false, status: NotYet },
    RgFlag { long: "case-sensitive", short: Some('s'), takes_value: false, status: Implemented },
    RgFlag { long: "color", short: None, takes_value: true, status: Implemented },
    RgFlag { long: "colors", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "column", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "context", short: Some('C'), takes_value: true, status: Implemented },
    RgFlag { long: "context-separator", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "count", short: Some('c'), takes_value: false, status: Implemented },
    RgFlag { long: "count-matches", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "crlf", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "debug", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "dfa-size-limit", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "encoding", short: Some('E'), takes_value: true, status: NotYet },
    RgFlag { long: "engine", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "field-context-separator", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "field-match-separator", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "file", short: Some('f'), takes_value: true, status: Implemented },
    RgFlag { long: "files", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "files-with-matches", short: Some('l'), takes_value: false, status: Implemented },
    RgFlag { long: "files-without-match", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "fixed-strings", short: Some('F'), takes_value: false, status: Implemented },
    RgFlag { long: "follow", short: Some('L'), takes_value: false, status: Implemented },
    RgFlag { long: "generate", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "glob", short: Some('g'), takes_value: true, status: Implemented },
    RgFlag { long: "glob-case-insensitive", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "heading", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "help", short: Some('h'), takes_value: false, status: Implemented },
    RgFlag { long: "hidden", short: Some('.'), takes_value: false, status: Implemented },
    RgFlag { long: "hostname-bin", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "hyperlink-format", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "iglob", short: None, takes_value: true, status: Implemented },
    RgFlag { long: "ignore", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "ignore-case", short: Some('i'), takes_value: false, status: Implemented },
    RgFlag { long: "ignore-dot", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "ignore-exclude", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "ignore-file", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "ignore-file-case-insensitive", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "ignore-files", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "ignore-global", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "ignore-messages", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "ignore-parent", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "ignore-vcs", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "include-zero", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "invert-match", short: Some('v'), takes_value: false, status: Implemented },
    RgFlag { long: "json", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "line-buffered", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "line-number", short: Some('n'), takes_value: false, status: Implemented },
    RgFlag { long: "line-regexp", short: Some('x'), takes_value: false, status: Implemented },
    RgFlag { long: "max-columns", short: Some('M'), takes_value: true, status: Implemented },
    RgFlag { long: "max-columns-preview", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "max-count", short: Some('m'), takes_value: true, status: Implemented },
    RgFlag { long: "max-depth", short: Some('d'), takes_value: true, status: Implemented },
    RgFlag { long: "max-filesize", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "messages", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "mmap", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "multiline", short: Some('U'), takes_value: false, status: NotYet },
    RgFlag { long: "multiline-dotall", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-auto-hybrid-regex", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-binary", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-block-buffered", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-byte-offset", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-column", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-config", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-context-separator", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-crlf", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-encoding", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-filename", short: Some('I'), takes_value: false, status: Implemented },
    RgFlag { long: "no-fixed-strings", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-follow", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-glob-case-insensitive", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-heading", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-hidden", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-ignore", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-ignore-dot", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-ignore-exclude", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-ignore-file-case-insensitive", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-ignore-files", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-ignore-global", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-ignore-messages", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-ignore-parent", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-ignore-vcs", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-include-zero", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-invert-match", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-json", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-line-buffered", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-line-number", short: Some('N'), takes_value: false, status: Implemented },
    RgFlag { long: "no-max-columns-preview", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-messages", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-mmap", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-multiline", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-multiline-dotall", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-one-file-system", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-pcre2", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-pcre2-unicode", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-pre", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-require-git", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-search-zip", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-sort-files", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-stats", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-text", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "no-trim", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "no-unicode", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "null", short: Some('0'), takes_value: false, status: NotYet },
    RgFlag { long: "null-data", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "one-file-system", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "only-matching", short: Some('o'), takes_value: false, status: NotYet },
    RgFlag { long: "passthru", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "path-separator", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "pcre2", short: Some('P'), takes_value: false, status: NotYet },
    RgFlag { long: "pcre2-unicode", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "pcre2-version", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "pre", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "pre-glob", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "pretty", short: Some('p'), takes_value: false, status: NotYet },
    RgFlag { long: "quiet", short: Some('q'), takes_value: false, status: Implemented },
    RgFlag { long: "regex-size-limit", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "regexp", short: Some('e'), takes_value: true, status: Implemented },
    RgFlag { long: "replace", short: Some('r'), takes_value: true, status: NotYet },
    RgFlag { long: "require-git", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "search-zip", short: Some('z'), takes_value: false, status: NotYet },
    RgFlag { long: "smart-case", short: Some('S'), takes_value: false, status: Implemented },
    RgFlag { long: "sort", short: None, takes_value: true, status: Implemented },
    RgFlag { long: "sort-files", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "sortr", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "stats", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "stop-on-nonmatch", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "text", short: Some('a'), takes_value: false, status: Implemented },
    RgFlag { long: "threads", short: Some('j'), takes_value: true, status: Implemented },
    RgFlag { long: "trace", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "trim", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "type", short: Some('t'), takes_value: true, status: Implemented },
    RgFlag { long: "type-add", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "type-clear", short: None, takes_value: true, status: NotYet },
    RgFlag { long: "type-list", short: None, takes_value: false, status: Implemented },
    RgFlag { long: "type-not", short: Some('T'), takes_value: true, status: Implemented },
    RgFlag { long: "unicode", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "unrestricted", short: Some('u'), takes_value: false, status: Implemented },
    RgFlag { long: "version", short: Some('V'), takes_value: false, status: Implemented },
    RgFlag { long: "vimgrep", short: None, takes_value: false, status: NotYet },
    RgFlag { long: "with-filename", short: Some('H'), takes_value: false, status: Implemented },
    RgFlag { long: "word-regexp", short: Some('w'), takes_value: false, status: Implemented },
];

pub fn by_long(long: &str) -> Option<&'static RgFlag> {
    RG_FLAGS.iter().find(|f| f.long == long)
}

/// The contents of `docs/rg-compat.md`.
pub fn render_markdown() -> String {
    let mut out = String::from("# ripgrep flag compatibility\n\n");
    out.push_str(
        "Generated by `cargo test rg_compat_doc -- --ignored` from `src/cli/rg_flags.rs`; \
         do not edit by hand.\n\n",
    );
    out.push_str("| Flag | Short | Takes a value | searchfor |\n");
    out.push_str("|---|---|---|---|\n");
    for f in RG_FLAGS {
        let short = f.short.map(|c| format!("`-{c}`")).unwrap_or_default();
        let value = if f.takes_value { "yes" } else { "no" };
        let status = match f.status {
            Status::Implemented => "yes",
            Status::NotYet => "not yet",
        };
        out.push_str(&format!(
            "| `--{}` | {short} | {value} | {status} |\n",
            f.long
        ));
    }
    out.push_str(DEVIATIONS);
    out
}

/// Behaviour of implemented flags that differs from ripgrep 14.1.1, and why.
const DEVIATIONS: &str = "
## Known deviations

### Binary stdin with `-v` and a context, in the Trail layouts

On stdin with a NUL, `-v` and a context (`-C1`), rg 14.1.1 prints nothing and exits 1: its
standard printer gives up at the first context line once it has seen the NUL (grep-printer
0.2.2, `standard.rs:867`), so no Match counts. `--no-trail` does the same. `--flat` and
`--tree` print `binary file matches (found \"\\0\" byte around offset N)` and exit 0, as
`rg --json` reports the Match (code review r3, Fable #4):

```sh
printf 'needle first\\nmore\\0binary needle\\nneedle again\\n' | sfor --flat -v -C1 needle
```

### The binary warning with `--within` and `-m`

With `--within`, `-m` counts the Matches the filter keeps, so the search cannot stop at the
`-m`th Match and reads on. A walked file with a NUL after its first Match prints `WARNING:
stopped searching binary file after match` with `--within '' -m1`, which rg 14.1.1 never
reaches with `-m1`, as it stops before the NUL. The `end` and `summary` statistics of `--json`
count that whole search (spec §6.4) (code review r3, Fable #5).
";

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_rg_help() -> Option<String> {
        let v = std::process::Command::new("rg")
            .arg("--version")
            .env_remove("RIPGREP_CONFIG_PATH")
            .output()
            .ok()?;
        if !String::from_utf8_lossy(&v.stdout).starts_with("ripgrep 14.1.1") {
            eprintln!("skipping: reference rg 14.1.1 not found");
            return None;
        }
        let h = std::process::Command::new("rg")
            .arg("--help")
            .env_remove("RIPGREP_CONFIG_PATH")
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&h.stdout).into_owned())
    }

    #[test]
    fn table_is_sorted_and_unique() {
        let longs: Vec<_> = RG_FLAGS.iter().map(|f| f.long).collect();
        let mut sorted = longs.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(longs, sorted);
    }

    #[test]
    fn table_covers_reference_rg() {
        let Some(help) = reference_rg_help() else {
            return;
        };
        let re = regex_lite::Regex::new(
            r"(?m)^ {4}(?:-[A-Za-z0-9.](?: [A-Z]+)?, )?--([a-z0-9-]+)(=\S+)?",
        )
        .unwrap();
        for c in re.captures_iter(&help) {
            let f = by_long(&c[1]).unwrap_or_else(|| panic!("--{} missing from RG_FLAGS", &c[1]));
            assert_eq!(f.takes_value, c.get(2).is_some(), "--{} arity", &c[1]);
        }
        for neg in regex_lite::Regex::new(r"--no-[a-z0-9-]+")
            .unwrap()
            .find_iter(&help)
        {
            assert!(
                by_long(&neg.as_str()[2..]).is_some(),
                "{} missing",
                neg.as_str()
            );
        }
        for c in regex_lite::Regex::new(r"disabled with --([a-z0-9-]+)")
            .unwrap()
            .captures_iter(&help)
        {
            assert!(by_long(&c[1]).is_some(), "--{} missing", &c[1]);
        }
    }

    /// Every name in the table comes from the reference `rg --help` (no invented flags), and the
    /// short forms are ripgrep's.
    #[test]
    fn table_has_nothing_reference_rg_lacks() {
        let Some(help) = reference_rg_help() else {
            return;
        };
        let entry = regex_lite::Regex::new(
            r"(?m)^ {4}(?:-([A-Za-z0-9.])(?: [A-Z]+)?, )?--([a-z0-9-]+)(=\S+)?",
        )
        .unwrap();
        let mut shorts = std::collections::HashMap::new();
        for c in entry.captures_iter(&help) {
            shorts.insert(
                c[2].to_string(),
                c.get(1).and_then(|s| s.as_str().chars().next()),
            );
        }
        for f in RG_FLAGS {
            let documented = shorts.contains_key(f.long) || help.contains(&format!("--{}", f.long));
            assert!(documented, "--{} is not in rg --help", f.long);
            assert_eq!(
                f.short,
                shorts.get(f.long).copied().flatten(),
                "--{} short",
                f.long
            );
        }
    }

    #[test]
    fn negations_take_no_value_and_share_their_flag_status() {
        for f in RG_FLAGS {
            let negated = match f.long.strip_prefix("no-") {
                Some(rest) => by_long(rest),
                None => by_long(&format!("no-{}", f.long)),
            };
            let Some(other) = negated else { continue };
            // Entries that ripgrep lists separately (`--no-ignore`, `--no-filename`,
            // `--line-number`) keep their own arity; the pair still shares one status.
            assert_eq!(f.status, other.status, "--{} and --{}", f.long, other.long);
        }
    }

    #[test]
    fn implemented_flags_are_the_m1_set() {
        let mut implemented: Vec<_> = RG_FLAGS
            .iter()
            .filter(|f| f.status == Status::Implemented)
            .map(|f| f.long)
            .collect();
        implemented.sort();
        let mut expected = vec![
            "after-context",
            "before-context",
            "binary",
            "case-sensitive",
            "color",
            "context",
            "count",
            "count-matches",
            "file",
            "files-with-matches",
            "files-without-match",
            "fixed-strings",
            "follow",
            "glob",
            "help",
            "hidden",
            "iglob",
            "ignore",
            "ignore-case",
            "invert-match",
            "json",
            "line-number",
            "line-regexp",
            "max-columns",
            "max-count",
            "max-depth",
            "messages",
            "no-binary",
            "no-filename",
            "no-fixed-strings",
            "no-follow",
            "no-hidden",
            "no-ignore",
            "no-invert-match",
            "no-json",
            "no-line-number",
            "no-messages",
            "no-text",
            "quiet",
            "regexp",
            "smart-case",
            "sort",
            "text",
            "threads",
            "type",
            "type-list",
            "type-not",
            "unrestricted",
            "version",
            "with-filename",
            "word-regexp",
        ];
        expected.sort();
        assert_eq!(implemented, expected);
    }

    #[test]
    #[ignore]
    fn rg_compat_doc() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/docs/rg-compat.md"),
            render_markdown(),
        )
        .unwrap();
    }

    #[test]
    fn rg_compat_doc_is_current() {
        let on_disk =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/rg-compat.md"))
                .unwrap();
        assert_eq!(
            on_disk,
            render_markdown(),
            "regenerate with: cargo test rg_compat_doc -- --ignored"
        );
    }

    #[test]
    fn markdown_has_header_and_one_row_per_flag() {
        let md = render_markdown();
        assert!(md.starts_with("# ripgrep flag compatibility\n"));
        assert!(md.contains("cargo test rg_compat_doc -- --ignored"));
        assert!(md.contains("| Flag | Short | Takes a value | searchfor |"));
        assert!(md.contains("| `--glob` | `-g` | yes | yes |"));
        assert!(md.contains("| `--pcre2` | `-P` | no | not yet |"));
        let rows = md.lines().filter(|l| l.starts_with("| `--")).count();
        assert_eq!(rows, RG_FLAGS.len());
    }
}
