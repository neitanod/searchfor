//! Command line: ripgrep-compatible flags plus searchfor's own (spec §8).
pub mod rg_flags;

use clap::{
    Arg, ArgAction, CommandFactory, FromArgMatches, Parser, ValueEnum, parser::ValueSource,
};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorWhen {
    Never,
    Auto,
    Always,
    Ansi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    Insensitive,
    Smart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryMode {
    Quit,
    Binary,
    Text,
}

/// What the command line asked for; `Auto` is ripgrep's default (binary for explicit files and
/// stdin, quit for walked files).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BinaryChoice {
    #[default]
    Auto,
    Binary,
    Text,
}

#[derive(Debug, Parser)]
#[command(
    name = "sfor",
    version,
    about = "ripgrep-compatible search that shows where each match lives",
    disable_help_subcommand = true,
    args_override_self = true
)]
pub struct Args {
    /// A regular expression used for searching.
    #[arg(allow_hyphen_values = false)]
    pub pattern: Option<String>,
    /// Files or directories to search. `-` is stdin.
    pub paths: Vec<PathBuf>,

    #[arg(short = 'e', long = "regexp", action = ArgAction::Append, allow_hyphen_values = true)]
    pub regexp: Vec<String>,
    #[arg(short = 'f', long = "file", action = ArgAction::Append)]
    pub pattern_files: Vec<PathBuf>,
    #[arg(short = 'i', long, overrides_with_all = ["case_sensitive", "smart_case"])]
    pub ignore_case: bool,
    #[arg(short = 's', long, overrides_with_all = ["ignore_case", "smart_case"])]
    pub case_sensitive: bool,
    #[arg(short = 'S', long, overrides_with_all = ["ignore_case", "case_sensitive"])]
    pub smart_case: bool,
    // -w and -x override each other, the last one wins (rg 14.1.1, code review r1 Codex #4).
    #[arg(short = 'w', long, overrides_with = "line_regexp")]
    pub word_regexp: bool,
    #[arg(short = 'x', long, overrides_with = "word_regexp")]
    pub line_regexp: bool,
    #[arg(short = 'F', long, overrides_with = "no_fixed_strings")]
    pub fixed_strings: bool,
    #[arg(long, hide = true)]
    pub no_fixed_strings: bool,
    #[arg(short = 'v', long, overrides_with = "no_invert_match")]
    pub invert_match: bool,
    #[arg(long, hide = true)]
    pub no_invert_match: bool,
    #[arg(short = 'g', long = "glob", action = ArgAction::Append, allow_hyphen_values = true)]
    pub globs: Vec<String>,
    #[arg(long = "iglob", action = ArgAction::Append, allow_hyphen_values = true)]
    pub iglobs: Vec<String>,
    #[arg(short = 't', long = "type", action = ArgAction::Append)]
    pub types: Vec<String>,
    #[arg(short = 'T', long = "type-not", action = ArgAction::Append)]
    pub types_not: Vec<String>,
    /// `-t` (true) and `-T` (false) in command-line order, resolved in `parse`: ripgrep applies
    /// them in that order, so the last selection of a type wins.
    #[arg(skip)]
    pub type_selections: Vec<(bool, String)>,
    #[arg(long)]
    pub type_list: bool,
    #[arg(short = '.', long, overrides_with = "no_hidden")]
    pub hidden: bool,
    #[arg(long, hide = true)]
    pub no_hidden: bool,
    #[arg(long, overrides_with = "ignore")]
    pub no_ignore: bool,
    #[arg(long, hide = true)]
    pub ignore: bool,
    // One entry per occurrence, so `parse` can see where each one is: clap's `Count` keeps a
    // single index (clap_builder parser.rs:1286–1299). clap reports one index per short flag
    // inside a cluster, and shows `-u, --unrestricted` with no placeholder (measured, r5).
    #[arg(short = 'u', long = "unrestricted", help = "Reduce the level of \"smart\" filtering.",
          action = ArgAction::Append, num_args = 0, default_missing_value = "1",
          value_parser = clap::value_parser!(u8))]
    pub unrestricted_flags: Vec<u8>,
    #[arg(skip)]
    pub unrestricted: u8,
    /// Resolved in `parse` by position, like `binary_choice`: the first -u acts as --no-ignore
    /// and the second as --hidden at the place where they appear (rg 14.1.1: `-u --ignore`
    /// keeps ignores, `--ignore -u` drops them, `-uu --no-hidden` hides).
    #[arg(skip)]
    pub no_ignore_resolved: bool,
    #[arg(skip)]
    pub hidden_resolved: bool,
    #[arg(short = 'L', long, overrides_with = "no_follow")]
    pub follow: bool,
    #[arg(long, hide = true)]
    pub no_follow: bool,
    #[arg(short = 'd', long)]
    pub max_depth: Option<usize>,
    #[arg(short = 'A', long)]
    pub after_context: Option<usize>,
    #[arg(short = 'B', long)]
    pub before_context: Option<usize>,
    #[arg(short = 'C', long)]
    pub context: Option<usize>,
    // The output modes override each other: the last one on the command line wins, as in
    // ripgrep (`--json -c` counts, `-c --json` prints JSON, `-l -c` counts, `-c -l` lists).
    #[arg(short = 'c', long, overrides_with_all = ["count_matches", "files_with_matches", "files_without_match", "json"])]
    pub count: bool,
    #[arg(long, overrides_with_all = ["count", "files_with_matches", "files_without_match", "json"])]
    pub count_matches: bool,
    #[arg(short = 'l', long, overrides_with_all = ["count", "count_matches", "files_without_match", "json"])]
    pub files_with_matches: bool,
    #[arg(long, overrides_with_all = ["count", "count_matches", "files_with_matches", "json"])]
    pub files_without_match: bool,
    #[arg(short = 'm', long)]
    pub max_count: Option<u64>,
    #[arg(short = 'n', long, overrides_with = "no_line_number")]
    pub line_number: bool,
    #[arg(short = 'N', long, overrides_with = "line_number")]
    pub no_line_number: bool,
    #[arg(short = 'H', long, overrides_with = "no_filename")]
    pub with_filename: bool,
    #[arg(short = 'I', long, overrides_with = "with_filename")]
    pub no_filename: bool,
    #[arg(long, value_enum, default_value = "auto")]
    pub color: ColorWhen,
    #[arg(short = 'M', long)]
    pub max_columns: Option<u64>,
    #[arg(long, overrides_with_all = ["no_json", "count", "count_matches", "files_with_matches", "files_without_match"])]
    pub json: bool,
    #[arg(long, hide = true)]
    pub no_json: bool,
    /// Only `path` and `none` in M1; any other value is «not supported yet».
    #[arg(long)]
    pub sort: Option<String>,
    #[arg(short = 'j', long)]
    pub threads: Option<usize>,
    // The four binary-mode flags override each other (last wins); the third -u competes with
    // the survivor by position, resolved in `parse` into `binary_choice`.
    #[arg(short = 'a', long, overrides_with_all = ["no_text", "binary", "no_binary"])]
    pub text: bool,
    #[arg(long, hide = true, overrides_with_all = ["text", "binary", "no_binary"])]
    pub no_text: bool,
    /// ripgrep's --binary: search binary files, stop at the first NUL and say so.
    #[arg(long, overrides_with_all = ["text", "no_text", "no_binary"])]
    pub binary: bool,
    #[arg(long, hide = true, overrides_with_all = ["text", "no_text", "binary"])]
    pub no_binary: bool,
    #[arg(skip)]
    pub binary_choice: BinaryChoice,
    #[arg(short = 'q', long)]
    pub quiet: bool,
    #[arg(long, overrides_with = "messages")]
    pub no_messages: bool,
    #[arg(long, hide = true, overrides_with = "no_messages")]
    pub messages: bool,

    // searchfor's own flags (spec §8)
    /// Keep Matches whose Trail contains these Segment names, in order ("A › B", "A > B", "A/B").
    #[arg(long)]
    pub within: Option<String>,
    /// Print the whole Section of the last Segment of each Match.
    #[arg(long)]
    pub expand: bool,
    /// Match only Segment names: headings, keys, function names.
    #[arg(long)]
    pub in_headings: bool,
    /// Force a Format: NAME for every file, or GLOB=NAME.
    #[arg(long = "format", action = ArgAction::Append)]
    pub formats: Vec<String>,
    #[arg(long, overrides_with = "tree")]
    pub flat: bool,
    #[arg(long, overrides_with = "flat")]
    pub tree: bool,
    /// Print exactly what ripgrep would print.
    #[arg(long)]
    pub no_trail: bool,
    /// Fragment width when there is no terminal.
    #[arg(long, default_value_t = 200)]
    pub fragment_width: usize,
    #[arg(long)]
    pub list_formats: bool,
    #[arg(long, value_name = "DIR")]
    pub export_formats: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("sfor: --{0} is not supported yet (see docs/rg-compat.md)")]
    NotYet(String),
    #[error("sfor: {0}")]
    Usage(String),
    #[error(transparent)]
    Clap(#[from] clap::Error),
}

