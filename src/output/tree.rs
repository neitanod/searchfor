//! Tree output (spec §6.1): each Match under the Segments of its Trail.
use super::{Piece, RenderOptions, style};
use crate::model::{Described, Segment};
use std::collections::VecDeque;
use termcolor::WriteColor;

/// How a Segment is shown in the tree, its control characters escaped (M3 plan, D14).
pub fn label(s: &Segment) -> String {
    if let Some(sig) = &s.signature {
        return crate::text::visible(sig).into_owned();
    }
    let name = crate::text::visible(&s.name);
    match s.kind.as_str() {
        "heading" => format!(
            "{} {name}",
            "#".repeat(s.level.unwrap_or(1).max(1) as usize)
        ),
        "code" => format!("```{name}"),
        "item" => format!("• {name}"),
        "column" => format!("column «{name}»"),
        // M3: markup elements (`tag`) and CSV rows (`row 12`) show their name alone.
        // M4: the image, section and symbol of an object file, and a document's pages,
        // sheets, slides, tables, chapters, cells and notes show their name alone.
        "document" | "key" | "element" | "tag" | "row" | "object" | "section" | "symbol"
        | "page" | "sheet" | "slide" | "table" | "chapter" | "cell" | "notes" => name.into_owned(),
        kind => format!("{kind} {name}"),
    }
}

fn indent(depth: usize) -> String {
    " ".repeat(2 * (depth + 1))
}

/// The indentation of `depth`, written without building it.
fn write_indent(w: &mut dyn WriteColor, depth: usize) -> std::io::Result<()> {
    const SPACES: &[u8] = b"                                                                ";
    let mut n = 2 * (depth + 1);
    while n > 0 {
        let k = n.min(SPACES.len());
        w.write_all(&SPACES[..k])?;
        n -= k;
    }
    Ok(())
}

fn shared(a: &[Segment], b: &[Segment]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// (depth, line) of the last rows printed, newest last; at most `SEEN` of them. A Segment's
/// label is a row printed just before the rows inside it (rows between them lie on its label,
/// so they do not have it in their Trail), so the last few rows are enough.
type Seen = VecDeque<(usize, u64)>;
const SEEN: usize = 64;

/// Prints the Segments of `trail` that differ from the Trail printed before. `seen` holds the
/// line numbers of the rows on screen under the part of the Trail that stays: a Segment whose
/// label is on one of those lines is already shown by that row, so it is not printed again
/// (code review r1, Fable #6).
fn write_trail(
    w: &mut dyn WriteColor,
    printed: &[Segment],
    trail: &[Segment],
    seen: &Seen,
    o: &RenderOptions,
) -> std::io::Result<()> {
    for (depth, s) in trail.iter().enumerate().skip(shared(printed, trail)) {
        if seen.iter().any(|&(_, line)| line == s.line) {
            continue;
        }
        w.write_all(indent(depth).as_bytes())?;
        style::write_in(w, &o.look.palette, "trail", label(s).as_bytes())?;
        w.write_all(b"\n")?;
    }
    Ok(())
}

/// The numbers before a row's text: the line number (an object file's row: its hex offset,
/// spec §5.7), the column (`--column`) and the byte offset (`-b`), each followed by the field
/// separator, `:` for a Match and `-` for a context line (M5a plan, D4).
#[derive(Default)]
pub(crate) struct Numbers<'a> {
    pub line: u64,
    pub offset: Option<u64>,
    pub column: Option<u64>,
    pub byte_offset: Option<u64>,
    /// The file, for a link over the numbers with a `--hyperlink-format` that names `{line}`.
    pub path: Option<&'a std::path::Path>,
}

