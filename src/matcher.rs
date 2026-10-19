//! The pattern matcher of a run (M5a plan, D7): Rust's regex engine, as ripgrep's default, or
//! PCRE2 (`-P`, `--engine`). One type for both, so the searcher, the sinks and the printers
//! take a `&Matcher` whatever the engine; each method goes to the engine's own.
use crate::cli::{Args, Case, Engine};
use grep_matcher::{ByteSet, LineMatchKind, LineTerminator, Match, NoError};

/// The matcher of a run.
#[derive(Clone, Debug)]
pub enum PatternMatcher {
    Rust(grep_regex::RegexMatcher),
    Pcre2(Pcre2),
}

/// A PCRE2 matcher shared by the search threads. PCRE2 needs scratch space per search, which
/// the pcre2 crate keeps in a pool behind a lock: twelve threads searching with one matcher
/// waited on it, 1.55 × rg's time over the M4 corpus (rg builds a matcher per thread). Each
/// thread here matches with a clone of its own, made on its first search and kept for the run
/// (`with`): a clone shares the compiled pattern and has a pool of its own.
#[derive(Clone, Debug)]
pub struct Pcre2 {
    shared: grep_pcre2::RegexMatcher,
    /// Tells this matcher's clones from another matcher's in a thread's cache.
    id: u64,
}

impl Pcre2 {
    fn new(m: grep_pcre2::RegexMatcher) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Pcre2 {
            shared: m,
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// `f` over this thread's clone of the matcher.
    fn with<R>(&self, f: impl FnOnce(&grep_pcre2::RegexMatcher) -> R) -> R {
        thread_local! {
            static LOCAL: std::cell::RefCell<Option<(u64, grep_pcre2::RegexMatcher)>> =
                const { std::cell::RefCell::new(None) };
        }
        LOCAL.with(|l| {
            let mut l = l.borrow_mut();
            if l.as_ref().is_none_or(|(id, _)| *id != self.id) {
                *l = Some((self.id, self.shared.clone()));
            }
            f(&l.as_ref().expect("set above").1)
        })
    }
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
            PatternMatcher::Pcre2(m) => m.with(|m| m.find_at(haystack, at)).map_err(Error::Pcre2),
        }
    }

    fn new_captures(&self) -> Result<Captures, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(Captures::Rust(m.new_captures()?)),
            PatternMatcher::Pcre2(m) => m
                .shared
                .new_captures()
                .map(Captures::Pcre2)
                .map_err(Error::Pcre2),
        }
    }

    fn capture_count(&self) -> usize {
        match self {
            PatternMatcher::Rust(m) => m.capture_count(),
            PatternMatcher::Pcre2(m) => m.shared.capture_count(),
        }
    }

    fn capture_index(&self, name: &str) -> Option<usize> {
        match self {
            PatternMatcher::Rust(m) => m.capture_index(name),
            PatternMatcher::Pcre2(m) => m.shared.capture_index(name),
        }
    }

    fn captures_at(&self, haystack: &[u8], at: usize, caps: &mut Captures) -> Result<bool, Error> {
        match (self, caps) {
            (PatternMatcher::Rust(m), Captures::Rust(c)) => Ok(m.captures_at(haystack, at, c)?),
            (PatternMatcher::Pcre2(m), Captures::Pcre2(c)) => m
                .with(|m| m.captures_at(haystack, at, c))
                .map_err(Error::Pcre2),
            _ => unreachable!("captures of another engine"),
        }
    }

    fn shortest_match_at(&self, haystack: &[u8], at: usize) -> Result<Option<usize>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.shortest_match_at(haystack, at)?),
            PatternMatcher::Pcre2(m) => m
                .with(|m| m.shortest_match_at(haystack, at))
                .map_err(Error::Pcre2),
        }
    }

    fn non_matching_bytes(&self) -> Option<&ByteSet> {
        match self {
            PatternMatcher::Rust(m) => m.non_matching_bytes(),
            PatternMatcher::Pcre2(m) => m.shared.non_matching_bytes(),
        }
    }

    fn line_terminator(&self) -> Option<LineTerminator> {
        match self {
            PatternMatcher::Rust(m) => m.line_terminator(),
            PatternMatcher::Pcre2(m) => m.shared.line_terminator(),
        }
    }

    fn find_candidate_line(&self, haystack: &[u8]) -> Result<Option<LineMatchKind>, Error> {
        match self {
            PatternMatcher::Rust(m) => Ok(m.find_candidate_line(haystack)?),
            PatternMatcher::Pcre2(m) => m
                .with(|m| m.find_candidate_line(haystack))
                .map_err(Error::Pcre2),
        }
    }
}