fn notyet_id(long: &str) -> String {
    format!("notyet--{long}")
}

/// Args::command() plus every NotYet ripgrep flag, hidden, with its arity.
pub fn command() -> clap::Command {
    let mut cmd = Args::command();
    for f in rg_flags::RG_FLAGS
        .iter()
        .filter(|f| f.status == rg_flags::Status::NotYet)
    {
        let mut a = Arg::new(notyet_id(f.long)).long(f.long).hide(true);
        if let Some(c) = f.short {
            a = a.short(c);
        }
        a = if f.takes_value {
            a.num_args(1)
                .action(ArgAction::Append)
                .allow_hyphen_values(true)
        } else {
            a.action(ArgAction::Count)
        };
        cmd = cmd.arg(a);
    }
    cmd
}

pub fn parse(args: Vec<OsString>) -> Result<Args, CliError> {
    let m = command().try_get_matches_from(args)?;
    for f in rg_flags::RG_FLAGS
        .iter()
        .filter(|f| f.status == rg_flags::Status::NotYet)
    {
        if m.value_source(&notyet_id(f.long)) == Some(ValueSource::CommandLine) {
            return Err(CliError::NotYet(f.long.to_string()));
        }
    }
    let mut a = Args::from_arg_matches(&m)?;
    if a.unrestricted_flags.len() > 3 {
        // ripgrep 14.1.1's message and exit 2; ripgrep names the flag as spelled the fourth time
        // (`--unrestricted` or `-u`), we always say `-u` (accepted deviation).
        return Err(CliError::Usage(
            "error parsing flag -u: flag can only be repeated up to 3 times".into(),
        ));
    }
    a.unrestricted = a.unrestricted_flags.len() as u8;
    let u = |n: usize| {
        m.indices_of("unrestricted_flags")
            .and_then(|mut it| it.nth(n))
    };
    a.no_ignore_resolved = last_by_index(&m, &[("no_ignore", true), ("ignore", false)], u(0), true);
    a.hidden_resolved = last_by_index(&m, &[("hidden", true), ("no_hidden", false)], u(1), true);
    a.binary_choice = binary_choice(&m);
    a.type_selections = type_selections(&m);
    // `-M0` turns the limit off, as in ripgrep 14.1.1 (code review r2, Codex #3).
    a.max_columns = a.max_columns.filter(|&n| n > 0);
    if let Some(s) = &a.sort
        && s != "path"
        && s != "none"
    {
        return Err(CliError::NotYet(format!("sort={s}")));
    }
    if a.no_trail && a.structural() {
        return Err(CliError::Usage(
            "--no-trail cannot be combined with --within, --expand or --in-headings".into(),
        ));
    }
    if a.in_headings && a.invert_match {
        return Err(CliError::Usage(
            "--in-headings selects names; -v has no meaning there".into(),
        ));
    }
    let summary = a.count || a.count_matches || a.files_with_matches || a.files_without_match;
    if a.expand && (summary || a.quiet) {
        return Err(CliError::Usage("--expand prints lines; it cannot be combined with -c, --count-matches, -l, --files-without-match or -q".into()));
    }
    Ok(a)
}

