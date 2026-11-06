//! reStructuredText (M6 spec §6.2): a line of text underlined, and optionally overlined, by
//! one repeated punctuation character at least as long as the text is a heading; levels follow
//! the order in which each style (the character, with or without an overline) first appears,
//! and no lone top section is promoted to a document title. `.. code-block:: LANG`,
//! `.. code:: LANG` and `.. sourcecode:: LANG` are `code` Segments over their options and
//! indented body; a paragraph ending in `::` opens a `literal` one over the indented lines
//! after it; any other directive is a `directive` Segment named by its type; a comment holds
//! no Segment. A block ends at the first line indented no deeper than its opening line.
use super::{Item, Items, Line, MAX_DEPTH, Named, Open, blank, indent, line_at, trim};
use std::ops::Range;

/// The adornment character of a line made of one repeated punctuation character, with no
/// indentation, and its length in characters.
fn adornment(src: &[u8], l: Line) -> Option<(u8, usize)> {
    let t = &src[l.start..trim(src, l.start..l.end).end];
    let c = *t.first()?;
    (c.is_ascii_punctuation() && t.iter().all(|&b| b == c)).then_some((c, t.len()))
}

/// The width of a title, in characters.
fn width(src: &[u8], r: Range<usize>) -> usize {
    String::from_utf8_lossy(&src[r]).chars().count()
}

/// An explicit markup line, `.. ` at its start: the directive's type range and what follows
/// its `::`, or None for a comment, a target or a footnote.
fn directive(src: &[u8], l: Line) -> Option<Option<(Range<usize>, Range<usize>)>> {
    let r = trim(src, l.start..l.end);
    let t = &src[r.clone()];
    if !(t == b".." || t.starts_with(b".. ")) {
        return None;
    }
    let from = r.start + 2;
    let body = trim(src, from..r.end);
    let rest = &src[body.clone()];
    let Some(k) = rest.windows(2).position(|w| w == b"::") else {
        return Some(None);
    };
    let ty = body.start..body.start + k;
    let simple = !ty.is_empty()
        && src[ty.clone()]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+' | b':'));
    let after = body.start + k + 2;
    if !simple || !matches!(src.get(after), None | Some(b' ' | b'\t' | b'\r' | b'\n')) {
        return Some(None);
    }
    Some(Some((ty, trim(src, after.min(r.end)..r.end))))
}

/// An open block: the indentation of its opening line, and its item when it has one.
struct Block {
    indent: usize,
    item: Option<usize>,
    /// Code, literal and comment bodies: nothing in them is read.
    opaque: bool,
}

pub(crate) fn scan(src: &[u8]) -> Items {
    let mut items = Items::default();
    let mut open = Open::default();
    let mut styles: Vec<(u8, bool)> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    // Where the last line that is not blank ends: a block closed now ends there.
    let mut content_end = 0;
    // Whether the previous line is text of a paragraph: then no heading starts on this one.
    let mut paragraph = false;
    // A paragraph line that ended in `::`, and its paragraph's indentation.
    let mut literal: Option<(Line, usize)> = None;
    let mut para_indent = 0;
    let mut pos = 0;
    while let Some(l) = line_at(src, pos) {
        pos = l.next;
        if blank(src, l) {
            paragraph = false;
            continue;
        }
        let ind = indent(src, l);
        while blocks.last().is_some_and(|b| ind <= b.indent) {
            let b = blocks.pop().unwrap();
            if let Some(i) = b.item {
                items.set_end(i, content_end);
            }
        }
        if let Some((at, p)) = literal.take()
            && ind > p
        {
            let item = (open.len() + blocks.len() < MAX_DEPTH).then(|| {
                let mut it = Item::new(
                    "code",
                    at.start..src.len(),
                    at.start..at.next,
                    Named::Word("literal"),
                );
                it.signature = Some(at.start..at.end);
                items.push(it);
                items.len() - 1
            });
            blocks.push(Block {
                indent: p,
                item,
                opaque: true,
            });
        }
        content_end = l.next;
        if blocks.last().is_some_and(|b| b.opaque) {
            continue;
        }
        if let Some(d) = directive(src, l) {
            paragraph = false;
            let room = open.len() + blocks.len() < MAX_DEPTH;
            let (item, opaque) = match d {
                None => (None, true),
                Some((ty, arg)) => {
                    let code = matches!(&src[ty.clone()], b"code-block" | b"code" | b"sourcecode");
                    let item = room.then(|| {
                        let name = if code && !arg.is_empty() {
                            let word = arg.start
                                + src[arg.clone()]
                                    .iter()
                                    .position(|b| matches!(b, b' ' | b'\t'))
                                    .unwrap_or(arg.len());
                            arg.start..word
                        } else {
                            ty.clone()
                        };
                        let kind = if code { "code" } else { "directive" };
                        let mut it = Item::new(
                            kind,
                            l.start..src.len(),
                            l.start..l.next,
                            Named::Source(vec![name.clone()]),
                        );
                        it.anchor = name.start;
                        if code {
                            it.signature = Some(trim(src, l.start..l.end).start..l.end);
                        }
                        items.push(it);
                        items.len() - 1
                    });
                    (item, code)
                }
            };
            blocks.push(Block {
                indent: ind,
                item,
                opaque,
            });
            continue;
        }
        if blocks.is_empty()
            && !paragraph
            && let Some(h) = title(src, l, &mut styles)
        {
            let (level, name, label_end, next) = h;
            open.close_to(&mut items, level, l.start);
            let mut it = Item::new(
                "heading",
                l.start..src.len(),
                l.start..label_end,
                Named::Source(vec![name.clone()]),
            );
            it.anchor = name.start;
            it.level = Some(level);
            it.signature = Some(name.clone());
            open.push(&mut items, it, level, 0);
            content_end = label_end;
            pos = next;
            paragraph = false;
            continue;
        }
        if !paragraph {
            para_indent = ind;
        }
        paragraph = true;
        if src[trim(src, l.start..l.end)].ends_with(b"::") {
            literal = Some((l, para_indent));
        }
    }
    while let Some(b) = blocks.pop() {
        if let Some(i) = b.item {
            items.set_end(i, content_end);
        }
    }
    open.finish(&mut items, src.len());
    items
}

