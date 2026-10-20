//! Colours of the printers.
use termcolor::{Color, ColorSpec, WriteColor};

/// The colours of a run (M5a plan, D11): ripgrep's defaults with each `--colors` spec applied
/// in order, as grep-printer's `ColorSpecs` applies them, and s4's: the Trail in cyan (8381),
/// which `--colors 'trail:…'` changes with the same attributes, and its markers dimmed.
#[derive(Debug, Clone)]
pub struct Palette {
    path: ColorSpec,
    line: ColorSpec,
    column: ColorSpec,
    matched: ColorSpec,
    trail: ColorSpec,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::new(&[])
    }
}

impl Palette {
    /// The palette of the `--colors` specs, each one valid (`cli::values::color_spec`).
    pub fn new(specs: &[String]) -> Palette {
        let mut rg = grep_printer::default_color_specs();
        let mut trail: Vec<grep_printer::UserColorSpec> = vec![parse("path:fg:cyan")];
        for s in specs {
            match s.strip_prefix("trail:") {
                Some(rest) => trail.push(parse(&format!("path:{rest}"))),
                None => rg.push(parse(s)),
            }
        }
        let rg = grep_printer::ColorSpecs::new(&rg);
        Palette {
            path: rg.path().clone(),
            line: rg.line().clone(),
            column: rg.column().clone(),
            matched: rg.matched().clone(),
            trail: grep_printer::ColorSpecs::new(&trail).path().clone(),
        }
    }

    /// The colour of `kind`: `path`, `line`, `column`, `match`, `trail`; anything else is a
    /// marker (`│`, `…[+N]`), dimmed.
    pub fn spec(&self, kind: &str) -> ColorSpec {
        match kind {
            "path" => self.path.clone(),
            "line" => self.line.clone(),
            "column" => self.column.clone(),
            "match" => self.matched.clone(),
            "trail" => self.trail.clone(),
            _ => {
                let mut c = ColorSpec::new();
                c.set_dimmed(true);
                c
            }
        }
    }
}

/// A spec `cli::values::color_spec` accepted.
fn parse(s: &str) -> grep_printer::UserColorSpec {
    s.parse()
        .unwrap_or_else(|_| "path:none".parse().expect("valid"))
}

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

/// `write_styled` with the run's palette.
pub fn write_in(w: &mut dyn WriteColor, p: &Palette, kind: &str, s: &[u8]) -> std::io::Result<()> {
    w.set_color(&p.spec(kind))?;
    w.write_all(s)?;
    w.reset()
}

/// `write_highlighted` with the run's palette.
pub fn write_highlighted_in(
    w: &mut dyn WriteColor,
    p: &Palette,
    text: &str,
    subs: &[(usize, usize)],
) -> std::io::Result<()> {
    highlighted(w, &p.spec("match"), text, subs)
}

/// Writes `text`, with each byte range of `subs` in the match colour. Ranges are clamped to
/// `text` and moved to char boundaries.
pub fn write_highlighted(
    w: &mut dyn WriteColor,
    text: &str,
    subs: &[(usize, usize)],
) -> std::io::Result<()> {
    highlighted(w, &spec("match"), text, subs)
}

fn highlighted(
    w: &mut dyn WriteColor,
    color: &ColorSpec,
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
        w.set_color(color)?;
        w.write_all(&text.as_bytes()[s..e])?;
        w.reset()?;
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

    /// M5a plan, D11: `--colors` over rg's defaults, and the `trail` type of s4.
    #[test]
    fn palette_of_colors_specs() {
        let d = Palette::default();
        // rg's default path colour: magenta, and cyan on Windows (grep-printer's defaults).
        let path = if cfg!(windows) {
            Color::Cyan
        } else {
            Color::Magenta
        };
        assert_eq!(d.spec("path").fg(), Some(&path));
        assert_eq!(d.spec("trail").fg(), Some(&Color::Cyan));
        assert!(d.spec("match").bold());
        let p = Palette::new(&[
            "path:none".into(),
            "trail:fg:yellow".into(),
            "column:fg:blue".into(),
        ]);
        assert_eq!(p.spec("path").fg(), None);
        assert_eq!(p.spec("trail").fg(), Some(&Color::Yellow));
        assert_eq!(p.spec("column").fg(), Some(&Color::Blue));
        assert!(p.spec("dim").dimmed());
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