/// The value of the last-placed of several boolean flags (`(id, value)` when present) and an
/// optional extra occurrence at `extra_index` with value `extra_value`; false when none appears.
fn last_by_index(
    m: &clap::ArgMatches,
    flags: &[(&str, bool)],
    extra_index: Option<usize>,
    extra_value: bool,
) -> bool {
    let mut last: Option<(usize, bool)> = extra_index.map(|i| (i, extra_value));
    for &(id, v) in flags {
        if m.get_flag(id)
            && let Some(i) = m.index_of(id)
            && last.is_none_or(|(j, _)| i > j)
        {
            last = Some((i, v));
        }
    }
    last.is_some_and(|(_, v)| v)
}

/// ripgrep's binary-mode flags in command-line order: -a → Text; --binary and the third -u →
/// Binary; --no-text and --no-binary → Auto. The last one wins (`rg -a -uuu` and
/// `rg -a --no-binary` both print the binary message, measured in review r3).
fn binary_choice(m: &clap::ArgMatches) -> BinaryChoice {
    let mut last: Option<(usize, BinaryChoice)> = None;
    let mut see = |idx: Option<usize>, c: BinaryChoice| {
        if let Some(i) = idx
            && last.is_none_or(|(j, _)| i > j)
        {
            last = Some((i, c));
        }
    };
    for (id, c) in [
        ("text", BinaryChoice::Text),
        ("binary", BinaryChoice::Binary),
        ("no_text", BinaryChoice::Auto),
        ("no_binary", BinaryChoice::Auto),
    ] {
        if m.get_flag(id) {
            see(m.index_of(id), c);
        }
    }
    see(
        m.indices_of("unrestricted_flags")
            .and_then(|mut it| it.nth(2)),
        BinaryChoice::Binary,
    );
    last.map(|(_, c)| c).unwrap_or_default()
}