/// The matcher the command line asks for, as rg 14.1.1 builds it (hiargs.rs `matcher`):
/// `--engine default` (Rust's, with the suggestions of rg's errors), `pcre2`, or `auto`, which
/// tries Rust's and then PCRE2 and reports both errors when neither compiles.
pub fn build(args: &Args, patterns: &[String]) -> anyhow::Result<PatternMatcher> {
    match args.engine {
        Engine::Default => rust(args, patterns).map_err(|e| anyhow::anyhow!(suggest_pcre2(e))),
        Engine::Pcre2 => pcre2(args, patterns).map_err(|e| anyhow::anyhow!(e)),
        Engine::Auto => {
            let rust_err = match rust(args, patterns) {
                Ok(m) => return Ok(m),
                Err(e) => e,
            };
            let pcre_err = match pcre2(args, patterns) {
                Ok(m) => return Ok(m),
                Err(e) => e,
            };
            let divider = "~".repeat(79);
            anyhow::bail!(
                "regex could not be compiled with either the default regex engine or with \
                 PCRE2.\n\ndefault regex engine error:\n{divider}\n{rust_err}\n{divider}\n\n\
                 PCRE2 regex engine error:\n{pcre_err}"
            )
        }
    }
}

/// Rust's engine (hiargs.rs `matcher_rust`). A pattern that cannot match a NUL while binary
/// data is detected is refused, as rg refuses it (`ban_byte`).
fn rust(args: &Args, patterns: &[String]) -> Result<PatternMatcher, String> {
    let mut b = grep_regex::RegexMatcherBuilder::new();
    b.multi_line(true)
        .unicode(!args.no_unicode)
        .octal(false)
        .fixed_strings(args.fixed_strings && !args.no_fixed_strings);
    match args.case() {
        Case::Insensitive => b.case_insensitive(true),
        Case::Smart => b.case_smart(true),
        Case::Sensitive => b.case_insensitive(false),
    };
    if args.line_regexp {
        b.whole_line(true);
    } else if args.word_regexp {
        b.word(true);
    }
    if args.multiline {
        b.dot_matches_new_line(args.multiline_dotall);
        if args.crlf {
            b.crlf(true).line_terminator(None);
        }
    } else {
        b.line_terminator(Some(b'\n')).dot_matches_new_line(false);
        if args.crlf {
            b.crlf(true);
        }
        if args.null_data {
            b.line_terminator(Some(b'\0'));
        }
    }
    if let Some(n) = args.regex_size_limit {
        b.size_limit(n);
    }
    if let Some(n) = args.dfa_size_limit {
        b.dfa_size_limit(n);
    }
    if detects_binary(args) {
        b.ban_byte(Some(b'\0'));
    }
    b.build_many(patterns)
        .map(PatternMatcher::Rust)
        .map_err(|e| suggest_text(suggest_multiline(e.to_string())))
}

/// PCRE2 (hiargs.rs `matcher_pcre2`): JIT on 64-bit systems with a 10 MiB stack, Unicode
/// unless `--no-unicode`.
fn pcre2(args: &Args, patterns: &[String]) -> Result<PatternMatcher, String> {
    let mut b = grep_pcre2::RegexMatcherBuilder::new();
    b.multi_line(true)
        .fixed_strings(args.fixed_strings && !args.no_fixed_strings);
    match args.case() {
        Case::Insensitive => b.caseless(true),
        Case::Smart => b.case_smart(true),
        Case::Sensitive => b.caseless(false),
    };
    if args.line_regexp {
        b.whole_line(true);
    } else if args.word_regexp {
        b.word(true);
    }
    if cfg!(target_pointer_width = "64") {
        b.jit_if_available(true).max_jit_stack_size(Some(10 << 20));
    }
    if !args.no_unicode {
        b.utf(true).ucp(true);
    }
    if args.multiline {
        b.dotall(args.multiline_dotall);
    }
    if args.crlf {
        b.crlf(true);
    }
    b.build_many(patterns)
        .map(|m| PatternMatcher::Pcre2(Pcre2::new(m)))
        .map_err(|e| e.to_string())
}

