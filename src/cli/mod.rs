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
    name = "s4",
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
    /// `-e` and `-f` in command-line order (resolved in `parse`), as rg 14.1.1's
    /// `PatternSource`s: the order of the alternatives decides what matches.
    #[arg(skip)]
    pub pattern_sources: Vec<PatternSource>,
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
    /// ripgrep's -z: search compressed files through their decompressors on PATH (gzip,
    /// bzip2, xz, LZ4, LZMA, Brotli, Zstd). -z and --pre turn each other off (last wins).
    #[arg(short = 'z', long)]
    pub search_zip: bool,
    #[arg(long, hide = true)]
    pub no_search_zip: bool,
    /// ripgrep's --pre: search the output of `COMMAND PATH` instead of PATH; an empty COMMAND
    /// turns it off. Its value may start with a dash, as in rg (code review r2, Codex #3).
    #[arg(long = "pre", value_name = "COMMAND", action = ArgAction::Append, allow_hyphen_values = true)]
    pub pre_flags: Vec<OsString>,
    /// --pre's command, once -z, --pre and their negations are applied in order (`pre_and_zip`).
    #[arg(skip)]
    pub pre: Option<OsString>,
    #[arg(long, hide = true)]
    pub no_pre: bool,
    /// ripgrep's --pre-glob: run --pre only on files these globs match (gitignore syntax, `!`
    /// excludes). Its value may start with a dash, as in rg.
    #[arg(long = "pre-glob", value_name = "GLOB", action = ArgAction::Append, allow_hyphen_values = true)]
    pub pre_globs: Vec<String>,
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
    /// Print what ripgrep would print; with --no-docs, exactly that.
    #[arg(long)]
    pub no_trail: bool,
    /// Search PDF, Word/ODT, Excel/ODS, PowerPoint/ODP, EPUB and SQLite files through the
    /// built-in Extractors (spec §5.8). On by default; turns it back on after --no-docs.
    #[arg(long = "docs", overrides_with = "no_docs")]
    pub docs_flag: bool,
    /// Search documents as ripgrep does, at ripgrep's cost: no Extractor runs.
    #[arg(long, overrides_with = "docs_flag")]
    pub no_docs: bool,
    /// Whether documents are searched through their Extractors: on unless --no-docs comes
    /// after the last --docs (grill, 8750), resolved in `parse`.
    #[arg(skip = true)]
    pub docs: bool,
    /// Fragment width when there is no terminal.
    #[arg(long, default_value_t = 200)]
    pub fragment_width: usize,
    #[arg(long)]
    pub list_formats: bool,
    #[arg(long, value_name = "DIR")]
    pub export_formats: Option<PathBuf>,
    /// List the compiled-in, installed and available Grammars (spec §7).
    #[arg(long)]
    pub list_grammars: bool,
    /// Install a Grammar Helper from the Grammar index (spec §7).
    #[arg(long, value_name = "LANG")]
    pub install_grammar: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("s4: --{0} is not supported yet (see docs/rg-compat.md)")]
    NotYet(String),
    #[error("s4: {0}")]
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
    a.docs = !a.no_docs;
    (a.pre, a.search_zip) = pre_and_zip(&m);
    a.type_selections = type_selections(&m);
    a.pattern_sources = pattern_sources(&m);
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

