//! LaTeX (M6 spec §6.4): `\part` to `\subparagraph`, starred or not, are headings of levels
//! 1 to 7, named by their braced title as written (balanced braces, over several lines; the
//! optional short title ignored); `\title{…}` is the document. `\begin{X}` … `\end{X}` is an
//! `environment` Segment named `X`; `lstlisting` with `[language=L]` and `minted` with `{L}` are
//! `code` Segments named `L`, and `verbatim`, `Verbatim`, `comment` and the listings without a
//! language are `code` Segments named by the environment, with nothing in them read as a
//! command. The Definition's hidden environments hold Segments without showing. A heading ends
//! at its enclosing environment's end at the latest; an `\end` with no open environment of its
//! name closes nothing; an environment left open runs to the end of the file. A `%` comment
//! (not `\%`) is text, and so is the body of a macro definition. `\input` and `\include` are
//! never followed.
use super::{Item, Items, MAX_DEPTH, Named, trim};
use std::collections::HashMap;
use std::ops::Range;

/// The sectioning commands, in the order of their levels, 1 to 7.
const SECTIONS: [&[u8]; 7] = [
    b"part",
    b"chapter",
    b"section",
    b"subsection",
    b"subsubsection",
    b"paragraph",
    b"subparagraph",
];

/// Environments whose bodies are code: nothing in them is a command.
const VERBATIM: [&[u8]; 6] = [
    b"verbatim",
    b"verbatim*",
    b"Verbatim",
    b"comment",
    b"lstlisting",
    b"minted",
];

/// Bytes a braced title or an optional argument may span: past them it does not close, and the
/// command is text. A title is a line or a few; the bound keeps an unbalanced brace from making
/// every later command scan to the end of the file.
const GROUP_MAX: usize = 4096;

/// The environments hidden by default (M6 spec §6.4).
pub(crate) const HIDDEN: [&str; 4] = ["document", "itemize", "enumerate", "description"];

/// An open heading or environment.
struct Frame {
    /// Its index entry; None when it opened past `MAX_DEPTH`.
    item: Option<usize>,
    /// A heading's level.
    level: Option<usize>,
    /// An environment's name.
    env: Option<Range<usize>>,
    /// Environments of that name the frame stands for: past `MAX_DEPTH`, where they hold no
    /// Segment, a run of one name is one frame (code review M6 r1, Fable #14). 1 otherwise.
    count: usize,
}

/// Where the line holding `at` ends, without its `\r\n` or `\n`.
fn line_end(src: &[u8], at: usize) -> usize {
    let end = memchr::memchr(b'\n', &src[at..]).map_or(src.len(), |k| at + k);
    if end > at && src[end - 1] == b'\r' {
        end - 1
    } else {
        end
    }
}

/// Where the signature that starts at `at` ends: its line's end, looked for no further than
/// `compact::signature` decodes, so that in a file without `\n` (a lone CR ends no line, M6
/// spec §3, point 7) each command does not look for it to the end of the file.
fn signature_end(src: &[u8], at: usize) -> usize {
    let to = src
        .len()
        .min(at.saturating_add(super::compact::SIGNATURE_BYTES));
    line_end(&src[..to], at)
}

/// `r` without spaces, tabs and carriage returns around it.
fn trim_cr(src: &[u8], r: Range<usize>) -> Range<usize> {
    let mut r = trim(src, r);
    while r.end > r.start && src[r.end - 1] == b'\r' {
        r = trim(src, r.start..r.end - 1);
    }
    r
}