/// `-t` and `-T` values, each with whether it selects, sorted by their place on the command line.
fn type_selections(m: &clap::ArgMatches) -> Vec<(bool, String)> {
    let mut v: Vec<(usize, bool, String)> = Vec::new();
    for (id, select) in [("types", true), ("types_not", false)] {
        if let (Some(vals), Some(idx)) = (m.get_many::<String>(id), m.indices_of(id)) {
            v.extend(idx.zip(vals).map(|(i, t)| (i, select, t.clone())));
        }
    }
    v.sort_by_key(|e| e.0);
    v.into_iter().map(|(_, s, t)| (s, t)).collect()
}

/// Where the patterns come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patterns {
    pub patterns: Vec<String>,
    pub paths: Vec<PathBuf>,
    /// True when `-f -` consumed stdin, so stdin cannot also be searched.
    pub stdin_used: bool,
}

impl Args {
    /// ripgrep's precedence: with -e or -f, the positional is a path. `-f -` reads stdin.
    /// An empty pattern file is an empty pattern set: ripgrep 14.1.1 then searches nothing and
    /// exits 1 in every mode, `-v`, `-c` and `--json` included (no summary either); C9 does the
    /// same before walking.
    pub fn patterns(&self) -> std::io::Result<Patterns> {
        let mut paths = self.paths.clone();
        let mut patterns = self.regexp.clone();
        let mut stdin_used = false;
        for f in &self.pattern_files {
            let text = if f.as_os_str() == "-" {
                stdin_used = true;
                std::io::read_to_string(std::io::stdin())?
            } else {
                std::fs::read_to_string(f)?
            };
            patterns.extend(text.lines().map(str::to_string));
        }
        if self.regexp.is_empty() && self.pattern_files.is_empty() {
            if let Some(p) = &self.pattern {
                patterns.push(p.clone());
            }
        } else if let Some(p) = &self.pattern {
            paths.insert(0, PathBuf::from(p));
        }
        Ok(Patterns {
            patterns,
            paths,
            stdin_used,
        })
    }

    pub fn case(&self) -> Case {
        if self.ignore_case {
            Case::Insensitive
        } else if self.smart_case {
            Case::Smart
        } else {
            Case::Sensitive
        }
    }

    pub fn before(&self) -> usize {
        self.before_context.or(self.context).unwrap_or(0)
    }
    pub fn after(&self) -> usize {
        self.after_context.or(self.context).unwrap_or(0)
    }
    /// --hidden after the flags that override it and the second -u, by position. The raw
    /// `hidden` field only says that --hidden was the last of --hidden and --no-hidden.
    #[allow(clippy::misnamed_getters)]
    pub fn hidden(&self) -> bool {
        self.hidden_resolved
    }
    /// --no-ignore after --ignore and the first -u, by position (see `hidden`).
    #[allow(clippy::misnamed_getters)]
    pub fn no_ignore(&self) -> bool {
        self.no_ignore_resolved
    }
    /// ripgrep's three binary modes for one file: quit at NUL, «binary» (search, stop at NUL
    /// and print the binary message), text.
    pub fn binary_mode(&self, explicit: bool) -> BinaryMode {
        match self.binary_choice {
            BinaryChoice::Text => BinaryMode::Text,
            BinaryChoice::Binary => BinaryMode::Binary,
            BinaryChoice::Auto => {
                if explicit {
                    BinaryMode::Binary
                } else {
                    BinaryMode::Quit
                }
            }
        }
    }
    pub fn structural(&self) -> bool {
        self.within.is_some() || self.expand || self.in_headings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }
    fn p(v: &[&str]) -> Args {
        parse(os(v)).unwrap()
    }

