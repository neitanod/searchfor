//! GraphQL (M6 spec §8.6): definitions are Segments with their keyword as kind; a field with
//! arguments or a selection set, and every field of a type, is a `field` named by its alias or
//! name. Strings, block strings (`"""`) and `#` comments are lexed; commas are white space.
use super::{BlockIndex, Builder, Kind, Part, is_space, tail_end};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Name,
    /// One punctuator byte; `...` is `Punct(b'.')`.
    Punct(u8),
    Str,
    /// A number or a byte GraphQL has no token for.
    Other,
    End,
}

#[derive(Clone, Copy)]
struct Lexer<'s> {
    src: &'s [u8],
    i: usize,
}

impl<'s> Lexer<'s> {
    fn next(&mut self) -> (Tok, Range<usize>) {
        let src = self.src;
        let mut i = self.i;
        loop {
            match src.get(i) {
                Some(&c) if is_space(c) || c == b',' => i += 1,
                Some(b'#') => {
                    i = memchr::memchr(b'\n', &src[i..]).map_or(src.len(), |k| i + k);
                }
                _ => break,
            }
        }
        let s = i;
        let Some(&c) = src.get(i) else {
            self.i = i;
            return (Tok::End, i..i);
        };
        let tok = if c.is_ascii_alphabetic() || c == b'_' {
            while src
                .get(i)
                .is_some_and(|&c| c.is_ascii_alphanumeric() || c == b'_')
            {
                i += 1;
            }
            Tok::Name
        } else if c == b'"' {
            i = string_end(src, i);
            Tok::Str
        } else if src[i..].starts_with(b"...") {
            i += 3;
            Tok::Punct(b'.')
        } else if b"!$&()[]{}:=@|".contains(&c) {
            i += 1;
            Tok::Punct(c)
        } else if c == b'-' || c.is_ascii_digit() {
            i += 1;
            while src
                .get(i)
                .is_some_and(|&c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'-'))
            {
                i += 1;
            }
            Tok::Other
        } else {
            i += 1;
            Tok::Other
        };
        self.i = i;
        (tok, s..i)
    }

    fn peek(&self) -> (Tok, Range<usize>) {
        let mut l = *self;
        l.next()
    }

    /// Past a `(`, `[` or `{` just read, up to its closer: the end of the closer.
    fn balanced(&mut self, open: u8) -> usize {
        let close = match open {
            b'(' => b')',
            b'[' => b']',
            _ => b'}',
        };
        let mut depth = 1usize;
        loop {
            match self.next() {
                (Tok::End, r) => return r.end,
                (Tok::Punct(c), r) if c == close => {
                    depth -= 1;
                    if depth == 0 {
                        return r.end;
                    }
                }
                (Tok::Punct(c), _) if c == open => depth += 1,
                _ => {}
            }
        }
    }

    /// Past the directives (`@name(args)`) that come next: the end of the last, or `end`.
    fn directives(&mut self, mut end: usize) -> usize {
        while let (Tok::Punct(b'@'), _) = self.peek() {
            self.next();
            if let (Tok::Name, r) = self.peek() {
                self.next();
                end = r.end;
            }
            if let (Tok::Punct(b'('), _) = self.peek() {
                self.next();
                end = self.balanced(b'(');
            }
        }
        end
    }

    /// The Name that comes next, read.
    fn name(&mut self) -> Option<Range<usize>> {
        match self.peek() {
            (Tok::Name, r) => {
                self.next();
                Some(r)
            }
            _ => None,
        }
    }

    /// Whether `c` comes next; read when it does.
    fn eat(&mut self, c: u8) -> Option<Range<usize>> {
        match self.peek() {
            (Tok::Punct(p), r) if p == c => {
                self.next();
                Some(r)
            }
            _ => None,
        }
    }

    /// Where the next token starts.
    fn next_start(&self) -> usize {
        self.peek().1.start
    }
}

