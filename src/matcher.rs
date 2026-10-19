//! The pattern matcher of a run (M5a plan, D7): Rust's regex engine, as ripgrep's default, or
//! PCRE2 (`-P`, `--engine`). One type for both, so the searcher, the sinks and the printers
//! take a `&Matcher` whatever the engine; each method goes to the engine's own.
use crate::cli::{Args, Case};
use grep_matcher::{ByteSet, LineMatchKind, LineTerminator, Match, NoError};

/// The matcher of a run.
#[derive(Clone, Debug)]
pub enum PatternMatcher {
    Rust(grep_regex::RegexMatcher),
    Pcre2(grep_pcre2::RegexMatcher),
}

/// The capture groups of one match, of either engine.
#[derive(Clone, Debug)]
pub enum Captures {
    Rust(grep_regex::RegexCaptures),
    Pcre2(grep_pcre2::RegexCaptures),
}

impl grep_matcher::Captures for Captures {
    fn len(&self) -> usize {
        match self {
            Captures::Rust(c) => c.len(),
            Captures::Pcre2(c) => c.len(),
        }
    }

    fn get(&self, i: usize) -> Option<Match> {
        match self {
            Captures::Rust(c) => c.get(i),
            Captures::Pcre2(c) => c.get(i),
        }
    }
}

/// A search error: Rust's engine has none; PCRE2's (a match limit, for one) is reported as an
/// error of the file, as rg reports it.
#[derive(Debug)]
pub enum Error {
    Pcre2(grep_pcre2::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Pcre2(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<NoError> for Error {
    fn from(_: NoError) -> Error {
        unreachable!("Rust's regex engine reports no error")
    }
}

impl grep_matcher::Matcher for PatternMatcher {
    type Captures = Captures;
    type Error = Error;

    fn find_at(&self, haystack: &[u8], at: usize) -> Result<Option<Match>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.find_at(haystack, at)?),
            PatternMatcher::Pcre2(m) => m.find_at(haystack, at).map_err(Error::Pcre2),
        }
    }

    fn new_captures(&self) -> Result<Captures, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(Captures::Rust(m.new_captures()?)),
            PatternMatcher::Pcre2(m) => m.new_captures().map(Captures::Pcre2).map_err(Error::Pcre2),
        }
    }

    fn capture_count(&self) -> usize {
        match self {
            PatternMatcher::Rust(m) => m.capture_count(),
            PatternMatcher::Pcre2(m) => m.capture_count(),
        }
    }

    fn capture_index(&self, name: &str) -> Option<usize> {
        match self {
            PatternMatcher::Rust(m) => m.capture_index(name),
            PatternMatcher::Pcre2(m) => m.capture_index(name),
        }
    }

    fn captures_at(&self, haystack: &[u8], at: usize, caps: &mut Captures) -> Result<bool, Error> {
        match (self, caps) {
            (PatternMatcher::Rust(m), Captures::Rust(c)) => Ok(m.captures_at(haystack, at, c)?),
            (PatternMatcher::Pcre2(m), Captures::Pcre2(c)) => {
                m.captures_at(haystack, at, c).map_err(Error::Pcre2)
            }
            _ => unreachable!("captures of another engine"),
        }
    }

    fn shortest_match_at(&self, haystack: &[u8], at: usize) -> Result<Option<usize>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.shortest_match_at(haystack, at)?),
            PatternMatcher::Pcre2(m) => m.shortest_match_at(haystack, at).map_err(Error::Pcre2),
        }
    }

    fn non_matching_bytes(&self) -> Option<&ByteSet> {
        match self {
            PatternMatcher::Rust(m) => m.non_matching_bytes(),
            PatternMatcher::Pcre2(m) => m.non_matching_bytes(),
        }
    }

    fn line_terminator(&self) -> Option<LineTerminator> {
        match self {
            PatternMatcher::Rust(m) => m.line_terminator(),
            PatternMatcher::Pcre2(m) => m.line_terminator(),
        }
    }

    fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.find_candidate_line(haystack)?),
            PatternMatcher::Pcre2(m) => m.find_candidate_line(haystack).map_err(Error::Pcre2),
        }
    }
}

/// The matcher the command line asks for (M5a plan, Task 1.4: the Rust engine with the flags
/// of M1–M4; chunk 2 adds PCRE2 and the rest of rg's matcher flags).
pub fn build(args: &Args, patterns: &[String]) -> anyhow::Result<PatternMatcher> {
    let mut b = grep_regex::RegexMatcherBuilder::new();
    match args.case() {
        Case::Insensitive => {
            b.case_insensitive(true);
        }
        Case::Smart => {
            b.case_smart(true);
        }
        Case::Sensitive => {}
    }
    b.word(args.word_regexp)
        .whole_line(args.line_regexp)
        .fixed_strings(args.fixed_strings && !args.no_fixed_strings)
        .multi_line(true); // as ripgrep: ^ and $ match at line boundaries inside a buffer
    // The line terminator agrees with the searcher's (C3), as rg's `matcher_rust` sets it:
    // none with -U, NUL with --null-data, CRLF-aware with --crlf.
    if args.multiline {
        b.dot_matches_new_line(args.multiline_dotall);
        if args.crlf {
            b.crlf(true).line_terminator(None);
        }
    } else {
        b.line_terminator(Some(b'\n'));
        if args.crlf {
            b.crlf(true);
        }
        if args.null_data {
            b.line_terminator(Some(b'\0'));
        }
    }
    // An empty pattern set never reaches here: C9 exits 1 before searching.
    b.build_many(patterns)
        .map(PatternMatcher::Rust)
        .map_err(|e| anyhow::anyhow!(suggestion(e.to_string())))
}

/// The message of a pattern that does not compile, with ripgrep 14.1.1's suggestions when it
/// has them (hiargs.rs, `suggest_multiline` and then `suggest_pcre2` on its result): multiline
/// mode for a literal `\n`, PCRE2 for a look-around or a backreference, both when the message,
/// which repeats the pattern, names both (Fable M4 code r9 #4). s4 has neither yet, as
/// rg-compat.md says, and prints rg's message whole (Fable M4 code r8 #4).
fn suggestion(msg: String) -> String {
    let msg = if msg.contains("the literal") && msg.contains("not allowed") {
        format!(
            "{msg}\n\nConsider enabling multiline mode with the --multiline flag (or -U for \
             short).\nWhen multiline mode is enabled, new line characters can be matched."
        )
    } else {
        msg
    };
    if msg.contains("backreferences") || msg.contains("look-around") {
        format!(
            "{msg}\n\nConsider enabling PCRE2 with the --pcre2 flag, which can handle \
             backreferences\nand look-around."
        )
    } else {
        msg
    }
}

/// What `--pcre2-version` prints (chunk 2).
pub fn pcre2_version() -> String {
    String::new()
}
