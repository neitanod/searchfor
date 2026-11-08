//! The SQL lexer (M6 spec §8.1): one permissive dialect, PostgreSQL's, MySQL's, SQLite's and
//! SQL Server's literals and comments together. The statement scanner (`units`) and the
//! readers of a statement's details share `lexeme`, so they agree on where every literal ends.
use std::ops::Range;

/// What a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tok {
    /// An unquoted word: a keyword, an identifier or a number.
    Word,
    /// A quoted identifier: `"…"`, `` `…` `` or `[…]`.
    Quoted,
    /// A string: `'…'` or a dollar quote, `$tag$…$tag$`.
    Str,
    /// Any other byte that is not white space.
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub tok: Tok,
    pub range: Range<usize>,
}

/// What starts at a byte that is not white space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Lexeme {
    /// A comment, which ends at the byte given.
    Comment(usize),
    Token(Token),
    /// A string, quoted identifier or block comment left open: the structure ends here (M6
    /// spec §8.1, Failure).
    Open,
}

/// ASCII white space, a lone CR included (M6 spec §3, point 7: no line break).
pub(super) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// A byte of an unquoted word: ASCII letters, digits, `_`, `$` past the first byte, and every
/// byte of a non-ASCII character.
pub(super) fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

/// The lexeme at `i`, which is not white space. `esc`: a backslash escapes the byte after it in
/// every `'…'` string (MySQL's rule; see `backslash_escapes`).
pub(super) fn lexeme(src: &[u8], i: usize, esc: bool) -> Lexeme {
    let n = src.len();
    let next = src.get(i + 1).copied();
    match src[i] {
        b'-' if next == Some(b'-') => Lexeme::Comment(line_end(src, i)),
        b'#' if hash_comment(src, i) => Lexeme::Comment(line_end(src, i)),
        b'/' if next == Some(b'*') => match conditional(src, i) {
            Some(end) => Lexeme::Comment(end),
            None => match memchr::memmem::find(&src[i + 2..], b"*/") {
                Some(k) => Lexeme::Comment(i + 2 + k + 2),
                None => Lexeme::Open,
            },
        },
        // The `*/` that closes a conditional comment: no SQL token starts `*/` (a `*` before
        // a comment is `*/*`).
        b'*' if next == Some(b'/') && src.get(i + 2) != Some(&b'*') => Lexeme::Comment(i + 2),
        b'\'' => {
            // PostgreSQL's `E'…'` has escapes whatever the dialect.
            let e = esc
                || (i > 0 && matches!(src[i - 1], b'E' | b'e') && (i < 2 || !is_word(src[i - 2])));
            match string_end(src, i, e) {
                Some(end) => token(Tok::Str, i, end),
                None => Lexeme::Open,
            }
        }
        q @ (b'"' | b'`' | b'[') => {
            let close = if q == b'[' { b']' } else { q };
            match quoted_end(src, i, close) {
                Some(end) => token(Tok::Quoted, i, end),
                None => Lexeme::Open,
            }
        }
        b'$' if i == 0 || !is_word(src[i - 1]) => match dollar_tag(src, i) {
            Some(tag_end) => {
                let tag = &src[i..tag_end];
                match memchr::memmem::find(&src[tag_end..], tag) {
                    Some(k) => token(Tok::Str, i, tag_end + k + tag.len()),
                    None => Lexeme::Open,
                }
            }
            None => token(Tok::Punct, i, i + 1),
        },
        // SQL Server's `#tmp` and `##global` tables: a word.
        b'#' if src[i..]
            .iter()
            .find(|&&b| b != b'#')
            .is_some_and(|&b| is_word(b)) =>
        {
            let mut end = i + 1;
            while end < n && (src[end] == b'#' || is_word(src[end])) {
                end += 1;
            }
            token(Tok::Word, i, end)
        }
        b if is_word(b) => {
            let mut end = i + 1;
            while end < n && is_word(src[end]) {
                end += 1;
            }
            token(Tok::Word, i, end)
        }
        _ => token(Tok::Punct, i, i + 1),
    }
}

