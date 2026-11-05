//! Protobuf (M6 spec §8.7): `message`, `enum`, `service`, `rpc`, `oneof` and `extend` are
//! Segments with their keyword as kind, nested; a field (`string email = 1;`) and an enum value
//! are `field` Segments, the rest of their line, an end-of-line comment included, theirs.
//! `package`, `import` and `option` statements are text. `//` and `/* */` comments, strings.
use super::{BlockIndex, Builder, Kind, Part, is_space, tail_end};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Ident,
    Punct(u8),
    /// A string, a number or a byte with no token of its own.
    Other,
    End,
}

#[derive(Clone, Copy)]
struct Lexer<'s> {
    src: &'s [u8],
    i: usize,
}

impl Lexer<'_> {
    fn next(&mut self) -> (Tok, Range<usize>) {
        let src = self.src;
        let mut i = self.i;
        loop {
            match src.get(i) {
                Some(&c) if is_space(c) => i += 1,
                Some(b'/') if src.get(i + 1) == Some(&b'/') => {
                    i = memchr::memchr(b'\n', &src[i..]).map_or(src.len(), |k| i + k);
                }
                Some(b'/') if src.get(i + 1) == Some(&b'*') => {
                    i = memchr::memmem::find(&src[i + 2..], b"*/").map_or(src.len(), |k| i + k + 4);
                }
                _ => break,
            }
        }
        let s = i;
        let Some(&c) = src.get(i) else {
            self.i = i;
            return (Tok::End, i..i);
        };
        let tok = if c.is_ascii_alphabetic() || c == b'_' || c == b'.' {
            while src
                .get(i)
                .is_some_and(|&c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.'))
            {
                i += 1;
            }
            Tok::Ident
        } else if let q @ (b'"' | b'\'') = c {
            i += 1;
            while i < src.len() && src[i] != q && src[i] != b'\n' {
                i += if src[i] == b'\\' { 2 } else { 1 };
            }
            if src.get(i) == Some(&q) {
                i += 1;
            }
            Tok::Other
        } else if b"{}()[]<>;=,".contains(&c) {
            i += 1;
            Tok::Punct(c)
        } else {
            i += 1;
            Tok::Other
        };
        self.i = i.min(src.len());
        (tok, s..self.i)
    }

    fn peek(&self) -> (Tok, Range<usize>) {
        let mut l = *self;
        l.next()
    }
}

fn block_kind(word: &[u8]) -> Option<Kind> {
    Some(match word {
        b"message" => Kind::Message,
        b"enum" => Kind::Enum,
        b"service" => Kind::Service,
        b"oneof" => Kind::Oneof,
        b"extend" => Kind::Extend,
        b"rpc" => Kind::Rpc,
        _ => return None,
    })
}

pub(super) fn scan(src: &[u8], name_chars: usize) -> anyhow::Result<BlockIndex> {
    let mut b = Builder::new(name_chars);
    // One per open `{`: its Segment (None past the depth, or a brace that holds no Segment).
    let mut frames: Vec<Option<u32>> = Vec::new();
    let mut lx = Lexer { src, i: 0 };
    loop {
        let (tok, r) = lx.next();
        match tok {
            Tok::End => break,
            Tok::Punct(b'}') => match frames.pop() {
                Some(f) => {
                    if let Some(id) = f {
                        b.close(id, r.end);
                    }
                }
                // A `}` that closes nothing ends the structure.
                None => break,
            },
            Tok::Punct(b'{') => frames.push(None),
            Tok::Ident => {
                let word = &src[r.clone()];
                if matches!(
                    word,
                    b"syntax"
                        | b"edition"
                        | b"package"
                        | b"import"
                        | b"option"
                        | b"reserved"
                        | b"extensions"
                ) {
                    statement(&mut lx);
                    continue;
                }
                if let Some(kind) = block_kind(word)
                    && let (Tok::Ident, name) = lx.peek()
                {
                    lx.next();
                    let label = r.start..name.end;
                    let parts = [Part::src(name.clone())];
                    // An rpc's request and response, up to its body or its `;`.
                    let (end, last) = if kind == Kind::Rpc {
                        statement(&mut lx)
                    } else {
                        match lx.peek() {
                            (Tok::Punct(b'{'), o) => {
                                lx.next();
                                (b'{', o.end)
                            }
                            _ => statement(&mut lx),
                        }
                    };
                    if end == b'{' {
                        let id = b.open(kind, r.start, label, &parts, false);
                        frames.push(id);
                    } else if kind == Kind::Rpc {
                        let range = r.start..tail_end(src, last, lx.peek().1.start);
                        b.leaf(kind, range, label, &parts);
                    }
                    continue;
                }
                field(&mut lx, &mut b, &mut frames, r);
            }
            _ => {}
        }
    }
    Ok(b.finish(src.len()))
}

