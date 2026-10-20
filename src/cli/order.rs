//! Flags whose effect depends on their place on the command line (M5a plan, D2). ripgrep
//! 14.1.1 applies each flag in order to its `LowArgs` (crates/core/flags/defs.rs, `update`), so
//! a later flag undoes part of an earlier one: `--no-ignore --ignore-vcs` reads `.gitignore`
//! again, `--pretty --no-heading` prints no heading. clap keeps, for each switch, the place of
//! its last occurrence, which is enough: a switch sets the same values every time, so only its
//! last occurrence can be the last to set any of them. Flags with values keep every place.
use super::{
    Args, BufferMode, ColorWhen, EncodingMode, Engine, IgnoreRules, Logging, MmapMode, SortKind,
    SortMode, TypeChange, values,
};
use clap::ArgMatches;
use clap::parser::ValueSource;

/// The place of the last occurrence of a switch given on the command line.
fn at(m: &ArgMatches, id: &str) -> Option<usize> {
    if m.value_source(id) != Some(ValueSource::CommandLine) {
        return None;
    }
    m.index_of(id)
}

/// Every value of a flag given on the command line, with its place.
fn values<T: Clone + Send + Sync + 'static>(m: &ArgMatches, id: &str) -> Vec<(usize, T)> {
    if m.value_source(id) != Some(ValueSource::CommandLine) {
        return Vec::new();
    }
    match (m.get_many::<T>(id), m.indices_of(id)) {
        (Some(vals), Some(idx)) => idx.zip(vals.cloned()).collect(),
        _ => Vec::new(),
    }
}

/// Applies `steps` in command-line order to `state`.
fn replay<S, E>(state: &mut S, mut steps: Vec<(usize, E)>, apply: impl Fn(&mut S, E)) {
    steps.sort_by_key(|(i, _)| *i);
    for (_, e) in steps {
        apply(state, e);
    }
}

/// Resolves every order-dependent flag of `a` from the matches it was parsed from. `first_u`
/// is the place of the first -u, which acts as --no-ignore there.
pub(super) fn resolve(m: &ArgMatches, a: &mut Args, first_u: Option<usize>) {
    a.ignores = ignores(m, first_u);
    a.sort = sort(m);
    a.engine = engine(m);
    a.encoding = encoding(m);
    a.type_changes = type_changes(m);
    a.no_unicode = no_unicode(m);
    (a.crlf, a.null_data) = terminators(m);
    (a.multiline, a.stop_on_nonmatch) = multiline(m);
    a.buffer = buffer(m);
    a.logging = logging(m);
    a.context_separator = context_separator(m);
    passthru(m, a);
    headings(m, a);
    modes(m, a);
}

fn ignores(m: &ArgMatches, first_u: Option<usize>) -> IgnoreRules {
    #[derive(Clone, Copy)]
    enum Kind {
        All,
        Dot,
        Exclude,
        Global,
        Parent,
        Vcs,
    }
    let mut steps: Vec<(usize, (Kind, bool))> = Vec::new();
    let mut push = |id: &str, kind: Kind, no: bool| {
        if let Some(i) = at(m, id) {
            steps.push((i, (kind, no)));
        }
    };
    push("no_ignore", Kind::All, true);
    push("ignore", Kind::All, false);
    push("no_ignore_dot", Kind::Dot, true);
    push("ignore_dot", Kind::Dot, false);
    push("no_ignore_exclude", Kind::Exclude, true);
    push("ignore_exclude", Kind::Exclude, false);
    push("no_ignore_global", Kind::Global, true);
    push("ignore_global", Kind::Global, false);
    push("no_ignore_parent", Kind::Parent, true);
    push("ignore_parent", Kind::Parent, false);
    push("no_ignore_vcs", Kind::Vcs, true);
    push("ignore_vcs", Kind::Vcs, false);
    if let Some(i) = first_u {
        steps.push((i, (Kind::All, true)));
    }
    let mut r = IgnoreRules::default();
    replay(&mut r, steps, |r, (kind, no)| match kind {
        Kind::All => *r = IgnoreRules::all(no),
        Kind::Dot => r.no_dot = no,
        Kind::Exclude => r.no_exclude = no,
        Kind::Global => r.no_global = no,
        Kind::Parent => r.no_parent = no,
        Kind::Vcs => r.no_vcs = no,
    });
    r
}

/// rg's `--sort`, `--sortr`, `--sort-files` (`--sort path`) and `--no-sort-files` (none).
fn sort(m: &ArgMatches) -> Option<SortMode> {
    let mut steps: Vec<(usize, Option<SortMode>)> = Vec::new();
    for (id, reverse) in [("sort_flags", false), ("sortr_flags", true)] {
        for (i, v) in values::<String>(m, id) {
            let kind = match v.as_str() {
                "path" => SortKind::Path,
                "modified" => SortKind::Modified,
                "accessed" => SortKind::Accessed,
                "created" => SortKind::Created,
                _ => {
                    steps.push((i, None));
                    continue;
                }
            };
            steps.push((i, Some(SortMode { kind, reverse })));
        }
    }
    if let Some(i) = at(m, "sort_files") {
        steps.push((
            i,
            Some(SortMode {
                kind: SortKind::Path,
                reverse: false,
            }),
        ));
    }
    if let Some(i) = at(m, "no_sort_files") {
        steps.push((i, None));
    }
    let mut s = None;
    replay(&mut s, steps, |s, v| *s = v);
    s
}