/// `--pre` and `-z` as ripgrep 14.1.1 sets them (flags/defs.rs, `Pre` and `SearchZip`): each
/// flag in command-line order, where `--pre COMMAND` also turns -z off and `-z` also turns
/// --pre off, an empty COMMAND and `--no-pre` turn --pre off, and `--no-search-zip` turns -z
/// off. A negation turns off only its own flag, so it never brings back the one the other
/// turned off (code review r1, Codex #4, Fable #2).
fn pre_and_zip(m: &clap::ArgMatches) -> (Option<OsString>, bool) {
    enum Step<'a> {
        Pre(&'a OsString),
        NoPre,
        Zip,
        NoZip,
    }
    let mut steps: Vec<(usize, Step)> = Vec::new();
    if let (Some(values), Some(indices)) = (
        m.get_many::<OsString>("pre_flags"),
        m.indices_of("pre_flags"),
    ) {
        steps.extend(indices.zip(values.map(Step::Pre)));
    }
    // A repeated switch keeps its last index, which is enough: it sets the same each time.
    for (id, step) in [
        ("no_pre", Step::NoPre),
        ("search_zip", Step::Zip),
        ("no_search_zip", Step::NoZip),
    ] {
        if m.get_flag(id)
            && let Some(i) = m.index_of(id)
        {
            steps.push((i, step));
        }
    }
    steps.sort_by_key(|(i, _)| *i);
    let (mut pre, mut zip) = (None, false);
    for (_, step) in steps {
        match step {
            Step::Pre(c) if c.is_empty() => pre = None,
            Step::Pre(c) => {
                pre = Some(c.clone());
                zip = false;
            }
            Step::NoPre => pre = None,
            Step::Zip => {
                zip = true;
                pre = None;
            }
            Step::NoZip => zip = false,
        }
    }
    (pre, zip)
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

/// One `-e` or `-f`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternSource {
    Regexp(String),
    File(PathBuf),
}

