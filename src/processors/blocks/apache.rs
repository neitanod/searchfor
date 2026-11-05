//! Apache (M6 spec §7.7): `<Name args>` … `</Name>` sections, compared without case, nest; a
//! directive is one line, `\` continuing it; `#` starts a comment line.
use super::{BlockIndex, Builder, Kind, Part, is_space};
use std::ops::Range;

/// One open section: its Segment (None past the depth), its name, and for a `VirtualHost`
/// that has not met its `ServerName` yet, true.
struct Section {
    id: Option<u32>,
    name: Range<usize>,
    vhost: bool,
}

pub(super) fn scan(src: &[u8], name_chars: usize) -> anyhow::Result<BlockIndex> {
    let mut b = Builder::new(name_chars);
    let keep = if name_chars == 0 {
        usize::MAX
    } else {
        name_chars + 1
    };
    let mut open: Vec<Section> = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let (end, next) = logical_line(src, i);
        let mut s = i;
        while s < end && is_space(src[s]) {
            s += 1;
        }
        let mut e = end;
        while e > s && is_space(src[e - 1]) {
            e -= 1;
        }
        i = next;
        if s == e || src[s] == b'#' {
            continue;
        }
        if src[s] == b'<' && src.get(s + 1) == Some(&b'/') {
            let name = s + 2..word_end(src, s + 2, e);
            // A closing tag closes its section and the sections inside it; one that matches
            // no open section is ignored (M6 spec §7.7).
            if let Some(k) = open
                .iter()
                .rposition(|o| src[o.name.clone()].eq_ignore_ascii_case(&src[name.clone()]))
            {
                for o in open.drain(k..).rev() {
                    if let Some(id) = o.id {
                        b.close(id, e);
                    }
                }
            }
        } else if src[s] == b'<' {
            let close = memchr::memrchr(b'>', &src[s..e]).map_or(e, |k| s + k);
            let words = words(src, s + 1..close, keep);
            let Some(name) = words.first().cloned() else {
                continue;
            };
            let parts: Vec<Part> = words.into_iter().map(Part::src).collect();
            let id = b.open(Kind::Block, s, s..(close + 1).min(e), &parts, false);
            let vhost = src[name.clone()].eq_ignore_ascii_case(b"VirtualHost");
            open.push(Section { id, name, vhost });
        } else {
            let name = s..word_end(src, s, e);
            let id = b.leaf(Kind::Key, s..e, name.clone(), &[Part::src(name.clone())]);
            if id.is_some()
                && src[name.clone()].eq_ignore_ascii_case(b"ServerName")
                && let Some(o) = open.last_mut()
                && o.vhost
                && let Some(vhost) = o.id
                && let Some(value) = words(src, name.end..e, 1).into_iter().next()
            {
                b.late(vhost, value);
                o.vhost = false;
            }
        }
    }
    Ok(b.finish(src.len()))
}

/// The end of the logical line that starts at `i` (before its `\n`), physical lines whose last
/// character is `\` joined, and where the next one starts.
fn logical_line(src: &[u8], mut i: usize) -> (usize, usize) {
    loop {
        let Some(k) = memchr::memchr(b'\n', &src[i..]) else {
            return (src.len(), src.len());
        };
        let nl = i + k;
        let body = src[i..nl].strip_suffix(b"\r").unwrap_or(&src[i..nl]);
        if body.ends_with(b"\\") {
            i = nl + 1;
            continue;
        }
        return (nl, nl + 1);
    }
}

/// Where the word that starts at `i` ends, at white space, `>` or `end`.
fn word_end(src: &[u8], mut i: usize, end: usize) -> usize {
    while i < end && !is_space(src[i]) && src[i] != b'>' {
        i += 1;
    }
    i
}

/// The first `keep` white-space separated words of `src[r]`, a continuation `\` left out.
fn words(src: &[u8], r: Range<usize>, keep: usize) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut i = r.start;
    while i < r.end && out.len() < keep {
        while i < r.end && is_space(src[i]) {
            i += 1;
        }
        let s = i;
        while i < r.end && !is_space(src[i]) {
            i += 1;
        }
        if s < i && &src[s..i] != b"\\" {
            out.push(s..i);
        }
    }
    out
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
    fn sections_close_without_case_and_with_their_children() {
        let src =
            b"<IfModule x>\n<Directory /a>\n<Files f>\nA 1\n</directory>\nB 2\n</IfModule>\nC 3\n";
        let ix = scan(src, 40).unwrap();
        assert_eq!(
            names(src),
            ["IfModule x", "Directory /a", "Files f", "A", "B", "C"]
        );
        // `</directory>` closed Files too: B is IfModule's.
        assert_eq!(ix.entries[4].parent(), Some(0));
        assert_eq!(ix.entries[5].parent(), None);
    }

    #[test]
    fn continuations_join_a_directive_and_server_names_come_from_the_own_scope() {
        let src = b"<VirtualHost *:80>\n<Location />\nServerName inner\n</Location>\nRewriteRule a \\\n  b\nservername  site.example other\n</VirtualHost>\n";
        assert_eq!(
            names(src),
            [
                "VirtualHost *:80 site.example",
                "Location /",
                "ServerName",
                "RewriteRule",
                "servername"
            ]
        );
        let ix = scan(src, 40).unwrap();
        let rule = &ix.entries[3];
        assert_eq!(&src[rule.range()], b"RewriteRule a \\\n  b");
    }
}