/// One line at `depth`: indentation, numbers, text; the text one the rows before may have
/// worked out already.
fn write_row_text(
    w: &mut dyn WriteColor,
    depth: usize,
    n: &Numbers,
    line: &super::LineText,
    subs: &[(usize, usize)],
    is_match: bool,
    o: &RenderOptions,
) -> std::io::Result<()> {
    let at = match n.offset {
        Some(off) => format!("0x{off:x}"),
        None => n.line.to_string(),
    };
    let sep: &[u8] = if is_match {
        &o.look.sep_match
    } else {
        &o.look.sep_context
    };
    let mut fields: Vec<(&str, String)> = Vec::new();
    if o.line_numbers {
        fields.push(("line", at));
    }
    if o.look.column
        && let Some(c) = n.column
    {
        fields.push(("column", c.to_string()));
    }
    if o.look.byte_offset
        && let Some(b) = n.byte_offset
    {
        fields.push(("column", b.to_string()));
    }
    let sep_width = String::from_utf8_lossy(sep).chars().count();
    let used = 2 * (depth + 1)
        + fields
            .iter()
            .map(|(_, f)| f.len() + sep_width)
            .sum::<usize>();
    // rg links a heading's row numbers when the format names `{line}` or `{column}`.
    let link_path = n.path.filter(|_| {
        o.look
            .hyperlinks
            .as_ref()
            .is_some_and(|h| h.line_dependent())
    });
    let prefix = |w: &mut dyn WriteColor| -> std::io::Result<()> {
        write_indent(w, depth)?;
        let link = match (link_path, fields.is_empty()) {
            (Some(p), false) => super::link_start(
                w,
                &o.look,
                p,
                Some(n.line).filter(|_| n.offset.is_none()),
                n.column,
            )?,
            _ => false,
        };
        for (kind, text) in &fields {
            style::write_in(w, &o.look.palette, kind, text.as_bytes())?;
            w.write_all(sep)?;
        }
        super::link_end(w, link)
    };
    super::write_line_text(
        w,
        &prefix,
        line,
        subs,
        is_match,
        o.width.saturating_sub(used),
        o,
    )
}

/// The file's first line: its path (`--path-separator`, `--colors`, `--hyperlink-format`),
/// ended by a line break or, with `-0`, by a NUL (grep-printer's `write_path_line`).
pub(crate) fn write_header(
    w: &mut dyn WriteColor,
    path: &std::path::Path,
    o: &RenderOptions,
) -> std::io::Result<()> {
    let link = super::link_start(w, &o.look, path, None, None)?;
    style::write_in(
        w,
        &o.look.palette,
        "path",
        &super::shown_path(path, &o.look),
    )?;
    super::link_end(w, link)?;
    w.write_all(&[o.look.path_terminator.unwrap_or(b'\n')])
}

/// ripgrep's binary message, after a file's rows.
pub(crate) fn write_binary_message(w: &mut dyn WriteColor, msg: &str) -> std::io::Result<()> {
    write_indent(w, 0)?;
    w.write_all(format!("{msg}\n").as_bytes())
}

/// What the tree remembers between rows: the Trail on screen, and the last rows under it.
#[derive(Default)]
pub(crate) struct TreeState {
    printed: Vec<Segment>,
    /// Rows printed under the Trail on screen: a row may be the label of a Segment printed
    /// later. Rows deeper than the part of the Trail that stays are out of view.
    seen: Seen,
    /// The file, for the links of its rows (`header`).
    path: Option<std::path::PathBuf>,
}

impl TreeState {
    /// The file's header (`write_header`), and its path kept for the links of its rows.
    pub(crate) fn header(
        &mut self,
        w: &mut dyn WriteColor,
        path: &std::path::Path,
        o: &RenderOptions,
    ) -> std::io::Result<()> {
        self.path = Some(path.to_path_buf());
        write_header(w, path, o)
    }

    /// One row under its Trail: the group separator first after a gap, the Segments not on
    /// screen yet, the line.
    pub(crate) fn row(
        &mut self,
        w: &mut dyn WriteColor,
        r: &super::Row,
        o: &RenderOptions,
    ) -> std::io::Result<()> {
        let depth = r.trail.len();
        let shown = shared(&self.printed, r.trail);
        if r.gap_before
            && let Some(sep) = &o.look.sep_group
        {
            // At the depth of what is printed next: the first new Segment, or the line.
            let at = depth.min(shown);
            w.write_all(indent(at).as_bytes())?;
            w.write_all(sep)?;
            w.write_all(b"\n")?;
        }
        // Every row on screen is at most as deep as the Trail on screen: nothing to drop while
        // that Trail stays whole.
        if shown < self.printed.len() {
            self.seen.retain(|&(d, _)| d <= shown);
        }
        if shown != depth || self.printed.len() != depth {
            write_trail(w, &self.printed, r.trail, &self.seen, o)?;
            self.printed = r.trail.to_vec();
        }
        let numbers = Numbers {
            line: r.line_number,
            offset: r.offset,
            column: r.column,
            byte_offset: Some(r.byte_offset),
            path: self.path.as_deref(),
        };
        write_row_text(w, depth, &numbers, &r.line, &r.subs, r.is_match, o)?;
        if self.seen.len() == SEEN {
            self.seen.pop_front();
        }
        self.seen.push_back((depth, r.line_number));
        Ok(())
    }