/// rg bans a NUL in the pattern unless binary detection is off everywhere: `-a`, or
/// `--null-data` (hiargs.rs `BinaryDetection::is_none`).
fn detects_binary(args: &Args) -> bool {
    args.binary_choice != crate::cli::BinaryChoice::Text && !args.null_data
}

/// rg's `suggest_pcre2`, applied to the default engine's errors (`suggest_other_engine`).
fn suggest_pcre2(msg: String) -> String {
    if msg.contains("backreferences") || msg.contains("look-around") {
        format!(
            "{msg}\n\nConsider enabling PCRE2 with the --pcre2 flag, which can handle \
             backreferences\nand look-around."
        )
    } else {
        msg
    }
}

/// rg's `suggest_multiline`: a literal line terminator in the pattern.
fn suggest_multiline(msg: String) -> String {
    if msg.contains("the literal") && msg.contains("not allowed") {
        format!(
            "{msg}\n\nConsider enabling multiline mode with the --multiline flag (or -U for \
             short).\nWhen multiline mode is enabled, new line characters can be matched."
        )
    } else {
        msg
    }
}

/// rg's `suggest_text`: a NUL in the pattern while binary data is detected.
fn suggest_text(msg: String) -> String {
    if msg.contains("pattern contains \"\\0\"") {
        format!(
            "{msg}\n\nConsider enabling text mode with the --text flag (or -a for short). \
             Otherwise,\nbinary detection is enabled and matching a NUL byte is impossible."
        )
    } else {
        msg
    }
}

