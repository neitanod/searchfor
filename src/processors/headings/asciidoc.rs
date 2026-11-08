//! AsciiDoc (M6 spec §6.1): `= Title` at the top is the document; `==` to `======`, or the
//! Markdown-style `##` to `######`, are headings of levels 1 to 5, a space after the markers.
//! A delimited block runs to the same delimiter, and the headings in it are text: a listing is
//! a `code` Segment named by the `[source,LANG]` before it, or `listing`; a literal block, by
//! that `LANG` or `literal`; a fence, its info string, that `LANG` or `listing`. The other
//! blocks, tables included, hold no Segment.
//! `include::` is never followed.
use super::{Item, Items, Line, Named, Open, blank, lines, trim};
use std::ops::Range;

/// What a delimiter line opens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Listing,
    Literal,
    Fence,
    /// Comment, passthrough, sidebar, example, quote blocks and tables: text, no Segment.
    Opaque,
}

/// The backticks that open a fence line: three or more (code review M6 r1, Fable #21).
fn fence_run(src: &[u8], l: Line) -> usize {
    src[l.start..l.end]
        .iter()
        .take_while(|&&b| b == b'`')
        .count()
}

/// The block a line opens, when it is a delimiter: four or more of one delimiter character,
/// or a fence of three backticks and its info string.
fn delimiter(src: &[u8], l: Line) -> Option<Block> {
    let t = &src[trim(src, l.start..l.end)];
    if l.start < l.end && src[l.start] == b'`' && t.starts_with(b"```") {
        return Some(Block::Fence);
    }
    if src.get(l.start) != t.first() || t.len() < 4 {
        return None;
    }
    // A table, `|===` or a data table's `,===`, `:===`, `!===`: its rows are text (code
    // review M6 r1, Fable #22).
    if matches!(t[0], b'|' | b',' | b':' | b'!') && t[1..].iter().all(|&b| b == b'=') {
        return Some(Block::Opaque);
    }
    let c = t[0];
    if !t.iter().all(|&b| b == c) {
        return None;
    }
    match c {
        b'-' => Some(Block::Listing),
        b'.' => Some(Block::Literal),
        b'/' | b'+' | b'*' | b'=' | b'_' => Some(Block::Opaque),
        _ => None,
    }
}

/// Whether `l` underlines the text line `text` as a legacy two-line title does: one repeated
/// `=`, `-`, `~`, `^` or `+`, as long as the text within one character (Asciidoctor's rule).
/// Such a title is text (M6 spec §6.1), and its underline opens no block (code review M6 r1,
/// Fable #7).
fn underline(src: &[u8], text: Line, l: Line) -> bool {
    let u = &src[l.start..l.end];
    let Some(&c) = u.first() else {
        return false;
    };
    if !matches!(c, b'=' | b'-' | b'~' | b'^' | b'+') || !u.iter().all(|&b| b == c) {
        return false;
    }
    let t = &src[trim(src, text.start..text.end)];
    let chars = String::from_utf8_lossy(t).chars().count();
    u.len().abs_diff(chars) <= 1
}

/// When the line is an attribute entry, `:name: value`, `:!name:` or `:name!:`, which may
/// stand above the document title (code review M6 r1, Fable #24; Asciidoctor's header, from
/// memory: the change only lets more files have a title): its value, what follows `:name:`,
/// the blanks before it included.
fn attribute_value(src: &[u8], l: Line) -> Option<Range<usize>> {
    let t = &src[l.start..l.end];
    let rest = t.strip_prefix(b":")?;
    let rest = rest.strip_prefix(b"!").unwrap_or(rest);
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    if !rest.first().is_some_and(|&b| word(b)) {
        return None;
    }
    let n = rest.iter().take_while(|&&b| word(b) || b == b'-').count();
    let rest = &rest[n..];
    let rest = rest.strip_prefix(b"!").unwrap_or(rest);
    let value = rest.strip_prefix(b":")?;
    value
        .first()
        .is_none_or(|b| matches!(b, b' ' | b'\t'))
        .then_some(l.end - value.len()..l.end)
}