/// A string from its opening quote: a block string runs to its `"""` (`\"""` is text); a
/// normal one to its `"`, `\` escaping, or to its line's end when left open.
fn string_end(src: &[u8], i: usize) -> usize {
    if src[i..].starts_with(b"\"\"\"") {
        let mut k = i + 3;
        while k < src.len() {
            if src[k..].starts_with(b"\\\"\"\"") {
                k += 4;
            } else if src[k..].starts_with(b"\"\"\"") {
                return k + 3;
            } else {
                k += 1;
            }
        }
        return src.len();
    }
    let mut k = i + 1;
    while k < src.len() {
        match src[k] {
            b'\\' => k += 2,
            b'"' => return k + 1,
            b'\n' => return k,
            _ => k += 1,
        }
    }
    src.len()
}

/// An open `{`: its Segment (None past the depth, or an inline fragment's) and what it holds.
struct Frame {
    id: Option<u32>,
    /// A type's fields (or an enum's values when `values`), else a selection set.
    fields: bool,
    values: bool,
}

fn definition_kind(word: &[u8]) -> Option<Kind> {
    Some(match word {
        b"type" => Kind::Type,
        b"interface" => Kind::Interface,
        b"input" => Kind::Input,
        b"enum" => Kind::Enum,
        b"union" => Kind::Union,
        b"scalar" => Kind::Scalar,
        b"schema" => Kind::Schema,
        b"directive" => Kind::Directive,
        b"query" => Kind::Query,
        b"mutation" => Kind::Mutation,
        b"subscription" => Kind::Subscription,
        b"fragment" => Kind::Fragment,
        _ => return None,
    })
}

pub(super) fn scan(src: &[u8], name_chars: usize) -> anyhow::Result<BlockIndex> {
    let mut b = Builder::new(name_chars);
    let mut frames: Vec<Frame> = Vec::new();
    let mut lx = Lexer { src, i: 0 };
    loop {
        let (tok, r) = lx.next();
        match tok {
            Tok::End => break,
            Tok::Punct(b'}') => match frames.pop() {
                Some(f) => {
                    if let Some(id) = f.id {
                        b.close(id, r.end);
                    }
                }
                // A `}` that closes nothing ends the structure.
                None => break,
            },
            _ if frames.last().is_some_and(|f| f.fields) => {
                let values = frames.last().is_some_and(|f| f.values);
                if tok == Tok::Name {
                    field_definition(&mut lx, &mut b, r, values);
                }
            }
            _ if !frames.is_empty() => selection(&mut lx, &mut b, &mut frames, tok, r),
            Tok::Punct(b'{') => {
                // An anonymous operation is a `query` named `query`, a word of s4's.
                let id = b.open(
                    Kind::Query,
                    r.start,
                    r.start..r.start,
                    &[Part::Word(0)],
                    false,
                );
                frames.push(Frame {
                    id,
                    fields: false,
                    values: false,
                });
            }
            Tok::Name => definition(&mut lx, &mut b, &mut frames, r),
            _ => {}
        }
    }
    Ok(b.finish(src.len()))
}

