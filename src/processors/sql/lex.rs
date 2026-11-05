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

/// The lexeme at `i`, which is not white space.
pub(super) fn lexeme(src: &[u8], i: usize) -> Lexeme {
    let n = src.len();
    let next = src.get(i + 1).copied();
    match src[i] {
        b'-' if next == Some(b'-') => Lexeme::Comment(line_end(src, i)),
        b'#' => Lexeme::Comment(line_end(src, i)),
        b'/' if next == Some(b'*') => match memchr::memmem::find(&src[i + 2..], b"*/") {
            Some(k) => Lexeme::Comment(i + 2 + k + 2),
            None => Lexeme::Open,
        },
        b'\'' => match string_end(src, i) {
            Some(end) => token(Tok::Str, i, end),
            None => Lexeme::Open,
        },
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

/// The end of the string opened at `i`: `''` is a quote inside it, and so is `\'`, which
/// mysqldump writes (a backslash escapes the byte after it).
fn string_end(src: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    loop {
        let k = memchr::memchr2(b'\'', b'\\', &src[j..])?;
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
}

impl<'s> Lexer<'s> {
    pub fn new(src: &'s [u8], from: usize, to: usize) -> Self {
        Self {
            src,
            pos: from,
            end: to.min(src.len()),
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
            match lexeme(self.src, self.pos) {
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
        match lexeme(self.src, self.pos) {
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
        Lexer::new(src, 0, src.len())
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
            assert_eq!(lexeme(src, 0), Lexeme::Open, "{src:?}");
        }
    }
}