    #[test]
    fn max_columns_zero_is_no_limit() {
        assert_eq!(p(&["sfor", "-M0", "x"]).max_columns, None);
        assert_eq!(p(&["sfor", "-M", "5", "x"]).max_columns, Some(5));
        assert_eq!(p(&["sfor", "x"]).max_columns, None);
    }

    #[test]
    fn command_is_valid() {
        command().debug_assert();
    }

    #[test]
    fn every_rg_flag_is_known_to_the_command() {
        // `build` adds clap's own --help and --version, which ripgrep also has.
        let mut cmd = command();
        cmd.build();
        let longs: Vec<_> = cmd.get_arguments().filter_map(|a| a.get_long()).collect();
        for f in rg_flags::RG_FLAGS {
            assert!(longs.contains(&f.long), "--{} not registered", f.long);
        }
        // Implemented flags live in Args itself, not among the hidden NotYet arguments.
        let mut own = Args::command();
        own.build();
        for f in rg_flags::RG_FLAGS
            .iter()
            .filter(|f| f.status == rg_flags::Status::Implemented)
        {
            let arg = own.get_arguments().find(|a| a.get_long() == Some(f.long));
            let arg = arg.unwrap_or_else(|| panic!("--{} missing from Args", f.long));
            assert_eq!(arg.get_short(), f.short, "--{} short", f.long);
        }
    }

    #[test]
    fn not_yet_flags_are_reported_by_name() {
        assert_eq!(
            parse(os(&["sfor", "--pcre2", "x"]))
                .unwrap_err()
                .to_string(),
            "sfor: --pcre2 is not supported yet (see docs/rg-compat.md)"
        );
        assert!(
            matches!(parse(os(&["sfor", "-nP", "x"])), Err(CliError::NotYet(f)) if f == "pcre2")
        );
        assert!(matches!(
            parse(os(&["sfor", "--sort", "modified", "x"])),
            Err(CliError::NotYet(_))
        ));
        assert!(
            matches!(parse(os(&["sfor", "--no-pcre2", "x"])), Err(CliError::NotYet(f)) if f == "no-pcre2")
        );
        assert!(
            matches!(parse(os(&["sfor", "--unicode", "x"])), Err(CliError::NotYet(f)) if f == "unicode")
        );
    }

    #[test]
    fn not_yet_flags_consume_their_value() {
        assert!(
            matches!(parse(os(&["sfor", "--max-filesize", "10", "x"])), Err(CliError::NotYet(f)) if f == "max-filesize")
        );
        assert!(
            matches!(parse(os(&["sfor", "-r", "--json", "x"])), Err(CliError::NotYet(f)) if f == "replace")
        );
        assert!(
            matches!(parse(os(&["sfor", "-E=utf-8", "x"])), Err(CliError::NotYet(f)) if f == "encoding")
        );
    }

    #[test]
    fn values_that_look_like_flags_are_values() {
        for argv in [
            &["sfor", "-e", "--pcre2"][..],
            &["sfor", "-e--pcre2"],
            &["sfor", "--regexp=--pcre2"],
        ] {
            assert_eq!(
                p(argv).patterns().unwrap().patterns,
                ["--pcre2"],
                "{argv:?}"
            );
        }
    }

    #[test]
    fn repeated_patterns_and_globs_accumulate() {
        let a = p(&["sfor", "-e", "a", "-e", "b", "-g", "*.md", "-g", "!x.md"]);
        assert_eq!(a.patterns().unwrap().patterns, ["a", "b"]);
        assert_eq!(a.globs, ["*.md", "!x.md"]);
    }

    #[test]
    fn positional_becomes_a_path_with_e() {
        let pats = p(&["sfor", "-e", "foo", "src"]).patterns().unwrap();
        assert_eq!(
            (pats.patterns, pats.paths),
            (vec!["foo".to_string()], vec![PathBuf::from("src")])
        );
    }

