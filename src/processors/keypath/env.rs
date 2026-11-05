//! `.env` files (M6 spec §7.4): `KEY=value` and `export KEY=value`, each a `key` Segment over
//! its value. A value in quotes may span lines, and its Section covers them; `#` starts a
//! comment outside quotes. Recovery: a quote that never closes owns the rest of the file, as a
//! permissive continuation (§7.4 gives no terminal error rule).
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

fn blank(c: u8) -> bool {
    matches!(c, b' ' | b'\t')
}

/// Where the quoted value that starts at `src[open]` closes: one past its closing quote, or
/// None when it never closes. In `"…"` a `\` escapes the next byte.
fn closing(src: &[u8], open: usize) -> Option<usize> {
    let q = src[open];
    let mut i = open + 1;
    while i < src.len() {
        match src[i] {
            b'\\' if q == b'"' => i += 2,
            c if c == q => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn scan(src: &[u8], b: &mut Builder) {
    let mut at = 0;
    while let Some((start, end, next)) = Lines::starting(src, at).next() {
        at = next;
        let body = trim(src, start..end);
        if body.is_empty() || src[body.start] == b'#' {
            continue;
        }
        let mut k = body.start;
        if src[k..body.end].starts_with(b"export") && src.get(k + 6).is_some_and(|&c| blank(c)) {
            k += 6;
            while k < body.end && blank(src[k]) {
                k += 1;
            }
        }
        let mut i = k;
        while i < body.end && src[i] != b'=' && !blank(src[i]) {
            i += 1;
        }
        let key = k..i;
        while i < body.end && blank(src[i]) {
            i += 1;
        }
        if key.is_empty() || src.get(i) != Some(&b'=') || i >= body.end {
            continue;
        }
        let mut v = i + 1;
        while v < end && blank(src[v]) {
            v += 1;
        }
        let close = if v < end && matches!(src[v], b'"' | b'\'') {
            match closing(src, v) {
                // The value's last line, comment included, is the key's.
                Some(c) => memchr::memchr(b'\n', &src[c - 1..]).map_or(src.len(), |n| c + n),
                None => src.len(),
            }
        } else {
            next
        };
        b.open(start, key.clone(), Kind::Key, NameSrc::Verbatim(key));
        b.close_to(0, close);
        at = close;
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::tests::cursor_is_model;
    use super::super::tests::needles;

    #[test]
    fn keys_quotes_and_comments() {
        let src = b"# needle\nexport A=needle # c\nB='x\nneedle'\nC=\"a\\\"needle\n\" # needle\nnot a key needle\nD = needle\n";
        assert_eq!(
            needles(src, "env"),
            vec![
                vec![],
                vec!["A"],
                vec!["B"],
                vec!["C"],
                vec!["C"],
                vec![],
                vec!["D"]
            ]
        );
    }

    #[test]
    fn the_cursor_gives_the_model_trails_at_every_byte() {
        for src in [
            &b"# config\nexport DATABASE_URL=postgres://needle@db/app\nTEXT=\"first\nneedle # literal\nlast\" # end\n"[..],
            b"A=needle\r\nB=\"needle\r\nneedle tail\r\n",
            b"A='x'",
            b"export\nexport =\n=x\n",
            b"",
        ] {
            cursor_is_model("env", src);
        }
    }
}
