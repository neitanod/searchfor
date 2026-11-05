//! OpenSSH client and server configuration (M6 spec §7.5): `Host …` and `Match …` open a `key`
//! section named by the line as written until the next one; every other line is a keyword,
//! a `key` Segment over its line, with white space or `=` before its value. Keywords compare
//! without case, as ssh does, and keep their spelling. Keywords before the first section are
//! at the top. `Include` is a keyword like any other: it is never followed.
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
    for (start, end, next) in Lines::new(src) {
        let body = trim(src, start..end);
        if body.is_empty() || src[body.start] == b'#' {
            continue;
        }
        let word = body.start
            + src[body.clone()]
                .iter()
                .position(|&c| matches!(c, b' ' | b'\t' | b'='))
                .unwrap_or(body.len());
        let keyword = &src[body.start..word];
        if keyword.eq_ignore_ascii_case(b"host") || keyword.eq_ignore_ascii_case(b"match") {
            b.close_to(0, start);
            b.open(start, body.clone(), Kind::Key, NameSrc::Verbatim(body));
            section = true;
            continue;
        }
        let key = body.start..word;
        b.close_to(usize::from(section), start);
        b.open(start, key.clone(), Kind::Key, NameSrc::Verbatim(key));
        b.close_to(usize::from(section), next);
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::tests::cursor_is_model;
    use super::super::tests::needles;

    #[test]
    fn sections_keywords_and_comments() {
        let src =
            b"# needle\nUser needle\n\nHOST=a\n  # needle\n  Port=needle\n\n  needle\nmatch all\n";
        assert_eq!(
            needles(src, "ssh"),
            vec![
                vec![],
                vec!["User"],
                vec!["HOST=a"],
                vec!["HOST=a", "Port"],
                vec!["HOST=a"]
            ]
        );
    }

    #[test]
    fn the_cursor_gives_the_model_trails_at_every_byte() {
        for src in [
            &b"User root\nHost github.com gitlab.com\n  IdentityFile ~/.ssh/needle_ed25519\nMatch user ana\n  ProxyCommand=needle --proxy\nInclude /never/read/needle\n"[..],
            b"hOsT example\r\nidentityfile needle\r\nUnrecognized needle\r\nMatch\r\nUser needle",
            b"Host\n\n#\n=\n",
            b"",
        ] {
            cursor_is_model("ssh", src);
        }
    }
}
