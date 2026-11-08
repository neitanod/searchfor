//! Terraform and HCL (M6 spec §7.8): blocks (`resource "aws_instance" "web" { … }`) named by
//! their type and labels without quotes, attributes as keys, an object value's keys inside
//! its key, and a list's elements as `key[N]`. `#`, `//` and `/* */` comments; strings with
//! `${ … }` and `%{ … }`, whose braces and quotes nest inside the string; heredocs. Nothing is
//! evaluated: no include, no variable, no Terraform address.
use super::{BlockIndex, Builder, Kind, Part, is_space};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// The file or a block's body: blocks and attributes, one a line.
    Body,
    /// An object value: attributes separated by new lines or commas.
    Object,
    /// A list value: elements separated by commas.
    List,
}

/// One open `{` or `[`: its Segment (None past the depth or for an expression's brace) and,
/// for a list, the name its elements extend and how many it has had.
struct Frame {
    ctx: Ctx,
    id: Option<u32>,
    holder: Vec<Part>,
    n: u32,
}

pub(super) fn scan(src: &[u8], name_chars: usize) -> anyhow::Result<BlockIndex> {
    let mut b = Builder::new(name_chars);
    let keep = if name_chars == 0 {
        usize::MAX
    } else {
        name_chars + 1
    };
    let mut frames: Vec<Frame> = Vec::new();
    let mut i = 0;
    'scan: while i < src.len() {
        let ctx = frames.last().map_or(Ctx::Body, |f| f.ctx);
        i = skip(src, i, true);
        if i >= src.len() {
            break;
        }
        let c = src[i];
        // Closers: the open frame's, else the structure ends there (M6 spec §7.8, Failure).
        if matches!(c, b'}' | b']' | b')') {
            let fits = match (c, ctx) {
                (b'}', Ctx::Body | Ctx::Object) => !frames.is_empty(),
                (b']', Ctx::List) => true,
                _ => false,
            };
            if !fits {
                break;
            }
            let f = frames.pop().expect("a frame closes");
            if let Some(id) = f.id {
                b.close(id, i + 1);
            }
            i += 1;
            continue;
        }
        if c == b',' && ctx != Ctx::Body {
            i += 1;
            continue;
        }
        if ctx == Ctx::List {
            let f = frames.last_mut().expect("a list frame");
            // A list that opened no Segment (past the depth) holds none either, so its
            // elements need no name: building one copied the holder's at every level (Codex
            // r1 #2). A name keeps no more pieces than its cut shows.
            let mut parts = Vec::new();
            if f.id.is_some() {
                parts.extend_from_slice(&f.holder[..f.holder.len().min(keep)]);
                if parts.len() < keep {
                    parts.push(Part::Index(f.n));
                }
            }
            f.n = f.n.saturating_add(1);
            match c {
                b'{' if !comprehension(src, i + 1) => {
                    let id = b.open(Kind::Element, i, i..i, &parts, false);
                    frames.push(Frame::new(Ctx::Object, id));
                    i += 1;
                }
                b'[' if !comprehension(src, i + 1) => {
                    let id = b.open(Kind::Element, i, i..i, &parts, true);
                    frames.push(Frame {
                        holder: parts,
                        ..Frame::new(Ctx::List, id)
                    });
                    i += 1;
                }
                _ => {
                    let (stop, last) = expression(src, i, Ctx::List);
                    if last > i {
                        b.leaf(Kind::Element, i..last, i..i, &parts);
                    }
                    i = stop.max(i + usize::from(stop == i && !is_closer(src[i])));
                }
            }
            continue;
        }
        // A block header or an attribute's key: names and quoted labels.
        let start = i;
        let mut labels: Vec<Range<usize>> = Vec::new();
        let mut count = 0usize;
        loop {
            i = skip(src, i, false);
            let Some(&c) = src.get(i) else {
                break 'scan;
            };
            match c {
                b'"' => {
                    let end = string_end(src, i);
                    let inner = i + 1..end.saturating_sub(1).max(i + 1);
                    if count < keep {
                        labels.push(inner);
                    }
                    count += 1;
                    i = end;
                }
                b'=' if !matches!(src.get(i + 1), Some(b'=' | b'>')) => break,
                b':' if ctx == Ctx::Object => break,
                b'{' if !labels.is_empty() => break,
                c if ident(c) => {
                    let s = i;
                    while i < src.len() && ident(src[i]) {
                        i += 1;
                    }
                    if count < keep {
                        labels.push(s..i);
                    }
                    count += 1;
                }
                // Neither a key nor a block: skip the rest as an expression.
                _ => {
                    if i == start {
                        let (stop, _) = expression(src, i, ctx);
                        i = stop.max(i + usize::from(stop == i && !is_closer(src[i])));
                    }
                    continue 'scan;
                }
            }
        }
        if labels.is_empty() {
            // `= x` or `: x` with no name before it: an expression, no key (Fable r1 #18).
            let (stop, _) = expression(src, i, ctx);
            i = stop.max(i + usize::from(stop == i && !is_closer(src[i])));
            continue;
        }
        let parts: Vec<Part> = labels.iter().cloned().map(Part::src).collect();
        if src[i] == b'{' {
            let id = b.open(Kind::Block, start, start..i + 1, &parts, false);
            frames.push(Frame::new(Ctx::Body, id));
            i += 1;
            continue;
        }
        // An attribute: its key is its first name.
        let key = labels[0].clone();
        let label = start..key.end.max(start);
        let key_parts = &parts[..parts.len().min(1)];
        i = skip(src, i + 1, false);
        match src.get(i) {
            Some(b'{') if !comprehension(src, i + 1) => {
                let id = b.open(Kind::Key, start, label, key_parts, false);
                frames.push(Frame::new(Ctx::Object, id));
                i += 1;
            }
            Some(b'[') if !comprehension(src, i + 1) => {
                let id = b.open(Kind::Key, start, label, key_parts, true);
                frames.push(Frame {
                    holder: key_parts.to_vec(),
                    ..Frame::new(Ctx::List, id)
                });
                i += 1;
            }
            _ => {
                // The Section takes the rest of its last line, an end-of-line comment, as
                // GraphQL's and Protobuf's members do (Fable r1 #19).
                let (stop, last) = expression(src, i, ctx);
                let end = super::tail_end(src, last.max(key.end), stop);
                b.leaf(Kind::Key, start..end, label, key_parts);
                i = stop;
            }
        }
    }
    Ok(b.finish(src.len()))
}

