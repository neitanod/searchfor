//! The incremental statement index (M6 spec §8.1, §11; M6 plan D8): statements are found one
//! after another, only as far as a cursor asks, so a dump is never scanned past the statement
//! of its last Match.
use super::lex::{self, Lexeme, is_space, is_word};
use std::ops::Range;

/// Statements in file order. A statement ends at the current delimiter (`;`, or what a
/// `DELIMITER` line set) outside literals and comments, before a `GO` line, or at the end of
/// the Source; a `COPY … FROM stdin` statement also holds its data, up to `\.`.
pub(super) struct Units<'s> {
    src: &'s [u8],
    /// Where the scan goes on.
    pos: usize,
    /// Only white space since the last line break: a `GO` line may start here.
    bol: bool,
    delim: Vec<u8>,
    /// A literal or comment was left open: no more statements (M6 spec §8.1, Failure).
    broken: bool,
    /// Where the SQL text of the last statement ends (its delimiter included): a `COPY`'s
    /// data follow it.
    sql_end: usize,
    /// A backslash escapes in strings (`lex::backslash_escapes`).
    esc: bool,
    /// The last `line_break`: where it looked from, and the `\n` it found.
    next_break: Option<(usize, Option<usize>)>,
}

impl<'s> Units<'s> {
    pub fn new(src: &'s [u8]) -> Self {
        Self {
            src,
            pos: 0,
            bol: true,
            delim: b";".to_vec(),
            broken: false,
            sql_end: 0,
            esc: lex::backslash_escapes(src),
            next_break: None,
        }
    }

    /// The first `\n` at or after `from`. The last answer holds while `from` stays between
    /// where it looked from and what it found, so the `COPY` statements of one long line, or of
    /// a file of lone CRs (M6 spec §3, point 7), look at its bytes once, not once each.
    fn line_break(&mut self, from: usize) -> Option<usize> {
        if let Some((looked, found)) = self.next_break
            && looked <= from
            && found.is_none_or(|f| from <= f)
        {
            return found;
        }
        let found = memchr::memchr(b'\n', &self.src[from..]).map(|k| from + k);
        self.next_break = Some((from, found));
        found
    }

    /// Whether a backslash escapes in this Source's strings.
    pub fn esc(&self) -> bool {
        self.esc
    }

    /// How far the scan went.
    #[cfg(test)]
    pub fn scanned(&self) -> usize {
        self.pos
    }

    /// Where the SQL text of the statement `next_statement` gave last ends.
    pub fn sql_end(&self) -> usize {
        self.sql_end
    }

    /// The next statement's bytes, from its first token to its delimiter, its `COPY` data
    /// included; None at the end, and from a literal left open on.
    pub fn next_statement(&mut self) -> Option<Range<usize>> {
        if self.broken {
            return None;
        }
        let src = self.src;
        let n = src.len();
        // White space, comments, `GO` and `DELIMITER` lines and empty statements.
        let start = loop {
            let i = self.pos;
            if i >= n {
                return None;
            }
            let b = src[i];
            if b == b'\n' {
                self.pos += 1;
                self.bol = true;
                continue;
            }
            if is_space(b) {
                self.pos += 1;
                continue;
            }
            if self.bol
                && let Some(end) = go_line(src, i)
            {
                self.pos = end;
                continue;
            }
            if let Some((delim, end)) = delimiter_line(src, i) {
                self.delim = src[delim].to_vec();
                self.pos = end;
                self.bol = false;
                continue;
            }
            if src[i..].starts_with(&self.delim) {
                self.pos = i + self.delim.len();
                self.bol = false;
                continue;
            }
            match lex::lexeme(src, i, self.esc) {
                Lexeme::Comment(end) => {
                    self.pos = end;
                    self.bol = false;
                }
                Lexeme::Open => {
                    self.broken = true;
                    return None;
                }
                Lexeme::Token(_) => break i,
            }
        };
        self.bol = false;
        let mut i = start;
        // One past the last byte of the statement's last token.
        let mut last = start;
        loop {
            if i >= n {
                self.pos = n;
                self.sql_end = last;
                return Some(start..last);
            }
            let b = src[i];
            if b == b'\n' {
                i += 1;
                if let Some(end) = go_line(src, i) {
                    self.pos = end;
                    self.bol = true;
                    self.sql_end = last;
                    return Some(start..last);
                }
                continue;
            }
            if is_space(b) {
                i += 1;
                continue;
            }
            if src[i..].starts_with(&self.delim) {
                let end = i + self.delim.len();
                self.pos = end;
                self.sql_end = end;
                return Some(start..self.copy_data(start, end));
            }
            match lex::lexeme(src, i, self.esc) {
                Lexeme::Comment(end) => i = end,
                Lexeme::Open => {
                    self.broken = true;
                    return None;
                }
                Lexeme::Token(t) => {
                    // A word may end with the delimiter `DELIMITER` set: `END$$`.
                    let inner = (t.tok == lex::Tok::Word && self.delim != b";")
                        .then(|| memchr::memmem::find(&src[i + 1..t.range.end], &self.delim))
                        .flatten();
                    i = inner.map_or(t.range.end, |k| i + 1 + k);
                    last = i;
                }
            }
        }
    }

