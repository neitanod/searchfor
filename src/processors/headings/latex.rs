//! LaTeX (M6 spec §6.4): `\part` to `\subparagraph`, starred or not, are headings of levels
//! 1 to 7, named by their braced title as written (balanced braces, over several lines; the
//! optional short title ignored); `\title{…}` is the document. `\begin{X}` … `\end{X}` is an
//! `environment` Segment named `X`; `lstlisting` with `[language=L]` and `minted` with `{L}` are
//! `code` Segments named `L`, and `verbatim`, `Verbatim`, `comment` and the listings without a
//! language are `code` Segments named by the environment, with nothing in them read as a
//! command. The Definition's hidden environments hold Segments without showing. A heading ends
//! at its enclosing environment's end at the latest; an `\end` with no open environment of its
//! name closes nothing; an environment left open runs to the end of the file. A `%` comment
//! (not `\%`) is text. `\input` and `\include` are never followed.
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
    let mut items = Items::default();
    let mut frames: Vec<Frame> = Vec::new();
    // Open environments by name: an `\end` of a name none holds closes nothing.
    let mut envs: HashMap<&[u8], usize> = HashMap::new();
    // Segments open, the document included.
    let mut depth = 0usize;
    let mut document = false;
    let mut i = 0;
    while let Some(k) = memchr::memchr2(b'\\', b'%', &src[i..]) {
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
                let end = line_end(src, d + 1);
                if let Some(k) = memchr::memchr(delim, &src[d + 1..end]) {
                    i = d + 2 + k;
                }
            }
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
                    let item = (depth < MAX_DEPTH).then(|| {
                        let mut it =
                            Item::new("heading", at..src.len(), at..end, Named::Source(pieces));
                        it.anchor = anchor;
                        it.level = Some(level);
                        it.signature = Some(at..line_end(src, at));
                        items.push(it);
                        depth += 1;
                        items.len() - 1
                    });
                    frames.push(Frame {
                        item,
                        level: Some(level),
                        env: None,
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
                    it.signature = Some(at..line_end(src, at));
                    items.push(it);
                }
                i = close;
                continue;
            }
            i = end;
            let item = (depth < MAX_DEPTH).then(|| {
                let mut it = Item::new(
                    "environment",
                    at..src.len(),
                    at..end,
                    Named::Source(vec![name.clone()]),
                );
                it.anchor = name.start;
                it.signature = Some(at..line_end(src, at));
                it.hidden = hidden.contains(&env);
                items.push(it);
                depth += 1;
                items.len() - 1
            });
            *envs.entry(env).or_default() += 1;
            frames.push(Frame {
                item,
                level: None,
                env: Some(name),
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
            while let Some(f) = frames.pop() {
                if let Some(x) = f.item {
                    items.set_end(x, end);
                    depth -= 1;
                }
                if let Some(e) = f.env {
                    let e = &src[e];
                    if let Some(n) = envs.get_mut(e) {
                        *n -= 1;
                    }
                    if e == env {
                        break;
                    }
                }
            }
        }
    }
    items
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
}