impl Frame {
    fn new(ctx: Ctx, id: Option<u32>) -> Self {
        Self {
            ctx,
            id,
            holder: Vec::new(),
            n: 0,
        }
    }
}

fn ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'*') || c >= 0x80
}

fn is_closer(c: u8) -> bool {
    matches!(c, b'}' | b']' | b')')
}

/// Whether the brace or bracket before `i` holds a `for` expression, not an object or a list.
fn comprehension(src: &[u8], i: usize) -> bool {
    let i = skip(src, i, true);
    src[i.min(src.len())..].starts_with(b"for") && src.get(i + 3).is_some_and(|&c| is_space(c))
}

/// Past white space and comments from `i`; past line breaks too when `lines`.
fn skip(src: &[u8], mut i: usize, lines: bool) -> usize {
    while i < src.len() {
        match src[i] {
            b'\n' if !lines => break,
            c if is_space(c) => i += 1,
            b'#' => i = line_end(src, i),
            b'/' if src.get(i + 1) == Some(&b'/') => i = line_end(src, i),
            b'/' if src.get(i + 1) == Some(&b'*') => i = comment_end(src, i),
            _ => break,
        }
    }
    i
}

fn line_end(src: &[u8], i: usize) -> usize {
    memchr::memchr(b'\n', &src[i..]).map_or(src.len(), |k| i + k)
}

fn comment_end(src: &[u8], i: usize) -> usize {
    memchr::memmem::find(&src[i + 2..], b"*/").map_or(src.len(), |k| i + 2 + k + 2)
}

/// An expression from `i`: where it stops (a line break in a body or an object, a comma in an
/// object or a list, a closer it did not open, all at its own depth) and the end of its last
/// token.
fn expression(src: &[u8], mut i: usize, ctx: Ctx) -> (usize, usize) {
    let mut depth = 0usize;
    let mut last = i;
    while i < src.len() {
        match src[i] {
            b'\n' if depth == 0 && ctx != Ctx::List => break,
            b',' if depth == 0 && ctx != Ctx::Body => break,
            c if is_space(c) => i += 1,
            b'#' => i = line_end(src, i),
            b'/' if src.get(i + 1) == Some(&b'/') => i = line_end(src, i),
            b'/' if src.get(i + 1) == Some(&b'*') => i = comment_end(src, i),
            b'"' => {
                i = string_end(src, i);
                last = i;
            }
            b'<' if src.get(i + 1) == Some(&b'<')
                && src
                    .get(i + 2)
                    .is_some_and(|&c| c == b'-' || c.is_ascii_alphabetic() || c == b'_') =>
            {
                i = heredoc_end(src, i);
                last = i;
            }
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
                last = i;
            }
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                i += 1;
                last = i;
            }
            _ => {
                i += 1;
                last = i;
            }
        }
    }
    (i, last)
}