/// What `--pcre2-version` prints: PCRE2's version and whether its JIT is available, as rg
/// 14.1.1's `special` prints them.
pub fn pcre2_version() -> String {
    let (major, minor) = pcre2::version();
    let jit = if pcre2::is_jit_available() {
        "JIT is available"
    } else {
        "JIT is unavailable"
    };
    format!("PCRE2 {major}.{minor} is available ({jit})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use grep_matcher::Matcher as _;

    fn args(v: &[&str]) -> Args {
        let mut argv = vec!["s4"];
        argv.extend_from_slice(v);
        crate::cli::parse(argv.iter().map(Into::into).collect()).unwrap()
    }

    fn find(argv: &[&str], pat: &str, hay: &[u8]) -> Option<(usize, usize)> {
        let m = build(&args(argv), &[pat.to_string()]).unwrap();
        m.find(hay).unwrap().map(|m| (m.start(), m.end()))
    }

    fn error(argv: &[&str], pat: &str) -> String {
        build(&args(argv), &[pat.to_string()])
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn the_engine_the_flags_choose() {
        assert!(matches!(
            build(&args(&["x"]), &["x".into()]).unwrap(),
            PatternMatcher::Rust(_)
        ));
        assert!(matches!(
            build(&args(&["-P", "x"]), &["x".into()]).unwrap(),
            PatternMatcher::Pcre2(_)
        ));
        // auto: Rust's when it compiles, PCRE2's for look-around.
        let auto = |p: &str| build(&args(&["--engine", "auto", "x"]), &[p.into()]).unwrap();
        assert!(matches!(auto("a+"), PatternMatcher::Rust(_)));
        assert!(matches!(auto("a(?=b)"), PatternMatcher::Pcre2(_)));
    }

    #[test]
    fn pcre2_matches_what_rust_cannot() {
        assert_eq!(
            find(&["-P", "x"], r"needle(?= two)", b"a needle two"),
            Some((2, 8))
        );
        assert_eq!(find(&["-P", "x"], r"(\w)\1", b"abccd"), Some((2, 4)));
        assert_eq!(
            find(&["-P", "-i", "x"], "NEEDLE", b"a needle"),
            Some((2, 8))
        );
        assert_eq!(
            find(&["-P", "-w", "x"], "need", b"needle need"),
            Some((7, 11))
        );
        assert_eq!(
            find(&["-P", "-x", "x"], "need", b"needle\nneed\n"),
            Some((7, 11))
        );
        // Unicode classes unless --no-unicode.
        assert_eq!(find(&["-P", "x"], r"\w+", "ñandú".as_bytes()), Some((0, 7)));
        assert_eq!(
            find(&["-P", "--no-unicode", "x"], r"^\w+", "ñandú".as_bytes()),
            None
        );
    }

    /// Each thread matches with its own clone of a PCRE2 matcher, which the next matcher of the
    /// thread replaces: two matchers in one thread keep their own patterns, and threads give
    /// what one thread gives.
    #[test]
    fn pcre2_clones_per_thread() {
        let a = build(&args(&["-P", "x"]), &["a(?=b)".into()]).unwrap();
        let b = build(&args(&["-P", "x"]), &["b(?=c)".into()]).unwrap();
        for _ in 0..2 {
            assert_eq!(a.find(b"xabc").unwrap().map(|m| m.start()), Some(1));
            assert_eq!(b.find(b"xabc").unwrap().map(|m| m.start()), Some(2));
        }
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for _ in 0..100 {
                        assert_eq!(a.find(b"xxab").unwrap().map(|m| m.start()), Some(2));
                    }
                });
            }
        });
    }

    #[test]
    fn rust_engine_flags() {
        // --no-unicode: \w is ASCII, and a non-ASCII byte is matched only by its bytes.
        assert_eq!(
            find(&["--no-unicode", "x"], r"\w+", "ñx".as_bytes()),
            Some((2, 3))
        );
        // --crlf: $ before \r\n.
        assert_eq!(
            find(&["--crlf", "x"], r"needle$", b"needle\r\n"),
            Some((0, 6))
        );
        assert_eq!(find(&["x"], r"needle$", b"needle\r\n"), None);
        // -U: \n in a pattern, and --multiline-dotall makes `.` cross lines.
        assert_eq!(find(&["-U", "x"], r"a\nb", b"a\nb"), Some((0, 3)));
        assert_eq!(find(&["-U", "x"], r"a.b", b"a\nb"), None);
        assert_eq!(
            find(&["-U", "--multiline-dotall", "x"], r"a.b", b"a\nb"),
            Some((0, 3))
        );
        // --null-data: NUL is the line terminator, which only a -U pattern may match, as in rg;
        // and binary detection is off, so no NUL is banned.
        let e = error(&["--null-data", "x"], r"a\x00");
        assert!(
            e.starts_with("the literal \"\\0\" is not allowed in a regex"),
            "{e}"
        );
        assert_eq!(
            find(&["-U", "--null-data", "x"], r"a\x00b", b"a\x00b"),
            Some((0, 3))
        );
    }

    #[test]
    fn errors_with_ripgreps_suggestions() {
        let e = error(&["x"], r"a(?=b)");
        assert!(e.contains("look-around"), "{e}");
        assert!(e.ends_with("Consider enabling PCRE2 with the --pcre2 flag, which can handle backreferences\nand look-around."), "{e}");
        let e = error(&["x"], r"a\nb");
        assert!(
            e.contains(
                "Consider enabling multiline mode with the --multiline flag (or -U for short)."
            ),
            "{e}"
        );
        let e = error(&["x"], r"a\x00b");
        assert!(
            e.starts_with("pattern contains \"\\0\" but it is impossible to match"),
            "{e}"
        );
        assert!(
            e.ends_with("binary detection is enabled and matching a NUL byte is impossible."),
            "{e}"
        );
        assert!(build(&args(&["-a", "x"]), &[r"a\x00b".into()]).is_ok());
        let e = error(&["-P", "x"], "(");
        // rg's offset: grep-pcre2 wraps each pattern in a group.
        assert_eq!(
            e,
            "PCRE2: error compiling pattern at offset 5: missing closing parenthesis"
        );
        let e = error(&["--engine", "auto", "x"], "(?<=a)(");
        assert!(e.starts_with("regex could not be compiled with either the default regex engine or with PCRE2.\n\ndefault regex engine error:\n~~~"), "{e}");
        assert!(
            e.contains("\n\nPCRE2 regex engine error:\nPCRE2: error compiling pattern"),
            "{e}"
        );
        let e = error(&["--regex-size-limit", "10", "x"], "a{1000}");
        assert!(e.contains("exceeds size limit"), "{e}");
    }

    #[test]
    fn pcre2_version_as_ripgrep_prints_it() {
        let v = pcre2_version();
        assert!(v.starts_with("PCRE2 10."), "{v}");
        assert!(
            v.ends_with(" is available (JIT is available)") || v.ends_with("(JIT is unavailable)"),
            "{v}"
        );
    }
}
