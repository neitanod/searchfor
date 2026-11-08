//! justfile (M6 spec §7.11): a recipe (`name params: deps`, with `[attributes]` and `@` before
//! it) is a `recipe` named by its identifier, its signature the declaration line, over its
//! indented body; a variable (`NAME := …`, `export NAME := …`), a setting (`set NAME …`) or an
//! `alias NAME := …` is a `key`. The comments above a recipe are its documentation and lie
//! outside it. Nothing is evaluated: no interpolation, import, `just` or shell runs.
use super::{BuildIndex, Item, Named, line_at, trim};
use std::ops::Range;

/// The identifier that starts at `i`: `[A-Za-z_][A-Za-z0-9_-]*`, possibly empty.
fn identifier(src: &[u8], i: usize, end: usize) -> Range<usize> {
    if i >= end || !(src[i].is_ascii_alphabetic() || src[i] == b'_') {
        return i..i;
    }
    let mut j = i + 1;
    while j < end && (src[j].is_ascii_alphanumeric() || matches!(src[j], b'_' | b'-')) {
        j += 1;
    }
    i..j
}

/// The first byte at or past `i` that is no space or tab.
fn skip_blanks(src: &[u8], mut i: usize, end: usize) -> usize {
    while i < end && matches!(src[i], b' ' | b'\t') {
        i += 1;
    }
    i
}

/// Whether `:=` starts at `i`.
fn assigns(src: &[u8], i: usize, end: usize) -> bool {
    end >= i + 2 && &src[i..i + 2] == b":="
}