/// `-P`, `--no-pcre2`, `--engine`, `--auto-hybrid-regex` and its negation.
fn engine(m: &ArgMatches) -> Engine {
    let mut steps: Vec<(usize, Engine)> = values::<String>(m, "engine_flags")
        .into_iter()
        .map(|(i, v)| {
            let e = match v.as_str() {
                "pcre2" => Engine::Pcre2,
                "auto" => Engine::Auto,
                _ => Engine::Default,
            };
            (i, e)
        })
        .collect();
    for (id, e) in [
        ("pcre2", Engine::Pcre2),
        ("no_pcre2", Engine::Default),
        ("auto_hybrid_regex", Engine::Auto),
        ("no_auto_hybrid_regex", Engine::Default),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, e));
        }
    }
    let mut e = Engine::Default;
    replay(&mut e, steps, |e, v| *e = v);
    e
}

/// `-E` and `--no-encoding` (which is `auto`, rg's default).
fn encoding(m: &ArgMatches) -> EncodingMode {
    let mut steps: Vec<(usize, EncodingMode)> = values::<String>(m, "encoding_flags")
        .into_iter()
        .map(|(i, v)| {
            let e = match v.as_str() {
                "auto" => EncodingMode::Auto,
                "none" => EncodingMode::Disabled,
                label => EncodingMode::Label(label.to_string()),
            };
            (i, e)
        })
        .collect();
    if let Some(i) = at(m, "no_encoding") {
        steps.push((i, EncodingMode::Auto));
    }
    let mut e = EncodingMode::Auto;
    replay(&mut e, steps, |e, v| *e = v);
    e
}

/// -t, -T, --type-add and --type-clear, in command-line order.
fn type_changes(m: &ArgMatches) -> Vec<TypeChange> {
    let mut v: Vec<(usize, TypeChange)> = Vec::new();
    for (id, make) in [
        ("types", TypeChange::Select as fn(String) -> TypeChange),
        ("types_not", TypeChange::Negate),
        ("type_add", TypeChange::Add),
        ("type_clear", TypeChange::Clear),
    ] {
        v.extend(
            values::<String>(m, id)
                .into_iter()
                .map(|(i, t)| (i, make(t))),
        );
    }
    v.sort_by_key(|e| e.0);
    v.into_iter().map(|(_, c)| c).collect()
}

/// `--no-unicode`, `--no-pcre2-unicode` and their negations set one thing.
fn no_unicode(m: &ArgMatches) -> bool {
    let mut steps = Vec::new();
    for (id, no) in [
        ("no_unicode", true),
        ("unicode", false),
        ("no_pcre2_unicode", true),
        ("pcre2_unicode", false),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, no));
        }
    }
    let mut no = false;
    replay(&mut no, steps, |n, v| *n = v);
    no
}

/// `--crlf` turns `--null-data` off and `--null-data` turns `--crlf` off.
fn terminators(m: &ArgMatches) -> (bool, bool) {
    #[derive(Clone, Copy)]
    enum T {
        Crlf(bool),
        NullData,
    }
    let mut steps = Vec::new();
    for (id, t) in [
        ("crlf", T::Crlf(true)),
        ("no_crlf", T::Crlf(false)),
        ("null_data", T::NullData),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, t));
        }
    }
    let mut s = (false, false);
    replay(&mut s, steps, |s, t| match t {
        T::Crlf(on) => {
            s.0 = on;
            if on {
                s.1 = false;
            }
        }
        T::NullData => *s = (false, true),
    });
    s
}

/// `-U` turns `--stop-on-nonmatch` off and `--stop-on-nonmatch` turns `-U` off.
fn multiline(m: &ArgMatches) -> (bool, bool) {
    #[derive(Clone, Copy)]
    enum T {
        Multiline(bool),
        Stop,
    }
    let mut steps = Vec::new();
    for (id, t) in [
        ("multiline", T::Multiline(true)),
        ("no_multiline", T::Multiline(false)),
        ("stop_on_nonmatch", T::Stop),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, t));
        }
    }
    let mut s = (false, false);
    replay(&mut s, steps, |s, t| match t {
        T::Multiline(on) => {
            s.0 = on;
            if on {
                s.1 = false;
            }
        }
        T::Stop => *s = (false, true),
    });
    s
}

fn buffer(m: &ArgMatches) -> BufferMode {
    let mut steps = Vec::new();
    for (id, b) in [
        ("line_buffered", BufferMode::Line),
        ("no_line_buffered", BufferMode::Auto),
        ("block_buffered", BufferMode::Block),
        ("no_block_buffered", BufferMode::Auto),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, b));
        }
    }
    let mut b = BufferMode::Auto;
    replay(&mut b, steps, |b, v| *b = v);
    b
}

