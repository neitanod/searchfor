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
/// Names print with their control characters escaped (M3 plan, D14): the widths count the
/// escapes, and the cut keeps or drops each escape whole (M3 code review r1, Fable #5).
fn fit_trail(trail: &[Segment], room: usize) -> String {
    let shown = |s: &str| width(&text::visible(s));
    let fits = |s: &str| shown(s) <= room;
    let names: Vec<&str> = trail.iter().map(|s| s.name.as_str()).collect();
    let full = names.join(" › ");
    if fits(&full) {
        return text::visible(&full).into_owned();
    }
    let mut parts: Vec<&str> = Vec::new();
    for (s, n) in trail.iter().zip(&names) {
        if s.kind != "item" {
            parts.push(n);
        } else if parts.last() != Some(&"…") {
            parts.push("…");
        }
    }
    let mut shortest = parts.join(" › ");
    if fits(&shortest) {
        return text::visible(&shortest).into_owned();
    }
    if trail.len() > 2 {
        let ends = format!("{} › … › {}", names[0], names[names.len() - 1]);
        if shown(&ends) < shown(&shortest) {
            shortest = ends;
        }
    }
    if fits(&shortest) {
        return text::visible(&shortest).into_owned();
    }
    let mut cut = String::new();
    let mut used = 0;
    let mut buf = [0u8; 4];
    for c in shortest.chars() {
        let v = text::visible(c.encode_utf8(&mut buf));
        let w = width(&v);
        if used + w > room.saturating_sub(1) {
            break;
        }
        used += w;
        cut.push_str(&v);
    }
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

/// The file-name part of a row, computed once per file: present with file names on.
pub(crate) struct FileHead {
    /// The path as printed (`shown_path`), with file names on.
    path: Option<Vec<u8>>,
    /// The path to link to with `--hyperlink-format`.
    link: std::path::PathBuf,
    /// Width of `path` in characters, plus its separator.
    width: usize,
    /// The last Trail fitted, the room it had and the result: consecutive rows usually share
    /// them.
    fitted: std::cell::RefCell<Option<(Vec<Segment>, usize, String)>>,
}

impl FileHead {
    pub(crate) fn new(path: &std::path::Path, o: &RenderOptions) -> Self {
        let shown = o.with_filename.then(|| super::shown_path(path, &o.look));
        let width = shown
            .as_ref()
            .map_or(0, |p| width(&String::from_utf8_lossy(p)) + 1);
        FileHead {
            path: shown,
            link: path.to_path_buf(),
            width,
            fitted: Default::default(),
        }
    }

    /// `fit_trail`, reusing the last result for the same Trail and room.
    fn fit(&self, trail: &[Segment], room: usize) -> String {
        let mut cache = self.fitted.borrow_mut();
        if let Some((t, r, s)) = cache.as_ref()
            && *r == room
            && t.as_slice() == trail
        {
            return s.clone();
        }
        let s = fit_trail(trail, room);
        *cache = Some((trail.to_vec(), room, s.clone()));
        s
    }
}

fn digits(mut n: u64) -> usize {
    let mut d = 1;
    while n >= 10 {
        n /= 10;
        d += 1;
    }
    d
}

/// The prelude of a row, as ripgrep's `PreludeWriter` writes it (M5a plan, D4): the path, the
/// line number, the column (`--column`) and the byte offset (`-b`), each present when asked
/// for and followed by the field separator, `:` for a Match and `-` for a context line
/// (`--field-match-separator`, `--field-context-separator`); `-0` puts a NUL after the path in
/// place of its separator. An object file's row has `path:0x1f3a:`, its hex offset in place of
/// the line (spec §5.7). With `--hyperlink-format`, the prelude is one link.
struct Head<'a> {
    file: &'a FileHead,
    line: Option<u64>,
    /// `line` is an object row's offset, printed in hex.
    hex: bool,
    column: Option<u64>,
    byte_offset: Option<u64>,
    sep: &'a [u8],
}