/// The `:` that ends a recipe's parameters, outside quotes and parentheses; None when the line
/// has none, or has `:=` first (a variable).
fn dependency_colon(src: &[u8], mut i: usize, end: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    let mut depth = 0usize;
    while i < end {
        let b = src[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'\'' | b'"' | b'`' => quote = Some(b),
                b'(' => depth += 1,
                b')' => depth = depth.saturating_sub(1),
                b'#' if depth == 0 => return None,
                b':' if depth == 0 => return (!assigns(src, i, end)).then_some(i),
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Where what follows the attribute lists that start at `i` (`[a] [b("x")]`) starts, past the
/// blanks after them; None when one does not close on its line. Fable r2 #13: a recipe may
/// follow them on their line (from memory of `just`, unverified; reading it accepts more).
fn after_attributes(src: &[u8], mut i: usize, end: usize) -> Option<usize> {
    while i < end && src[i] == b'[' {
        let mut quote: Option<u8> = None;
        let mut depth = 0usize;
        loop {
            let &b = src[..end].get(i)?;
            i += 1;
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None => match b {
                    b'\'' | b'"' | b'`' => quote = Some(b),
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                },
            }
        }
        i = skip_blanks(src, i, end);
    }
    Some(i)
}

/// The name of a `key` statement at `t`: `alias`, `export` or `set` and a name, or a name and
/// `:=`.
fn key_name(src: &[u8], t: Range<usize>) -> Option<Range<usize>> {
    let first = identifier(src, t.start, t.end);
    let after = skip_blanks(src, first.end, t.end);
    if !first.is_empty() && assigns(src, after, t.end) {
        return Some(first);
    }
    let keyword = &src[first.clone()];
    if !matches!(keyword, b"alias" | b"export" | b"set") || after == first.end {
        return None;
    }
    let name = identifier(src, after, t.end);
    if name.is_empty() {
        return None;
    }
    let next = skip_blanks(src, name.end, t.end);
    // `set NAME` alone is a boolean setting; the others need their `:=`.
    (assigns(src, next, t.end) || (keyword == b"set" && next == t.end)).then_some(name)
}

pub(crate) fn scan(src: &[u8]) -> BuildIndex {
    let mut items = BuildIndex::default();
    // The open recipe, as an index into `items`, and where the attribute lines before the next
    // declaration start.
    let mut recipe: Option<usize> = None;
    let mut attributes: Option<usize> = None;
    let mut pos = 0;
    while let Some(l) = line_at(src, pos) {
        pos = l.next;
        let t = trim(src, l.start..l.end);
        let indented = t.start > l.start;
        if let Some(r) = recipe {
            if t.is_empty() {
                // A gap: the body may go on after it.
                continue;
            }
            if indented {
                items.set_end(r, l.next);
                continue;
            }
        }
        recipe = None;
        if t.is_empty() {
            attributes = None;
            continue;
        }
        if src[t.start] == b'#' {
            continue;
        }
        if indented {
            attributes = None;
            continue;
        }
        let start = attributes.unwrap_or(l.start);
        let line = t.clone();
        let mut t = t;
        if src[t.start] == b'[' {
            match after_attributes(src, t.start, t.end) {
                Some(d) if d < t.end && src[d] != b'#' => t.start = d,
                _ => {
                    attributes.get_or_insert(l.start);
                    continue;
                }
            }
        }
        attributes = None;
        if let Some(name) = key_name(src, t.clone()) {
            items.push(Item {
                kind: "key",
                range: l.start..l.next,
                label: name.clone(),
                name: Named::Source(name),
                signature: None,
            });
            continue;
        }
        let at = t.start + usize::from(src[t.start] == b'@');
        let name = identifier(src, at, t.end);
        if name.is_empty() || dependency_colon(src, name.end, t.end).is_none() {
            // Text: an unrecognized line.
            continue;
        }
        // The label is the name and its `@`: the attributes, parameters and dependencies are
        // inside the recipe, as a function's parameters are inside it (Fable r1 #13, decision 3).
        items.push(Item {
            kind: "recipe",
            range: start..l.next,
            label: t.start..name.end,
            name: Named::Source(name),
            signature: Some(line),
        });
        recipe = Some(items.len() - 1);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &[u8]) -> Vec<(&'static str, String, Range<usize>)> {
        scan(src)
            .into_iter()
            .map(|i| match i.name {
                Named::Source(r) | Named::Again(r) => (
                    i.kind,
                    String::from_utf8_lossy(&src[r]).into_owned(),
                    i.range,
                ),
                Named::Ordinal(w, n) => (i.kind, format!("{w} {n}"), i.range),
            })
            .collect()
    }

    #[test]
    fn parameters_hide_their_colons_and_variables_are_keys() {
        let src =
            b"a x='h:1' (y):\n  z\nb := 'c: d'\nexport E := 1\nset shell := ['sh']\nset quiet\n";
        let n: Vec<(&str, String)> = names(src).into_iter().map(|(k, n, _)| (k, n)).collect();
        assert_eq!(
            n,
            [
                ("recipe", "a".into()),
                ("key", "b".into()),
                ("key", "E".into()),
                ("key", "shell".into()),
                ("key", "quiet".into()),
            ]
        );
    }

    #[test]
    fn a_body_spans_its_gaps_and_ends_at_a_top_level_line() {
        let src = b"[a]\n[b]\nr:\n  x\n\n  y\n\n# doc\ns: r\n";
        let n = names(src);
        assert_eq!(n[0], ("recipe", "r".into(), 0..20));
        assert_eq!(n[1], ("recipe", "s".into(), 27..src.len()));
        // An attribute followed by a blank line is no recipe's.
        let src = b"[a]\n\nr:\n";
        assert_eq!(names(src)[0].2, 5..src.len());
        // `mod`, `import` and a stray line are text; an indented line with no recipe too.
        assert_eq!(names(b"import 'x'\nmod y\n  z\nstray\n"), []);
    }

    /// Fable r2 #13 (unverified: `just` is not installed here): attributes on the recipe's own
    /// line. Reading them accepts more input, so it is harmless if `just` refuses the line.
    #[test]
    fn attributes_may_share_the_recipe_line() {
        let src = b"[group(\"x]\")] [private] @build: dep\n  echo\n[no-cd]\n[a] other:\n  echo\n[b] # c\nlast:\n";
        let items: Vec<Item> = scan(src).into_iter().collect();
        let n: Vec<(&str, String, Range<usize>)> = names(src);
        assert_eq!(
            n,
            [
                ("recipe", "build".into(), 0..43),
                ("recipe", "other".into(), 43..69),
                ("recipe", "last".into(), 69..src.len()),
            ]
        );
        // The label is the name and its `@`; the signature is the whole line.
        assert_eq!(&src[items[0].label.clone()], b"@build");
        assert_eq!(&src[items[0].signature.clone().unwrap()], &src[0..35]);
        // An attribute that never closes leaves the line to the attributes.
        assert_eq!(names(b"[a x:\n  y\n"), []);
    }
}