/// A top-level definition whose first word is `word`.
fn definition(lx: &mut Lexer, b: &mut Builder, frames: &mut Vec<Frame>, word: Range<usize>) {
    let src = lx.src;
    let start = word.start;
    let (kind, keyword) = if &src[word.clone()] == b"extend" {
        match lx.peek() {
            (Tok::Name, k) if definition_kind(&src[k.clone()]).is_some() => {
                lx.next();
                (Kind::Extend, k)
            }
            _ => return,
        }
    } else {
        match definition_kind(&src[word.clone()]) {
            Some(kind) => (kind, word),
            None => return,
        }
    };
    let base = definition_kind(&src[keyword.clone()]).expect("checked");
    let name = match base {
        Kind::Schema => Some(keyword.clone()),
        Kind::Directive => {
            lx.eat(b'@');
            lx.name()
        }
        _ => lx.name(),
    };
    let named = name.clone().unwrap_or(keyword.clone());
    let label = start..named.end;
    let parts = [Part::src(named.clone())];
    let mut end = named.end;
    match base {
        Kind::Query | Kind::Mutation | Kind::Subscription => {
            if lx.eat(b'(').is_some() {
                end = lx.balanced(b'(');
            }
            end = lx.directives(end);
        }
        Kind::Fragment => {
            if let (Tok::Name, r) = lx.peek()
                && &src[r.clone()] == b"on"
            {
                lx.next();
                end = lx.name().map_or(r.end, |n| n.end);
            }
            end = lx.directives(end);
        }
        Kind::Directive => {
            if lx.eat(b'(').is_some() {
                end = lx.balanced(b'(');
            }
            if let (Tok::Name, r) = lx.peek()
                && &src[r.clone()] == b"repeatable"
            {
                lx.next();
                end = r.end;
            }
            // The locations: names joined by `|`.
            if let (Tok::Name, r) = lx.peek()
                && &src[r.clone()] == b"on"
            {
                lx.next();
                end = r.end;
                lx.eat(b'|');
                while let Some(n) = lx.name() {
                    end = n.end;
                    if lx.eat(b'|').is_none() {
                        break;
                    }
                }
            }
            b.leaf(
                kind,
                start..tail_end(src, end, lx.next_start()),
                label,
                &parts,
            );
            return;
        }
        Kind::Union => {
            end = lx.directives(end);
            if let Some(eq) = lx.eat(b'=') {
                end = eq.end;
                lx.eat(b'|');
                while let Some(n) = lx.name() {
                    end = n.end;
                    if lx.eat(b'|').is_none() {
                        break;
                    }
                }
            }
            b.leaf(
                kind,
                start..tail_end(src, end, lx.next_start()),
                label,
                &parts,
            );
            return;
        }
        Kind::Scalar => {
            end = lx.directives(end);
            b.leaf(
                kind,
                start..tail_end(src, end, lx.next_start()),
                label,
                &parts,
            );
            return;
        }
        _ => {
            // type, interface, input, enum, schema: `implements A & B`, directives.
            if let (Tok::Name, r) = lx.peek()
                && &src[r.clone()] == b"implements"
            {
                lx.next();
                end = r.end;
                lx.eat(b'&');
                while let Some(n) = lx.name() {
                    end = n.end;
                    if lx.eat(b'&').is_none() {
                        break;
                    }
                }
            }
            end = lx.directives(end);
        }
    }
    let fields = !matches!(
        base,
        Kind::Query | Kind::Mutation | Kind::Subscription | Kind::Fragment
    );
    if lx.eat(b'{').is_some() {
        let id = b.open(kind, start, label, &parts, false);
        frames.push(Frame {
            id,
            fields,
            values: base == Kind::Enum,
        });
    } else {
        // A definition without a body still delimits its own Section.
        b.leaf(
            kind,
            start..tail_end(src, end, lx.next_start()),
            label,
            &parts,
        );
    }
}

/// A field of a type, an input or the schema (`name(args): Type = default @dir`), or an enum's
/// value (`NAME @dir`), whose name `name` was just read: a `field` up to its line's end.
fn field_definition(lx: &mut Lexer, b: &mut Builder, name: Range<usize>, values: bool) {
    let src = lx.src;
    let mut end = name.end;
    if !values {
        if lx.eat(b'(').is_some() {
            end = lx.balanced(b'(');
        }
        if lx.eat(b':').is_some() {
            if lx.eat(b'[').is_some() {
                end = lx.balanced(b'[');
            } else if let Some(t) = lx.name() {
                end = t.end;
            }
            if let Some(bang) = lx.eat(b'!') {
                end = bang.end;
            }
        }
        if lx.eat(b'=').is_some() {
            end = value(lx).unwrap_or(end);
        }
    }
    end = lx.directives(end);
    let range = name.start..tail_end(src, end, lx.next_start());
    b.leaf(Kind::Field, range, name.clone(), &[Part::src(name)]);
}