impl<'a> Head<'a> {
    fn new(
        file: &'a FileHead,
        line_number: Option<u64>,
        is_match: bool,
        o: &'a RenderOptions,
    ) -> Self {
        Head {
            file,
            line: line_number.filter(|_| o.line_numbers),
            hex: false,
            column: None,
            byte_offset: None,
            sep: if is_match {
                &o.look.sep_match
            } else {
                &o.look.sep_context
            },
        }
    }

    /// The column and byte offset of a row, where `--column` and `-b` ask for them.
    fn of_row(mut self, r: &Row, o: &RenderOptions) -> Self {
        self.column = r.column.filter(|_| o.look.column);
        self.byte_offset = o.look.byte_offset.then_some(r.byte_offset);
        self
    }

    fn at_offset(mut self, offset: Option<u64>) -> Self {
        if let (Some(_), Some(off)) = (self.line, offset) {
            self.line = Some(off);
            self.hex = true;
        }
        self
    }

    fn is_empty(&self) -> bool {
        self.file.path.is_none()
            && self.line.is_none()
            && self.column.is_none()
            && self.byte_offset.is_none()
    }

    fn width(&self) -> usize {
        let sep = width(&String::from_utf8_lossy(self.sep));
        let digits = |l: u64| {
            if self.hex {
                // `0x` and the hex digits.
                2 + (l.checked_ilog2().unwrap_or(0) / 4 + 1) as usize
            } else {
                digits(l)
            }
        };
        let field = |n: Option<u64>, f: &dyn Fn(u64) -> usize| n.map_or(0, |n| f(n) + sep);
        let path = if self.file.path.is_some() {
            self.file.width - 1 + sep
        } else {
            0
        };
        path + field(self.line, &digits)
            + field(self.column, &digits)
            + field(self.byte_offset, &digits)
    }

    fn write(&self, w: &mut dyn WriteColor, o: &RenderOptions) -> std::io::Result<()> {
        let p = &o.look.palette;
        let link = if self.file.path.is_some() {
            super::link_start(
                w,
                &o.look,
                &self.file.link,
                self.line.filter(|_| !self.hex),
                self.column,
            )?
        } else {
            false
        };
        if let Some(path) = &self.file.path {
            style::write_in(w, p, "path", path)?;
            match o.look.path_terminator {
                Some(t) => w.write_all(&[t])?,
                None => w.write_all(self.sep)?,
            }
        }
        if let Some(l) = self.line {
            let n = if self.hex {
                format!("0x{l:x}")
            } else {
                l.to_string()
            };
            style::write_in(w, p, "line", n.as_bytes())?;
            w.write_all(self.sep)?;
        }
        for n in [self.column, self.byte_offset].into_iter().flatten() {
            style::write_in(w, p, "column", n.to_string().as_bytes())?;
            w.write_all(self.sep)?;
        }
        super::link_end(w, link)
    }
}

/// The separator between groups of context (`--context-separator`), none with
/// `--no-context-separator`.
fn write_group_separator(w: &mut dyn WriteColor, o: &RenderOptions) -> std::io::Result<()> {
    if let Some(sep) = &o.look.sep_group {
        w.write_all(sep)?;
        w.write_all(b"\n")?;
    }
    Ok(())
}