    #[test]
    fn positional_is_the_pattern_without_e() {
        let pats = p(&["sfor", "foo", "a", "b"]).patterns().unwrap();
        assert_eq!(pats.patterns, ["foo"]);
        assert_eq!(pats.paths, [PathBuf::from("a"), PathBuf::from("b")]);
        assert!(!pats.stdin_used);
    }

    #[test]
    fn pattern_file_lines_are_patterns() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, b"one\ntwo\n").unwrap();
        let pats = p(&[
            "sfor",
            "-e",
            "zero",
            "-f",
            f.path().to_str().unwrap(),
            "dir",
        ])
        .patterns()
        .unwrap();
        assert_eq!(pats.patterns, ["zero", "one", "two"]);
        assert_eq!(pats.paths, [PathBuf::from("dir")]);
    }

    #[test]
    fn last_case_flag_wins() {
        assert_eq!(p(&["sfor", "x"]).case(), Case::Sensitive);
        assert_eq!(p(&["sfor", "-s", "-i", "x"]).case(), Case::Insensitive);
        assert_eq!(p(&["sfor", "-i", "-s", "x"]).case(), Case::Sensitive);
        assert_eq!(p(&["sfor", "-i", "-S", "x"]).case(), Case::Smart);
        assert_eq!(p(&["sfor", "-S", "-i", "x"]).case(), Case::Insensitive);
    }

    #[test]
    fn last_of_each_pair_wins() {
        let a = p(&["sfor", "-n", "-N", "x"]);
        assert!(!a.line_number && a.no_line_number);
        let b = p(&["sfor", "--hidden", "--no-hidden", "x"]);
        assert!(!b.hidden());
        assert!(p(&["sfor", "--no-hidden", "-.", "x"]).hidden());
        let c = p(&["sfor", "-v", "--no-invert-match", "x"]);
        assert!(!c.invert_match);
        let d = p(&["sfor", "--no-messages", "--messages", "x"]);
        assert!(!d.no_messages);
        let e = p(&[
            "sfor",
            "-F",
            "--no-fixed-strings",
            "-L",
            "--no-follow",
            "--json",
            "--no-json",
            "x",
        ]);
        assert!(!e.fixed_strings && !e.follow && !e.json);
        assert!(p(&["sfor", "--ignore", "--no-ignore", "x"]).no_ignore());
        assert!(!p(&["sfor", "--no-ignore", "--ignore", "x"]).no_ignore());
    }

    #[test]
    fn word_and_line_regexp_override_each_other() {
        let a = p(&["sfor", "-x", "-w", "x"]);
        assert!(a.word_regexp && !a.line_regexp);
        let b = p(&["sfor", "-w", "-x", "x"]);
        assert!(b.line_regexp && !b.word_regexp);
    }

    #[test]
    fn last_output_mode_wins() {
        let a = p(&["sfor", "--json", "-c", "x"]);
        assert!(a.count && !a.json);
        let b = p(&["sfor", "-c", "--json", "x"]);
        assert!(b.json && !b.count);
        let c = p(&["sfor", "-c", "-l", "x"]);
        assert!(c.files_with_matches && !c.count);
    }

    #[test]
    fn binary_modes() {
        assert_eq!(p(&["sfor", "x"]).binary_mode(false), BinaryMode::Quit);
        assert_eq!(p(&["sfor", "x"]).binary_mode(true), BinaryMode::Binary);
        assert_eq!(
            p(&["sfor", "-uuu", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(p(&["sfor", "-a", "x"]).binary_mode(false), BinaryMode::Text);
        assert_eq!(
            p(&["sfor", "-a", "--binary", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["sfor", "-a", "-uuu", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["sfor", "-uuu", "-a", "x"]).binary_mode(false),
            BinaryMode::Text
        );
        assert_eq!(
            p(&["sfor", "-a", "--no-binary", "x"]).binary_mode(true),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["sfor", "--no-binary", "-a", "x"]).binary_mode(true),
            BinaryMode::Text
        );
        assert_eq!(
            p(&["sfor", "-auuu", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["sfor", "-uuua", "x"]).binary_mode(false),
            BinaryMode::Text
        );
        assert_eq!(
            p(&["sfor", "-u", "-a", "-u", "-u", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert!(matches!(
            parse(os(&["sfor", "-uuuu", "x"])),
            Err(CliError::Usage(_))
        ));
        assert_eq!(p(&["sfor", "-uu", "x"]).unrestricted, 2);
    }

    #[test]
    fn fourth_unrestricted_is_ripgreps_error() {
        for argv in [
            &["sfor", "-uuuu", "x"][..],
            &["sfor", "-uuu", "--unrestricted", "x"],
        ] {
            assert_eq!(
                parse(os(argv)).unwrap_err().to_string(),
                "sfor: error parsing flag -u: flag can only be repeated up to 3 times",
                "{argv:?}"
            );
        }
    }

    #[test]
    fn unrestricted_effects_by_position() {
        assert!(!p(&["sfor", "x"]).no_ignore());
        assert!(!p(&["sfor", "x"]).hidden());
        assert!(p(&["sfor", "-u", "x"]).no_ignore());
        assert!(!p(&["sfor", "-u", "x"]).hidden());
        assert!(!p(&["sfor", "-u", "--ignore", "x"]).no_ignore());
        assert!(p(&["sfor", "--ignore", "-u", "x"]).no_ignore());
        assert!(!p(&["sfor", "-uu", "--no-hidden", "x"]).hidden());
        assert!(p(&["sfor", "--no-hidden", "-uu", "x"]).hidden());
        let a = p(&["sfor", "-u", "--ignore", "-u", "x"]);
        assert!(!a.no_ignore() && a.hidden());
        let b = p(&["sfor", "--unrestricted", "--unrestricted", "x"]);
        assert!(b.no_ignore() && b.hidden() && b.unrestricted == 2);
    }

    #[test]
    fn empty_pattern_file_is_an_empty_set() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let pats = p(&["sfor", "-f", f.path().to_str().unwrap()])
            .patterns()
            .unwrap();
        assert!(pats.patterns.is_empty());
    }

    #[test]
    fn context_defaults() {
        let a = p(&["sfor", "-C", "2", "-A", "5", "x"]);
        assert_eq!((a.before(), a.after()), (2, 5));
        // -A and -B partially override -C regardless of order (rg 14.1.1 --help, measured).
        let b = p(&["sfor", "-A", "1", "-C", "2", "x"]);
        assert_eq!((b.before(), b.after()), (2, 1));
        assert_eq!(
            (p(&["sfor", "x"]).before(), p(&["sfor", "x"]).after()),
            (0, 0)
        );
    }

    #[test]
    fn sort_path_and_none_are_accepted() {
        assert_eq!(
            p(&["sfor", "--sort", "path", "x"]).sort.as_deref(),
            Some("path")
        );
        assert_eq!(
            p(&["sfor", "--sort=none", "x"]).sort.as_deref(),
            Some("none")
        );
        assert_eq!(
            parse(os(&["sfor", "--sort", "modified", "x"]))
                .unwrap_err()
                .to_string(),
            "sfor: --sort=modified is not supported yet (see docs/rg-compat.md)"
        );
    }

    #[test]
    fn invalid_combinations() {
        assert!(matches!(
            parse(os(&["sfor", "--no-trail", "--expand", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["sfor", "--no-trail", "--within", "A", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["sfor", "--no-trail", "--in-headings", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["sfor", "--expand", "-c", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["sfor", "--expand", "-q", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["sfor", "--expand", "-l", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(p(&["sfor", "--expand", "--within", "A", "x"]).structural());
        assert!(!p(&["sfor", "--no-trail", "-c", "x"]).structural());
    }

    #[test]
    fn help_shows_unrestricted_without_a_placeholder_and_hides_not_yet_flags() {
        let help = command().render_help().to_string();
        assert!(help.contains("-u, --unrestricted"), "{help}");
        assert!(!help.contains("--unrestricted <"), "{help}");
        assert!(
            help.contains("Reduce the level of \"smart\" filtering."),
            "{help}"
        );
        assert!(!help.contains("--pcre2"), "{help}");
        assert!(!help.contains("--no-hidden"), "{help}");
    }

    #[test]
    fn in_headings_rejects_invert_match() {
        let e = parse(os(&["sfor", "--in-headings", "-v", "x"])).unwrap_err();
        assert!(matches!(e, CliError::Usage(_)));
        assert_eq!(
            e.to_string(),
            "sfor: --in-headings selects names; -v has no meaning there"
        );
        assert!(p(&["sfor", "--in-headings", "-v", "--no-invert-match", "x"]).in_headings);
    }

    #[test]
    fn the_word_formats_is_a_pattern() {
        assert_eq!(p(&["sfor", "formats"]).pattern.as_deref(), Some("formats"));
    }
}