/// Whether a line ends in a space and `\`, which continues an attribute entry's value on the
/// next line (code review M6 r2, Fable #19; Asciidoctor's header, from memory: the change only
/// lets more files have a title). A bare trailing `\`, as in a Windows path, ends the value
/// (code review M6 r3, Fable #1). `r` is the value: the blanks after `:name:` are none of it,
/// so a value that is a bare `\` ends too (r4, Fable #4).
fn continued(src: &[u8], r: Range<usize>) -> bool {
    src[trim(src, r)].ends_with(b" \\")
}

/// Whether the line is a list item: `*`, `-` or `.` runs, or a number or a letter and a
/// `.`, followed by a space. A list item starts no paragraph, so it is no two-line title
/// (code review M6 r2, Fable #1).
fn list_item(src: &[u8], l: Line) -> bool {
    let t = &src[trim(src, l.start..l.end)];
    let n = match t.first() {
        Some(&c @ (b'*' | b'.')) => t.iter().take_while(|&&b| b == c).count(),
        Some(b'-') => 1,
        Some(b'0'..=b'9') => {
            let d = t.iter().take_while(|b| b.is_ascii_digit()).count();
            if t.get(d) != Some(&b'.') {
                return false;
            }
            d + 1
        }
        Some(c) if c.is_ascii_alphabetic() && t.get(1) == Some(&b'.') => 2,
        _ => return false,
    };
    matches!(t.get(n), Some(b' ' | b'\t'))
}

/// A heading line: (markers, name range). The markers are one run of `=` or `#`, six at most,
/// followed by a space and some text.
fn heading(src: &[u8], l: Line) -> Option<(usize, Range<usize>)> {
    let c = *src.get(l.start).filter(|&&c| c == b'=' || c == b'#')?;
    let n = src[l.start..l.end].iter().take_while(|&&b| b == c).count();
    if n > 6 || !matches!(src.get(l.start + n), Some(b' ' | b'\t')) {
        return None;
    }
    let name = trim(src, l.start + n..l.end);
    (!name.is_empty()).then_some((n, name))
}

/// The language of a `[source,LANG]` (or `[,LANG]`) attribute line, when it is one.
fn source_language(src: &[u8], l: Line) -> Option<Option<Range<usize>>> {
    let r = trim(src, l.start..l.end);
    let t = &src[r.clone()];
    if t.len() < 2 || t[0] != b'[' || t[t.len() - 1] != b']' {
        return None;
    }
    let inner = r.start + 1..r.end - 1;
    let mut parts = src[inner.clone()]
        .split(|&b| b == b',')
        .scan(inner.start, |at, p| {
            let r = *at..*at + p.len();
            *at += p.len() + 1;
            Some(trim(src, r))
        });
    let style = parts.next()?;
    if !(style.is_empty() || &src[style] == b"source") {
        return None;
    }
    Some(parts.next().filter(|r| !r.is_empty()))
}

/// Whether the line is an attribute list or a block title, which may stand between a
/// `[source,LANG]` and its listing.
fn block_metadata(src: &[u8], l: Line) -> bool {
    let t = &src[trim(src, l.start..l.end)];
    (t.len() >= 2 && t[0] == b'[' && t[t.len() - 1] == b']')
        || (t.len() >= 2 && t[0] == b'.' && !matches!(t[1], b'.' | b' ' | b'\t'))
}