/// One row: the group separator first when it opens a group after a gap, then
/// `path:line: Trail │ text`.
pub(crate) fn write_row(
    w: &mut dyn WriteColor,
    file: &FileHead,
    r: &Row,
    o: &RenderOptions,
) -> std::io::Result<()> {
    if r.gap_before {
        write_group_separator(w, o)?;
    }
    let head = Head::new(file, Some(r.line_number), r.is_match, o)
        .at_offset(r.offset)
        .of_row(r, o);
    if !r.is_match {
        let prefix = |w: &mut dyn WriteColor| head.write(w, o);
        let avail = o.width.saturating_sub(head.width());
        return super::write_line_text(w, &prefix, &r.line, &r.subs, false, avail, o);
    }
    let space = usize::from(!head.is_empty());
    let trail = if r.trail.is_empty() {
        None
    } else {
        let text_width = if super::omitted(r.line.bytes(), o) {
            width(&super::omission_message(
                r.subs.len(),
                true,
                w.supports_color(),
            ))
        } else {
            r.line.width()
        };
        // The text keeps up to half the width for itself; the Trail gets what is left.
        let reserve = text_width.min(o.width.div_ceil(2));
        let room = o.width.saturating_sub(head.width() + space + 3 + reserve);
        Some(file.fit(r.trail, room))
    };
    let used = head.width() + space + trail.as_ref().map_or(0, |t| width(t) + 3);
    let p = &o.look.palette;
    let prefix = |w: &mut dyn WriteColor| -> std::io::Result<()> {
        head.write(w, o)?;
        if space == 1 {
            w.write_all(b" ")?;
        }
        if let Some(t) = &trail {
            style::write_in(w, p, "trail", t.as_bytes())?;
            style::write_in(w, p, "dim", " │ ".as_bytes())?;
        }
        Ok(())
    };
    let avail = o.width.saturating_sub(used);
    super::write_line_text(w, &prefix, &r.line, &r.subs, true, avail, o)
}

/// ripgrep's binary message line, after a file's rows.
pub(crate) fn write_binary_message(
    w: &mut dyn WriteColor,
    path: &std::path::Path,
    msg: &str,
    o: &RenderOptions,
) -> std::io::Result<()> {
    if o.with_filename {
        let link = super::link_start(w, &o.look, path, None, None)?;
        style::write_in(
            w,
            &o.look.palette,
            "path",
            &super::shown_path(path, &o.look),
        )?;
        super::link_end(w, link)?;
        w.write_all(b": ")?;
    }
    w.write_all(format!("{msg}\n").as_bytes())
}

/// With --expand, a Section: its header, `path:start-end: Trail`, and its lines (`sections`),
/// the rows of its Match lines from `kept` (`section_rows`), then an empty line.
pub(crate) fn write_section<'s>(
    w: &mut dyn WriteColor,
    file: &FileHead,
    (trail, start, end): (&[Segment], u64, u64),
    sections: &super::SectionLines<'s>,
    kept: &[super::SectionBlock<'s>],
    o: &RenderOptions,
) -> std::io::Result<()> {
    // The header: `path:start-end: Trail`; with --vimgrep `path:line:column:` at
    // the Section's first line, as its rows (spec §8; code review r2 of M5a, Codex
    // #2).
    let p = &o.look.palette;
    let (head, range) = if o.look.per_match {
        let head = Head {
            line: Some(start),
            column: Some(1),
            ..Head::new(file, None, true, o)
        };
        (head, String::new())
    } else {
        (Head::new(file, None, true, o), format!("{start}-{end}"))
    };
    head.write(w, o)?;
    if !range.is_empty() {
        style::write_in(w, p, "line", range.as_bytes())?;
        w.write_all(&o.look.sep_match)?;
    }
    if !trail.is_empty() {
        let sep = if range.is_empty() {
            0
        } else {
            o.look.sep_match.len()
        };
        let fixed = head.width() + range.len() + sep + 1;
        w.write_all(b" ")?;
        let fitted = fit_trail(trail, o.width.saturating_sub(fixed));
        style::write_in(w, p, "trail", fitted.as_bytes())?;
    }
    w.write_all(b"\n")?;
    // Each line verbatim (M1 plan), or where --column, -b or --vimgrep ask for a
    // row's coordinates a row of this layout, its prefix as any row's, without the
    // Trail its header has: whole, as Fragments, or -M's message (spec §6.3).
    let prefixed = o.look.column || o.look.byte_offset || o.look.per_match;
    let no_prefix = |_: &mut dyn WriteColor| Ok(());
    for mut r in sections.rows(start, end, kept, o) {
        if prefixed {
            r.trail = &[];
            write_row(w, file, &r, o)?;
        } else {
            super::write_line_text(w, &no_prefix, &r.line, &r.subs, r.is_match, o.width, o)?;
        }
    }
    w.write_all(b"\n")
}

