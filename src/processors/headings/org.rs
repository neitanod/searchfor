//! Org-mode (M6 spec §6.3): `*` to any depth followed by a space is a heading of that many
//! stars, its name the text after them with its TODO keyword, priority and tags as written;
//! `#+TITLE:` is the document. `#+BEGIN_SRC LANG` … `#+END_SRC`, without case, is a `code`
//! Segment named `LANG` (`src` when absent) and `#+BEGIN_EXAMPLE` is `example`; other blocks
//! hold no Segment, and nothing in a block is a heading or a table. A table whose first row is
//! followed by a rule names its columns by that row's cells: a Match in a later row's cell gets
//! a `column` Segment, whose Section is the table.
use super::{Item, Items, Line, MAX_DEPTH, Named, Open, line_at, trim};
use std::ops::Range;

/// The text of a block marker, `#+BEGIN_` or `#+END_`, compared without case: what follows it.
fn after<'a>(text: &'a [u8], marker: &[u8]) -> Option<&'a [u8]> {
    (text.len() >= marker.len() && text[..marker.len()].eq_ignore_ascii_case(marker))
        .then(|| &text[marker.len()..])
}

/// The cells of a table row: the bytes between its pipes, the last one up to the end of the
/// row when no pipe closes it.
fn cells(src: &[u8], row: Range<usize>) -> Vec<Range<usize>> {
    let base = row.start + 1;
    let mut out = Vec::new();
    let mut from = base;
    for k in memchr::memchr_iter(b'|', &src[base..row.end]) {
        out.push(from..base + k);
        from = base + k + 1;
    }
    if from < row.end {
        out.push(from..row.end);
    }
    out
}

/// Whether a table row is a rule, `|---+---|`.
fn rule(src: &[u8], row: &Range<usize>) -> bool {
    src[row.clone()].starts_with(b"|-")
}

pub(crate) fn scan(src: &[u8]) -> Items {
    let mut items = Items::default();
    let mut open = Open::default();
    let mut document = false;
    let mut pos = 0;
    let mut line_no: u64 = 0;
    while let Some(l) = line_at(src, pos) {
        pos = l.next;
        line_no += 1;
        let t = trim(src, l.start..l.end);
        let text = &src[t.clone()];
        let extra = usize::from(document);
        if let Some(rest) = after(text, b"#+begin_") {
            let word = rest
                .iter()
                .position(|b| matches!(b, b' ' | b'\t'))
                .unwrap_or(rest.len());
            let kind = &rest[..word];
            let mut end = src.len();
            while let Some(m) = line_at(src, pos) {
                pos = m.next;
                line_no += 1;
                let close = after(&src[trim(src, m.start..m.end)], b"#+end_");
                if close.is_some_and(|c| c.eq_ignore_ascii_case(kind)) {
                    end = m.next;
                    break;
                }
            }
            let name = if kind.eq_ignore_ascii_case(b"src") {
                let from = t.start + 8 + word;
                let lang = trim(src, from..t.end);
                let lang = lang.start
                    ..lang.start
                        + src[lang.clone()]
                            .iter()
                            .position(|b| matches!(b, b' ' | b'\t'))
                            .unwrap_or(lang.len());
                Some(if lang.is_empty() {
                    Named::Word("src")
                } else {
                    Named::Source(vec![lang])
                })
            } else if kind.eq_ignore_ascii_case(b"example") {
                Some(Named::Word("example"))
            } else {
                None
            };
            if let Some(name) = name
                && open.len() + extra < MAX_DEPTH
            {
                let mut it = Item::new("code", l.start..end, l.start..l.next, name);
                if let Named::Source(p) = &it.name {
                    it.anchor = p[0].start;
                }
                it.signature = Some(t);
                items.push(it);
            }
            continue;
        }
        if !document && let Some(rest) = after(text, b"#+title:") {
            let name = trim(src, t.end - rest.len()..t.end);
            if !name.is_empty() {
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
            continue;
        }
        if let Some((level, name)) = heading(src, l) {
            open.close_to(&mut items, level, l.start);
            let mut it = Item::new(
                "heading",
                l.start..src.len(),
                l.start..l.next,
                Named::Source(vec![name.clone()]),
            );
            it.anchor = name.start;
            it.level = Some(level);
            it.signature = Some(trim(src, l.start..l.end));
            open.push(&mut items, it, level, extra);
            continue;
        }
        if text.first() == Some(&b'|') {
            // The table's rows, this one first.
            let mut rows = vec![t];
            let first = line_no;
            while let Some(m) = line_at(src, pos) {
                let r = trim(src, m.start..m.end);
                if src.get(r.start) != Some(&b'|') {
                    break;
                }
                rows.push(r);
                pos = m.next;
                line_no += 1;
            }
            if rows.len() < 3 || rule(src, &rows[0]) || !rule(src, &rows[1]) {
                continue;
            }
            if open.len() + extra >= MAX_DEPTH {
                continue;
            }
            let header = cells(src, rows[0].clone());
            for row in &rows[2..] {
                if rule(src, row) {
                    continue;
                }
                for (k, cell) in cells(src, row.clone()).into_iter().enumerate() {
                    let name = match header.get(k).map(|h| trim(src, h.clone())) {
                        Some(h) if !h.is_empty() => Named::Source(vec![h]),
                        _ => Named::Ordinal("column", k + 1),
                    };
                    let mut it = Item::new("column", cell.clone(), cell.start..cell.start, name);
                    it.search = false;
                    it.lines = Some((first, line_no));
                    items.push(it);
                }
            }
        }
    }
    open.finish(&mut items, src.len());
    items
}

/// A heading line: (stars, name range).
fn heading(src: &[u8], l: Line) -> Option<(usize, Range<usize>)> {
    let n = src[l.start..l.end]
        .iter()
        .take_while(|&&b| b == b'*')
        .count();
    if n == 0 || !matches!(src.get(l.start + n), Some(b' ' | b'\t')) || l.start + n >= l.end {
        return None;
    }
    Some((n, trim(src, l.start + n..l.end)))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{cursor_equals_model, params, trail};

    #[test]
    fn headings_blocks_and_tables() {
        let p = params("org");
        let src = b"* A [#B] x :t:\n** B\n#+begin_quote\n* q-needle\n#+end_QUOTE\nb-needle\n| H1 |  |\n|----|\n|----|\n| a-needle | c-needle | d-needle\n* C\n| x | y |\n| e-needle |\n";
        assert_eq!(trail(src, &p, "q-needle"), ["A [#B] x :t:", "B"]);
        assert_eq!(trail(src, &p, "b-needle"), ["A [#B] x :t:", "B"]);
        assert_eq!(trail(src, &p, "a-needle"), ["A [#B] x :t:", "B", "H1"]);
        assert_eq!(
            trail(src, &p, "c-needle"),
            ["A [#B] x :t:", "B", "column 2"]
        );
        assert_eq!(
            trail(src, &p, "d-needle"),
            ["A [#B] x :t:", "B", "column 3"]
        );
        assert_eq!(trail(src, &p, "e-needle"), ["C"]);
        cursor_equals_model(src, &p);
    }

    #[test]
    fn deep_headings_stop_opening_past_the_depth() {
        let p = params("org");
        let mut src = String::new();
        for n in 1..=300 {
            src.push_str(&format!("{} h{n}\n", "*".repeat(n)));
        }
        src.push_str("needle\n");
        let t = trail(src.as_bytes(), &p, "needle");
        assert_eq!(t.len(), super::MAX_DEPTH);
        assert_eq!(t.last().unwrap(), "h256");
    }
}