    /// The end of a `COPY … FROM stdin` statement whose SQL text is `start..end`: its data
    /// lines follow the line it ends on, up to a line `\.` or the end of the Source. `end`
    /// for any other statement.
    fn copy_data(&mut self, start: usize, end: usize) -> usize {
        let src = self.src;
        if !is_copy_from_stdin(src, start, end, self.esc) {
            return end;
        }
        let Some(nl) = self.line_break(end) else {
            return end;
        };
        let mut line = nl + 1;
        while line < src.len() {
            let eol = lex::line_end(src, line);
            let text = &src[line..eol];
            if text.strip_suffix(b"\r").unwrap_or(text) == b"\\." {
                self.pos = line + 2;
                self.bol = false;
                return line + 2;
            }
            line = eol + 1;
        }
        self.pos = src.len();
        src.len()
    }
}

/// Where the data of the `COPY … FROM stdin` statement `range`, whose SQL text ends at
/// `sql_end`, start and end (`\.` left out); None for any other statement.
pub(super) fn copy_rows(
    src: &[u8],
    range: &Range<usize>,
    sql_end: usize,
    esc: bool,
) -> Option<Range<usize>> {
    if !is_copy_from_stdin(src, range.start, sql_end, esc) {
        return None;
    }
    let from =
        memchr::memchr(b'\n', &src[sql_end..range.end]).map_or(range.end, |k| sql_end + k + 1);
    let mut to = range.end;
    if to >= from + 2 && &src[to - 2..to] == b"\\." && (to - 2 == from || src[to - 3] == b'\n') {
        to -= 2;
    }
    Some(from..to.max(from))
}

/// Whether `src[start..end]` is `COPY … FROM stdin`.
fn is_copy_from_stdin(src: &[u8], start: usize, end: usize, esc: bool) -> bool {
    if !src[start..]
        .get(..4)
        .is_some_and(|w| w.eq_ignore_ascii_case(b"COPY"))
        || src.get(start + 4).is_some_and(|&b| is_word(b))
    {
        return false;
    }
    let mut from = false;
    for t in lex::Lexer::new(src, start, end, esc) {
        if from && lex::is_kw(src, &t, "STDIN") {
            return true;
        }
        from = lex::is_kw(src, &t, "FROM");
    }
    false
}

/// The end of the line at `i` (past its `\n`) when it is SQL Server's batch separator: `GO`,
/// in any case, maybe a count, alone on its line.
fn go_line(src: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    while j < src.len() && matches!(src[j], b' ' | b'\t') {
        j += 1;
    }
    if !src[j..]
        .get(..2)
        .is_some_and(|w| w.eq_ignore_ascii_case(b"GO"))
    {
        return None;
    }
    j += 2;
    if src.get(j).is_some_and(|&b| is_word(b)) {
        return None;
    }
    // `GO 5`: a count after white space.
    let skip = |j: &mut usize, f: fn(u8) -> bool| {
        while *j < src.len() && f(src[*j]) {
            *j += 1;
        }
    };
    skip(&mut j, |b| matches!(b, b' ' | b'\t'));
    skip(&mut j, |b| b.is_ascii_digit());
    skip(&mut j, |b| matches!(b, b' ' | b'\t' | b'\r'));
    match src.get(j) {
        None => Some(j),
        Some(b'\n') => Some(j + 1),
        Some(_) => None,
    }
}

/// A `DELIMITER x` line at `i` (the MySQL client's command): the new delimiter's bytes and
/// where the line ends.
fn delimiter_line(src: &[u8], i: usize) -> Option<(Range<usize>, usize)> {
    const KW: &[u8] = b"DELIMITER";
    if !src[i..]
        .get(..KW.len())
        .is_some_and(|w| w.eq_ignore_ascii_case(KW))
    {
        return None;
    }
    let mut j = i + KW.len();
    if !src.get(j).is_some_and(|&b| matches!(b, b' ' | b'\t')) {
        return None;
    }
    while j < src.len() && matches!(src[j], b' ' | b'\t') {
        j += 1;
    }
    let from = j;
    while j < src.len() && !is_space(src[j]) {
        j += 1;
    }
    (j > from).then(|| (from..j, lex::line_end(src, j)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statements(src: &[u8]) -> Vec<&str> {
        let mut u = Units::new(src);
        std::iter::from_fn(|| u.next_statement())
            .map(|r| std::str::from_utf8(&src[r]).unwrap())
            .collect()
    }

    #[test]
    fn delimiters_go_lines_and_literals() {
        let src =
            b"SELECT ';' -- ;\n; ;\nA\n go\nB\nGO 2\nDELIMITER $$\nC; D$$\nDELIMITER ;\nE $$;$$; F";
        assert_eq!(
            statements(src),
            ["SELECT ';' -- ;\n;", "A", "B", "C; D$$", "E $$;$$;", "F"]
        );
    }

    #[test]
    fn copy_data_runs_to_the_end_marker_or_the_end() {
        let src = b"COPY t (a) FROM stdin;\n1;x\n\\.\nSELECT 1;\nCOPY u FROM STDIN;\r\n2\r\n";
        let got = statements(src);
        assert_eq!(
            got,
            [
                "COPY t (a) FROM stdin;\n1;x\n\\.",
                "SELECT 1;",
                "COPY u FROM STDIN;\r\n2\r\n"
            ]
        );
        let mut u = Units::new(src);
        let r = u.next_statement().unwrap();
        assert_eq!(copy_rows(src, &r, u.sql_end(), true), Some(23..27));
    }

    #[test]
    fn a_literal_left_open_ends_the_statements() {
        assert_eq!(
            statements(b"SELECT 1;\nSELECT 'x;\nSELECT 2;"),
            ["SELECT 1;"]
        );
        assert_eq!(statements(b"/* x"), Vec::<&str>::new());
    }
}