    /// With --expand: a Section's Trail and its lines; `kept` has the rows of its Match lines
    /// (`section_rows`).
    fn section<'s>(
        &mut self,
        w: &mut dyn WriteColor,
        (trail, start, end): (&[Segment], u64, u64),
        sections: &super::SectionLines<'s>,
        kept: &std::collections::HashMap<u64, Vec<super::Row<'s>>>,
        o: &RenderOptions,
    ) -> std::io::Result<()> {
        write_trail(w, &self.printed, trail, &Seen::new(), o)?;
        self.printed = trail.to_vec();
        self.seen.clear();
        for r in sections.rows(start, end, kept, o) {
            let numbers = Numbers {
                line: r.line_number,
                offset: r.offset,
                column: r.column,
                byte_offset: Some(r.byte_offset),
                path: self.path.as_deref(),
            };
            write_row_text(w, trail.len(), &numbers, &r.line, &r.subs, r.is_match, o)?;
        }
        Ok(())
    }
}

pub fn render(
    d: &Described,
    src: &[u8],
    o: &RenderOptions,
    w: &mut dyn WriteColor,
) -> std::io::Result<()> {
    let mut state = TreeState::default();
    state.header(w, &d.file.path, o)?;
    let kept = if o.expand {
        super::section_rows(d, src, o)
    } else {
        Default::default()
    };
    let sections = super::SectionLines::new(src);
    for piece in super::pieces(d, src, o) {
        match piece {
            Piece::Row(r) => state.row(w, &r, o)?,
            Piece::Section { trail, start, end } => {
                state.section(w, (trail, start, end), &sections, &kept, o)?
            }
        }
    }
    if let Some(msg) = super::binary_message_of(d, o) {
        write_binary_message(w, &msg)?;
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

    #[test]
    fn labels_of_the_m3_kinds_and_escaped_names() {
        assert_eq!(label(&seg("tag", "li[3]", 1, 1)), "li[3]");
        assert_eq!(label(&seg("row", "row 2", 1, 1)), "row 2");
        assert_eq!(label(&seg("column", "Plan", 1, 1)), "column «Plan»");
        let mut cue = seg("cue", "cue 1 00:00:01,000", 1, 3);
        cue.signature = Some("cue 1 00:00:01,000 --> 00:00:02,000".into());
        assert_eq!(label(&cue), "cue 1 00:00:01,000 --> 00:00:02,000");
        // M3 plan, D14.
        assert_eq!(label(&seg("key", "a\nb", 1, 1)), "a\\nb");
        let mut sig = seg("method", "m", 1, 1);
        sig.signature = Some("fn m(\u{1b})".into());
        assert_eq!(label(&sig), "fn m(\\u{1b})");
    }

    #[test]
    fn labels_of_the_m4_kinds() {
        for kind in [
            "object", "section", "symbol", "page", "sheet", "slide", "table", "chapter", "cell",
            "notes",
        ] {
            assert_eq!(label(&seg(kind, "N", 1, 1)), "N", "{kind}");
        }
    }

    #[test]
    fn object_rows_in_the_tree() {
        let (src, d) = crate::output::fixture::object_rows();
        assert_eq!(
            plain(&d, &src, &opts()),
            "docs/a.md\n  ELF x86-64\n    .rodata\n      main.version+0x8\n        0xe:version=needle\n      0x1e:another needle\n"
        );
    }

    /// Codex r1 #9: a string with two Matches is one row, under its first Match's Trail.
    #[test]
    fn an_object_row_with_two_matches_is_one_row() {
        let (src, d) = crate::output::fixture::object_row_twice();
        assert_eq!(
            plain(&d, &src, &opts()),
            "docs/a.md\n  ELF x86-64\n    .rodata\n      s\n        0x6:needle needle\n"
        );
    }

    fn plain(d: &Described, src: &[u8], o: &RenderOptions) -> String {
        let mut w = NoColor::new(Vec::new());
        render(d, src, o, &mut w).unwrap();
        String::from_utf8(w.into_inner()).unwrap()
    }

    /// M5a plan, D4: the tree's rows with `--column`, `-b`, rg's separators, `-0` after the
    /// header's path, `--path-separator` and `--context-separator`.
    #[test]
    fn rows_with_ripgreps_prelude_flags() {
        use crate::output::Look;
        let s = src(&["# Install", "  run needle", "after", "x", "y", "z needle"]);
        let d = described(
            &s,
            vec![mat(&s, 2), ctx(&s, 3), mat(&s, 6)],
            vec![
                vec![vec![h(1, "Install", 1, 6)]],
                vec![vec![h(1, "Install", 1, 6)]],
            ],
        );
        let o = |look| RenderOptions {
            context: true,
            look,
            ..opts()
        };
        assert_eq!(
            plain(&d, &s, &o(Look::default())),
            "docs/a.md\n  # Install\n    2:  run needle\n    3-after\n    --\n    6:z needle\n"
        );
        let look = Look {
            column: true,
            byte_offset: true,
            sep_match: b"=".to_vec(),
            sep_group: Some(b"##".to_vec()),
            path_terminator: Some(0),
            path_separator: Some(b'\\'),
            ..Look::default()
        };
        assert_eq!(
            plain(&d, &s, &o(look)),
            "docs\\a.md\x00  # Install\n    2=7=10=  run needle\n    3-23-after\n    ##\n    6=3=33=z needle\n"
        );
    }

    /// The header's path and, with a format that names `{line}`, each row's numbers are links.
    #[test]
    fn links_in_the_tree() {
        use crate::output::{Look, hyperlink::Hyperlinks};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, "x").unwrap();
        let s = src(&["# Install", "needle"]);
        let mut d = described(
            &s,
            vec![mat(&s, 2)],
            vec![vec![vec![h(1, "Install", 1, 2)]]],
        );
        d.file.path = path.clone();
        let o = RenderOptions {
            look: Look {
                hyperlinks: Hyperlinks::new("file://{path}:{line}", None).map(std::sync::Arc::new),
                ..Look::default()
            },
            ..opts()
        };
        let mut w = Ansi::new(Vec::new());
        render(&d, &s, &o, &mut w).unwrap();
        let out = String::from_utf8(w.into_inner()).unwrap();
        let url = format!("file://{}", path.canonicalize().unwrap().display());
        assert!(
            out.starts_with(&format!("\x1b]8;;{url}:1\x1b\\")),
            "{out:?}"
        );
        assert!(out.contains(&format!("\x1b]8;;{url}:2\x1b\\")), "{out:?}");
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
    fn key_and_element_segments_show_their_names() {
        // Spec §5.2: `server › ports[2] › host` reads as the three names, without a kind word.
        let s = src(&["server:", "  ports:", "    - host: needle"]);
        let trail = vec![
            seg("key", "server", 1, 3),
            seg("element", "ports[2]", 3, 3),
            seg("key", "host", 3, 3),
        ];
        let d = described(&s, vec![mat(&s, 3)], vec![vec![trail]]);
        let out = plain(&d, &s, &opts());
        assert!(!out.contains("key "), "{out}");
        assert!(!out.contains("element "), "{out}");
        insta::assert_snapshot!("key_path", out);
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

    /// Code review r1 of M5a, Codex #3: a Section's lines are rows as any other's: a Match
    /// line with its Matches replaced (`-r`), its column and byte offset (`--column`, `-b`),
    /// once with every submatch of its Trails; the other lines with their byte offset and the
    /// context separator; and with `--crlf`, none keeps its `\r`.
    #[test]
    fn expand_prints_its_lines_as_rows() {
        use crate::output::Look;
        let s = src(&["# A", "one needle needle", "plain"]);
        let (a, b) = (h(1, "A", 1, 3), seg("key", "B", 2, 2));
        let mut d = described(
            &s,
            vec![mat(&s, 2)],
            vec![vec![vec![a.clone()], vec![a, b]]],
        );
        d.sections = vec![vec![Some((1, 3)), Some((1, 3))]];
        let o = |look| RenderOptions {
            expand: true,
            look,
            ..opts()
        };
        let look = Look {
            column: true,
            byte_offset: true,
            ..crate::output::tests::replacer("needle", b"X")
        };
        assert_eq!(
            plain(&d, &s, &o(look)),
            "docs/a.md\n  # A\n    1-0-# A\n    2:5:4:one X X\n    3-22-plain\n"
        );
        let s = b"# A\r\none needle\r\nplain\r\n".to_vec();
        let a = h(1, "A", 1, 3);
        let mut d = described(&s, vec![mat(&s, 2)], vec![vec![vec![a]]]);
        d.sections = vec![vec![Some((1, 3))]];
        let look = Look {
            crlf: true,
            ..Look::default()
        };
        assert_eq!(
            plain(&d, &s, &o(look)),
            "docs/a.md\n  # A\n    1-# A\n    2:one needle\n    3-plain\n"
        );
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