/// The end of the string that starts at `i` (past its closing quote). Interpolations and
/// directives nest, with their own strings; `$${` and `%%{` are text. A line break outside
/// every interpolation ends a string left open.
fn string_end(src: &[u8], mut i: usize) -> usize {
    #[derive(Clone, Copy)]
    enum In {
        Text,
        Template(usize),
    }
    let mut stack = vec![In::Text];
    i += 1;
    while i < src.len() {
        let Some(top) = stack.last_mut() else {
            return i;
        };
        let c = src[i];
        match top {
            In::Text => match c {
                b'\\' => i += 2,
                b'"' => {
                    stack.pop();
                    i += 1;
                }
                b'\n' if stack.len() == 1 => return i,
                b'$' | b'%' if src.get(i + 1) == Some(&c) && src.get(i + 2) == Some(&b'{') => {
                    i += 3
                }
                b'$' | b'%' if src.get(i + 1) == Some(&b'{') => {
                    stack.push(In::Template(0));
                    i += 2;
                }
                _ => i += 1,
            },
            In::Template(depth) => {
                match c {
                    b'"' => stack.push(In::Text),
                    b'{' => *depth += 1,
                    b'}' if *depth == 0 => {
                        stack.pop();
                    }
                    b'}' => *depth -= 1,
                    _ => {}
                }
                i += 1;
            }
        }
    }
    i.min(src.len())
}

/// The end of the heredoc that starts at `i` (`<<EOT` or `<<-EOT`): past its marker line,
/// which `<<-` lets be indented; the end of the file when the marker never comes.
fn heredoc_end(src: &[u8], i: usize) -> usize {
    let mut s = i + 2;
    let indent = src.get(s) == Some(&b'-');
    if indent {
        s += 1;
    }
    let mut e = s;
    while e < src.len() && (src[e].is_ascii_alphanumeric() || src[e] == b'_') {
        e += 1;
    }
    let marker = &src[s..e];
    let mut at = line_end(src, e);
    while at < src.len() {
        let from = at + 1;
        let to = line_end(src, from);
        let mut line = &src[from..to];
        line = line.strip_suffix(b"\r").unwrap_or(line);
        let trimmed = if indent {
            line.trim_ascii_start()
        } else {
            line
        };
        if !marker.is_empty() && trimmed.trim_ascii_end() == marker {
            return from + (line.len() - (trimmed.len() - trimmed.trim_ascii_end().len()));
        }
        at = to;
    }
    src.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &[u8]) -> Vec<String> {
        let ix = scan(src, 40).unwrap();
        (0..ix.entries.len())
            .map(|i| ix.name(src, i, 40).0)
            .collect()
    }

    #[test]
    fn blocks_keys_and_elements() {
        assert_eq!(
            names(b"a \"b\" c {\n x = 1\n l = [1, [2, 3], {k = \"v\"}]\n o = { p: 1, \"q r\" = 2 }\n}\n"),
            ["a b c", "x", "l", "l[0]", "l[1]", "l[1][0]", "l[1][1]", "l[2]", "k", "o", "p", "q r"]
        );
    }

    #[test]
    fn heredocs_and_comprehensions_hold_no_structure() {
        let src = b"a {\n s = <<EOT\n}\n  EOT\nEOT\n f = { for k, v in m : k => v }\n g = [for x in y : x]\n}\n";
        assert_eq!(names(src), ["a", "s", "f", "g"]);
        let ix = scan(src, 40).unwrap();
        // The heredoc's indented `EOT` is text: `<<` needs its marker at the line start.
        assert!(src[ix.entries[1].range()].ends_with(b"  EOT\nEOT"));
    }

    /// Codex r1 #2: each nested list copied its holder's whole name, past the depth too: 5 000
    /// levels in 10 KB held 225 MB. Now as many levels take about what as many flat lists do.
    #[test]
    fn nested_lists_cost_what_flat_ones_do() {
        let n = 20_000;
        let deep = format!("a = {}\"needle\"{}\n", "[".repeat(n), "]".repeat(n));
        let flat = format!("a = [{}\"needle\"]\n", "[],".repeat(n));
        let t_deep = super::super::tests::fastest(|| drop(scan(deep.as_bytes(), 40).unwrap()));
        let t_flat = super::super::tests::fastest(|| drop(scan(flat.as_bytes(), 40).unwrap()));
        assert!(
            t_deep < t_flat * 4 + std::time::Duration::from_millis(20),
            "deep {t_deep:?}, flat {t_flat:?}"
        );
        let ix = scan(deep.as_bytes(), 40).unwrap();
        assert_eq!(ix.entries.len(), super::super::MAX_DEPTH);
        assert_eq!(ix.name(deep.as_bytes(), 3, 40).0, "a[0][0][0]");
    }

    #[test]
    fn templates_nest_inside_strings() {
        assert_eq!(
            names(b"x = \"${a(\"}\")} $${b} %{if c}{%{endif}\"\ny = 1\n"),
            ["x", "y"]
        );
    }
}
