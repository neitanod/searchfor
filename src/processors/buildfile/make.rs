//! Makefile (M6 spec §7.10): a rule is a `target` named by its targets as written, over its
//! recipe; a variable (`=`, `:=`, `::=`, `:::=`, `?=`, `+=`, `!=`, after `override`, `export`
//! or `private`) and `define NAME` … `endef` are `key`s; `ifeq`, `ifneq`, `ifdef`, `ifndef`
//! … `endif` is a `conditional` named by its line, its `else` branches inside it.
//!
//! A recipe is the lines that start with the recipe prefix (a tab, or the first character
//! `.RECIPEPREFIX` is given), with the comments, blank lines and conditional directives among
//! them. Its shell text is opaque: no `:`, `#` or directive in it changes the structure.
use super::{BuildIndex, Item, Line, Named, line_at, trim};
use std::ops::Range;

/// A logical line: physical lines joined by a `\` before their end.
struct Logical {
    first: Line,
    /// The end of the last physical line, without its terminator.
    end: usize,
    next: usize,
}

/// Whether a line ends with an odd number of `\`: a backslash-newline.
fn continued(src: &[u8], l: &Line) -> bool {
    src[l.start..l.end]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}

fn logical(src: &[u8], pos: usize) -> Option<Logical> {
    let first = line_at(src, pos)?;
    let mut last = first;
    while continued(src, &last) {
        match line_at(src, last.next) {
            Some(l) => last = l,
            None => break,
        }
    }
    Some(Logical {
        first,
        end: last.end,
        next: last.next,
    })
}

/// The first word of `src[r]` (which starts at a non-blank byte) and the rest after its blanks.
fn word(src: &[u8], r: Range<usize>) -> (&[u8], Range<usize>) {
    let mut i = r.start;
    while i < r.end && !matches!(src[i], b' ' | b'\t') {
        i += 1;
    }
    (&src[r.start..i], trim(src, i..r.end))
}

/// The closer of a reference's opening bracket.
fn closer(open: u8) -> Option<u8> {
    match open {
        b'(' => Some(b')'),
        b'{' => Some(b'}'),
        _ => None,
    }
}

/// Where a variable reference `$(…)` or `${…}` that starts at `i` ends, nested references
/// included; `$$` and `$x` are two bytes. The walk keeps a stack of the open references, so
/// any nesting fits (Codex r1 #1: 300 000 levels overflowed a recursive one).
fn reference_end(src: &[u8], i: usize, end: usize) -> usize {
    let Some(close) = src.get(i + 1).copied().and_then(closer) else {
        return (i + 2).min(end);
    };
    // Each open reference: its opening bracket, and how many of them are open inside it.
    let mut open: Vec<(u8, u32)> = vec![(src[i + 1], 0)];
    let mut close = close;
    let mut j = i + 1;
    while j < end {
        let b = src[j];
        if b == b'$'
            && let Some(c) = src.get(j + 1).copied().and_then(closer)
        {
            open.push((src[j + 1], 0));
            close = c;
            j += 1;
            continue;
        }
        let (bracket, depth) = open.last_mut().expect("a reference is open");
        if b == *bracket {
            *depth += 1;
        } else if b == close {
            *depth -= 1;
            if *depth == 0 {
                open.pop();
                match open.last() {
                    None => return j + 1,
                    Some(&(o, _)) => close = closer(o).expect("an opening bracket"),
                }
            }
        }
        j += 1;
    }
    end
}

/// What a statement's first separator makes of it.
enum Separator {
    /// A variable: where its operator starts and ends.
    Assignment(Range<usize>),
    /// A rule: where its `:` is.
    Rule(usize),
}