/// Past the spaces and tabs at `at`.
fn skip_blanks(src: &[u8], mut at: usize) -> usize {
    while matches!(src.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}

/// The balanced group that opens at `at` with `open` and closes with `close`: its text's
/// pieces, one per line without its comments, and where it ends, past its closing character.
/// None when it does not close within `GROUP_MAX` bytes.
fn group(src: &[u8], at: usize, open: u8, close: u8) -> Option<(Vec<Range<usize>>, usize)> {
    let limit = src.len().min(at.saturating_add(GROUP_MAX));
    let mut pieces = Vec::new();
    let mut piece = Some(at + 1);
    let mut depth = 0usize;
    let mut i = at;
    let push = |pieces: &mut Vec<Range<usize>>, from: Option<usize>, to: usize| {
        if let Some(from) = from {
            let r = trim_cr(src, from..to);
            if !r.is_empty() {
                pieces.push(r);
            }
        }
    };
    while i < limit {
        let b = src[i];
        if b == b'\\' {
            i += 2;
        } else if b == b'%' {
            push(&mut pieces, piece.take(), i);
            i = memchr::memchr(b'\n', &src[i..limit]).map_or(limit, |k| i + k);
        } else if b == b'\n' {
            push(&mut pieces, piece.take(), i);
            piece = Some(i + 1);
            i += 1;
        } else if b == open {
            depth += 1;
            i += 1;
        } else if b == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                push(&mut pieces, piece.take(), i);
                return Some((pieces, i + 1));
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Commands that define a macro or an environment: their bodies are text.
const DEFINITIONS: [&[u8]; 9] = [
    b"newcommand",
    b"renewcommand",
    b"providecommand",
    b"newenvironment",
    b"renewenvironment",
    b"def",
    b"gdef",
    b"edef",
    b"xdef",
];

/// Where a definition whose command `cmd` ends at `at` ends, past its body (both bodies of an
/// environment): what it defines is text there, never a heading or an environment (code review
/// M6 r1, Fable #25; spec §6.4 makes macros that expand to sectioning commands text). None
/// when it is not written as one, or a part does not close within `GROUP_MAX` bytes: then the
/// walk reads on after the command, as it read before.
fn definition_end(src: &[u8], cmd: &[u8], at: usize) -> Option<usize> {
    let mut j = at;
    if src.get(j) == Some(&b'*') {
        j += 1;
    }
    j = skip_blanks(src, j);
    let bodies = if matches!(cmd, b"def" | b"gdef" | b"edef" | b"xdef") {
        // `\def\name#1#2{…}`: the name, its parameter text up to the body's brace.
        if src.get(j) != Some(&b'\\') {
            return None;
        }
        let to = src.len().min(j.saturating_add(GROUP_MAX));
        j += memchr::memchr(b'{', &src[j..to])?;
        1
    } else {
        // `{\name}` or `\name` (`{name}` for an environment), `[n]`, `[default]`, the body.
        if src.get(j) == Some(&b'{') {
            j = group(src, j, b'{', b'}')?.1;
        } else if src.get(j) == Some(&b'\\') {
            j += 1 + src[j + 1..]
                .iter()
                .take_while(|b| b.is_ascii_alphabetic())
                .count()
                .max(1);
        } else {
            return None;
        }
        for _ in 0..2 {
            j = optional(src, skip_blanks(src, j))?.1;
        }
        if cmd.ends_with(b"environment") { 2 } else { 1 }
    };
    for _ in 0..bodies {
        j = skip_blanks(src, j);
        if src.get(j) != Some(&b'{') {
            return None;
        }
        j = group(src, j, b'{', b'}')?.1;
    }
    Some(j)
}

/// The text of a one-line `{name}` at `at`, with no nesting: (name, end past `}`).
fn simple(src: &[u8], at: usize) -> Option<(Range<usize>, usize)> {
    if src.get(at) != Some(&b'{') {
        return None;
    }
    let to = src.len().min(at + 1 + 256);
    let k = src[at + 1..to]
        .iter()
        .position(|&b| matches!(b, b'}' | b'\n' | b'{' | b'%' | b'\\'))?;
    (src[at + 1 + k] == b'}').then(|| (trim_cr(src, at + 1..at + 1 + k), at + k + 2))
}

/// The value of `language=` in a listings option list `[…]` (its pieces' range), a pair of
/// braces around it removed.
fn language_option(src: &[u8], r: Range<usize>) -> Option<Range<usize>> {
    let mut from = r.start;
    let mut depth = 0usize;
    for i in r.start..=r.end {
        let b = src.get(i).copied().filter(|_| i < r.end);
        match b {
            Some(b'{') => depth += 1,
            Some(b'}') => depth = depth.saturating_sub(1),
            Some(b',') | None if depth == 0 => {
                let part = trim_cr(src, from..i);
                if let Some(eq) = src[part.clone()].iter().position(|&b| b == b'=') {
                    let key = trim_cr(src, part.start..part.start + eq);
                    if src[key].eq_ignore_ascii_case(b"language") {
                        let mut v = trim_cr(src, part.start + eq + 1..part.end);
                        if v.len() >= 2 && src[v.start] == b'{' && src[v.end - 1] == b'}' {
                            v = trim_cr(src, v.start + 1..v.end - 1);
                        }
                        return (!v.is_empty()).then_some(v);
                    }
                }
                from = i + 1;
            }
            _ => {}
        }
    }
    None
}

/// The optional `[…]` at `at`: its inner range and where it ends; `at` itself when there is
/// none. None when it opens and never closes.
fn optional(src: &[u8], at: usize) -> Option<(Option<Range<usize>>, usize)> {
    if src.get(at) != Some(&b'[') {
        return Some((None, at));
    }
    let (_, end) = group(src, at, b'[', b']')?;
    Some((Some(at + 1..end - 1), end))
}

pub(crate) fn scan(src: &[u8], hidden: &[&[u8]]) -> Items {
    scan_counted(src, hidden).0
}

/// `scan`, and the most frames its walk held.
fn scan_counted(src: &[u8], hidden: &[&[u8]]) -> (Items, usize) {
    let mut peak = 0;
    let mut items = Items::default();
    let mut frames: Vec<Frame> = Vec::new();
    // Open environments by name: an `\end` of a name none holds closes nothing.
    let mut envs: HashMap<&[u8], usize> = HashMap::new();
    // Segments open, the document included.
    let mut depth = 0usize;
    let mut document = false;
    let mut i = 0;
    while let Some(k) = memchr::memchr2(b'\\', b'%', &src[i..]) {
        peak = peak.max(frames.len());
        let at = i + k;
        if src[at] == b'%' {
            i = memchr::memchr(b'\n', &src[at..]).map_or(src.len(), |k| at + k);
            continue;
        }
        let name_end = at
            + 1
            + src[at + 1..]
                .iter()
                .take_while(|b| b.is_ascii_alphabetic())
                .count();
        if name_end == at + 1 {
            // `\%`, `\\`, `\{`: an escaped character.
            i = (at + 2).min(src.len());
            continue;
        }
        i = name_end;
        let cmd = &src[at + 1..name_end];
        if cmd == b"verb" {
            // `\verb|…|`: its text holds no command, up to the same delimiter on its line.
            let mut d = name_end;
            if src.get(d) == Some(&b'*') {
                d += 1;
            }
            if let Some(&delim) = src.get(d)
                && !delim.is_ascii_alphabetic()
                && !matches!(delim, b' ' | b'\n' | b'\r')
            {
                // The delimiter or the line's end, whichever comes first: a line's end looked
                // for first was the end of a file without `\n`, for each `\verb`.
                let rest = &src[d + 1..];
                if let Some(k) = memchr::memchr2(delim, b'\n', rest)
                    && rest[k] == delim
                {
                    i = d + 2 + k;
                }
            }
            continue;
        }
        if DEFINITIONS.contains(&cmd)
            && let Some(end) = definition_end(src, cmd, name_end)
        {
            i = end;
            continue;
        }
        let section = SECTIONS.iter().position(|s| *s == cmd);
        if cmd == b"title" || section.is_some() {
            let mut j = name_end;
            if section.is_some() && src.get(j) == Some(&b'*') {
                j += 1;
            }
            j = skip_blanks(src, j);
            if section.is_some() {
                let Some((_, after)) = optional(src, j) else {
                    continue;
                };
                j = skip_blanks(src, after);
            }
            if src.get(j) != Some(&b'{') {
                continue;
            }
            let Some((pieces, end)) = group(src, j, b'{', b'}') else {
                continue;
            };
            i = end;
            let anchor = j + 1;
            match section {
                None => {
                    if !document {
                        document = true;
                        depth += 1;
                        let mut it =
                            Item::new("document", 0..src.len(), at..end, Named::Source(pieces));
                        it.anchor = anchor;
                        items.push(it);
                    }
                }
                Some(s) => {
                    let level = s + 1;
                    // Headings of this level or deeper close here, never past an environment.
                    while let Some(f) = frames.last() {
                        match f.level {
                            Some(l) if l >= level => {
                                if let Some(x) = f.item {
                                    items.set_end(x, at);
                                    depth -= 1;
                                }
                                frames.pop();
                            }
                            _ => break,
                        }
                    }
                    if depth >= MAX_DEPTH {
                        // A heading past the depth opens no Segment, and its frame would
                        // change no later close: headings close only while the frames on top
                        // are headings of their level or deeper.
                        continue;
                    }
                    let mut it = Item::new(
                        "heading",
                        at..src.len(),
                        at..end,
                        Named::source_or(pieces, "heading"),
                    );
                    it.anchor = anchor;
                    it.level = Some(level);
                    it.signature = Some(at..signature_end(src, at));
                    items.push(it);
                    depth += 1;
                    frames.push(Frame {
                        item: Some(items.len() - 1),
                        level: Some(level),
                        env: None,
                        count: 1,
                    });
                }
            }
            continue;
        }
        if cmd == b"begin" {
            let j = skip_blanks(src, name_end);
            let Some((name, mut end)) = simple(src, j) else {
                continue;
            };
            let env = &src[name.clone()];
            if VERBATIM.contains(&env) {
                // The code's language: lstlisting's `language=`, minted's argument.
                let mut lang = None;
                if let Some((opts, after)) = optional(src, end) {
                    if env == b"lstlisting" {
                        lang = opts.and_then(|o| language_option(src, o));
                        end = after;
                    } else if env == b"minted" {
                        let a = skip_blanks(src, after);
                        if let Some((l, after)) = simple(src, a) {
                            lang = Some(l).filter(|l| !l.is_empty());
                            end = after;
                        }
                    }
                }
                let mut needle = b"\\end{".to_vec();
                needle.extend_from_slice(env);
                needle.push(b'}');
                let close = memchr::memmem::find(&src[end..], &needle)
                    .map_or(src.len(), |k| end + k + needle.len());
                if depth < MAX_DEPTH {
                    let n = lang.unwrap_or(name.clone());
                    let mut it =
                        Item::new("code", at..close, at..end, Named::Source(vec![n.clone()]));
                    it.anchor = n.start;
                    it.signature = Some(at..signature_end(src, at));
                    items.push(it);
                }
                i = close;
                continue;
            }
            i = end;
            *envs.entry(env).or_default() += 1;
            if depth >= MAX_DEPTH
                && let Some(top) = frames.last_mut()
                && top.item.is_none()
                && top.env.as_ref().is_some_and(|e| &src[e.clone()] == env)
            {
                top.count += 1;
                continue;
            }
            let item = (depth < MAX_DEPTH).then(|| {
                let mut it = Item::new(
                    "environment",
                    at..src.len(),
                    at..end,
                    Named::source_or(vec![name.clone()], "environment"),
                );
                it.anchor = name.start;
                it.signature = Some(at..signature_end(src, at));
                it.hidden = hidden.contains(&env);
                items.push(it);
                depth += 1;
                items.len() - 1
            });
            frames.push(Frame {
                item,
                level: None,
                env: Some(name),
                count: 1,
            });
            continue;
        }
        if cmd == b"end" {
            let j = skip_blanks(src, name_end);
            let Some((name, end)) = simple(src, j) else {
                continue;
            };
            i = end;
            let env = &src[name];
            if envs.get(env).is_none_or(|&n| n == 0) {
                continue;
            }
            // The environment closes, and everything opened inside it.
            while let Some(f) = frames.last_mut() {
                let same = f.env.as_ref().is_some_and(|e| &src[e.clone()] == env);
                if same && f.count > 1 {
                    // The innermost of a run past the depth.
                    f.count -= 1;
                    if let Some(n) = envs.get_mut(env) {
                        *n -= 1;
                    }
                    break;
                }
                let f = frames.pop().unwrap();
                if let Some(x) = f.item {
                    items.set_end(x, end);
                    depth -= 1;
                }
                if let Some(e) = f.env {
                    if let Some(n) = envs.get_mut(&src[e]) {
                        *n -= f.count;
                    }
                    if same {
                        break;
                    }
                }
            }
        }
    }
    peak = peak.max(frames.len());
    (items, peak)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cursor_equals_model, params, trail};

    #[test]
    fn headings_close_by_level_and_inside_their_environment() {
        let p = params("latex");
        let src = b"\\chapter{C}\n\\section{A}\n\\begin{table}[h]\n\\paragraph{P}\nt-needle\n\\end{table}\na-needle\n\\section{B} b-needle\n\\begin{itemize}\n\\item i-needle\n\\end{itemize}\n";
        assert_eq!(trail(src, &p, "t-needle"), ["C", "A", "table", "P"]);
        assert_eq!(trail(src, &p, "a-needle"), ["C", "A"]);
        assert_eq!(trail(src, &p, "b-needle"), ["C", "B"]);
        assert_eq!(trail(src, &p, "i-needle"), ["C", "B"]);
        assert_eq!(trail(src, &p, "\\section{B}"), ["C"]);
        cursor_equals_model(src, &p);
    }

    #[test]
    fn listings_name_their_language_and_hide_their_commands() {
        let p = params("latex");
        let src = b"\\section{S}\n\\begin{lstlisting}[caption=x, language={Python}]\n\\section{l-needle}\n\\end{lstlisting}\n\\begin{lstlisting}\nm-needle\n\\end{lstlisting}\n\\begin{minted}[linenos]{rust}\nr-needle\n\\end{minted}\n\\verb|\\section{v}| v-needle\n";
        assert_eq!(trail(src, &p, "l-needle"), ["S", "Python"]);
        assert_eq!(trail(src, &p, "m-needle"), ["S", "lstlisting"]);
        assert_eq!(trail(src, &p, "r-needle"), ["S", "rust"]);
        assert_eq!(trail(src, &p, "v-needle"), ["S"]);
        cursor_equals_model(src, &p);
    }

    /// Code review M6 r1, Fable #14: past `MAX_DEPTH`, environments that hold no Segment
    /// keep one frame per run of a name, and headings none, so unclosed `\begin{x}` lines
    /// cost the walk no memory each; their `\end`s close what they closed before.
    #[test]
    fn environments_past_the_depth_cost_a_frame_per_run() {
        let p = params("latex");
        let mut src = "\\begin{a}\n".repeat(super::MAX_DEPTH);
        src.push_str(&"\\begin{x}\n\\section{s}\n".repeat(20_000));
        src.push_str("\\begin{y}\n\\begin{x}\nneedle-in\n");
        src.push_str(&"\\end{x}\n".repeat(20_001));
        src.push_str("\\end{a}\nneedle-out\n\\end{a}\n\\section{T}\nneedle-t\n");
        let (_, frames) = super::scan_counted(src.as_bytes(), &[]);
        assert!(frames <= super::MAX_DEPTH + 3, "{frames} frames");
        let t = trail(src.as_bytes(), &p, "needle-in");
        assert_eq!(t.len(), super::MAX_DEPTH);
        // The `\end{x}`s close the run and `y` inside it; one `\end{a}` the innermost `a`.
        let t = trail(src.as_bytes(), &p, "needle-out");
        assert_eq!(t.len(), super::MAX_DEPTH - 1);
        let t = trail(src.as_bytes(), &p, "needle-t");
        assert_eq!(t.len(), super::MAX_DEPTH - 1);
        assert_eq!(t.last().unwrap(), "T");
    }

    /// Code review M6 r1, Fable #25: the body of a macro definition is text, so a sectioning
    /// command or a `\\begin` in it opens nothing (`\\newcommand`, `\\renewcommand`,
    /// `\\providecommand`, `\\newenvironment`, `\\def` and its kin).
    #[test]
    fn a_macro_definition_is_text() {
        let p = params("latex");
        let src = b"\\newcommand{\\mysec}[1]{\\section{#1}}\n\\renewcommand*\\other[2][x]{\\chapter{#2}}\n\\def\\d#1#2{\\section{#1}\\begin{figure}}\n\\newenvironment{box}[1]{\\begin{center}\\section{#1}}{\\end{center}}\nneedle-top\n\\section{Real}\nneedle-a\n\\mysec{Fake}\nneedle-b\n\\providecommand{\\p}{\\section{P}}\nneedle-c\n";
        assert_eq!(trail(src, &p, "needle-top"), Vec::<String>::new());
        assert_eq!(trail(src, &p, "needle-a"), ["Real"]);
        assert_eq!(trail(src, &p, "needle-b"), ["Real"]);
        assert_eq!(trail(src, &p, "needle-c"), ["Real"]);
        cursor_equals_model(src, &p);
    }

    /// Code review M6 r1, Fable #56: an environment or a heading with an empty name is named
    /// by its kind, a word s4 adds.
    #[test]
    fn an_empty_name_is_named_by_its_kind() {
        let p = params("latex");
        let src = b"\\section{A}\n\\begin{}\nneedle-a\n\\end{}\n\\subsection{ }\nneedle-b\n";
        assert_eq!(trail(src, &p, "needle-a"), ["A", "environment"]);
        assert_eq!(trail(src, &p, "needle-b"), ["A", "heading"]);
        cursor_equals_model(src, &p);
    }

    #[test]
    fn an_unclosed_title_is_text_and_depth_is_bounded() {
        let p = params("latex");
        let src = b"\\section{A}\n\\section{open\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["A"]);
        let deep = format!(
            "{}needle{}",
            "\\begin{x}\n".repeat(5000),
            "\\end{x}\n".repeat(5000)
        );
        let t = trail(deep.as_bytes(), &p, "needle");
        assert_eq!(t.len(), super::MAX_DEPTH);
        let after = format!("{deep}\\section{{T}}\nlast-needle\n");
        assert_eq!(trail(after.as_bytes(), &p, "last-needle"), ["T"]);
    }

    /// Found by tests/fuzz_m6.rs's CR mutation: with no `\n` in the file (a lone CR ends no
    /// line, M6 spec §3, point 7) each heading's, environment's and listing's signature and
    /// each `\verb` looked for their line's end at the end of the file. The same commands with
    /// CR or LF take about the same time; 256 KiB with CR took 30 times longer.
    #[test]
    fn commands_with_no_line_break_after_them_scan_in_linear_time() {
        use crate::processors::blocks::tests::fastest;
        for unit in [
            "\\section{S} needle",
            "\\begin{x} needle",
            "\\begin{verbatim} needle \\end{verbatim}",
            "\\verb|x| needle",
        ] {
            let lf = format!("{unit}\n").repeat((256 << 10) / (unit.len() + 1));
            let cr = lf.replace('\n', "\r");
            let t_lf = fastest(|| drop(super::scan(lf.as_bytes(), &[])));
            let t_cr = fastest(|| drop(super::scan(cr.as_bytes(), &[])));
            assert!(
                t_cr < t_lf * 4 + std::time::Duration::from_millis(20),
                "{unit}: CR {t_cr:?}, LF {t_lf:?}"
            );
        }
    }

    /// A signature line longer than what `compact::signature` decodes keeps its text when its
    /// end is looked for that far only.
    #[test]
    fn a_long_signature_line_keeps_its_text() {
        use crate::processors::Processor;
        let p = params("latex");
        let lf = format!("\\section{{S}} {}\nneedle\n", "a".repeat(5000));
        let cr = lf.replace('\n', "\r");
        for src in [lf, cr] {
            let at = src.find("needle").unwrap();
            let input = super::super::tests::input(src.as_bytes(), "x");
            let t = super::super::Headings.trails(&input, &p, &[at]).unwrap();
            let want = format!("\\section{{S}} {}", "a".repeat(188));
            assert_eq!(t[0][0].signature.as_deref(), Some(want.as_str()));
            cursor_equals_model(src.as_bytes(), &p);
        }
    }
}
