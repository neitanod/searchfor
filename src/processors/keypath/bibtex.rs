//! BibTeX (M6 spec §10.3): an entry, `@article{knuth1984, …}` or with parentheses, is an
//! `entry` Segment named by its key, with the signature `@article{knuth1984,`; each field is a
//! `key` Segment over its value, in braces (nesting), quotes or bare, `#` concatenations
//! included. `@string` and `@preamble` are named as written. `@comment` and the text between
//! entries have an empty Trail. An entry without a key (its first item is a field) is named
//! by its type. An entry whose braces never close ends at the next `@` at a line's start,
//! keeping the Trails before it; one that closes keeps such an `@` as text of its values.
//! Braces nest on a counter, never on the call stack.
use super::lines::{self, Builder, Kind, NameSrc};
use std::collections::HashMap;
use std::ops::Range;

pub(crate) fn prepare<'a>(
    input: crate::model::Input<'a>,
    params: &'a ::toml::Table,
) -> anyhow::Result<Box<dyn crate::processors::Prepared + 'a>> {
    lines::prepare(input, params, scan)
}

pub(crate) fn outline(
    src: &[u8],
    params: &::toml::Table,
    anchors: Option<&[usize]>,
) -> anyhow::Result<crate::trail::Outline> {
    lines::outline(src, params, anchors, scan)
}

/// The reading of one entry, which ends at its closing delimiter or, when its braces never
/// close, at an `@` that starts a line (`Cut`).
struct Reader<'s> {
    /// The file, up to the entry's closing delimiter when it has one.
    src: &'s [u8],
    at: usize,
    close: u8,
    /// Whether a line-start `@` cuts the entry: only when it never closes.
    cut: bool,
}

/// Where an unclosed entry ends: the start of the line whose first character is `@`.
struct Cut(usize);

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.at).copied()
    }

    /// Moves past one byte; a line break before an `@` that starts a line cuts the entry.
    fn bump(&mut self) -> Result<(), Cut> {
        let c = self.src[self.at];
        self.at += 1;
        if c == b'\n' && self.cut {
            let mut i = self.at;
            while i < self.src.len() && matches!(self.src[i], b' ' | b'\t' | b'\r') {
                i += 1;
            }
            if self.src.get(i) == Some(&b'@') {
                return Err(Cut(self.at));
            }
        }
        Ok(())
    }

    fn blanks(&mut self) -> Result<(), Cut> {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.bump()?;
        }
        Ok(())
    }

    /// Past a braced group whose `{` is at `at`: to its matching `}` or the end.
    fn braces(&mut self) -> Result<(), Cut> {
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            self.bump()?;
            match c {
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Past a quoted value whose `"` is at `at`; a `"` inside braces is text.
    fn quoted(&mut self) -> Result<(), Cut> {
        self.bump()?;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            self.bump()?;
            match c {
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                b'"' if depth == 0 => return Ok(()),
                _ => {}
            }
        }
        Ok(())
    }

    fn bare(c: u8) -> bool {
        !c.is_ascii_whitespace()
            && !matches!(c, b',' | b'{' | b'}' | b'(' | b')' | b'"' | b'#' | b'=')
    }

    /// A value: its parts joined by `#`. Where it ends; None when no part is there.
    fn value(&mut self) -> Result<Option<usize>, Cut> {
        let mut end = None;
        loop {
            match self.peek() {
                Some(b'{') => self.braces()?,
                Some(b'"') => self.quoted()?,
                Some(c) if Self::bare(c) && c != self.close => {
                    while self
                        .peek()
                        .is_some_and(|c| Self::bare(c) && c != self.close)
                    {
                        self.bump()?;
                    }
                }
                _ => return Ok(end),
            }
            end = Some(self.at);
            self.blanks()?;
            if self.peek() != Some(b'#') {
                return Ok(end);
            }
            self.bump()?;
            self.blanks()?;
        }
    }

    /// Past what is no field, to the next `,` or the closing delimiter outside braces.
    fn skip(&mut self) -> Result<(), Cut> {
        while let Some(c) = self.peek() {
            match c {
                b'{' => self.braces()?,
                b'"' => self.quoted()?,
                b',' => return Ok(()),
                c if c == self.close => return Ok(()),
                _ => self.bump()?,
            }
        }
        Ok(())
    }

    /// The fields up to the closing delimiter, each a `key` Segment inside `depth` open ones.
    /// Where the entry ends.
    fn fields(&mut self, b: &mut Builder, depth: usize) -> Result<usize, Cut> {
        loop {
            self.blanks()?;
            match self.peek() {
                None => return Ok(self.at),
                Some(b',') => {
                    self.bump()?;
                    continue;
                }
                Some(c) if c == self.close => {
                    self.at += 1;
                    return Ok(self.at);
                }
                _ => {}
            }
            let start = self.at;
            while self.peek().is_some_and(Self::bare) {
                self.bump()?;
            }
            let name = start..self.at;
            self.blanks()?;
            if name.is_empty() || self.peek() != Some(b'=') {
                if name.is_empty() && self.peek().is_some() {
                    self.bump()?;
                }
                self.skip()?;
                continue;
            }
            self.bump()?;
            self.blanks()?;
            b.open(start, name.clone(), Kind::Key, NameSrc::Verbatim(name));
            let end = self.value();
            match end {
                Ok(Some(e)) => b.close_to(depth, e),
                Ok(None) => {
                    b.close_to(depth, self.at);
                    self.skip()?;
                }
                Err(cut) => return Err(cut),
            }
        }
    }
}