/// Writes a Described file; `lead`, ripgrep's separator between files when it searches with one
/// thread, goes before the first row, so a file whose output is only the binary message has
/// none (code review r5, Fable #2).
pub fn render(
    d: &Described,
    src: &[u8],
    o: &RenderOptions,
    lead: Option<&[u8]>,
    w: &mut dyn WriteColor,
) -> std::io::Result<()> {
    let kept = if o.expand {
        super::section_rows(d, src, o)
    } else {
        Default::default()
    };
    let file = FileHead::new(&d.file.path, o);
    let sections = super::SectionLines::new(src);
    let mut lead = lead;
    for piece in super::pieces(d, src, o) {
        if let Some(l) = lead.take() {
            w.write_all(l)?;
            w.write_all(b"\n")?;
        }
        match piece {
            Piece::Row(r) => write_row(w, &file, &r, o)?,
            Piece::Section { trail, start, end } => {
                write_section(w, &file, (trail, start, end), &sections, &kept, o)?
            }
        }
    }
    if let Some(msg) = super::binary_message_of(d, o) {
        write_binary_message(w, &d.file.path, &msg, o)?;
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

    #[test]
    fn control_characters_in_names_print_escaped() {
        // M3 plan, D14: a decoded line break or ESC stays on the row and drives nothing.
        let t = [seg("key", "a\nb", 1, 1), seg("key", "\u{1b}[31m", 1, 1)];
        assert_eq!(fit_trail(&t, 80), "a\\nb › \\u{1b}[31m");
        // The width counts the escapes: 4 + 3 + 10 characters.
        assert_eq!(fit_trail(&t, 17), "a\\nb › \\u{1b}[31m");
        assert_eq!(fit_trail(&t, 16), "a\\nb › \\u{1b}[3…");
        // The cut never splits an escape (M3 code review r1, Fable #5).
        assert_eq!(fit_trail(&t, 10), "a\\nb ›…");
    }

    fn plain(d: &Described, src: &[u8], o: &RenderOptions) -> String {
        let mut w = NoColor::new(Vec::new());
        render(d, src, o, None, &mut w).unwrap();
        String::from_utf8(w.into_inner()).unwrap()
    }

    /// The file of the M5 tests: a heading, a Match under it, a context line, a gap, a Match.
    fn m5_file() -> (Vec<u8>, Described) {
        let s = src(&["# Install", "  run needle", "after", "x", "y", "z needle"]);
        let d = described(
            &s,
            vec![mat(&s, 2), ctx(&s, 3), mat(&s, 6)],
            vec![
                vec![vec![h(1, "Install", 1, 6)]],
                vec![vec![h(1, "Install", 1, 6)]],
            ],
        );
        (s, d)
    }

    fn with(look: super::super::Look) -> RenderOptions {
        RenderOptions {
            context: true,
            look,
            ..flat()
        }
    }

    /// M5a plan, D4: `--column` and `-b` after the line number, rg's separators, `-0` after the
    /// path, `--path-separator`, `--context-separator`.
    #[test]
    fn rows_with_ripgreps_prelude_flags() {
        use super::super::Look;
        let (s, d) = m5_file();
        assert_eq!(
            plain(&d, &s, &with(Look::default())),
            "docs/a.md:2: Install │   run needle\ndocs/a.md-3-after\n--\ndocs/a.md:6: Install │ z needle\n"
        );
        let look = Look {
            column: true,
            byte_offset: true,
            ..Look::default()
        };
        assert_eq!(
            plain(&d, &s, &with(look)),
            "docs/a.md:2:7:10: Install │   run needle\ndocs/a.md-3-23-after\n--\ndocs/a.md:6:3:33: Install │ z needle\n"
        );
        let look = Look {
            sep_match: b"=".to_vec(),
            sep_context: b"~".to_vec(),
            sep_group: Some(b"##".to_vec()),
            path_terminator: Some(0),
            path_separator: Some(b'\\'),
            trim: true,
            ..Look::default()
        };
        assert_eq!(
            plain(&d, &s, &with(look)),
            "docs\\a.md\x002= Install │ run needle\ndocs\\a.md\x003~after\n##\ndocs\\a.md\x006= Install │ z needle\n"
        );
        let look = Look {
            sep_group: None,
            ..Look::default()
        };
        assert!(!plain(&d, &s, &with(look)).contains("--"));
    }

    /// `-o` and `--vimgrep` rows in the one-line layout, each Match with its Trail.
    #[test]
    fn only_matching_and_vimgrep_rows() {
        use super::super::Look;
        let (s, d) = m5_file();
        let look = Look {
            only_matching: true,
            column: true,
            ..Look::default()
        };
        assert_eq!(
            plain(&d, &s, &with(look)),
            "docs/a.md:2:7: Install │ needle\ndocs/a.md-3-after\n--\ndocs/a.md:6:3: Install │ needle\n"
        );
    }

    /// `--colors` and `--hyperlink-format` in the prelude: the path's and the numbers' colours,
    /// the Trail's, and one link over path, line and column.
    #[test]
    fn colours_and_links() {
        use super::super::{Look, hyperlink::Hyperlinks, style::Palette};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a #1.md");
        std::fs::write(&path, "x").unwrap();
        let (s, mut d) = m5_file();
        d.file.path = path.clone();
        d.file.items.truncate(1);
        let look = Look {
            column: true,
            palette: Palette::new(&["column:fg:blue".into(), "trail:fg:yellow".into()]),
            hyperlinks: Hyperlinks::new("file://{path}:{line}:{column}", None)
                .map(std::sync::Arc::new),
            ..Look::default()
        };
        let mut w = Ansi::new(Vec::new());
        render(&d, &s, &with(look), None, &mut w).unwrap();
        let out = String::from_utf8(w.into_inner()).unwrap();
        let link = format!(
            "\x1b]8;;file://{}:2:7\x1b\\",
            super::super::hyperlink::rg_url_path(&path)
        );
        assert!(out.starts_with(&link), "{out:?}");
        assert!(
            out.contains("\x1b[34m7\x1b[0m:\x1b]8;;\x1b\\ \x1b[0m\x1b[33mInstall"),
            "{out:?}"
        );
    }

    /// M4 plan, D6 and D8: an object file's row is at the hex offset of its first Match, and
    /// rows are strings, not lines: no `--` between them, even with context on.
    #[test]
    fn object_rows_print_the_hex_offset_of_their_match() {
        let (src, d) = object_rows();
        let o = RenderOptions {
            context: true,
            ..flat()
        };
        assert_eq!(
            plain(&d, &src, &o),
            "docs/a.md:0xe: ELF x86-64 › .rodata › main.version+0x8 │ version=needle\n\
             docs/a.md:0x1e: ELF x86-64 › .rodata │ another needle\n"
        );
        let o = RenderOptions {
            with_filename: false,
            ..flat()
        };
        assert!(plain(&d, &src, &o).starts_with("0xe: ELF"));
        // -N hides the offset as it hides line numbers.
        let o = RenderOptions {
            line_numbers: false,
            with_filename: false,
            ..flat()
        };
        assert!(plain(&d, &src, &o).starts_with("ELF x86-64"));
    }

    /// Codex r1 #9: a string with two Matches is one row, at its first Match and with its
    /// Trail, both Matches highlighted.
    #[test]
    fn an_object_row_with_two_matches_is_one_row() {
        let (src, d) = object_row_twice();
        assert_eq!(
            plain(&d, &src, &flat()),
            "docs/a.md:0x6: ELF x86-64 › .rodata › s │ needle needle\n"
        );
        let colored = ansi(&d, &src, &flat());
        assert_eq!(colored.lines().count(), 1, "{colored}");
        assert_eq!(colored.matches("needle\x1b[0m").count(), 2, "{colored}");
    }

    fn ansi(d: &Described, src: &[u8], o: &RenderOptions) -> String {
        let mut w = Ansi::new(Vec::new());
        render(d, src, o, None, &mut w).unwrap();
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
        // rg's default path colour is magenta, and cyan on Windows.
        let path = if cfg!(windows) { "36" } else { "35" };
        assert!(
            out.starts_with(&format!(
                "\x1b[0m\x1b[{path}mdocs/a.md\x1b[0m:\x1b[0m\x1b[32m3\x1b[0m: \x1b[0m\x1b[36mA › B\x1b[0m"
            )),
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

    /// Code review r1 of M5a, Codex #3: a Section's Match lines are replaced (`-r`), and with
    /// `--crlf` its lines leave their `\r` out.
    #[test]
    fn expand_prints_its_lines_as_rows() {
        let s = b"# A\r\none needle\r\nplain\r\n".to_vec();
        let a = h(1, "A", 1, 3);
        let mut d = described(&s, vec![mat(&s, 2)], vec![vec![vec![a]]]);
        d.sections = vec![vec![Some((1, 3))]];
        let o = RenderOptions {
            expand: true,
            look: super::super::Look {
                crlf: true,
                ..crate::output::tests::replacer("needle", b"X")
            },
            ..flat()
        };
        assert_eq!(plain(&d, &s, &o), "docs/a.md:1-3: A\n# A\none X\nplain\n\n");
    }

    /// Code review r2 of M5a, Codex #2: a Section's lines are verbatim (M1 plan), but where
    /// `--column`, `-b` or `--vimgrep` ask for a row's coordinates each line takes the prefix
    /// of the one-line layout's rows: the path and line number, `--column` and `-b` after it,
    /// and with `--vimgrep` each Match's `path:line:column:`, the header's too at the Section's
    /// first line (spec §8).
    #[test]
    fn expand_rows_take_the_rows_prefixes() {
        let s = src(&["# A", "one needle", "plain"]);
        let mut d = described(&s, vec![mat(&s, 2)], vec![vec![vec![h(1, "A", 1, 3)]]]);
        d.sections = vec![vec![Some((1, 3))]];
        let o = |look| RenderOptions {
            expand: true,
            look,
            ..flat()
        };
        let look = super::super::Look {
            column: true,
            byte_offset: true,
            ..Default::default()
        };
        assert_eq!(
            plain(&d, &s, &o(look)),
            "docs/a.md:1-3: A\ndocs/a.md-1-0-# A\ndocs/a.md:2:5:4: one needle\n\
             docs/a.md-3-15-plain\n\n"
        );
        let look = super::super::Look {
            column: true,
            per_match: true,
            ..Default::default()
        };
        assert_eq!(
            plain(&d, &s, &o(look)),
            "docs/a.md:1:1: A\ndocs/a.md-1-# A\ndocs/a.md:2:5: one needle\n\
             docs/a.md-3-plain\n\n"
        );
    }

    /// Code review r2 of M5a, Fable #2: a file `--passthru` prints without a Match has no
    /// Section, so --expand prints its rows as without --expand.
    #[test]
    fn expand_without_sections_prints_the_rows() {
        let s = src(&["# H", "nothing"]);
        let d = described(&s, vec![ctx(&s, 1), ctx(&s, 2)], vec![]);
        let o = |expand| RenderOptions { expand, ..flat() };
        assert_eq!(
            plain(&d, &s, &o(true)),
            "docs/a.md-1-# H\ndocs/a.md-2-nothing\n"
        );
        assert_eq!(plain(&d, &s, &o(true)), plain(&d, &s, &o(false)));
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