/// Whether the `#` at `i` starts MySQL's comment: before white space or the end, or first on
/// its line. Elsewhere it starts a SQL Server temporary table's name (`#tmp`).
fn hash_comment(src: &[u8], i: usize) -> bool {
    if src.get(i + 1).is_none_or(|&b| is_space(b)) {
        return true;
    }
    let lead = src[..i]
        .iter()
        .rev()
        .take_while(|&&b| matches!(b, b' ' | b'\t'))
        .count();
    i == lead || src[i - lead - 1] == b'\n'
}

/// The end of the opening of MySQL's conditional comment at `i`, `/*!` or MariaDB's `/*M!`
/// and the version digits: its text is SQL, and the `*/` that closes it is blank.
fn conditional(src: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 2;
    if src.get(j) == Some(&b'M') {
        j += 1;
    }
    if src.get(j) != Some(&b'!') {
        return None;
    }
    j += 1;
    while src.get(j).is_some_and(u8::is_ascii_digit) {
        j += 1;
    }
    Some(j)
}

/// Whether a backslash escapes the byte after it in a `'…'` string: MySQL's rule, which
/// mysqldump relies on, unless the Source's head says it is PostgreSQL's
/// (`standard_conforming_strings = on`, which pg_dump writes, or pg_dump's banner): there
/// only an `E'…'` string has escapes.
pub(super) fn backslash_escapes(src: &[u8]) -> bool {
    const HEAD: usize = 16 * 1024;
    let head = &src[..src.len().min(HEAD)];
    if memchr::memmem::find(head, b"PostgreSQL database dump").is_some() {
        return false;
    }
    const KEY: &[u8] = b"standard_conforming_strings";
    for k in memchr::memmem::find_iter(head, KEY) {
        let rest = &head[k + KEY.len()..];
        let skip = |r: &[u8]| -> usize {
            r.iter()
                .take_while(|&&b| matches!(b, b' ' | b'\t' | b'\''))
                .count()
        };
        let mut r = &rest[skip(rest)..];
        if r.first() == Some(&b'=') {
            r = &r[1..];
        } else if r.get(..2).is_some_and(|w| w.eq_ignore_ascii_case(b"TO")) {
            r = &r[2..];
        } else {
            continue;
        }
        let r = &r[skip(r)..];
        if r.get(..2).is_some_and(|w| w.eq_ignore_ascii_case(b"ON"))
            && r.get(2).is_none_or(|&b| !is_word(b))
        {
            return false;
        }
    }
    true
}

fn token(tok: Tok, start: usize, end: usize) -> Lexeme {
    Lexeme::Token(Token {
        tok,
        range: start..end,
    })
}

/// Where the line holding `i` ends: its `\n`, or the end of the Source.
pub(super) fn line_end(src: &[u8], i: usize) -> usize {
    memchr::memchr(b'\n', &src[i..]).map_or(src.len(), |k| i + k)
}

/// The end of the string opened at `i`: `''` is a quote inside it, and with `esc` so is `\'`,
/// which mysqldump writes (a backslash escapes the byte after it).
fn string_end(src: &[u8], i: usize, esc: bool) -> Option<usize> {
    let mut j = i + 1;
    loop {
        let k = if esc {
            memchr::memchr2(b'\'', b'\\', &src[j..])?
        } else {
            memchr::memchr(b'\'', &src[j..])?
        };
        j += k;
        if src[j] == b'\\' {
            j += 2;
            if j > src.len() {
                return None;
            }
            continue;
        }
        if src.get(j + 1) == Some(&b'\'') {
            j += 2;
            continue;
        }
        return Some(j + 1);
    }
}