/// Where each group that may open an entry (`@word{` or `@word(`) closes, by the offset of
/// its `{` or `(`: the `}` that balances its braces, or the first `)` outside braces after it.
/// Absent when it never closes. One pass over the file, with a stack of the groups open.
fn closings(src: &[u8]) -> HashMap<usize, usize> {
    let mut out = HashMap::new();
    // (offset of the delimiter, brace depth outside it, closing byte).
    let mut open: Vec<(usize, usize, u8)> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < src.len() {
        match src[i] {
            b'@' => {
                let mut t = i + 1;
                while t < src.len()
                    && (src[t].is_ascii_alphanumeric() || matches!(src[t], b'_' | b'-'))
                {
                    t += 1;
                }
                let mut o = t;
                while o < src.len() && matches!(src[o], b' ' | b'\t') {
                    o += 1;
                }
                if t > i + 1 && matches!(src.get(o), Some(b'{' | b'(')) {
                    let close = if src[o] == b'{' { b'}' } else { b')' };
                    open.push((o, depth, close));
                    if close == b'}' {
                        depth += 1;
                    }
                    i = o + 1;
                    continue;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                // A group in parentheses opened inside the braces that end here never closes.
                while open.last().is_some_and(|&(_, d, c)| c == b')' && d > depth) {
                    open.pop();
                }
                if let Some(&(o, d, b'}')) = open.last()
                    && d == depth
                {
                    out.insert(o, i);
                    open.pop();
                }
            }
            b')' => {
                if let Some(&(o, d, b')')) = open.last()
                    && depth <= d
                {
                    out.insert(o, i);
                    open.pop();
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn scan(src: &[u8], b: &mut Builder) {
    let closings = closings(src);
    let mut at = 0;
    while let Some(i) = memchr::memchr(b'@', &src[at..]) {
        let start = at + i;
        at = start + 1;
        let mut t = at;
        while t < src.len() && (src[t].is_ascii_alphanumeric() || matches!(src[t], b'_' | b'-')) {
            t += 1;
        }
        let mut open = t;
        while open < src.len() && matches!(src[open], b' ' | b'\t') {
            open += 1;
        }
        if t == at || !matches!(src.get(open), Some(b'{' | b'(')) {
            continue;
        }
        let close = if src[open] == b'{' { b'}' } else { b')' };
        let kind = &src[at..t];
        let closed = closings.get(&open).copied();
        let mut r = Reader {
            src: closed.map_or(src, |c| &src[..c + 1]),
            at: open + 1,
            close,
            cut: closed.is_none(),
        };
        if kind.eq_ignore_ascii_case(b"comment") {
            // Outside structure: skipped as a group in braces or parentheses.
            r.at = open;
            // Cut or not, the reading goes on where the group ended.
            let _ = r.braces_or_parens();
            at = r.at.max(open + 1);
            continue;
        }
        let depth = b.depth();
        let named = kind.eq_ignore_ascii_case(b"string") || kind.eq_ignore_ascii_case(b"preamble");
        let (name, signature, body): (Range<usize>, Range<usize>, usize) = if named {
            (start..t, start..open + 1, open + 1)
        } else {
            let mut k = open + 1;
            while k < src.len() && matches!(src[k], b' ' | b'\t' | b'\r' | b'\n') {
                k += 1;
            }
            let key_start = k;
            while k < src.len() && !matches!(src[k], b',' | b'\n') && src[k] != close {
                k += 1;
            }
            let key = lines::trim(src, key_start..k);
            if memchr::memchr(b'=', &src[key.clone()]).is_some() {
                // No key: the first item is a field. The entry is named by its type.
                (start..t, start..open + 1, open + 1)
            } else {
                let sig_end = if src.get(k) == Some(&b',') { k + 1 } else { k };
                (key, start..sig_end, sig_end)
            }
        };
        b.open(
            start,
            signature.clone(),
            Kind::Entry,
            NameSrc::Entry { name, signature },
        );
        r.at = body;
        let end = if kind.eq_ignore_ascii_case(b"preamble") {
            r.at = open;
            r.braces_or_parens().map(|()| r.at)
        } else {
            r.fields(b, depth + 1)
        };
        let end = match end {
            Ok(e) => e,
            Err(Cut(c)) => c,
        };
        b.close_to(depth, end);
        at = end.max(start + 1);
    }
}

impl Reader<'_> {
    /// Past a group opened by `{` or `(` at `at`, to its closing delimiter or the end.
    fn braces_or_parens(&mut self) -> Result<(), Cut> {
        if self.peek() == Some(b'{') {
            return self.braces();
        }
        self.bump()?;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            self.bump()?;
            match c {
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                b')' if depth == 0 => return Ok(()),
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::tests::cursor_is_model;
    use super::super::tests::needles;

    #[test]
    fn entries_fields_and_strings() {
        let src = b"needle @misc{k1, a = needle # {x{needle}} , b=\"q{\"}needle\"}\n@String(s = {needle})\n@preamble{\"needle\"}\n@comment{a {needle} b}\n@book{k2,\n c = 12, d = {needle}\n}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![
                vec![],
                vec!["k1", "a"],
                vec!["k1", "a"],
                vec!["k1", "b"],
                vec!["@String", "s"],
                vec!["@preamble"],
                vec![],
                vec!["k2", "d"]
            ]
        );
    }

    #[test]
    fn an_unclosed_entry_ends_at_the_next_line_that_starts_with_at() {
        let src = b"@article{a,\n title={needle\n  @book(b, title=\"needle\")\nneedle";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["a", "title"], vec!["b", "title"], vec![]]
        );
    }

    #[test]
    fn an_at_inside_balanced_braces_cuts_nothing() {
        // The cut is for an entry whose braces never close (code round 1, Fable #38): a
        // well-formed entry keeps a line-start `@` in a value. A `}` inside quotes still ends
        // an entry where its braces balance, so the next entry is not swallowed.
        let src = b"@article{a,\n note = {Follow\n  @user on X},\n title = {needle}\n}\n@a{k, t = \"x}\n@b{k2, t = {needle}}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["a", "title"], vec!["k2", "t"]]
        );
        cursor_is_model("bibtex", src);
    }

    #[test]
    fn a_comment_in_parentheses_and_an_entry_without_a_key() {
        // `@comment(…)` ends at its `)` (Fable #39); an entry whose first item holds `=` has
        // no key and is named by its type (Fable #40).
        let src =
            b"@comment(no braces) @misc{k, a = {needle}}\n@article{title = {needle}, b = 1}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["k", "a"], vec!["@article", "title"]]
        );
        cursor_is_model("bibtex", src);
    }

    #[test]
    fn the_cursor_gives_the_model_trails_at_every_byte() {
        for src in [
            &b"@article{knuth1984,\n title = {Literate needle {programming}},\n note = \"needle\" # suffix,\n}\n@comment{needle ignored}\n"[..],
            b"@article{a,\r\n title={needle\r\n@book{b, title={needle}}\r\n",
            b"@x{,=,}@y(k)@z{k,a=}@",
            b"@preamble(\"a\" # {b)}\n@string{",
            b"",
        ] {
            cursor_is_model("bibtex", src);
        }
    }
}
