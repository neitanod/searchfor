//! One-line output (spec §6.2): `path:line: Trail │ text`.
use super::{Piece, RenderOptions, Row, style};
use crate::model::{Described, Segment};
use crate::text;
use termcolor::WriteColor;

fn width(s: &str) -> usize {
    s.chars().count()
}

/// The Trail as shown, fitted in `room` characters (spec §5.1): whole when it fits; else with
/// each run of `item` Segments as `…`; else first › … › last; else cut with `…` at the end.
fn fit_trail(trail: &[Segment], room: usize) -> String {
    let fits = |s: &str| width(s) <= room;
    let full = trail
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(" › ");
    if fits(&full) {
        return full;
    }
    let mut parts: Vec<&str> = Vec::new();
    for s in trail {
        if s.kind != "item" {
            parts.push(&s.name);
        } else if parts.last() != Some(&"…") {
            parts.push("…");
        }
    }
    let mut shortest = parts.join(" › ");
    if fits(&shortest) {
        return shortest;
    }
    if trail.len() > 2 {
        let ends = format!("{} › … › {}", trail[0].name, trail[trail.len() - 1].name);
        if width(&ends) < width(&shortest) {
            shortest = ends;
        }
    }
    if fits(&shortest) {
        return shortest;
    }
    let mut cut: String = shortest.chars().take(room.saturating_sub(1)).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

/// `path:line:` (or `path-line-` for context), each part present when asked for.
struct Head {
    path: Option<Vec<u8>>,
    line: Option<String>,
    sep: &'static str,
}

impl Head {
    fn new(d: &Described, line_number: Option<u64>, is_match: bool, o: &RenderOptions) -> Self {
        Head {
            path: o.with_filename.then(|| super::path_bytes(&d.file.path)),
            line: line_number
                .filter(|_| o.line_numbers)
                .map(|n| n.to_string()),
            sep: if is_match { ":" } else { "-" },
        }
    }

    fn is_empty(&self) -> bool {
        self.path.is_none() && self.line.is_none()
    }

    fn width(&self) -> usize {
        self.path
            .as_ref()
            .map_or(0, |p| width(&String::from_utf8_lossy(p)) + 1)
            + self.line.as_ref().map_or(0, |l| l.len() + 1)
    }

    fn write(&self, w: &mut dyn WriteColor) -> std::io::Result<()> {
        if let Some(p) = &self.path {
            w.set_color(&style::spec("path"))?;
            w.write_all(p)?;
            w.reset()?;
            w.write_all(self.sep.as_bytes())?;
        }
        if let Some(l) = &self.line {
            style::write_styled(w, "line", l)?;
            w.write_all(self.sep.as_bytes())?;
        }
        Ok(())
    }
}

fn write_row(
    w: &mut dyn WriteColor,
    d: &Described,
    r: &Row,
    o: &RenderOptions,
) -> std::io::Result<()> {
    if r.gap_before {
        w.write_all(b"--\n")?;
    }
    let head = Head::new(d, Some(r.line_number), r.is_match, o);
    if !r.is_match {
        let prefix = |w: &mut dyn WriteColor| head.write(w);
        let avail = o.width.saturating_sub(head.width());
        return super::write_line(w, &prefix, r.bytes, &r.subs, false, avail, o);
    }
    let text_width = if super::omitted(r.bytes, o) {
        width(&super::omission_message(
            r.subs.len(),
            true,
            w.supports_color(),
        ))
    } else {
        width(&text::lossy_with_map(text::strip_terminator(r.bytes)).0)
    };
    let space = usize::from(!head.is_empty());
    // The text keeps up to half the width for itself; the Trail gets what is left.
    let reserve = text_width.min(o.width.div_ceil(2));
    let room = o.width.saturating_sub(head.width() + space + 3 + reserve);
    let trail = (!r.trail.is_empty()).then(|| fit_trail(r.trail, room));
    let used = head.width() + space + trail.as_ref().map_or(0, |t| width(t) + 3);
    let prefix = |w: &mut dyn WriteColor| -> std::io::Result<()> {
        head.write(w)?;
        if space == 1 {
            w.write_all(b" ")?;
        }
        if let Some(t) = &trail {
            style::write_styled(w, "trail", t)?;
            style::write_styled(w, "dim", " │ ")?;
        }
        Ok(())
    };
    let avail = o.width.saturating_sub(used);
    super::write_line(w, &prefix, r.bytes, &r.subs, true, avail, o)
}

pub fn render(
    d: &Described,
    src: &[u8],
    o: &RenderOptions,
    w: &mut dyn WriteColor,
) -> std::io::Result<()> {
    let kept = if o.expand {
        super::match_lines(d)
    } else {
        Default::default()
    };
    for piece in super::pieces(d, o) {
        match piece {
            Piece::Row(r) => write_row(w, d, &r, o)?,
            Piece::Section { trail, start, end } => {
                let head = Head::new(d, None, true, o);
                head.write(w)?;
                let range = format!("{start}-{end}:");
                style::write_styled(w, "line", &range[..range.len() - 1])?;
                w.write_all(b":")?;
                if !trail.is_empty() {
                    let fixed = head.width() + range.len() + 1;
                    w.write_all(b" ")?;
                    style::write_styled(
                        w,
                        "trail",
                        &fit_trail(trail, o.width.saturating_sub(fixed)),
                    )?;
                }
                w.write_all(b"\n")?;
                for (n, line) in super::section_lines(src, start, end) {
                    let raw = text::strip_terminator(line);
                    let (t, map) = text::lossy_with_map(raw);
                    let subs: Vec<(usize, usize)> = kept
                        .get(&n)
                        .into_iter()
                        .flatten()
                        .map(|&(s, e)| (map[s.min(raw.len())], map[e.min(raw.len())]))
                        .collect();
                    style::write_highlighted(w, &t, &subs)?;
                    w.write_all(b"\n")?;
                }
                w.write_all(b"\n")?;
            }
        }
    }
    if let Some(msg) = super::binary_message(d, o) {
        if o.with_filename {
            super::write_path(w, &d.file.path)?;
            w.write_all(b": ")?;
        }
        w.write_all(format!("{msg}\n").as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Layout;
    use crate::output::fixture::*;
    use crate::output::tests::opts;
    use termcolor::{Ansi, NoColor};

    fn flat() -> RenderOptions {
        RenderOptions {
            layout: Layout::Flat,
            ..opts()
        }
    }

    fn plain(d: &Described, src: &[u8], o: &RenderOptions) -> String {
        let mut w = NoColor::new(Vec::new());
        render(d, src, o, &mut w).unwrap();
        String::from_utf8(w.into_inner()).unwrap()
    }

    fn ansi(d: &Described, src: &[u8], o: &RenderOptions) -> String {
        let mut w = Ansi::new(Vec::new());
        render(d, src, o, &mut w).unwrap();
        String::from_utf8(w.into_inner()).unwrap()
    }

    fn nested() -> (Vec<u8>, Described) {
        let s = src(&["# A", "## B", "one needle", "two"]);
        let trail = vec![h(1, "A", 1, 4), h(2, "B", 2, 4)];
        let d = described(&s, vec![mat(&s, 3), ctx(&s, 4)], vec![vec![trail]]);
        (s, d)
    }

    #[test]
    fn match_with_its_trail() {
        let (s, d) = nested();
        insta::assert_snapshot!("plain", plain(&d, &s, &flat()));
        let out = ansi(&d, &s, &flat());
        assert!(
            out.starts_with("\x1b[0m\x1b[35mdocs/a.md\x1b[0m:\x1b[0m\x1b[32m3\x1b[0m: \x1b[0m\x1b[36mA › B\x1b[0m"),
            "{out:?}"
        );
    }

    #[test]
    fn match_without_trail() {
        let s = src(&["one needle"]);
        let d = described(&s, vec![mat(&s, 1)], vec![vec![vec![]]]);
        assert_eq!(plain(&d, &s, &flat()), "docs/a.md:1: one needle\n");
    }

    #[test]
    fn without_file_names_and_line_numbers() {
        let (s, d) = nested();
        let o = RenderOptions {
            with_filename: false,
            line_numbers: false,
            ..flat()
        };
        insta::assert_snapshot!("no_names_no_numbers", plain(&d, &s, &o));
    }

    #[test]
    fn context_and_gap() {
        let s = src(&["# A", "l2 needle", "l3", "l4", "l5", "l6 needle"]);
        let a = h(1, "A", 1, 6);
        let d = described(
            &s,
            vec![mat(&s, 2), ctx(&s, 3), ctx(&s, 5), mat(&s, 6)],
            vec![vec![vec![a.clone()]], vec![vec![a]]],
        );
        let o = RenderOptions {
            context: true,
            ..flat()
        };
        insta::assert_snapshot!("context", plain(&d, &s, &o));
    }

    #[test]
    fn long_line_as_fragments() {
        let mut long = "x".repeat(3000);
        long.replace_range(1000..1006, "needle");
        long.replace_range(2500..2506, "needle");
        let s = src(&["# A", &long]);
        let d = described(&s, vec![mat(&s, 2)], vec![vec![vec![h(1, "A", 1, 2)]; 2]]);
        let o = RenderOptions {
            width: 80,
            ..flat()
        };
        let out = plain(&d, &s, &o);
        assert_eq!(out.lines().count(), 2);
        for l in out.lines() {
            assert!(l.chars().count() <= 80, "{l}");
        }
        insta::assert_snapshot!("fragments", out);
    }

    fn table() -> (Vec<u8>, Described) {
        let s = src(&[
            "# A",
            "| Name | Note |",
            "|---|---|",
            "| needle x | y needle |",
        ]);
        let a = h(1, "A", 1, 4);
        let d = described(
            &s,
            vec![mat(&s, 4)],
            vec![vec![
                vec![a.clone(), seg("column", "Name", 2, 4)],
                vec![a, seg("column", "Note", 2, 4)],
            ]],
        );
        (s, d)
    }

    #[test]
    fn two_trails_on_one_line() {
        let (s, d) = table();
        insta::assert_snapshot!("two_trails", plain(&d, &s, &flat()));
    }

    #[test]
    fn omitted_line_once_under_its_first_trail() {
        let (s, d) = table();
        let o = RenderOptions {
            max_columns: Some(10),
            ..flat()
        };
        assert_eq!(
            plain(&d, &s, &o),
            "docs/a.md:4: A › Name │ [Omitted long matching line]\n"
        );
        let out = ansi(&d, &s, &o);
        assert_eq!(out.lines().count(), 1, "{out}");
        assert!(out.contains("[Omitted long line with 2 matches]"), "{out}");
    }

    #[test]
    fn long_trail_is_elided_items_first_then_the_middle() {
        let s = src(&["x needle"]);
        let trail = vec![
            h(1, "Alpha", 1, 1),
            h(2, "Beta", 1, 1),
            seg("item", "one long item", 1, 1),
            seg("item", "two long item", 1, 1),
            h(3, "Gamma", 1, 1),
        ];
        let d = described(&s, vec![mat(&s, 1)], vec![vec![trail]]);
        // Prefix 13, full Trail 52, items collapsed 24, first and last 17, ` │ ` 3, text 8.
        let out: String = [80, 60, 45]
            .iter()
            .map(|&width| plain(&d, &s, &RenderOptions { width, ..flat() }))
            .collect();
        insta::assert_snapshot!("elided", out);
    }

    #[test]
    fn long_trail_leaves_half_the_width_to_a_long_text() {
        // The text keeps at least half the width: the Trail gives up its items first, then its
        // middle, and is cut with `…` at the end only when first › … › last still does not fit.
        let words = "lorem ipsum dolor sit amet ".repeat(6);
        let line = format!("{words}ExecStart=/usr/bin/needle --flag {words}");
        let s = src(&[&line]);
        let width = 120;
        let cases = [
            vec![
                h(1, "Alpha", 1, 1),
                h(2, "Beta", 1, 1),
                seg("item", "an item that is quite long here", 1, 1),
                h(3, "Gamma", 1, 1),
            ],
            vec![
                h(1, "Maustasten notes", 1, 1),
                h(2, "Middle section", 1, 1),
                h(3, "Last", 1, 1),
            ],
            vec![
                h(1, "Maustasten · Task 677 (teclados) — memoria", 1, 1),
                h(2, "Setup", 1, 1),
                h(3, "Install the systemd service unit", 1, 1),
            ],
        ];
        let mut out = String::new();
        for trail in cases {
            let mut d = described(&s, vec![mat(&s, 1)], vec![vec![trail]]);
            d.file.path = "notes/maustasten/index.md".into();
            let got = plain(&d, &s, &RenderOptions { width, ..flat() });
            for l in got.lines() {
                assert!(l.chars().count() <= width, "{l}");
                let prefix = l.split(" │ ").next().unwrap().chars().count() + 3;
                assert!(prefix <= width / 2, "{prefix} > {}: {l}", width / 2);
            }
            out.push_str(&got);
        }
        insta::assert_snapshot!("long_trail_half_width", out);
    }

    #[test]
    fn trail_cut_at_the_end_keeps_at_least_the_ellipsis() {
        let s = src(&[&"needle ".repeat(40)]);
        let trail = vec![h(1, "A long first heading", 1, 1), h(2, "Second", 1, 1)];
        let mut d = described(&s, vec![mat(&s, 1)], vec![vec![trail]]);
        d.file.path = "a/path/that/is/long/enough/to/eat/half.md".into();
        let out = plain(
            &d,
            &s,
            &RenderOptions {
                width: 80,
                ..flat()
            },
        );
        let first = out.lines().next().unwrap();
        assert!(
            first.starts_with("a/path/that/is/long/enough/to/eat/half.md:1: … │ "),
            "{first}"
        );
    }

    #[test]
    fn expand_prints_each_section_with_a_header() {
        let s = src(&["# A", "one needle", "plain", "two needle", "# C"]);
        let a = h(1, "A", 1, 4);
        let mut d = described(
            &s,
            vec![mat(&s, 2), mat(&s, 4)],
            vec![vec![vec![a.clone()]], vec![vec![a]]],
        );
        d.sections = vec![vec![Some((1, 4))], vec![Some((1, 4))]];
        let o = RenderOptions {
            expand: true,
            ..flat()
        };
        insta::assert_snapshot!("expand", plain(&d, &s, &o));
    }

    #[test]
    fn binary_message_with_and_without_the_path() {
        let s = src(&["one needle", "two needle"]);
        let mut d = described(&s, vec![mat(&s, 1), mat(&s, 2)], vec![vec![vec![]]; 2]);
        d.file.stats.binary_offset = Some(1234);
        d.file.stats.binary_cut = Some(1);
        let named = plain(&d, &s, &flat());
        let o = RenderOptions {
            with_filename: false,
            ..flat()
        };
        let unnamed = plain(&d, &s, &o);
        insta::assert_snapshot!("binary", format!("{named}{unnamed}"));
    }

    #[test]
    fn inverted_match_uses_its_line_trail() {
        let s = src(&["# A", "plain"]);
        let d = described(&s, vec![inverted(&s, 2)], vec![vec![vec![h(1, "A", 1, 2)]]]);
        assert_eq!(plain(&d, &s, &flat()), "docs/a.md:2: A │ plain\n");
    }

    #[test]
    fn prefix_wider_than_the_width_does_not_panic() {
        let (s, mut d) = nested();
        d.file.path = "a/very/long/path/that/is/wider/than/the/width.md".into();
        let o = RenderOptions { width: 5, ..flat() };
        assert!(plain(&d, &s, &o).contains("needle"));
    }
}
