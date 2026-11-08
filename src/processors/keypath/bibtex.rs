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
/// Absent when it never closes. A `"` among an entry's values opens a quoted value, read as
/// [`Reader::quoted`] reads it, so a delimiter between quotes closes nothing. A quote that
/// never closes leaves its entry open, and the pass goes on at the first line after the quote
/// that starts with `@word{` or `@word(`; the quotes of the entries after it read to their own
/// closing quotes (code review M6 r3, Codex #2). One pass over the file, with a stack of the
/// groups open, and one walk of [`Quotes`] for where the quotes close.
fn closings(src: &[u8]) -> HashMap<usize, usize> {
    let mut out = HashMap::new();
    // (offset of the delimiter, brace depth outside it, closing byte, whether its values may
    // be quoted: neither `@comment` nor `@preamble`, read as plain groups).
    let mut open: Vec<(usize, usize, u8, bool)> = Vec::new();
    let mut depth = 0usize;
    let mut quotes = Quotes::new(src);
    let mut i = 0;
    while i < src.len() {
        match src[i] {
            b'@' => {
                if let Some((t, o)) = group(src, i) {
                    let close = if src[o] == b'{' { b'}' } else { b')' };
                    let kind = &src[i + 1..t];
                    let quotes = !kind.eq_ignore_ascii_case(b"comment")
                        && !kind.eq_ignore_ascii_case(b"preamble");
                    open.push((o, depth, close, quotes));
                    if close == b'}' {
                        depth += 1;
                    }
                    i = o + 1;
                    continue;
                }
            }
            b'"' if open
                .last()
                .is_some_and(|&(_, d, c, q)| q && depth == if c == b'}' { d + 1 } else { d }) =>
            {
                if let Some(e) = quotes.close(i) {
                    i = e;
                } else {
                    // The entry never closes; the pass goes on at the line.
                    open.pop();
                    match group_line(src, i) {
                        Some(l) => {
                            i = l;
                            continue;
                        }
                        None => break,
                    }
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                // A group in parentheses opened inside the braces that end here never closes.
                while open
                    .last()
                    .is_some_and(|&(_, d, c, _)| c == b')' && d > depth)
                {
                    open.pop();
                }
                if let Some(&(o, d, b'}', _)) = open.last()
                    && d == depth
                {
                    out.insert(o, i);
                    open.pop();
                }
            }
            b')' => {
                if let Some(&(o, d, b')', _)) = open.last()
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

/// The group an `@` at `at` opens, `@word{` or `@word(`: the end of its word and the offset of
/// its delimiter.
fn group(src: &[u8], at: usize) -> Option<(usize, usize)> {
    let mut t = at + 1;
    while t < src.len() && (src[t].is_ascii_alphanumeric() || matches!(src[t], b'_' | b'-')) {
        t += 1;
    }
    let mut o = t;
    while o < src.len() && matches!(src[o], b' ' | b'\t') {
        o += 1;
    }
    (t > at + 1 && matches!(src.get(o), Some(b'{' | b'('))).then_some((t, o))
}

/// Where the quoted values of a file close, for [`closings`]: one walk forward over the file,
/// shared by every quote it is asked about. A `"` closes at the first `"` after it outside the
/// braces opened after it, as [`Reader::quoted`] reads it: with the level of a byte the count
/// of `{` less the count of `}` before it, the first `"` whose level is the least of the levels
/// since the opening one. The walk keeps the quotes it passed that have not closed, grouped by
/// that least level, so a quote asked after an earlier one ran to the end of the file without
/// closing is answered from the same walk: the file is read once however many quotes never
/// close (code review M6 r3, Codex #2).
struct Quotes<'s> {
    src: &'s [u8],
    /// The next byte the walk reads, and the level before it.
    at: usize,
    level: i64,
    /// The quotes passed that have not closed, in order.
    pending: Vec<usize>,
    /// For each group of `pending`: the least level since its quotes, and the index in
    /// `pending` of its first quote. The levels grow from the first group to the last.
    groups: Vec<(i64, usize)>,
    /// Where each quote passed that closed closes.
    closed: HashMap<usize, usize>,
}

impl<'s> Quotes<'s> {
    fn new(src: &'s [u8]) -> Self {
        Self {
            src,
            at: 0,
            level: 0,
            pending: Vec::new(),
            groups: Vec::new(),
            closed: HashMap::new(),
        }
    }

    /// Where the quote at `p` closes, or None when it never does. `p` grows from one call to
    /// the next.
    fn close(&mut self, p: usize) -> Option<usize> {
        if p >= self.at {
            // No quote before `p` is asked about again.
            self.pending.clear();
            self.groups.clear();
            self.closed.clear();
            for &c in &self.src[self.at..p] {
                self.level += level(c);
            }
            self.at = p;
        }
        loop {
            if let Some(&e) = self.closed.get(&p) {
                return Some(e);
            }
            if self.at >= self.src.len() {
                return None;
            }
            self.step();
        }
    }

    /// Reads the byte at `at`.
    fn step(&mut self) {
        let (c, v) = (self.src[self.at], self.level);
        // The groups whose least level is above this byte's join in one at its level.
        let mut joined = None;
        while let Some(&(m, first)) = self.groups.last()
            && m > v
        {
            self.groups.pop();
            joined = Some(first);
        }
        if let Some(first) = joined
            && self.groups.last().is_none_or(|&(m, _)| m < v)
        {
            self.groups.push((v, first));
        }
        if c == b'"' {
            // It closes the quotes whose least level is its own: the last group's, if any.
            if let Some(&(m, first)) = self.groups.last()
                && m == v
            {
                self.groups.pop();
                for q in self.pending.drain(first..) {
                    self.closed.insert(q, self.at);
                }
            }
            if self.groups.last().is_none_or(|&(m, _)| m < v) {
                self.groups.push((v, self.pending.len()));
            }
            self.pending.push(self.at);
        }
        self.level += level(c);
        self.at += 1;
    }
}

/// What a byte adds to the level of the bytes after it.
fn level(c: u8) -> i64 {
    match c {
        b'{' => 1,
        b'}' => -1,
        _ => 0,
    }
}

/// The start of the first line after `at` that opens a group (`@word{` or `@word(`, blanks
/// before it allowed), where the entry of a quote that never closes ends.
fn group_line(src: &[u8], at: usize) -> Option<usize> {
    let mut from = at;
    while let Some(n) = memchr::memchr(b'\n', &src[from..]) {
        let mut s = from + n + 1;
        while s < src.len() && matches!(src[s], b' ' | b'\t' | b'\r') {
            s += 1;
        }
        if src.get(s) == Some(&b'@') && group(src, s).is_some() {
            return Some(s);
        }
        from = s;
    }
    None
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
        // well-formed entry keeps a line-start `@` in a value. A quote that never closes leaves
        // its entry open up to the next line-start `@`, so the next entry is not swallowed.
        let src = b"@article{a,\n note = {Follow\n  @user on X},\n title = {needle}\n}\n@a{k, t = \"x}\n@b{k2, t = {needle}}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["a", "title"], vec!["k2", "t"]]
        );
        cursor_is_model("bibtex", src);
    }

    #[test]
    fn a_delimiter_inside_a_quoted_value_closes_nothing() {
        // Code round 2, Codex #4 = Fable #5: where an entry closes follows quotes as the
        // reading of its values does, so a `)` or `}` between quotes is text.
        let src = b"@article(k,\n  title = \"a ) needle-one\",\n  note = {needle-two}\n)\n@article{q, title = \"a } b\", note = {needle-three}}\n@article{at, note = \"x\n@foo{y} needle-four\", t = {needle-five}}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![
                vec!["k", "title"],
                vec!["k", "note"],
                vec!["q", "note"],
                vec!["at", "note"],
                vec!["at", "t"],
            ]
        );
        cursor_is_model("bibtex", src);
    }

    #[test]
    fn a_quote_that_never_closes_leaves_its_entry_open() {
        // The entry with the unclosed quote ends at the next line that starts with `@`, and
        // the entries after it keep their own closings: a line-start `@` inside one of their
        // braced values cuts nothing.
        let src =
            b"@a{k, t = \"x {\n@b{k2, t = {needle}}\n@c{k3,\n note = {Follow\n  @user needle}}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["k2", "t"], vec!["k3", "note"]]
        );
        cursor_is_model("bibtex", src);
    }

    #[test]
    fn after_a_quote_that_never_closes_a_later_quote_reads_to_its_own_close() {
        // Code review M6 r3, Codex #2 (it replaces the decision on Fable #3): the recovery
        // from a quote that never closes stays in its entry. `k`'s and `k2`'s quotes never
        // close, so each ends at the next line that starts with `@word{`; `k3`'s note closes on
        // line 5, so `@d{k4` on line 4 is text of it, as it was before any broken quote.
        let src = b"@a{k, t = \"x\n@b{k2, t = \"y\n@c{k3, note = \"multi\n@d{k4, t = {needle1}}\nline\", u = {needle2}}\n@e{k5, u = {needle3}}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["k3", "note"], vec!["k3", "u"], vec!["k5", "u"]]
        );
        cursor_is_model("bibtex", src);
        // Codex's case: the entry after the broken quote keeps its title and its note.
        let src = b"@a{k, t = \"x {\n@b{k2, title = \"needle-before\n@foo{bar} needle-after\", note={needle-note}}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["k2", "title"], vec!["k2", "title"], vec!["k2", "note"]]
        );
        cursor_is_model("bibtex", src);
        // Quotes that close inside braces opened in a broken quote, and broken quotes one
        // after another, each raising the brace depth.
        let src = b"@a{k, t = \"x {\n@b{k2, t = {\"needle1\"}, u = \"y {\n@c{k3, t = \"needle2\n@d{q} z\"}\n";
        assert_eq!(
            needles(src, "bibtex"),
            vec![vec!["k2", "t"], vec!["k3", "t"]]
        );
        cursor_is_model("bibtex", src);
    }

    #[test]
    fn the_walk_of_the_quotes_closes_each_as_the_reader_does() {
        // `Quotes` against a reading of each quote from its own start, as `Reader::quoted`
        // reads it, over the quotes of pseudo-random text, asked in order.
        fn naive(src: &[u8], at: usize) -> Option<usize> {
            let mut depth = 0usize;
            for (i, &c) in src.iter().enumerate().skip(at + 1) {
                match c {
                    b'{' => depth += 1,
                    b'}' => depth = depth.saturating_sub(1),
                    b'"' if depth == 0 => return Some(i),
                    _ => {}
                }
            }
            None
        }
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..2000 {
            let len = (seed % 40) as usize;
            let src: Vec<u8> = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    b"\"{}a\n"[(seed % 5) as usize]
                })
                .collect();
            // Every quote, then a third of them: a quote past the walk restarts it.
            for skip in [0, 3] {
                let mut quotes = super::Quotes::new(&src);
                for p in (0..src.len()).filter(|&p| src[p] == b'"' && (skip == 0 || p % skip == 0))
                {
                    assert_eq!(
                        quotes.close(p),
                        naive(&src, p),
                        "{}@{p}",
                        String::from_utf8_lossy(&src)
                    );
                }
            }
        }
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
