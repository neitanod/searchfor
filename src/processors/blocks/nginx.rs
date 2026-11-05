//! nginx (M6 spec §7.6): a directive ends at `;`, a block directive opens `{` … `}`. `#`
//! comments, `'…'` and `"…"` strings with `\` escapes; braces outside strings are structure.
use super::{BlockIndex, Builder, Kind, Part, is_space};
use std::ops::Range;

/// One open `{`: its Segment (None past the depth) and, for a `server` block that has not met
/// its `server_name` yet, true.
struct Frame {
    id: Option<u32>,
    server: bool,
}

pub(super) fn scan(src: &[u8], name_chars: usize) -> anyhow::Result<BlockIndex> {
    let mut b = Builder::new(name_chars);
    let mut frames: Vec<Frame> = Vec::new();
    // The words of the directive being read; past the cut only their count grows.
    let keep = if name_chars == 0 {
        usize::MAX
    } else {
        name_chars + 1
    };
    let mut words: Vec<Range<usize>> = Vec::new();
    let mut count = 0usize;
    let mut start: Option<usize> = None;
    let mut i = 0;
    while i < src.len() {
        let c = src[i];
        if is_space(c) {
            i += 1;
            continue;
        }
        match c {
            b'#' => i = memchr::memchr(b'\n', &src[i..]).map_or(src.len(), |k| i + k),
            b';' => {
                if let (Some(s), Some(name)) = (start, words.first().cloned()) {
                    let id = b.leaf(
                        Kind::Key,
                        s..i + 1,
                        name.clone(),
                        &[Part::src(name.clone())],
                    );
                    if id.is_some()
                        && &src[name] == b"server_name"
                        && let (Some(value), Some(f)) = (words.get(1), frames.last_mut())
                        && f.server
                        && let Some(server) = f.id
                    {
                        b.late(server, value.clone());
                        f.server = false;
                    }
                }
                (start, count) = (None, 0);
                words.clear();
                i += 1;
            }
            b'{' => {
                let s = start.unwrap_or(i);
                let parts: Vec<Part> = words.iter().cloned().map(Part::src).collect();
                let id = b.open(Kind::Block, s, s..i + 1, &parts, false);
                let server = count == 1 && &src[words[0].clone()] == b"server";
                frames.push(Frame { id, server });
                (start, count) = (None, 0);
                words.clear();
                i += 1;
            }
            b'}' => {
                // A directive left without its `;` names nothing.
                (start, count) = (None, 0);
                words.clear();
                match frames.pop() {
                    Some(f) => {
                        if let Some(id) = f.id {
                            b.close(id, i + 1);
                        }
                    }
                    // A `}` that closes nothing ends the structure (M6 spec §7.6, Failure).
                    None => break,
                }
                i += 1;
            }
            _ => {
                let end = word(src, i);
                start.get_or_insert(i);
                if count < keep {
                    words.push(i..end);
                }
                count += 1;
                i = end;
            }
        }
    }
    Ok(b.finish(src.len()))
}

/// The end of the word or string that starts at `i`.
fn word(src: &[u8], mut i: usize) -> usize {
    if let q @ (b'"' | b'\'') = src[i] {
        i += 1;
        while i < src.len() && src[i] != q {
            i += if src[i] == b'\\' { 2 } else { 1 };
        }
        return (i + 1).min(src.len());
    }
    while i < src.len() {
        match src[i] {
            c if is_space(c) => break,
            b';' | b'{' | b'}' | b'"' | b'\'' => break,
            // `${name}`: a variable's braces are not structure.
            b'$' if src.get(i + 1) == Some(&b'{') => {
                i = memchr::memchr(b'}', &src[i..]).map_or(src.len(), |k| i + k + 1);
            }
            b'\\' => i += 2,
            _ => i += 1,
        }
    }
    i.min(src.len())
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
    fn blocks_name_their_directive_and_arguments() {
        assert_eq!(
            names(b"location ~ \\.php$ {\n if ($host = x) { return 301; }\n}\n"),
            ["location ~ \\.php$", "if ($host = x)", "return"]
        );
        assert_eq!(
            names(b"set $a \"${b}c\"; map $x ${y} { default 1; }"),
            ["set", "map $x ${y}", "default"]
        );
    }

    #[test]
    fn a_server_takes_its_own_first_server_name() {
        let src = b"server {\n location / { server_name inner; }\n server_name a b;\n server_name c;\n}\nupstream u { server 10.0.0.1; }\n";
        assert_eq!(
            names(src),
            [
                "server a",
                "location /",
                "server_name",
                "server_name",
                "server_name",
                "upstream u",
                "server"
            ]
        );
    }
}
