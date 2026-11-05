//! INI, and systemd units through it (M6 spec §7.1, §7.2): `[section]` headers and
//! `key = value` / `key: value` lines, each a `key` Segment that closes at its next sibling.
//! A header is one key as written (`a.b`, `remote "origin"`). A line ending in `\` continues
//! on the next; an indented line continues the value above unless it holds `=` or `:`. Any
//! other line, a malformed header included, is text of the key or section above: INI has no
//! syntax error.
use super::lines::{self, Builder, Kind, Lines, NameSrc, trim};

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

fn scan(src: &[u8], b: &mut Builder) {
    let mut section = false;
    let mut continued = false;
    for (start, end, _) in Lines::new(src) {
        let line = &src[start..end];
        if continued {
            continued = line.ends_with(b"\\");
            continue;
        }
        let body = trim(src, start..end);
        let Some(&first) = src.get(body.start).filter(|_| !body.is_empty()) else {
            continue;
        };
        if first == b';' || first == b'#' {
            continue;
        }
        if first == b'['
            && let Some(close) = memchr::memchr(b']', &src[body.start..body.end])
        {
            b.close_to(0, start);
            let name = trim(src, body.start + 1..body.start + close);
            b.open(start, body.clone(), Kind::Key, NameSrc::Verbatim(name));
            section = true;
            continue;
        }
        let sep = memchr::memchr2(b'=', b':', &src[body.start..body.end]);
        let Some(sep) = sep else {
            // An indented line continues the value above; any other is text of the Segment
            // above.
            continue;
        };
        let key = trim(src, body.start..body.start + sep);
        if key.is_empty() {
            continue;
        }
        b.close_to(usize::from(section), start);
        b.open(start, key.clone(), Kind::Key, NameSrc::Verbatim(key));
        continued = line.ends_with(b"\\");
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::tests::cursor_is_model;
    use super::super::tests::needles;

    #[test]
    fn sections_keys_and_continuations() {
        let src =
            b"top=needle\n[a]\nk = 1 \\\n  [not] = header needle\n; c needle\nx\n  y: needle\n";
        assert_eq!(
            needles(src, "ini"),
            vec![vec!["top"], vec!["a", "k"], vec!["a", "k"], vec!["a", "y"]]
        );
    }

    #[test]
    fn the_cursor_gives_the_model_trails_at_every_byte() {
        for src in [
            &b"root=top\n[server]\nhost = needle.example\n  continued needle\n  port: 80\n[remote \"origin\"]\nurl=needle.git\n[a.b]\nk=needle\n"[..],
            b"[s]\r\nk=needle\r\n[broken\r\nneedle tail\r\n[s]\r\nk=needle again",
            b"[Service]\nExecStart=needle\\\n needle\n",
            b"",
            b"[",
            b"=\n:\n[]\n[ x ] tail\nk=\n",
        ] {
            cursor_is_model("ini", src);
        }
    }
}