/// The first `=` or `:` of `src[r]` outside variable references, the operators recognized
/// longest first; None when a comment or the end comes first.
fn separator(src: &[u8], r: Range<usize>) -> Option<Separator> {
    let mut i = r.start;
    while i < r.end {
        match src[i] {
            b'$' => {
                i = reference_end(src, i, r.end);
                continue;
            }
            b'#' => return None,
            // A drive letter, `C:/` or `C:\`, is part of its word (Fable r1 #54).
            b':' if i > r.start
                && src[i - 1].is_ascii_alphabetic()
                && (i - 1 == r.start || matches!(src[i - 2], b' ' | b'\t'))
                && matches!(src.get(i + 1), Some(b'/' | b'\\')) => {}
            b':' => {
                let rest = &src[i..r.end];
                for op in [&b":::="[..], b"::=", b":="] {
                    if rest.starts_with(op) {
                        return Some(Separator::Assignment(i..i + op.len()));
                    }
                }
                return Some(Separator::Rule(i));
            }
            b'=' => {
                let at = if i > r.start && matches!(src[i - 1], b'?' | b'+' | b'!') {
                    i - 1
                } else {
                    i
                };
                return Some(Separator::Assignment(at..i + 1));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `src[r]` without its leading `override`, `export` and `private` words.
fn modifiers(src: &[u8], mut r: Range<usize>) -> Range<usize> {
    loop {
        let (w, rest) = word(src, r.clone());
        if rest.is_empty() || !matches!(w, b"override" | b"export" | b"private") {
            return r;
        }
        r = rest;
    }
}

const CONDITIONALS: [&[u8]; 4] = [b"ifeq", b"ifneq", b"ifdef", b"ifndef"];

pub(crate) fn scan(src: &[u8]) -> BuildIndex {
    let mut items = BuildIndex::default();
    let mut prefix = b'\t';
    // The open conditionals and the open rule, as indices into `items`.
    let mut conditionals: Vec<usize> = Vec::new();
    let mut rule: Option<usize> = None;
    let mut pos = 0;
    while let Some(l) = logical(src, pos) {
        pos = l.next;
        if let Some(r) = rule
            && src.get(l.first.start) == Some(&prefix)
            && l.first.end > l.first.start
        {
            items.set_end(r, l.next);
            continue;
        }
        let t = trim(src, l.first.start..l.end);
        if t.is_empty() || src[t.start] == b'#' {
            continue;
        }
        let (w, _) = word(src, t.clone());
        if CONDITIONALS.contains(&w) {
            let line = trim(src, l.first.start..l.first.end);
            items.push(Item {
                kind: "conditional",
                range: l.first.start..src.len(),
                label: l.first.start..l.end,
                name: Named::Source(line.clone()),
                signature: Some(line),
            });
            conditionals.push(items.len() - 1);
            continue;
        }
        if w == b"else" && !conditionals.is_empty() {
            continue;
        }
        if w == b"endif"
            && let Some(c) = conditionals.pop()
        {
            items.set_end(c, l.next);
            continue;
        }
        // Any other statement ends the recipe.
        rule = None;
        let body = modifiers(src, t.clone());
        let (w, rest_of_define) = word(src, body.clone());
        if w == b"define" && !rest_of_define.is_empty() {
            let n = rest_of_define.start;
            let mut e = n;
            while e < rest_of_define.end
                && !matches!(src[e], b' ' | b'\t' | b'=' | b':' | b'?' | b'+' | b'!')
            {
                e += 1;
            }
            // `define` nests: GNU make 4.4.1 keeps an inner `define` … `endef` in the value
            // and ends it at the `endef` that matches (Fable r1 #20). An `override define`
            // in the body opens none.
            let mut end = src.len();
            let mut depth = 1usize;
            let mut p = l.next;
            while let Some(b) = line_at(src, p) {
                p = b.next;
                let bt = trim(src, b.start..b.end);
                if bt.is_empty() {
                    continue;
                }
                match word(src, bt).0 {
                    b"define" => depth += 1,
                    b"endef" => {
                        depth -= 1;
                        if depth == 0 {
                            end = b.next;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            pos = end;
            items.push(Item {
                kind: "key",
                range: l.first.start..end,
                label: n..e,
                name: Named::Source(n..e),
                signature: None,
            });
            continue;
        }
        match separator(src, body.clone()) {
            Some(Separator::Assignment(op)) => {
                let name = trim(src, body.start..op.start);
                if name.is_empty() {
                    continue;
                }
                if &src[name.clone()] == b".RECIPEPREFIX" {
                    let value = trim(src, op.end..l.end);
                    prefix = src
                        .get(value.start)
                        .copied()
                        .filter(|_| !value.is_empty())
                        .unwrap_or(b'\t');
                }
                items.push(Item {
                    kind: "key",
                    range: l.first.start..l.next,
                    label: name.clone(),
                    name: Named::Source(name),
                    signature: None,
                });
            }
            Some(Separator::Rule(colon)) => {
                let targets = trim(src, t.start..colon);
                if targets.is_empty() {
                    continue;
                }
                // The label is the targets: the prerequisites are inside the rule, as a
                // function's parameters are inside it (Fable r1 #13, decision 3).
                items.push(Item {
                    kind: "target",
                    range: l.first.start..l.next,
                    label: targets.clone(),
                    name: Named::Source(targets),
                    signature: Some(t.start..l.first.end),
                });
                rule = Some(items.len() - 1);
            }
            None => {}
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &[u8]) -> Vec<(&'static str, String)> {
        scan(src)
            .into_iter()
            .map(|i| match i.name {
                Named::Source(r) => (i.kind, String::from_utf8_lossy(&src[r]).into_owned()),
                Named::Ordinal(w, n) => (i.kind, format!("{w} {n}")),
            })
            .collect()
    }

    #[test]
    fn references_hide_their_separators() {
        assert_eq!(
            names(b"A := $(B:.o=.c) ${C:x=y}\n$(eval x: y)\nall: $(OBJ:%=%.o)\n"),
            [("key", "A".into()), ("target", "all".into())]
        );
        assert_eq!(names(b"x$(a:b): c\n"), [("target", "x$(a:b)".into())]);
        assert_eq!(
            names(b"x$(a ${b:c} (d:e) $(f $$:)):g\n"),
            [("target", "x$(a ${b:c} (d:e) $(f $$:))".into())]
        );
        // An unclosed reference runs to the end of its line.
        assert_eq!(names(b"x$(a ${b:c}: d\n"), []);
        assert_eq!(names(b"# a: b\nA = 1 # x: y\n"), [("key", "A".into())]);
    }

    /// Fable r1 #54: a drive letter (`C:/`, `C:\`) is part of its target, as GNU make reads
    /// it on Windows; a one-letter target before `: /x` with a blank is still a rule.
    #[test]
    fn drive_letters_are_part_of_their_targets() {
        assert_eq!(
            names(b"C:/out/needle.exe: src/needle.c\na D:\\x: y\nc: /x\n"),
            [
                ("target", "C:/out/needle.exe".into()),
                ("target", "a D:\\x".into()),
                ("target", "c".into())
            ]
        );
    }

    #[test]
    fn a_recipe_is_opaque_and_ends_at_a_statement() {
        let src = b"a: b\n\tifeq: x = y # z\n\techo \\\nmore: x\nB = 1\n\techo c\n";
        let items = scan(src);
        assert_eq!(items.len(), 2, "{items:?}");
        assert_eq!(items[0].range, 0..src.len() - 14);
        // With no rule open, a line after the recipe prefix is a statement.
        assert_eq!(names(b"\tx = y\n"), [("key", "x".into())]);
    }

    #[test]
    fn conditionals_keep_an_open_recipe_and_unknown_closers_are_text() {
        let src = b"a:\nifdef X\n\techo 1\nelse\n\techo 2\nendif\n\techo 3\n";
        let items = scan(src);
        assert_eq!(items[0].kind, "target");
        assert_eq!(items[0].range, 0..src.len());
        assert_eq!(items[1].kind, "conditional");
        let src = b"endif\nendef\nelse\nifndef Y\nA = 1\n";
        let items = scan(src);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].range, 17..src.len());
    }

    #[test]
    fn define_owns_its_body_through_endef_or_eof() {
        let src = b"override define X =\na: b\nendef\nC = 1\n";
        assert_eq!(names(src), [("key", "X".into()), ("key", "C".into())]);
        assert_eq!(scan(src)[0].range, 0..31);
        let src = b"define Y\nZ = 1\n";
        assert_eq!(scan(src)[0].range, 0..src.len());
    }

    #[test]
    fn the_recipe_prefix_is_the_first_character_given() {
        let src = b".RECIPEPREFIX := >x\na:\n>echo\n\techo\n.RECIPEPREFIX =\nb:\n\techo\n";
        let items = scan(src);
        let targets: Vec<_> = items.iter().filter(|i| i.kind == "target").collect();
        assert_eq!(targets[0].range, 20..29);
        assert_eq!(targets[1].range.end, src.len());
    }
}