/// `-e` and `-f` sorted by their place on the command line (code review r7, Fable #2).
fn pattern_sources(m: &clap::ArgMatches) -> Vec<PatternSource> {
    let mut v: Vec<(usize, PatternSource)> = Vec::new();
    if let (Some(vals), Some(idx)) = (m.get_many::<String>("regexp"), m.indices_of("regexp")) {
        v.extend(
            idx.zip(vals)
                .map(|(i, p)| (i, PatternSource::Regexp(p.clone()))),
        );
    }
    if let (Some(vals), Some(idx)) = (
        m.get_many::<PathBuf>("pattern_files"),
        m.indices_of("pattern_files"),
    ) {
        v.extend(
            idx.zip(vals)
                .map(|(i, f)| (i, PatternSource::File(f.clone()))),
        );
    }
    v.sort_by_key(|e| e.0);
    v.into_iter().map(|(_, s)| s).collect()
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
    ///
    /// Stdin is read once, as in ripgrep 14.1.1: a second `-f -` and the path `-` after a
    /// `-f -` are errors with rg's messages, the pattern files first in command-line order
    /// (hiargs.rs:1016-1035), then the paths (hiargs.rs:1074-1080). The pattern files are read
    /// with rg's grep-cli, so their errors carry the path or `<stdin>` as rg's.
    pub fn patterns(&self) -> std::io::Result<Patterns> {
        let mut paths = self.paths.clone();
        let mut patterns = Vec::new();
        let mut stdin_used = false;
        for source in &self.pattern_sources {
            let f = match source {
                PatternSource::Regexp(p) => {
                    patterns.push(p.clone());
                    continue;
                }
                PatternSource::File(f) => f,
            };
            if f.as_os_str() == "-" {
                if stdin_used {
                    return Err(std::io::Error::other(
                        "error reading -f/--file from stdin: stdin has already been consumed",
                    ));
                }
                patterns.extend(grep_cli::patterns_from_stdin()?);
                stdin_used = true;
            } else {
                patterns.extend(grep_cli::patterns_from_path(f)?);
            }
        }
        if self.regexp.is_empty() && self.pattern_files.is_empty() {
            if let Some(p) = &self.pattern {
                patterns.push(p.clone());
            }
        } else if let Some(p) = &self.pattern {
            paths.insert(0, PathBuf::from(p));
        }
        if stdin_used && paths.iter().any(|p| p.as_os_str() == "-") {
            return Err(std::io::Error::other(
                "error: attempted to read patterns from stdin while also searching stdin",
            ));
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

    /// D3: the configuration file's arguments go before the command line's, and `--no-config`
    /// and `--mcp` read no file (review r1 of the M5a plan, Fable #1).
    #[test]
    fn the_configuration_file_is_not_read_for_mcp() {
        let file = || (os(&["--generate=man", "--pre=cat"]), Vec::new());
        let a = parse_with_config(os(&["s4", "x"]), file).unwrap();
        assert!(a.generate.is_some() && a.pre.is_some());
        for argv in [&["s4", "--mcp"][..], &["s4", "--no-config", "x"]] {
            let a = parse_with_config(os(argv), file).unwrap();
            assert!(a.generate.is_none() && a.pre.is_none(), "{argv:?}");
        }
    }

    /// M4: ripgrep's -z and --pre override each other, the last one given winning, and an
    /// empty --pre is none (rg 14.1.1).
    #[test]
    fn z_and_pre_override_each_other() {
        let a = p(&["s4", "x", "-z", "--pre", "cat"]);
        assert_eq!(
            (a.search_zip, a.pre.as_deref()),
            (false, Some(std::ffi::OsStr::new("cat")))
        );
        let a = p(&["s4", "x", "--pre", "cat", "-z"]);
        assert_eq!((a.search_zip, a.pre), (true, None));
        assert_eq!(p(&["s4", "x", "--pre", ""]).pre, None);
        assert_eq!(p(&["s4", "x", "--pre", "cat", "--no-pre"]).pre, None);
        assert!(!p(&["s4", "x", "-z", "--no-search-zip"]).search_zip);
        // Each of -z and --pre turns the other off as it comes, and a later negation does not
        // bring that one back, as in ripgrep 14.1.1 (code review r1, Codex #4, Fable #2).
        let a = p(&["s4", "x", "-z", "--pre", "cat", "--no-pre"]);
        assert_eq!((a.search_zip, a.pre), (false, None));
        let a = p(&["s4", "x", "--pre", "cat", "-z", "--no-search-zip"]);
        assert_eq!((a.search_zip, a.pre), (false, None));
        let a = p(&["s4", "x", "-z", "--pre", "cat", "--pre", ""]);
        assert_eq!((a.search_zip, a.pre), (false, None));
        let a = p(&["s4", "x", "--pre", "cat", "--pre", "", "-z"]);
        assert_eq!((a.search_zip, a.pre), (true, None));
        let a = p(&["s4", "x", "--no-pre", "--pre", "cat"]);
        assert_eq!(a.pre.as_deref(), Some(std::ffi::OsStr::new("cat")));
        let a = p(&["s4", "x", "--no-search-zip", "-z"]);
        assert!(a.search_zip);
        let a = p(&["s4", "x", "--pre", "a", "--pre", "b"]);
        assert_eq!(a.pre.as_deref(), Some(std::ffi::OsStr::new("b")));
        let a = p(&[
            "s4",
            "x",
            "--pre-glob",
            "*.pdf",
            "--pre-glob",
            "!a.pdf",
            "--docs",
        ]);
        assert_eq!(a.pre_globs, ["*.pdf", "!a.pdf"]);
        assert!(a.docs);
    }

    /// Documents are searched by default; --no-docs turns that off and --docs turns it back
    /// on, the last one given winning, as ripgrep's negations (grill, 8750).
    #[test]
    fn docs_is_on_by_default_and_the_last_of_docs_and_no_docs_wins() {
        assert!(p(&["s4", "x"]).docs);
        assert!(p(&["s4", "x", "--docs"]).docs);
        assert!(!p(&["s4", "x", "--no-docs"]).docs);
        assert!(p(&["s4", "x", "--no-docs", "--docs"]).docs);
        assert!(!p(&["s4", "x", "--docs", "--no-docs"]).docs);
        assert!(!p(&["s4", "--no-docs", "x", "--docs", "--no-docs"]).docs);
        assert!(p(&["s4", "--docs", "--no-docs", "x", "--docs"]).docs);
    }

    /// Code review r2, Codex #3: --pre and --pre-glob take a value that starts with a dash,
    /// as rg 14.1.1 does, separated by a space or by `=`.
    #[test]
    fn pre_and_pre_glob_take_values_that_start_with_a_dash() {
        let a = p(&["s4", "--pre", "-cat", "--pre-glob", "-*.txt", "x", "a.txt"]);
        assert_eq!(a.pre.as_deref(), Some(std::ffi::OsStr::new("-cat")));
        assert_eq!(a.pre_globs, ["-*.txt"]);
        assert_eq!(a.pattern.as_deref(), Some("x"));
        let a = p(&["s4", "--pre=-cat", "--pre-glob=-x", "x"]);
        assert_eq!(a.pre.as_deref(), Some(std::ffi::OsStr::new("-cat")));
        assert_eq!(a.pre_globs, ["-x"]);
    }

    #[test]
    fn max_columns_zero_is_no_limit() {
        assert_eq!(p(&["s4", "-M0", "x"]).max_columns, None);
        assert_eq!(p(&["s4", "-M", "5", "x"]).max_columns, Some(5));
        assert_eq!(p(&["s4", "x"]).max_columns, None);
    }

    #[test]
    fn grammar_actions_need_no_pattern() {
        let a = p(&["s4", "--list-grammars"]);
        assert!(a.list_grammars);
        assert_eq!(a.pattern, None);
        assert_eq!(a.install_grammar, None);
        let a = p(&["s4", "--install-grammar", "lua"]);
        assert_eq!(a.install_grammar.as_deref(), Some("lua"));
        assert!(!a.list_grammars);
        assert_eq!(a.pattern, None);
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

    /// M5: the flags that were «not supported yet» parse, with their values (M5a plan, D1).
    #[test]
    fn every_ripgrep_flag_parses() {
        let a = p(&["s4", "--pcre2", "x"]);
        assert_eq!(a.engine, Engine::Pcre2);
        assert_eq!(p(&["s4", "-nP", "x"]).engine, Engine::Pcre2);
        assert_eq!(
            p(&["s4", "--max-filesize", "10K", "x"]).max_filesize,
            Some(10 << 10)
        );
        let a = p(&["s4", "-r", "--json", "x"]);
        assert_eq!(a.replace.as_deref(), Some("--json"));
        assert!(!a.json);
        assert_eq!(
            p(&["s4", "-E=utf-16le", "x"]).encoding,
            EncodingMode::Label("utf-16le".into())
        );
        assert!(!p(&["s4", "--no-unicode", "--unicode", "x"]).no_unicode);
        assert!(p(&["s4", "--unicode", "--no-pcre2-unicode", "x"]).no_unicode);
        assert!(p(&["s4", "--maxdepth", "2", "x"]).max_depth == Some(2));
        assert!(p(&["s4", "--passthrough", "x"]).passthru);
        for argv in [
            &["s4", "--max-filesize", "1k", "x"][..],
            &["s4", "-E", "klingon", "x"],
            &["s4", "--colors", "nope:fg:red", "x"],
            &["s4", "--path-separator", "ab", "x"],
            &["s4", "--engine", "fast", "x"],
            &["s4", "--sort", "size", "x"],
            &["s4", "--hyperlink-format", "{nope}", "x"],
        ] {
            assert!(
                matches!(parse(os(argv)), Err(CliError::Clap(_))),
                "{argv:?}"
            );
        }
    }

    /// rg 14.1.1 applies each flag in order (defs.rs `update`): a later flag undoes the part of
    /// an earlier one it shares with it.
    #[test]
    fn order_dependent_flags_resolve_as_ripgrep() {
        let r = p(&["s4", "--no-ignore", "--ignore-vcs", "x"]).ignores;
        assert!(r.no_dot && r.no_parent && r.no_global && r.no_exclude && !r.no_vcs);
        let r = p(&["s4", "--no-ignore-vcs", "--ignore", "x"]).ignores;
        assert_eq!(r, IgnoreRules::default());
        let r = p(&["s4", "--no-ignore-dot", "-u", "--ignore-parent", "x"]).ignores;
        assert!(r.no_dot && r.no_vcs && !r.no_parent);
        assert_eq!(p(&["s4", "-u", "x"]).ignores, IgnoreRules::all(true));
        let sort = |v: &[&str]| p(v).sort;
        assert_eq!(sort(&["s4", "x"]), None);
        let path = Some(SortMode {
            kind: SortKind::Path,
            reverse: false,
        });
        assert_eq!(sort(&["s4", "--sort-files", "x"]), path);
        assert_eq!(
            sort(&["s4", "--sortr", "modified", "--sort", "path", "x"]),
            path
        );
        assert_eq!(
            sort(&["s4", "--sort", "path", "--sortr", "created", "x"]),
            Some(SortMode {
                kind: SortKind::Created,
                reverse: true
            })
        );
        assert_eq!(
            sort(&["s4", "--sort", "path", "--no-sort-files", "x"]),
            None
        );
        assert_eq!(sort(&["s4", "--sort-files", "--sort", "none", "x"]), None);
        assert_eq!(
            p(&["s4", "-P", "--engine", "auto", "x"]).engine,
            Engine::Auto
        );
        assert_eq!(
            p(&["s4", "--engine", "pcre2", "--no-pcre2", "x"]).engine,
            Engine::Default
        );
        assert_eq!(
            p(&["s4", "-P", "--no-auto-hybrid-regex", "x"]).engine,
            Engine::Default
        );
        assert_eq!(
            p(&["s4", "-E", "latin1", "--no-encoding", "x"]).encoding,
            EncodingMode::Auto
        );
        assert_eq!(
            p(&["s4", "-E", "none", "x"]).encoding,
            EncodingMode::Disabled
        );
        let a = p(&["s4", "--crlf", "--null-data", "x"]);
        assert!(!a.crlf && a.null_data);
        let a = p(&["s4", "--null-data", "--crlf", "x"]);
        assert!(a.crlf && !a.null_data);
        let a = p(&["s4", "-U", "--stop-on-nonmatch", "x"]);
        assert!(!a.multiline && a.stop_on_nonmatch);
        let a = p(&["s4", "--stop-on-nonmatch", "-U", "x"]);
        assert!(a.multiline && !a.stop_on_nonmatch);
        let a = p(&["s4", "--line-buffered", "--block-buffered", "x"]);
        assert_eq!(a.buffer, BufferMode::Block);
        let a = p(&["s4", "--block-buffered", "--no-block-buffered", "x"]);
        assert_eq!(a.buffer, BufferMode::Auto);
        assert_eq!(
            p(&["s4", "--trace", "--debug", "x"]).logging,
            Some(Logging::Debug)
        );
        assert_eq!(
            p(&["s4", "--debug", "--trace", "x"]).logging,
            Some(Logging::Trace)
        );
        assert_eq!(
            p(&["s4", "x"]).context_separator.as_deref(),
            Some(&b"--"[..])
        );
        let a = p(&[
            "s4",
            "--context-separator",
            "##",
            "--no-context-separator",
            "x",
        ]);
        assert_eq!(a.context_separator, None);
        let a = p(&[
            "s4",
            "--no-context-separator",
            "--context-separator",
            r"\t",
            "x",
        ]);
        assert_eq!(a.context_separator.as_deref(), Some(&b"\t"[..]));
        assert_eq!(p(&["s4", "--mmap", "x"]).mmap_mode, MmapMode::Always);
        assert_eq!(
            p(&["s4", "--mmap", "--no-mmap", "x"]).mmap_mode,
            MmapMode::Never
        );
        assert_eq!(p(&["s4", "x"]).mmap_mode, MmapMode::Auto);
    }

    #[test]
    fn values_that_look_like_flags_are_values() {
        for argv in [
            &["s4", "-e", "--pcre2"][..],
            &["s4", "-e--pcre2"],
            &["s4", "--regexp=--pcre2"],
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
        let a = p(&["s4", "-e", "a", "-e", "b", "-g", "*.md", "-g", "!x.md"]);
        assert_eq!(a.patterns().unwrap().patterns, ["a", "b"]);
        assert_eq!(a.globs, ["*.md", "!x.md"]);
    }

    #[test]
    fn positional_becomes_a_path_with_e() {
        let pats = p(&["s4", "-e", "foo", "src"]).patterns().unwrap();
        assert_eq!(
            (pats.patterns, pats.paths),
            (vec!["foo".to_string()], vec![PathBuf::from("src")])
        );
    }

    #[test]
    fn positional_is_the_pattern_without_e() {
        let pats = p(&["s4", "foo", "a", "b"]).patterns().unwrap();
        assert_eq!(pats.patterns, ["foo"]);
        assert_eq!(pats.paths, [PathBuf::from("a"), PathBuf::from("b")]);
        assert!(!pats.stdin_used);
    }

    #[test]
    fn pattern_file_lines_are_patterns() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, b"one\ntwo\n").unwrap();
        let pats = p(&["s4", "-e", "zero", "-f", f.path().to_str().unwrap(), "dir"])
            .patterns()
            .unwrap();
        assert_eq!(pats.patterns, ["zero", "one", "two"]);
        assert_eq!(pats.paths, [PathBuf::from("dir")]);
    }

    #[test]
    fn last_case_flag_wins() {
        assert_eq!(p(&["s4", "x"]).case(), Case::Sensitive);
        assert_eq!(p(&["s4", "-s", "-i", "x"]).case(), Case::Insensitive);
        assert_eq!(p(&["s4", "-i", "-s", "x"]).case(), Case::Sensitive);
        assert_eq!(p(&["s4", "-i", "-S", "x"]).case(), Case::Smart);
        assert_eq!(p(&["s4", "-S", "-i", "x"]).case(), Case::Insensitive);
    }

    #[test]
    fn last_of_each_pair_wins() {
        let a = p(&["s4", "-n", "-N", "x"]);
        assert!(!a.line_number && a.no_line_number);
        let b = p(&["s4", "--hidden", "--no-hidden", "x"]);
        assert!(!b.hidden());
        assert!(p(&["s4", "--no-hidden", "-.", "x"]).hidden());
        let c = p(&["s4", "-v", "--no-invert-match", "x"]);
        assert!(!c.invert_match);
        let d = p(&["s4", "--no-messages", "--messages", "x"]);
        assert!(!d.no_messages);
        let e = p(&[
            "s4",
            "-F",
            "--no-fixed-strings",
            "-L",
            "--no-follow",
            "--json",
            "--no-json",
            "x",
        ]);
        assert!(!e.fixed_strings && !e.follow && !e.json);
        assert!(p(&["s4", "--ignore", "--no-ignore", "x"]).no_ignore());
        assert!(!p(&["s4", "--no-ignore", "--ignore", "x"]).no_ignore());
    }

    #[test]
    fn word_and_line_regexp_override_each_other() {
        let a = p(&["s4", "-x", "-w", "x"]);
        assert!(a.word_regexp && !a.line_regexp);
        let b = p(&["s4", "-w", "-x", "x"]);
        assert!(b.line_regexp && !b.word_regexp);
    }

    #[test]
    fn last_output_mode_wins() {
        let a = p(&["s4", "--json", "-c", "x"]);
        assert!(a.count && !a.json);
        let b = p(&["s4", "-c", "--json", "x"]);
        assert!(b.json && !b.count);
        let c = p(&["s4", "-c", "-l", "x"]);
        assert!(c.files_with_matches && !c.count);
    }

    #[test]
    fn binary_modes() {
        assert_eq!(p(&["s4", "x"]).binary_mode(false), BinaryMode::Quit);
        assert_eq!(p(&["s4", "x"]).binary_mode(true), BinaryMode::Binary);
        assert_eq!(
            p(&["s4", "-uuu", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(p(&["s4", "-a", "x"]).binary_mode(false), BinaryMode::Text);
        assert_eq!(
            p(&["s4", "-a", "--binary", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["s4", "-a", "-uuu", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["s4", "-uuu", "-a", "x"]).binary_mode(false),
            BinaryMode::Text
        );
        assert_eq!(
            p(&["s4", "-a", "--no-binary", "x"]).binary_mode(true),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["s4", "--no-binary", "-a", "x"]).binary_mode(true),
            BinaryMode::Text
        );
        assert_eq!(
            p(&["s4", "-auuu", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert_eq!(
            p(&["s4", "-uuua", "x"]).binary_mode(false),
            BinaryMode::Text
        );
        assert_eq!(
            p(&["s4", "-u", "-a", "-u", "-u", "x"]).binary_mode(false),
            BinaryMode::Binary
        );
        assert!(matches!(
            parse(os(&["s4", "-uuuu", "x"])),
            Err(CliError::Usage(_))
        ));
        assert_eq!(p(&["s4", "-uu", "x"]).unrestricted, 2);
    }

    #[test]
    fn fourth_unrestricted_is_ripgreps_error() {
        for argv in [
            &["s4", "-uuuu", "x"][..],
            &["s4", "-uuu", "--unrestricted", "x"],
        ] {
            assert_eq!(
                parse(os(argv)).unwrap_err().to_string(),
                "s4: error parsing flag -u: flag can only be repeated up to 3 times",
                "{argv:?}"
            );
        }
    }

    #[test]
    fn unrestricted_effects_by_position() {
        assert!(!p(&["s4", "x"]).no_ignore());
        assert!(!p(&["s4", "x"]).hidden());
        assert!(p(&["s4", "-u", "x"]).no_ignore());
        assert!(!p(&["s4", "-u", "x"]).hidden());
        assert!(!p(&["s4", "-u", "--ignore", "x"]).no_ignore());
        assert!(p(&["s4", "--ignore", "-u", "x"]).no_ignore());
        assert!(!p(&["s4", "-uu", "--no-hidden", "x"]).hidden());
        assert!(p(&["s4", "--no-hidden", "-uu", "x"]).hidden());
        let a = p(&["s4", "-u", "--ignore", "-u", "x"]);
        assert!(!a.no_ignore() && a.hidden());
        let b = p(&["s4", "--unrestricted", "--unrestricted", "x"]);
        assert!(b.no_ignore() && b.hidden() && b.unrestricted == 2);
    }

    #[test]
    fn empty_pattern_file_is_an_empty_set() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let pats = p(&["s4", "-f", f.path().to_str().unwrap()])
            .patterns()
            .unwrap();
        assert!(pats.patterns.is_empty());
    }

    #[test]
    fn context_defaults() {
        let a = p(&["s4", "-C", "2", "-A", "5", "x"]);
        assert_eq!((a.before(), a.after()), (2, 5));
        // -A and -B partially override -C regardless of order (rg 14.1.1 --help, measured).
        let b = p(&["s4", "-A", "1", "-C", "2", "x"]);
        assert_eq!((b.before(), b.after()), (2, 1));
        assert_eq!((p(&["s4", "x"]).before(), p(&["s4", "x"]).after()), (0, 0));
    }

    #[test]
    fn every_sort_is_accepted() {
        for k in ["path", "modified", "accessed", "created"] {
            assert!(p(&["s4", "--sort", k, "x"]).sort.is_some(), "{k}");
            assert!(
                p(&["s4", "--sortr", k, "x"])
                    .sort
                    .is_some_and(|s| s.reverse),
                "{k}"
            );
        }
        assert_eq!(p(&["s4", "--sort=none", "x"]).sort, None);
    }

    #[test]
    fn invalid_combinations() {
        assert!(matches!(
            parse(os(&["s4", "--no-trail", "--expand", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["s4", "--no-trail", "--within", "A", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["s4", "--no-trail", "--in-headings", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["s4", "--expand", "-c", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["s4", "--expand", "-q", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            parse(os(&["s4", "--expand", "-l", "x"])),
            Err(CliError::Usage(_))
        ));
        assert!(p(&["s4", "--expand", "--within", "A", "x"]).structural());
        assert!(!p(&["s4", "--no-trail", "-c", "x"]).structural());
    }

    #[test]
    fn help_shows_unrestricted_without_a_placeholder_and_hides_negations() {
        let help = command().render_help().to_string();
        assert!(help.contains("-u, --unrestricted"), "{help}");
        assert!(!help.contains("--unrestricted <"), "{help}");
        assert!(
            help.contains("Reduce the level of \"smart\" filtering."),
            "{help}"
        );
        assert!(help.contains("-P, --pcre2"), "{help}");
        assert!(
            !help.contains("--no-pcre2 ") && !help.contains("--no-pcre2\n"),
            "{help}"
        );
        assert!(!help.contains("--no-hidden"), "{help}");
    }

    #[test]
    fn in_headings_rejects_invert_match() {
        let e = parse(os(&["s4", "--in-headings", "-v", "x"])).unwrap_err();
        assert!(matches!(e, CliError::Usage(_)));
        assert_eq!(
            e.to_string(),
            "s4: --in-headings selects names; -v has no meaning there"
        );
        assert!(p(&["s4", "--in-headings", "-v", "--no-invert-match", "x"]).in_headings);
    }

    #[test]
    fn the_word_formats_is_a_pattern() {
        assert_eq!(p(&["s4", "formats"]).pattern.as_deref(), Some("formats"));
    }
}
