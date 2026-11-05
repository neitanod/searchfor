//! BibTeX (M6 spec §10.3): an entry, `@article{knuth1984, …}` or with parentheses, is an
//! `entry` Segment named by its key, with the signature `@article{knuth1984,`; each field is a
//! `key` Segment over its value, in braces (nesting), quotes or bare, `#` concatenations
//! included. `@string` and `@preamble` are named as written. `@comment` and the text between
//! entries have an empty Trail. An entry whose braces never close ends at the next `@` at a
//! line's start, keeping the Trails before it. Braces nest on a counter, never on the call
//! stack.
use super::lines::{self, Builder, Kind, NameSrc};
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

/// The reading of one entry, which ends at its closing delimiter or at an `@` that starts a
/// line (`Cut`).
struct Reader<'s> {
    src: &'s [u8],
    at: usize,
    close: u8,
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
        if c == b'\n' {
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

fn scan(src: &[u8], b: &mut Builder) {
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
        let mut r = Reader {
            src,
            at: open + 1,
            close,
        };
        if kind.eq_ignore_ascii_case(b"comment") {
            // Outside structure: skipped as a braced group.
            r.at = open;
            // Cut or not, the reading goes on where the group ended.
            let _ = r.braces();
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
            let sig_end = if src.get(k) == Some(&b',') { k + 1 } else { k };
            (key, start..sig_end, sig_end)
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
