//! Colours of the printers.
use termcolor::{Color, ColorSpec, WriteColor};

/// ripgrep's default colours (path magenta, line number green, match bold red), plus cyan
/// Trails and dim markers (grill decision 8381).
pub fn spec(kind: &str) -> ColorSpec {
    let mut c = ColorSpec::new();
    match kind {
        "path" => {
            c.set_fg(Some(Color::Magenta));
        }
        "line" => {
            c.set_fg(Some(Color::Green));
        }
        "match" => {
            c.set_fg(Some(Color::Red)).set_bold(true);
        }
        "trail" => {
            c.set_fg(Some(Color::Cyan));
        }
        _ => {
            c.set_dimmed(true);
        }
    }
    c
}

pub fn write_styled(w: &mut dyn WriteColor, kind: &str, s: &str) -> std::io::Result<()> {
    w.set_color(&spec(kind))?;
    w.write_all(s.as_bytes())?;
    w.reset()
}

/// Writes `text`, with each byte range of `subs` in the match colour. Ranges are clamped to
/// `text` and moved to char boundaries.
pub fn write_highlighted(
    w: &mut dyn WriteColor,
    text: &str,
    subs: &[(usize, usize)],
) -> std::io::Result<()> {
    if subs.is_empty() || !w.supports_color() {
        return w.write_all(text.as_bytes());
    }
    let floor = |mut b: usize| {
        b = b.min(text.len());
        while !text.is_char_boundary(b) {
            b -= 1;
        }
        b
    };
    let ceil = |mut b: usize| {
        b = b.min(text.len());
        while !text.is_char_boundary(b) {
            b += 1;
        }
        b
    };
    let mut ranges: Vec<(usize, usize)> = subs
        .iter()
        .map(|&(s, e)| (floor(s), ceil(e)))
        .filter(|&(s, e)| s < e)
        .collect();
    ranges.sort_unstable();
    let mut at = 0;
    for (s, e) in ranges {
        // Overlapping ranges extend the highlight already written.
        let s = s.max(at);
        if s >= e {
            continue;
        }
        w.write_all(&text.as_bytes()[at..s])?;
        write_styled(w, "match", &text[s..e])?;
        at = e;
    }
    w.write_all(&text.as_bytes()[at..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use termcolor::{Ansi, NoColor};

    fn plain(text: &str, subs: &[(usize, usize)]) -> String {
        let mut w = NoColor::new(Vec::new());
        write_highlighted(&mut w, text, subs).unwrap();
        String::from_utf8(w.into_inner()).unwrap()
    }

    fn ansi(text: &str, subs: &[(usize, usize)]) -> String {
        let mut w = Ansi::new(Vec::new());
        write_highlighted(&mut w, text, subs).unwrap();
        String::from_utf8(w.into_inner()).unwrap()
    }

    #[test]
    fn no_colour_writes_the_text_unchanged() {
        assert_eq!(plain("a NEEDLE b", &[(2, 8)]), "a NEEDLE b");
        assert_eq!(plain("a NEEDLE b", &[]), "a NEEDLE b");
    }

    #[test]
    fn matches_are_bold_red_and_reset_after() {
        // The same escapes ripgrep 14.1.1 writes: reset, bold, red, text, reset.
        assert_eq!(
            ansi("a NEEDLE b", &[(2, 8)]),
            "a \x1b[0m\x1b[1m\x1b[31mNEEDLE\x1b[0m b"
        );
    }

    #[test]
    fn odd_ranges_do_not_panic() {
        // Past the end, inside a multi-byte char, reversed, overlapping.
        let text = "añb";
        assert_eq!(plain(text, &[(2, 99)]), text);
        assert_eq!(plain(text, &[(5, 9), (3, 2), (0, 2), (1, 3)]), text);
        let out = ansi(text, &[(2, 3)]);
        assert!(out.contains("\x1b[31mñ\x1b[0m"), "{out:?}");
    }
}