/// Past a statement up to its `;` or its `{` at its own depth: which one (0 at the end of the
/// file) and the end of its last token.
fn statement(lx: &mut Lexer) -> (u8, usize) {
    let mut depth = 0usize;
    let mut last = lx.i;
    loop {
        let (tok, r) = lx.next();
        match tok {
            Tok::End => return (0, last),
            Tok::Punct(b'(' | b'[') => depth += 1,
            Tok::Punct(b')' | b']') => depth = depth.saturating_sub(1),
            // An option's aggregate value: its braces are not structure.
            Tok::Punct(b'{') if depth > 0 || lx.src[..r.start].trim_ascii_end().ends_with(b"=") => {
                depth += 1
            }
            Tok::Punct(b'}') if depth > 0 => depth -= 1,
            Tok::Punct(c @ (b';' | b'{')) if depth == 0 => return (c, r.end),
            // A `}` that closes the enclosing block ends the statement before it.
            Tok::Punct(b'}') => {
                lx.i = r.start;
                return (0, last);
            }
            _ => {}
        }
        last = r.end;
    }
}

/// A field or an enum value, whose first token `first` was just read: a `field` named by the
/// identifier before its `=`, its name its label, up to its line's end. A statement with no
/// `=` holds no Segment; one ending in `{` (a group) opens a brace with none.
fn field(lx: &mut Lexer, b: &mut Builder, frames: &mut Vec<Option<u32>>, first: Range<usize>) {
    let src = lx.src;
    let mut name = None;
    let mut prev = (Tok::Ident, first.clone());
    let mut depth = 0usize;
    let (end, last) = loop {
        let (tok, r) = lx.next();
        match tok {
            Tok::End => break (0, prev.1.end),
            Tok::Punct(b'(' | b'[' | b'<') => depth += 1,
            Tok::Punct(b')' | b']' | b'>') => depth = depth.saturating_sub(1),
            Tok::Punct(b'=') if depth == 0 && name.is_none() && prev.0 == Tok::Ident => {
                name = Some(prev.1.clone());
            }
            Tok::Punct(b'{') if depth > 0 => depth += 1,
            Tok::Punct(b'}') if depth > 0 => depth -= 1,
            Tok::Punct(c @ (b';' | b'{')) if depth == 0 => break (c, r.end),
            Tok::Punct(b'}') if depth == 0 => {
                lx.i = r.start;
                break (0, prev.1.end);
            }
            _ => {}
        }
        prev = (tok, r);
    };
    match (end, name) {
        (b'{', _) => frames.push(None),
        (b';', Some(name)) if !frames.is_empty() => {
            let range = first.start..tail_end(src, last, lx.peek().1.start);
            b.leaf(Kind::Field, range, name.clone(), &[Part::src(name)]);
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
    fn statements_members_and_options() {
        let src = b"syntax = \"proto3\";\npackage a.b;\noption (x) = { y: 1 };\nmessage M {\n option z = { w: \"}\" };\n map<string, int32> m = 1 [(o) = { p: 2 }];\n /* } */ repeated .a.T t = 2;\n reserved 3;\n enum E { A = 0; }\n}\nservice S { rpc R (M) returns (stream M); }\nextend google.protobuf.FieldOptions { string f = 50; }\n";
        assert_eq!(
            names(src),
            [
                ("M".to_string(), "message"),
                ("m".into(), "field"),
                ("t".into(), "field"),
                ("E".into(), "enum"),
                ("A".into(), "field"),
                ("S".into(), "service"),
                ("R".into(), "rpc"),
                ("google.protobuf.FieldOptions".into(), "extend"),
                ("f".into(), "field"),
            ]
        );
    }
}