pub(crate) fn scan(src: &[u8]) -> Items {
    let mut items = Items::default();
    let mut open = Open::default();
    // Whether a line other than a blank line or a `//` comment came: then `= X` is text.
    let mut content = false;
    // The `[source,LANG]` line waiting for its listing, and its language.
    let mut source: Option<(Line, Option<Range<usize>>)> = None;
    let mut document = false;
    // The open block: (delimiter line, its item).
    let mut block: Option<(Line, Option<usize>)> = None;
    // The line before, when it was a line of text that started a paragraph: a two-line
    // title's underline may follow. A line inside a paragraph or a list item is no title
    // (code review M6 r2, Fable #1).
    let mut text: Option<Line> = None;
    // Whether the line before was a line of a paragraph or a list item.
    let mut paragraph = false;
    // Whether the line before was an attribute entry continued by a trailing `\`.
    let mut entry = false;
    for l in lines(src) {
        let before = text.take();
        let inside = std::mem::take(&mut paragraph);
        if std::mem::take(&mut entry) && block.is_none() && !blank(src, l) {
            entry = continued(src, l.start..l.end);
            continue;
        }
        if let Some((d, item)) = block {
            let t = trim(src, l.start..l.end);
            let closes = if src[d.start] == b'`' {
                // A run of backticks alone, as long as the fence's or longer.
                let run = &src[t];
                run.len() >= fence_run(src, d) && run.iter().all(|&b| b == b'`')
            } else {
                src[t.clone()] == src[trim(src, d.start..d.end)] && t.start == l.start
            };
            if closes {
                if let Some(i) = item {
                    items.set_end(i, l.next);
                }
                block = None;
            }
            continue;
        }
        if blank(src, l) {
            source = None;
            continue;
        }
        if before.is_some_and(|t| underline(src, t, l)) {
            source = None;
            content = true;
            continue;
        }
        let comment = src[l.start..l.end].starts_with(b"//") && delimiter(src, l).is_none();
        if let Some(kind) = delimiter(src, l) {
            content = true;
            // `[source,LANG]` names a listing, a literal block and a fence without an info
            // string, as Asciidoctor applies the style to the three (code review M6 r1, Fable
            // #23); the block starts at it.
            let attr = source.take().filter(|_| kind != Block::Opaque);
            let (start, sig) = match &attr {
                Some((a, _)) => (a.start, a.start..a.end),
                None => (l.start, l.start..l.end),
            };
            let lang = attr.and_then(|(_, lang)| lang);
            let name = match kind {
                Block::Listing => match lang {
                    Some(lang) => Some(Named::Source(vec![lang])),
                    None => Some(Named::Word("listing")),
                },
                Block::Literal => match lang {
                    Some(lang) => Some(Named::Source(vec![lang])),
                    None => Some(Named::Word("literal")),
                },
                Block::Fence => {
                    let info = trim(src, l.start + fence_run(src, l)..l.end);
                    let word = info.start
                        + src[info.clone()]
                            .iter()
                            .position(|b| matches!(b, b' ' | b'\t' | b','))
                            .unwrap_or(info.len());
                    if word > info.start {
                        Some(Named::Source(vec![trim(src, info.start..word)]))
                    } else if let Some(lang) = lang {
                        Some(Named::Source(vec![lang]))
                    } else {
                        Some(Named::Word("listing"))
                    }
                }
                Block::Opaque => None,
            };
            let item = match name {
                Some(name) if open.len() + 2 <= super::MAX_DEPTH => {
                    let mut it = Item::new("code", start..src.len(), start..l.next, name);
                    if let Named::Source(p) = &it.name {
                        it.anchor = p[0].start;
                    }
                    it.signature = Some(sig);
                    items.push(it);
                    Some(items.len() - 1)
                }
                _ => None,
            };
            block = Some((l, item));
            continue;
        }
        if let Some(lang) = source_language(src, l) {
            source = Some((l, lang));
            content = true;
            continue;
        }
        if !(source.is_some() && block_metadata(src, l)) {
            source = None;
        }
        if let Some((n, name)) = heading(src, l) {
            if n == 1 {
                if !content && !document {
                    document = true;
                    let mut it = Item::new(
                        "document",
                        0..src.len(),
                        l.start..l.next,
                        Named::Source(vec![name.clone()]),
                    );
                    it.anchor = name.start;
                    items.push(it);
                }
                content = true;
                continue;
            }
            content = true;
            let level = n - 1;
            open.close_to(&mut items, level, l.start);
            let mut it = Item::new(
                "heading",
                l.start..src.len(),
                l.start..l.next,
                Named::Source(vec![name.clone()]),
            );
            it.anchor = name.start;
            it.level = Some(level);
            it.signature = Some(l.start..l.end);
            open.push(&mut items, it, level, usize::from(document));
            continue;
        }
        if comment {
            // A line comment neither starts nor ends a paragraph.
            paragraph = inside;
            continue;
        }
        if !inside && let Some(value) = attribute_value(src, l) {
            entry = continued(src, value);
            continue;
        }
        content = true;
        paragraph = true;
        if !inside && !list_item(src, l) && !block_metadata(src, l) {
            text = Some(l);
        }
    }
    open.finish(&mut items, src.len());
    items
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cursor_equals_model, params, trail};
    use crate::processors::Processor;

    #[test]
    fn titles_headings_and_blocks() {
        let p = params("asciidoc");
        let src = b"// c\n= Doc\n== A\n=== B\nb-needle\n## C\n[source, rust]\n.Title\n----\n== no\n----\n```py x\nf-needle\n```\n====\n=== hidden\n====\nafter-needle\n";
        assert_eq!(trail(src, &p, "b-needle"), ["Doc", "A", "B"]);
        assert_eq!(trail(src, &p, "== no"), ["Doc", "C", "rust"]);
        assert_eq!(trail(src, &p, "f-needle"), ["Doc", "C", "py"]);
        assert_eq!(trail(src, &p, "after-needle"), ["Doc", "C"]);
        assert_eq!(trail(src, &p, "=== B"), ["Doc", "A"]);
        cursor_equals_model(src, &p);
    }

    #[test]
    fn a_late_title_or_a_heading_without_space_is_text() {
        let p = params("asciidoc");
        let src = b"intro\n= Late\n==No\n== A\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["A"]);
        assert_eq!(trail(src, &p, "==No"), Vec::<String>::new());
    }

    /// Code review M6 r1, Fable #7: the underline of a legacy two-line title, within one
    /// character of the title's length, is text, as the title is, and opens no block.
    #[test]
    fn a_two_line_title_is_text_and_its_underline_opens_no_block() {
        let p = params("asciidoc");
        let src = "Legacy Title\n------------\n\n== Real\nneedle-a1\n\nAnother\n-------\nneedle-a1b\n\nÉtude\n=====\n== B\nneedle-a2\nShort\n+++++++++\nneedle-a3\n++++\n";
        let src = src.as_bytes();
        assert_eq!(trail(src, &p, "needle-a1"), ["Real"]);
        assert_eq!(trail(src, &p, "needle-a1b"), ["Real"]);
        assert_eq!(trail(src, &p, "needle-a2"), ["B"]);
        // Four characters longer than its line: a passthrough block, to the next `+++++++++`.
        assert_eq!(trail(src, &p, "needle-a3"), ["B"]);
        assert_eq!(trail(src, &p, "== B"), Vec::<String>::new());
        cursor_equals_model(src, &p);
    }

    /// Code review M6 r2, Fable #1: only a line that starts a paragraph can be a two-line
    /// title. A short line inside a paragraph, a list item or a block title above a
    /// delimiter leaves the delimiter opening its block, so its closing delimiter does not
    /// open a block that runs to the end of the file.
    #[test]
    fn only_a_line_that_starts_a_paragraph_is_a_two_line_title() {
        let p = params("asciidoc");
        for src in [
            "== A\nSome paragraph text here.\nRun:\n----\ncode needle-1\n----\n== B\nneedle-2\n",
            "== A\n* Run\n----\ncode needle-1\n----\n== B\nneedle-2\n",
            "== A\n* item\n------\ncode needle-1\n------\n== B\nneedle-2\n",
            "== A\n.Run\n----\ncode needle-1\n----\n== B\nneedle-2\n",
            "== A\nSome text here.\n// c\nRun:\n----\ncode needle-1\n----\n== B\nneedle-2\n",
        ] {
            let src = src.as_bytes();
            assert_eq!(
                trail(src, &p, "needle-1"),
                ["A", "listing"],
                "{}",
                String::from_utf8_lossy(src)
            );
            assert_eq!(
                trail(src, &p, "needle-2"),
                ["B"],
                "{}",
                String::from_utf8_lossy(src)
            );
            cursor_equals_model(src, &p);
        }
        let src = b"== A\nSome text here.\nNote:\n====\nexample needle-1\n====\n== B\nneedle-2\n";
        assert_eq!(trail(src, &p, "needle-1"), ["A"]);
        assert_eq!(trail(src, &p, "needle-2"), ["B"]);
        // A line that starts a paragraph still is: after a heading, a blank line, a closed
        // block or an attribute entry.
        for src in [
            "== A\nRun:\n----\nneedle-1\n== B\nneedle-2\n",
            "== A\ntext\n\nRun:\n----\nneedle-1\n== B\nneedle-2\n",
            "== A\n....\nx\n....\nRun:\n----\nneedle-1\n== B\nneedle-2\n",
            "== A\n:k: v\nRun:\n----\nneedle-1\n== B\nneedle-2\n",
        ] {
            let src = src.as_bytes();
            assert_eq!(
                trail(src, &p, "needle-1"),
                ["A"],
                "{}",
                String::from_utf8_lossy(src)
            );
            assert_eq!(
                trail(src, &p, "needle-2"),
                ["B"],
                "{}",
                String::from_utf8_lossy(src)
            );
        }
    }

    /// Code review M6 r2, Fable #19: an attribute entry continued by a trailing `\` (from
    /// memory of Asciidoctor, unverified: accepting it only lets more files have a title)
    /// leaves `= Doc` the document title.
    #[test]
    fn a_continued_attribute_entry_before_the_title_leaves_it_the_title() {
        let p = params("asciidoc");
        let src = b":desc: a long \\\n  continued \\\n  value\n:toc:\n= Doc\n== A\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["Doc", "A"]);
        cursor_equals_model(src, &p);
        let src = b":desc: a long\n  not continued\n= Doc\n== A\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["A"]);
    }

    /// Code review M6 r3, Fable #1: only a space and `\` continue an attribute entry, as
    /// Asciidoctor's line continuation is ` \` (from memory, unverified); a value that ends
    /// in a bare `\`, as a Windows path does, ends on its line and leaves `= Doc` the title.
    #[test]
    fn a_value_ending_in_a_bare_backslash_does_not_continue_the_attribute_entry() {
        let p = params("asciidoc");
        let src = b":path: C:\\Users\\\n= Doc\n== A\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["Doc", "A"]);
        cursor_equals_model(src, &p);
        // A continuation line that ends in a bare `\` ends the entry too.
        let src = b":desc: a \\\n  C:\\Users\\\n= Doc\n== A\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["Doc", "A"]);
        cursor_equals_model(src, &p);
        // A space and `\` still continue it, over the `= Doc` line.
        let src = b":desc: a \\\n= Doc\n== A\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["A"]);
    }

    /// Code review M6 r4, Fable #4: a value that is a bare `\` has no space before it, as the
    /// blanks after `:name:` are no part of the value, so it ends on its line (§6.1), on the
    /// entry's line and on a continuation line alike.
    #[test]
    fn a_value_that_is_a_bare_backslash_ends_the_attribute_entry() {
        let p = params("asciidoc");
        for src in [
            &b":desc: \\\n= Doc\n== A\nneedle\n"[..],
            b":desc:\t\\\n= Doc\n== A\nneedle\n",
            b":desc: a \\\n  \\\n= Doc\n== A\nneedle\n",
        ] {
            assert_eq!(
                trail(src, &p, "needle"),
                ["Doc", "A"],
                "{}",
                String::from_utf8_lossy(src)
            );
            cursor_equals_model(src, &p);
        }
        // Text before the space and `\` continues it.
        let src = b":desc: x \\\n= Doc\n== A\nneedle\n";
        assert_eq!(trail(src, &p, "needle"), ["A"]);
    }

    /// Code review M6 r1, Fable #21: a fence of four or more backticks takes its info string
    /// after the whole run, and only a run of backticks as long or longer closes it.
    #[test]
    fn a_long_fence_names_its_language_and_a_shorter_one_does_not_close_it() {
        let p = params("asciidoc");
        let src = b"= Doc\n== A\n````rust\n== inside needle-a\n```\nneedle-b\n`````\nneedle-c\n````\nneedle-d\n";
        assert_eq!(trail(src, &p, "needle-a"), ["Doc", "A", "rust"]);
        assert_eq!(trail(src, &p, "needle-b"), ["Doc", "A", "rust"]);
        assert_eq!(trail(src, &p, "needle-c"), ["Doc", "A"]);
        assert_eq!(trail(src, &p, "needle-d"), ["Doc", "A", "listing"]);
        cursor_equals_model(src, &p);
    }

    /// Code review M6 r1, Fable #22: a table (`|===`, and the `,===`, `:===` and `!===` of
    /// its data forms) holds text and no Segment, as the blocks of §6.1 do: a line in it that
    /// looks like a heading is a cell's text.
    #[test]
    fn a_table_is_a_block_and_its_lines_are_text() {
        let p = params("asciidoc");
        let src = b"= Doc\n== A\n|===\n| cell one\n== looks like heading\n| needle-a\n|===\nneedle-b\n,===\n== x\nneedle-c\n,===\n:====\n== y\n:====\n!===\n== z\nneedle-d\n";
        assert_eq!(trail(src, &p, "needle-a"), ["Doc", "A"]);
        assert_eq!(trail(src, &p, "needle-b"), ["Doc", "A"]);
        assert_eq!(trail(src, &p, "needle-c"), ["Doc", "A"]);
        assert_eq!(trail(src, &p, "needle-d"), ["Doc", "A"]);
        cursor_equals_model(src, &p);
    }

    /// Code review M6 r1, Fable #23: `[source,LANG]` names a literal block and a fence
    /// without an info string as it names a listing; a fence's info string comes first.
    #[test]
    fn source_names_a_literal_block_and_a_fence_too() {
        let p = params("asciidoc");
        let src = b"= Doc\n== A\n[source,rust]\n....\nneedle-a\n....\n[source,rust]\n```\nneedle-b\n```\n[source,rust]\n```py\nneedle-c\n```\n[source]\n....\nneedle-d\n....\n";
        assert_eq!(trail(src, &p, "needle-a"), ["Doc", "A", "rust"]);
        assert_eq!(trail(src, &p, "needle-b"), ["Doc", "A", "rust"]);
        assert_eq!(trail(src, &p, "needle-c"), ["Doc", "A", "py"]);
        assert_eq!(trail(src, &p, "needle-d"), ["Doc", "A", "literal"]);
        let at = src.windows(8).position(|w| w == b"needle-a").unwrap();
        let t = &super::super::Headings
            .trails(&super::super::tests::input(src, "x"), &p, &[at])
            .unwrap()[0];
        assert_eq!(t[2].line, 3, "the block starts at its [source] line");
        assert_eq!(t[2].signature.as_deref(), Some("[source,rust]"));
        cursor_equals_model(src, &p);
    }

    /// Code review M6 r1, Fable #24: attribute entries above the title (`:toc:`,
    /// `:author: X`, `:!sectnums:`) leave `= Doc` the document title; any other line before
    /// it still makes it text.
    #[test]
    fn attribute_entries_before_the_title_leave_it_the_title() {
        let p = params("asciidoc");
        let src = b":toc:\n:author: X\n:!sectnums:\n:tip-caption!:\n= Doc\n== A\nneedle-a\n";
        assert_eq!(trail(src, &p, "needle-a"), ["Doc", "A"]);
        cursor_equals_model(src, &p);
        let src = b":toc:\n:not an entry\n= Doc\n== A\nneedle-a\n";
        assert_eq!(trail(src, &p, "needle-a"), ["A"]);
    }

    #[test]
    fn a_listing_without_source_is_listing_and_an_open_block_runs_to_the_end() {
        let p = params("asciidoc");
        let src = b"== A\n[source,bash]\n\n----\nx-needle\n----\n----\ny-needle\n";
        assert_eq!(trail(src, &p, "x-needle"), ["A", "listing"]);
        assert_eq!(trail(src, &p, "y-needle"), ["A", "listing"]);
        cursor_equals_model(src, &p);
    }
}