/// Past a value (a default): the end of its last token.
fn value(lx: &mut Lexer) -> Option<usize> {
    match lx.next() {
        (Tok::Punct(c @ (b'{' | b'[')), _) => Some(lx.balanced(c)),
        (Tok::Punct(b'$'), r) => Some(lx.name().map_or(r.end, |n| n.end)),
        (Tok::End, _) => None,
        (_, r) => Some(r.end),
    }
}

/// A token `tok` at `r` in a selection set: a field (`alias: name(args) @dir { … }`), which is
/// a Segment when it has arguments or a selection set, or a fragment.
fn selection(lx: &mut Lexer, b: &mut Builder, frames: &mut Vec<Frame>, tok: Tok, r: Range<usize>) {
    let src = lx.src;
    match tok {
        Tok::Punct(b'.') => {
            // `... on Type @dir { … }` or `... @dir { … }`: its selections are its parent's.
            if let (Tok::Name, n) = lx.peek()
                && &src[n] == b"on"
            {
                lx.next();
                lx.name();
            } else if let (Tok::Name, _) = lx.peek() {
                lx.next();
            }
            lx.directives(0);
            if lx.eat(b'{').is_some() {
                frames.push(Frame {
                    id: None,
                    fields: false,
                    values: false,
                });
            }
        }
        Tok::Name => {
            let named = r.clone();
            let mut label = r.clone();
            if lx.eat(b':').is_some()
                && let Some(n) = lx.name()
            {
                label.end = n.end;
            }
            let mut end = label.end;
            let mut segment = false;
            if lx.eat(b'(').is_some() {
                end = lx.balanced(b'(');
                segment = true;
            }
            end = lx.directives(end);
            let parts = [Part::src(named)];
            if lx.eat(b'{').is_some() {
                let id = b.open(Kind::Field, r.start, label, &parts, false);
                frames.push(Frame {
                    id,
                    fields: false,
                    values: false,
                });
            } else if segment {
                let range = r.start..tail_end(src, end, lx.next_start());
                b.leaf(Kind::Field, range, label, &parts);
            }
        }
        Tok::Punct(b'{') => {
            // A selection set with no field before it: no Segment, its braces kept.
            frames.push(Frame {
                id: None,
                fields: false,
                values: false,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &[u8]) -> Vec<(String, &'static str)> {
        let ix = scan(src, 40).unwrap();
        (0..ix.entries.len())
            .map(|i| (ix.name(src, i, 40).0, ix.entries[i].kind.as_str()))
            .collect()
    }

    #[test]
    fn definitions_fields_and_selections() {
        let src = b"\"\"\"doc\"\"\"\ntype A implements B & C @k(x: 1) {\n \"d\" f(a: Int = 1): [String!]! @deprecated\n g: Int = 2\n}\nenum E { X Y @d }\nquery Q($v: ID = 3) @dir {\n a: b(x: {y: \"}\"}) { c ... on D { e(f: 1) } ...G }\n}\nextend schema { query: Q }\n";
        assert_eq!(
            names(src),
            [
                ("A".to_string(), "type"),
                ("f".into(), "field"),
                ("g".into(), "field"),
                ("E".into(), "enum"),
                ("X".into(), "field"),
                ("Y".into(), "field"),
                ("Q".into(), "query"),
                ("a".into(), "field"),
                ("e".into(), "field"),
                ("schema".into(), "extend"),
                ("query".into(), "field"),
            ]
        );
    }

    #[test]
    fn bodiless_definitions_end_with_their_line() {
        let src = b"union U =\n | A\n | B # c\nscalar S @x\ndirective @d(a: Int) repeatable on FIELD | QUERY\ntype T\n";
        let ix = scan(src, 40).unwrap();
        let texts: Vec<&[u8]> = ix.entries.iter().map(|e| &src[e.range()]).collect();
        assert_eq!(
            texts,
            [
                b"union U =\n | A\n | B # c".as_slice(),
                b"scalar S @x",
                b"directive @d(a: Int) repeatable on FIELD | QUERY",
                b"type T",
            ]
        );
    }
}
