//! Tree output (spec §6.1): each Match under the Segments of its Trail.
use super::{Piece, RenderOptions, style};
use crate::model::{Described, Segment};
use termcolor::WriteColor;

/// How a Segment is shown in the tree.
pub fn label(s: &Segment) -> String {
    if let Some(sig) = &s.signature {
        return sig.clone();
    }
    let name = &s.name;
    match s.kind.as_str() {
        "heading" => format!(
            "{} {name}",
            "#".repeat(s.level.unwrap_or(1).max(1) as usize)
        ),
        "code" => format!("```{name}"),
        "item" => format!("• {name}"),
        "column" => format!("column «{name}»"),
        "document" => name.clone(),
        kind => format!("{kind} {name}"),
    }
}

fn indent(depth: usize) -> String {
    " ".repeat(2 * (depth + 1))
}

fn shared(a: &[Segment], b: &[Segment]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Prints the Segments of `trail` that differ from the Trail printed before. `header` is the
/// (depth, line) of a Match row printed just before: the Segment at that depth whose label is
/// on that line is already on screen as the row, so it is not printed again.
fn write_trail(
    w: &mut dyn WriteColor,
    printed: &[Segment],
    trail: &[Segment],
    header: Option<(usize, u64)>,
) -> std::io::Result<()> {
    for (depth, s) in trail.iter().enumerate().skip(shared(printed, trail)) {
        if header == Some((depth, s.line)) {
            continue;
        }
        w.write_all(indent(depth).as_bytes())?;
        style::write_styled(w, "trail", &label(s))?;
        w.write_all(b"\n")?;
    }
    Ok(())
}

/// One line at `depth`: indentation, line number with `:` or `-`, text.
fn write_row(
    w: &mut dyn WriteColor,
    depth: usize,
    line_number: u64,
    bytes: &[u8],
    subs: &[(usize, usize)],
    is_match: bool,
    o: &RenderOptions,
) -> std::io::Result<()> {
    let pad = indent(depth);
    let number = o.line_numbers.then(|| line_number.to_string());
    let sep = if is_match { ":" } else { "-" };
    let used = pad.len() + number.as_ref().map_or(0, |n| n.len() + 1);
    let prefix = |w: &mut dyn WriteColor| -> std::io::Result<()> {
        w.write_all(pad.as_bytes())?;
        if let Some(n) = &number {
            style::write_styled(w, "line", n)?;
            w.write_all(sep.as_bytes())?;
        }
        Ok(())
    };
    super::write_line(
        w,
        &prefix,
        bytes,
        subs,
        is_match,
        o.width.saturating_sub(used),
        o,
    )
}

pub fn render(
    d: &Described,
    src: &[u8],
    o: &RenderOptions,
    w: &mut dyn WriteColor,
) -> std::io::Result<()> {
    super::write_path(w, &d.file.path)?;
    w.write_all(b"\n")?;
    let kept = if o.expand {
        super::match_lines(d)
    } else {
        Default::default()
    };
    let mut printed: &[Segment] = &[];
    // The (depth, line) of the last row when it was a Match: it may be a Segment's label.
    let mut header: Option<(usize, u64)> = None;
    for piece in super::pieces(d, o) {
        match piece {
            Piece::Row(r) => {
                let depth = r.trail.len();
                if r.gap_before {
                    // At the depth of what is printed next: the first new Segment, or the line.
                    let at = depth.min(shared(printed, r.trail));
                    w.write_all(format!("{}--\n", indent(at)).as_bytes())?;
                }
                let shown = shared(printed, r.trail);
                write_trail(w, printed, r.trail, header.filter(|&(at, _)| at == shown))?;
                printed = r.trail;
                write_row(w, depth, r.line_number, r.bytes, &r.subs, r.is_match, o)?;
                // Context lines right after that Match, at its depth, keep it as the header.
                let keep =
                    !r.is_match && shown == depth && header.is_some_and(|(at, _)| at == depth);
                header = if r.is_match {
                    Some((depth, r.line_number))
                } else {
                    header.filter(|_| keep)
                };
            }
            Piece::Section { trail, start, end } => {
                write_trail(w, printed, trail, None)?;
                printed = trail;
                header = None;
                for (n, line) in super::section_lines(src, start, end) {
                    let subs = kept.get(&n);
                    let s = subs.map_or(&[][..], |v| v.as_slice());
                    write_row(w, trail.len(), n, line, s, subs.is_some(), o)?;
                }
            }
        }
    }
    if let Some(msg) = super::binary_message(d, o) {
        w.write_all(format!("{}{msg}\n", indent(0)).as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Item;
    use crate::output::fixture::*;
    use crate::output::tests::opts;
    use termcolor::{Ansi, NoColor};

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

    #[test]
    fn nested_trails_share_their_prefix() {
        let s = src(&["# A", "## B", "one needle", "two needle", "three needle"]);
        let (a, b) = (h(1, "A", 1, 5), h(2, "B", 2, 4));
        let d = described(
            &s,
            vec![mat(&s, 3), mat(&s, 4), mat(&s, 5)],
            vec![
                vec![vec![a.clone(), b.clone()]],
                vec![vec![a.clone(), b]],
                vec![vec![a]],
            ],
        );
        insta::assert_snapshot!("nested", plain(&d, &s, &opts()));
    }

    #[test]
    fn context_lines_and_gaps() {
        let s = src(&[
            "# A",
            "l2",
            "l3 needle",
            "l4",
            "l5",
            "l6",
            "l7",
            "l8 needle",
            "l9",
        ]);
        let (a, b) = (h(1, "A", 1, 9), h(2, "B", 6, 9));
        let d = described(
            &s,
            vec![
                ctx(&s, 2),
                mat(&s, 3),
                ctx(&s, 4),
                ctx(&s, 7),
                mat(&s, 8),
                ctx(&s, 9),
            ],
            vec![vec![vec![a.clone()]], vec![vec![a, b]]],
        );
        let o = RenderOptions {
            context: true,
            ..opts()
        };
        insta::assert_snapshot!("context", plain(&d, &s, &o));
    }

    #[test]
    fn long_line_as_two_fragments() {
        let mut long = "x".repeat(3000);
        long.replace_range(1000..1006, "needle");
        long.replace_range(2500..2506, "needle");
        let s = src(&["# A", &long]);
        let d = described(&s, vec![mat(&s, 2)], vec![vec![vec![h(1, "A", 1, 2)]; 2]]);
        let o = RenderOptions {
            width: 60,
            ..opts()
        };
        let out = plain(&d, &s, &o);
        assert_eq!(out.lines().filter(|l| l.starts_with("    2:")).count(), 2);
        for l in out.lines() {
            assert!(l.chars().count() <= 60, "{l}");
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
        let name = seg("column", "Name", 2, 4);
        let note = seg("column", "Note", 2, 4);
        let d = described(
            &s,
            vec![mat(&s, 4)],
            vec![vec![vec![a.clone(), name], vec![a, note]]],
        );
        (s, d)
    }

    #[test]
    fn row_with_two_columns_is_printed_once_per_trail() {
        let (s, d) = table();
        insta::assert_snapshot!("two_columns", plain(&d, &s, &opts()));
        // Each copy highlights only the submatch of its own Trail.
        let out = ansi(&d, &s, &opts());
        let rows: Vec<&str> = out.lines().filter(|l| l.contains("| y ")).collect();
        assert_eq!(rows.len(), 2);
        assert!(rows[0].contains("| \x1b[0m\x1b[1m\x1b[31mneedle\x1b[0m x | y needle |"));
        assert!(rows[1].contains("| needle x | y \x1b[0m\x1b[1m\x1b[31mneedle\x1b[0m |"));
    }

    #[test]
    fn omitted_row_is_printed_once_counting_every_submatch() {
        let (s, d) = table();
        let o = RenderOptions {
            max_columns: Some(10),
            ..opts()
        };
        let out = ansi(&d, &s, &o);
        assert_eq!(out.matches("[Omitted").count(), 1, "{out}");
        assert!(out.contains("[Omitted long line with 2 matches]"), "{out}");
        assert!(out.contains("Name") && !out.contains("Note"), "{out}");
        let out = plain(&d, &s, &o);
        assert_eq!(
            out.matches("[Omitted long matching line]").count(),
            1,
            "{out}"
        );
    }

    #[test]
    fn expand_prints_a_shared_section_once() {
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
            ..opts()
        };
        insta::assert_snapshot!("expand_shared", plain(&d, &s, &o));
    }

    #[test]
    fn expand_prints_both_sections_of_one_line() {
        let s = src(&["# A", "alpha", "## B", "needle and needle"]);
        let (a, b) = (h(1, "A", 1, 4), h(2, "B", 3, 4));
        let mut d = described(
            &s,
            vec![mat(&s, 4)],
            vec![vec![vec![a.clone()], vec![a, b]]],
        );
        d.sections = vec![vec![Some((1, 4)), Some((3, 4))]];
        let o = RenderOptions {
            expand: true,
            ..opts()
        };
        insta::assert_snapshot!("expand_two_sections", plain(&d, &s, &o));
    }

    #[test]
    fn without_line_numbers() {
        let s = src(&["# A", "one needle", "two"]);
        let d = described(
            &s,
            vec![mat(&s, 2), ctx(&s, 3)],
            vec![vec![vec![h(1, "A", 1, 3)]]],
        );
        let o = RenderOptions {
            line_numbers: false,
            ..opts()
        };
        insta::assert_snapshot!("no_line_numbers", plain(&d, &s, &o));
    }

    #[test]
    fn binary_message_after_the_items_before_the_cut() {
        let s = src(&["one needle", "two needle"]);
        let mut d = described(&s, vec![mat(&s, 1), mat(&s, 2)], vec![vec![vec![]]; 2]);
        d.file.stats.binary_offset = Some(1234);
        d.file.stats.binary_cut = Some(1);
        let explicit = plain(&d, &s, &opts());
        d.file.explicit = false;
        let walked = plain(&d, &s, &opts());
        insta::assert_snapshot!("binary", format!("{explicit}{walked}"));
    }

    #[test]
    fn match_on_a_heading_line_heads_the_matches_below() {
        // The Match on `# needle notes` is the label of the heading the next Matches sit under:
        // it is printed once, at the heading's depth, and the heading is not repeated.
        let s = src(&[
            "# needle notes",
            "one needle",
            "## Sub",
            "two needle",
            "# Other",
            "three needle",
        ]);
        let (a, sub, other) = (
            h(1, "needle notes", 1, 4),
            h(2, "Sub", 3, 4),
            h(1, "Other", 5, 6),
        );
        let d = described(
            &s,
            vec![mat(&s, 1), mat(&s, 2), mat(&s, 4), mat(&s, 6)],
            vec![
                vec![vec![]],
                vec![vec![a.clone()]],
                vec![vec![a, sub]],
                vec![vec![other]],
            ],
        );
        assert_eq!(
            plain(&d, &s, &opts()),
            "docs/a.md\n  1:# needle notes\n    2:one needle\n    ## Sub\n      4:two needle\n  # Other\n    6:three needle\n"
        );
    }

    #[test]
    fn tiny_width_with_a_deep_trail_does_not_panic() {
        let long = format!("{}needle{}", "y".repeat(50), "z".repeat(50));
        let s = src(&["---", "# A", "## B", "- item", &long]);
        let trail = vec![
            seg("document", "Doc", 1, 5),
            h(1, "A", 2, 5),
            h(2, "B", 3, 5),
            seg("item", "item", 4, 5),
        ];
        let d = described(&s, vec![mat(&s, 5)], vec![vec![trail]]);
        let o = RenderOptions { width: 5, ..opts() };
        let out = plain(&d, &s, &o);
        assert!(out.contains("• item") && out.contains("needle"), "{out}");
        assert!(matches!(d.file.items[0], Item::Match(_)));
    }
}