/// The heading that starts at line `l`, when there is one: (level, name, end of its label,
/// where the next line starts). `styles` learns a style the first time it appears.
fn title(
    src: &[u8],
    l: Line,
    styles: &mut Vec<(u8, bool)>,
) -> Option<(usize, Range<usize>, usize, usize)> {
    let next = line_at(src, l.next)?;
    let (style, name, last) = if let Some((c, over)) = adornment(src, l) {
        // An overline, the title, and an underline of the same character.
        let under = line_at(src, next.next)?;
        let (u, ulen) = adornment(src, under)?;
        let name = trim(src, next.start..next.end);
        if u != c || name.is_empty() || adornment(src, next).is_some() {
            return None;
        }
        let w = width(src, name.clone());
        if over < w || ulen < w {
            return None;
        }
        ((c, true), name, under)
    } else {
        let (c, ulen) = adornment(src, next)?;
        if src[l.start..l.end]
            .first()
            .is_some_and(|b| matches!(b, b' ' | b'\t'))
        {
            return None;
        }
        let name = trim(src, l.start..l.end);
        if ulen < width(src, name.clone()) {
            return None;
        }
        ((c, false), name, next)
    };
    let level = match styles.iter().position(|&s| s == style) {
        Some(i) => i + 1,
        None => {
            styles.push(style);
            styles.len()
        }
    };
    Some((level, name, last.next, last.next))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cursor_equals_model, params, trail};

    #[test]
    fn styles_take_levels_in_order_of_appearance() {
        let p = params("rst");
        let src = b"=====\nTop\n=====\n\nA\n-\n\na-needle\n\nB\n~\n\nb-needle\n\nC\n-\n\nc-needle\n\n=====\nTop2\n=====\n\nd-needle\n";
        assert_eq!(trail(src, &p, "a-needle"), ["Top", "A"]);
        assert_eq!(trail(src, &p, "b-needle"), ["Top", "A", "B"]);
        assert_eq!(trail(src, &p, "c-needle"), ["Top", "C"]);
        assert_eq!(trail(src, &p, "d-needle"), ["Top2"]);
        cursor_equals_model(src, &p);
    }

    #[test]
    fn a_short_underline_or_a_paragraph_line_is_no_heading() {
        let p = params("rst");
        let src = b"Long title\n===\n\npara\nmore\n----\n\nx-needle\n";
        assert_eq!(trail(src, &p, "x-needle"), Vec::<String>::new());
    }

    #[test]
    fn directives_nest_and_comments_hold_nothing() {
        let p = params("rst");
        let src = b"T\n=\n\n.. note::\n\n   .. warning:: careful\n\n      w-needle\n\n   n-needle\n\n.. a comment\n   c-needle\n.. code::\n\n   .. note::\n   k-needle\n\nafter-needle\n";
        assert_eq!(trail(src, &p, "w-needle"), ["T", "note", "warning"]);
        assert_eq!(trail(src, &p, "n-needle"), ["T", "note"]);
        assert_eq!(trail(src, &p, "c-needle"), ["T"]);
        assert_eq!(trail(src, &p, "k-needle"), ["T", "code"]);
        assert_eq!(trail(src, &p, "after-needle"), ["T"]);
        cursor_equals_model(src, &p);
    }
}