/// The end of the identifier quoted at `i` that `close` closes; a doubled `close` is one
/// inside it.
fn quoted_end(src: &[u8], i: usize, close: u8) -> Option<usize> {
    let mut j = i + 1;
    loop {
        j += memchr::memchr(close, &src[j..])?;
        if src.get(j + 1) == Some(&close) {
            j += 2;
            continue;
        }
        return Some(j + 1);
    }
}

/// The end of a dollar quote's opening tag at `i` (`$$` or `$tag$`), if one starts there.
fn dollar_tag(src: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    if src.get(j).is_some_and(|b| b.is_ascii_digit()) {
        return None;
    }
    while j < src.len() && (src[j].is_ascii_alphanumeric() || src[j] == b'_' || src[j] >= 0x80) {
        j += 1;
    }
    (src.get(j) == Some(&b'$')).then_some(j + 1)
}

/// The tokens of `src[from..to]`, comments and white space left out. A literal left open ends
/// them.
#[derive(Debug, Clone)]
pub(super) struct Lexer<'s> {
    src: &'s [u8],
    pub pos: usize,
    end: usize,
    esc: bool,
}

impl<'s> Lexer<'s> {
    /// `esc` as `lexeme`'s.
    pub fn new(src: &'s [u8], from: usize, to: usize, esc: bool) -> Self {
        Self {
            src,
            pos: from,
            end: to.min(src.len()),
            esc,
        }
    }

    /// Where the next token starts, or the end: white space and comments skipped.
    pub fn skip_blank(&mut self) -> usize {
        while self.pos < self.end {
            let b = self.src[self.pos];
            if is_space(b) {
                self.pos += 1;
                continue;
            }
            match lexeme(self.src, self.pos, self.esc) {
                Lexeme::Comment(end) => self.pos = end.min(self.end),
                _ => break,
            }
        }
        self.pos
    }

    pub fn peek(&self) -> Option<Token> {
        self.clone().next()
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        if self.skip_blank() >= self.end {
            return None;
        }
        match lexeme(self.src, self.pos, self.esc) {
            Lexeme::Token(t) if t.range.end <= self.end => {
                self.pos = t.range.end;
                Some(t)
            }
            _ => {
                self.pos = self.end;
                None
            }
        }
    }
}

/// Whether the token is the word `kw` (ASCII, upper case), in any case.
pub(super) fn is_kw(src: &[u8], t: &Token, kw: &str) -> bool {
    t.tok == Tok::Word && src[t.range.clone()].eq_ignore_ascii_case(kw.as_bytes())
}

/// Whether the token is the punctuation byte `b`.
pub(super) fn is_punct(src: &[u8], t: &Token, b: u8) -> bool {
    t.tok == Tok::Punct && src[t.range.start] == b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &[u8]) -> Vec<(Tok, &[u8])> {
        Lexer::new(src, 0, src.len(), true)
            .map(|t| (t.tok, &src[t.range]))
            .collect()
    }

    #[test]
    fn literals_quotes_and_comments() {
        let src = b"a 'b''c\\'d' \"e\"\"f\" `g` [h]]i] $t$ j; $t$ $$k$$ -- x\n# y\n/* z */ $1 b$c$";
        assert_eq!(
            kinds(src),
            [
                (Tok::Word, b"a".as_slice()),
                (Tok::Str, b"'b''c\\'d'"),
                (Tok::Quoted, b"\"e\"\"f\""),
                (Tok::Quoted, b"`g`"),
                (Tok::Quoted, b"[h]]i]"),
                (Tok::Str, b"$t$ j; $t$"),
                (Tok::Str, b"$$k$$"),
                (Tok::Punct, b"$"),
                (Tok::Word, b"1"),
                (Tok::Word, b"b$c$"),
            ]
        );
    }

    #[test]
    fn open_literals() {
        for src in [&b"'a"[..], b"\"a", b"`a", b"[a", b"/* a", b"$x$ a", b"'a\\"] {
            assert_eq!(lexeme(src, 0, true), Lexeme::Open, "{src:?}");
        }
    }
}