fn logging(m: &ArgMatches) -> Option<Logging> {
    let mut steps = Vec::new();
    for (id, l) in [("debug", Logging::Debug), ("trace", Logging::Trace)] {
        if let Some(i) = at(m, id) {
            steps.push((i, l));
        }
    }
    let mut l = None;
    replay(&mut l, steps, |l, v| *l = Some(v));
    l
}

fn context_separator(m: &ArgMatches) -> Option<Vec<u8>> {
    let mut steps: Vec<(usize, Option<Vec<u8>>)> =
        values::<values::Bytes>(m, "context_separator_flag")
            .into_iter()
            .map(|(i, b)| (i, Some(b.0)))
            .collect();
    if let Some(i) = at(m, "no_context_separator") {
        steps.push((i, None));
    }
    let mut s = Some(b"--".to_vec());
    replay(&mut s, steps, |s, v| *s = v);
    s
}

/// `--passthru` replaces the context, and a later -A, -B or -C replaces `--passthru` with only
/// the context flags that come after it (rg's `ContextMode::set_*`).
fn passthru(m: &ArgMatches, a: &mut Args) {
    let Some(p) = at(m, "passthru") else {
        return;
    };
    let after_it = |id: &str| at(m, id).is_some_and(|i| i > p);
    let (b, af, c) = (
        after_it("before_context"),
        after_it("after_context"),
        after_it("context"),
    );
    if !b {
        a.before_context = None;
    }
    if !af {
        a.after_context = None;
    }
    if !c {
        a.context = None;
    }
    a.passthru = !(b || af || c);
}

/// The layout and what `--pretty` sets: `--heading`, `--tree` and `--pretty` ask for the tree
/// (rg's heading), `--no-heading` and `--flat` for the one-line layout, the last one winning;
/// `--pretty` also sets `--color always` and `-n` where it comes after them (M5a plan, D5).
fn headings(m: &ArgMatches, a: &mut Args) {
    let mut steps = Vec::new();
    for (id, h) in [
        ("heading", true),
        ("tree", true),
        ("pretty", true),
        ("no_heading", false),
        ("flat", false),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, h));
        }
    }
    let mut h = None;
    replay(&mut h, steps, |h, v| *h = Some(v));
    a.tree = h == Some(true);
    a.flat = h == Some(false);
    let Some(p) = at(m, "pretty") else {
        return;
    };
    if at(m, "color").is_none_or(|i| i < p) {
        a.color = ColorWhen::Always;
    }
    // -n and -N override each other; -p acts as a -n at its place.
    let n = at(m, "line_number").or(at(m, "no_line_number"));
    if n.is_none_or(|i| i < p) {
        a.line_number = true;
        a.no_line_number = false;
    }
}

/// `--mmap`/`--no-mmap`; and the modes, of which the last one given wins, as rg 14.1.1's
/// `Mode::update`: the search modes (-c, --count-matches, -l, --files-without-match, --json)
/// and the actions (`--files`, `--type-list`, `--generate`) override each other in either
/// order, and `--no-json` goes back to the standard search only from `--json` (code review r1,
/// Codex #4).
fn modes(m: &ArgMatches, a: &mut Args) {
    a.mmap_mode = match (at(m, "mmap"), at(m, "no_mmap")) {
        (Some(_), _) => MmapMode::Always,
        (_, Some(_)) => MmapMode::Never,
        _ => MmapMode::Auto,
    };
    #[derive(Clone, PartialEq)]
    enum Mode {
        Standard,
        Count,
        CountMatches,
        FilesWithMatches,
        FilesWithoutMatch,
        Json,
        NoJson,
        Files,
        Types,
        Generate(String),
    }
    let mut steps: Vec<(usize, Mode)> = values::<String>(m, "generate")
        .into_iter()
        .map(|(i, kind)| (i, Mode::Generate(kind)))
        .collect();
    for (id, mode) in [
        ("count", Mode::Count),
        ("count_matches", Mode::CountMatches),
        ("files_with_matches", Mode::FilesWithMatches),
        ("files_without_match", Mode::FilesWithoutMatch),
        ("json", Mode::Json),
        ("no_json", Mode::NoJson),
        ("files", Mode::Files),
        ("type_list", Mode::Types),
    ] {
        if let Some(i) = at(m, id) {
            steps.push((i, mode));
        }
    }
    let mut mode = Mode::Standard;
    replay(&mut mode, steps, |mode, new| match new {
        Mode::NoJson if *mode == Mode::Json => *mode = Mode::Standard,
        Mode::NoJson => {}
        new => *mode = new,
    });
    a.count = mode == Mode::Count;
    a.count_matches = mode == Mode::CountMatches;
    a.files_with_matches = mode == Mode::FilesWithMatches;
    a.files_without_match = mode == Mode::FilesWithoutMatch;
    a.json = mode == Mode::Json;
    a.files = mode == Mode::Files;
    a.type_list = mode == Mode::Types;
    a.generate = match mode {
        Mode::Generate(kind) => Some(kind),
        _ => None,
    };
}
