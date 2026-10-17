//! Markdown Processor: headings, frontmatter title, fenced code, table columns, list items
//! (spec §5.1, grill #11, design note 8).
use super::OutlineProcessor;
use crate::trail::{LineIndex, Outline, OutlineSegment};
use pulldown_cmark::{CodeBlockKind, Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub struct Markdown;

/// The first line of `r`, its newline included.
fn first_line(text: &str, r: &Range<usize>) -> Range<usize> {
    let end = text[r.clone()]
        .find('\n')
        .map(|i| r.start + i + 1)
        .unwrap_or(r.end);
    r.start..end
}

/// `s` trimmed and cut at `n` characters, with `…` when cut.
fn cut(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n).collect();
    out.push('…');
    out
}

/// `content` with every byte of every invalid UTF-8 sequence replaced by `?`: one byte for one
/// byte, so offsets into the result are offsets into `content`. Valid UTF-8 is not copied.
fn replace_invalid(content: &[u8]) -> std::borrow::Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(content) {
        return text.into();
    }
    let mut out = String::with_capacity(content.len());
    for chunk in content.utf8_chunks() {
        out.push_str(chunk.valid());
        out.extend(std::iter::repeat_n('?', chunk.invalid().len()));
    }
    out.into()
}

struct OpenHeading {
    seg: usize,
    text: String,
    pieces: Vec<(Range<usize>, Range<usize>)>,
}

struct OpenItem {
    seg: usize,
    text: String,
    /// The item's first line is over: later text does not join its name.
    done: bool,
}

struct OpenTable {
    seg: usize,
    header: Vec<String>,
    in_head: bool,
    col: usize,
    cell_text: String,
    /// (column, cell range) of every cell, header included.
    cells: Vec<(usize, Range<usize>)>,
}

impl OutlineProcessor for Markdown {
    fn outline(&self, content: &[u8], params: &toml::Table) -> anyhow::Result<Outline> {
        let text = &replace_invalid(content);
        let content = text.as_bytes();
        let list_items = params
            .get("list_items")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let item_chars = params
            .get("item_name_chars")
            .and_then(|v| v.as_integer())
            .unwrap_or(40) as usize;
        let opts = Options::ENABLE_TABLES
            | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
            | Options::ENABLE_HEADING_ATTRIBUTES;
        let lines = LineIndex::new(content);

        let mut segs: Vec<OutlineSegment> = Vec::new();
        let mut headings: Vec<(usize, u8)> = Vec::new(); // (seg index, level)
        let mut containers: Vec<Range<usize>> = Vec::new(); // items and blockquotes
        let mut heading: Option<OpenHeading> = None;
        let mut items: Vec<OpenItem> = Vec::new();
        let mut table: Option<OpenTable> = None;
        let mut meta: Option<usize> = None;

        for (ev, r) in Parser::new_ext(text, opts).into_offset_iter() {
            // A heading or paragraph that starts inside an item's first line (a tight item has
            // no paragraph) is joined to its name by a space (code review r3, Fable #6). A code
            // block, table, quote or HTML block after the item's text ends the name instead: it
            // is a Segment of its own (code review r4, Fable #3); an item that starts with one
            // takes its first line as its name.
            if let Some(it) = items.last_mut()
                && !it.done
                && !it.text.is_empty()
            {
                match ev {
                    Event::Start(
                        Tag::BlockQuote(_) | Tag::CodeBlock(_) | Tag::Table(_) | Tag::HtmlBlock,
                    ) => it.done = true,
                    Event::Start(Tag::Heading { .. } | Tag::Paragraph)
                        if !it.text.ends_with(' ') =>
                    {
                        it.text.push(' ')
                    }
                    _ => {}
                }
            }
            match ev {
                Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle)) => {
                    // The label is the whole block, the line break after the closing `---`
                    // included, as a heading's (code review r1, Fable #7).
                    let end = if text[r.end..].starts_with('\n') {
                        r.end + 1
                    } else {
                        r.end
                    };
                    segs.push(OutlineSegment::new(
                        "document",
                        "",
                        0..text.len(),
                        r.start..end,
                    ));
                    meta = Some(segs.len() - 1);
                }
                Event::End(TagEnd::MetadataBlock(_)) => meta = None,
                Event::Text(_) if meta.is_some() => {
                    // The YAML `title`, its pieces pointing at the value's bytes.
                    if let (Some(i), Some((name, pieces))) =
                        (meta, super::frontmatter::title(&text[r.clone()], r.start))
                    {
                        segs[i].segment.name = name;
                        segs[i].name = Some(pieces);
                    }
                }
                Event::Start(Tag::BlockQuote(_)) => containers.push(r.clone()),
                Event::Start(Tag::Heading { level, .. }) => {
                    let mut s = OutlineSegment::new("heading", "", r.clone(), r.clone());
                    s.segment.level = Some(level as u8);
                    segs.push(s);
                    headings.push((segs.len() - 1, level as u8));
                    heading = Some(OpenHeading {
                        seg: segs.len() - 1,
                        text: String::new(),
                        pieces: Vec::new(),
                    });
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(h) = heading.take() {
                        // `text` has no leading or trailing blanks: pulldown-cmark trims headings.
                        // An empty heading (`###`) has no pieces; its anchor is the end of the
                        // `#` run (ATX) or the start of the heading (setext).
                        let r0 = segs[h.seg].range.start;
                        let hashes = text[r0..].bytes().take_while(|&b| b == b'#').count();
                        segs[h.seg].name_anchor = r0 + hashes;
                        segs[h.seg].segment.name = h.text;
                        segs[h.seg].name = Some(h.pieces);
                    }
                    // A heading is a whole line: it ends an item's first line too.
                    if let Some(it) = items.last_mut() {
                        it.done = true;
                    }
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                    let name = info
                        .split_whitespace()
                        .next()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("code")
                        .to_string();
                    segs.push(OutlineSegment::new(
                        "code",
                        name,
                        r.clone(),
                        first_line(text, &r),
                    ));
                }
                Event::Start(Tag::Table(_)) => {
                    let mut s = OutlineSegment::new("table", "table", r.clone(), r.start..r.start);
                    s.hidden = true;
                    segs.push(s);
                    table = Some(OpenTable {
                        seg: segs.len() - 1,
                        header: vec![],
                        in_head: false,
                        col: 0,
                        cell_text: String::new(),
                        cells: vec![],
                    });
                }
                Event::Start(Tag::TableHead) => {
                    if let Some(t) = table.as_mut() {
                        t.in_head = true;
                        t.col = 0;
                    }
                }
                Event::End(TagEnd::TableHead) => {
                    if let Some(t) = table.as_mut() {
                        t.in_head = false;
                    }
                }
                Event::Start(Tag::TableRow) => {
                    if let Some(t) = table.as_mut() {
                        t.col = 0;
                    }
                }
                Event::Start(Tag::TableCell) => {
                    if let Some(t) = table.as_mut() {
                        t.cells.push((t.col, r.clone()));
                        t.cell_text.clear();
                    }
                }
                Event::End(TagEnd::TableCell) => {
                    if let Some(t) = table.as_mut() {
                        if t.in_head {
                            t.header
                                .push(std::mem::take(&mut t.cell_text).trim().to_string());
                        }
                        t.col += 1;
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(t) = table.take() {
                        let tr = segs[t.seg].range.clone();
                        let (l0, l1) = (
                            lines.line_of(tr.start),
                            lines.line_of(tr.end.saturating_sub(1)),
                        );
                        for (col, cr) in t.cells {
                            let name = t
                                .header
                                .get(col)
                                .cloned()
                                .unwrap_or_else(|| format!("column {}", col + 1));
                            let mut s = OutlineSegment::new("column", name, cr, 0..0);
                            s.segment.line = l0;
                            s.segment.end_line = l1;
                            segs.push(s);
                        }
                    }
                }
                Event::Start(Tag::Item) => {
                    containers.push(r.clone());
                    if list_items {
                        segs.push(OutlineSegment::new(
                            "item",
                            "",
                            r.clone(),
                            first_line(text, &r),
                        ));
                        items.push(OpenItem {
                            seg: segs.len() - 1,
                            text: String::new(),
                            done: false,
                        });
                    }
                }
                Event::End(TagEnd::Item) if list_items => {
                    if let Some(it) = items.pop() {
                        segs[it.seg].segment.name = cut(&it.text, item_chars);
                    }
                }
                Event::Text(ref t) | Event::Code(ref t) => {
                    if let Some(h) = heading.as_mut() {
                        // Text: the piece always covers the event's whole range. The range is
                        // verbatim when the Source slice equals the text, and decoded otherwise
                        // (an entity like `&amp;` gives `&` over 2..7); the lengths tell
                        // `source_ranges` which case it is. Code: the range includes the
                        // backticks, so the piece points at the code's bytes inside them (found in
                        // the slice), or the whole range when they are not found verbatim.
                        // Known, accepted: five two-code-point entities (&acE; &caps; &cups;
                        // &nvap; &nvge;) decode to as many bytes as they take, so look verbatim.
                        let src = if !matches!(ev, Event::Code(_)) {
                            r.clone()
                        } else {
                            match text[r.clone()].find(&**t) {
                                Some(off) => r.start + off..r.start + off + t.len(),
                                None => r.clone(),
                            }
                        };
                        let at = h.text.len();
                        h.text.push_str(t);
                        h.pieces.push((at..h.text.len(), src));
                    }
                    if let Some(tb) = table.as_mut() {
                        tb.cell_text.push_str(t);
                    }
                    if let Some(it) = items.last_mut()
                        && !it.done
                    {
                        // A code block's text spans lines: the name keeps its first one.
                        match t.find('\n') {
                            Some(k) => {
                                it.text.push_str(&t[..k]);
                                it.done = true;
                            }
                            None => it.text.push_str(t),
                        }
                    }
                }
                Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => {
                    // A soft break inside a (setext) heading becomes one space in its name, a
                    // piece over the line break it stands for.
                    if let (Some(h), Event::SoftBreak) = (heading.as_mut(), &ev) {
                        let at = h.text.len();
                        h.text.push(' ');
                        h.pieces.push((at..h.text.len(), r.clone()));
                    }
                    if let Some(it) = items.last_mut() {
                        it.done = true;
                    }
                }
                _ => {}
            }
        }

        close_sections(&mut segs, &headings, &containers, text.len());
        segs.retain(|s| !(s.segment.kind == "document" && s.segment.name.is_empty()));
        Ok(Outline::build(segs, content))
    }
}

/// Closes heading Sections inside their container (spec §5.1): at the next heading of the same
/// or a higher level in the same container (the innermost item or blockquote around its start),
/// else at the container's end, or the file's. One pass (code review r2, Fable #1): `containers`
/// come in document order and nest, so the containers around a position are a stack; each
/// container keeps the stack of its open headings, by increasing level.
fn close_sections(
    segs: &mut [OutlineSegment],
    headings: &[(usize, u8)],
    containers: &[Range<usize>],
    file_end: usize,
) {
    /// A container around the current position, and its open headings.
    struct Open<'c> {
        end: usize,
        container: Option<&'c Range<usize>>,
        headings: Vec<(usize, u8)>,
    }
    fn close(segs: &mut [OutlineSegment], o: Open) {
        for (j, _) in o.headings {
            segs[j].range.end = o.end;
        }
    }
    // The file is the outermost container.
    let mut around = vec![Open {
        end: file_end,
        container: None,
        headings: Vec::new(),
    }];
    let mut next_container = containers.iter().peekable();
    for &(i, level) in headings {
        let pos = segs[i].range.start;
        let mut leave = |around: &mut Vec<Open>, at: usize| {
            while around.len() > 1 && around.last().is_some_and(|o| o.end <= at) {
                close(segs, around.pop().expect("checked"));
            }
        };
        while let Some(c) = next_container.next_if(|c| c.start <= pos) {
            leave(&mut around, c.start);
            // Two containers over the same bytes are one, as the innermost around a position.
            if around.last().is_some_and(|o| o.container == Some(c)) {
                continue;
            }
            around.push(Open {
                end: c.end,
                container: Some(c),
                headings: Vec::new(),
            });
        }
        leave(&mut around, pos);
        let open = &mut around.last_mut().expect("the file").headings;
        while let Some(&(j, _)) = open.last().filter(|(_, l)| *l >= level) {
            segs[j].range.end = pos;
            open.pop();
        }
        open.push((i, level));
    }
    for o in around {
        close(segs, o);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Input, Name, Segment};
    use crate::processors::Processor;
    use crate::trail::LineIndex;
    use std::path::Path;

    const GUIDE: &str = include_str!("../../tests/fixtures/md/guide.md");
    const D: &str = "document:Deploy guide";
    const I: &str = "heading:Install";

    fn input(bytes: &[u8]) -> Input<'_> {
        Input {
            bytes,
            path: Path::new("guide.md"),
            format: "markdown",
        }
    }

    fn byte_of(text: &str, nth: usize) -> usize {
        GUIDE.match_indices(text).nth(nth).unwrap().0
    }

    fn segments_with(params: &toml::Table, text: &str, nth: usize) -> Vec<Segment> {
        let at = byte_of(text, nth);
        let mut t = Markdown
            .trails(&input(GUIDE.as_bytes()), params, &[at])
            .unwrap();
        assert_eq!(t.len(), 1);
        t.remove(0)
    }

    fn kinds(t: &[Segment]) -> Vec<String> {
        t.iter().map(|s| format!("{}:{}", s.kind, s.name)).collect()
    }

    fn trail_with(params: &toml::Table, text: &str, nth: usize) -> Vec<String> {
        kinds(&segments_with(params, text, nth))
    }

    fn trail(text: &str, nth: usize) -> Vec<String> {
        trail_with(&toml::Table::new(), text, nth)
    }

    fn names(src: &str) -> Vec<Name> {
        names_bytes(src.as_bytes())
    }

    fn names_bytes(src: &[u8]) -> Vec<Name> {
        Markdown.names(&input(src), &toml::Table::new()).unwrap()
    }

    #[test]
    fn fixture_is_the_planned_one() {
        assert_eq!(GUIDE.lines().count(), 38);
        assert_eq!(GUIDE.len(), 420);
    }

    #[test]
    fn trails_in_the_guide() {
        let cases: &[(&str, &[&str])] = &[
            ("needle-intro", &[D, I]),
            ("needle-pkg", &[D, I, "heading:Linux", "code:bash"]),
            ("needle-item", &[D, I, "heading:Linux", "item:step one"]),
            ("needle-10", &[D, I, "heading:Plans", "column:Price"]),
            ("needle-a", &[D, I, "heading:Plans", "column:Plan"]),
            ("needle-b", &[D, I, "heading:Plans", "column:Price"]),
            ("needle-quoted", &[D, I, "heading:Plans", "column:Cell"]),
            (
                "needle-in-item",
                &[
                    D,
                    I,
                    "heading:Plans",
                    "item:Heading in item",
                    "heading:Heading in item",
                ],
            ),
            ("needle-after-list", &[D, I, "heading:Plans"]),
            ("needle-setext", &[D, "heading:Setext Title"]),
            // A Match on a label leaves that Segment out.
            ("Install", &[D]),
        ];
        for (text, want) in cases {
            assert_eq!(trail(text, 0), *want, "Trail of {text}");
        }
    }

    #[test]
    fn list_items_can_be_turned_off() {
        let mut p = toml::Table::new();
        p.insert("list_items".into(), toml::Value::Boolean(false));
        assert_eq!(trail_with(&p, "needle-item", 0), [D, I, "heading:Linux"]);
        assert_eq!(
            trail_with(&p, "needle-in-item", 0),
            [D, I, "heading:Plans", "heading:Heading in item"]
        );
    }

    #[test]
    fn item_names_are_cut() {
        let mut p = toml::Table::new();
        p.insert("item_name_chars".into(), toml::Value::Integer(4));
        let t = Markdown
            .trails(&input(b"- step one\n  more text\n"), &p, &[13])
            .unwrap();
        assert_eq!(kinds(&t[0]), ["item:step…"]);
    }

    /// Code review r3, Fable #6: the blocks of an item's name are joined by a space, and the
    /// name stays on one line.
    #[test]
    fn item_names_separate_their_blocks() {
        let name = |src: &[u8], needle: &[u8]| {
            let at = src.windows(needle.len()).position(|w| w == needle).unwrap();
            let t = Markdown
                .trails(&input(src), &toml::Table::new(), &[at])
                .unwrap();
            kinds(&t[0])[0].clone()
        };
        let s = b"- item one\n  ## B in item\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:item one B in item");
        assert_eq!(name(b"- # Head\n  text b\n", b"text b"), "item:Head");
        // An item that starts with a block takes the block's first line as its name.
        let s = b"- ```\n  code\n  more\n  ```\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:code");
        let s = b"- > quoted\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:quoted");
    }

    /// Code review r4, Fable #3 and #4: a code block, table, quote or HTML block after an
    /// item's text ends the item's name; the block is a Segment of its own.
    #[test]
    fn a_block_after_an_items_text_ends_its_name() {
        let name = |src: &[u8], needle: &[u8]| {
            let at = src.windows(needle.len()).position(|w| w == needle).unwrap();
            let t = Markdown
                .trails(&input(src), &toml::Table::new(), &[at])
                .unwrap();
            kinds(&t[0])[0].clone()
        };
        let s = b"- Estructura relevante:\n  ```json\n  {\n  \"a\": 1\n  }\n  ```\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:Estructura relevante:");
        let s = b"- [ ] Commit 3: Final launch prep\n  ```\n  docs: prepare for open-source launch\n  ```\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:[ ] Commit 3: Final launch prep");
        let s = b"- item\n  | a | b |\n  |---|---|\n  | 1 | 2 |\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:item");
        let s = b"- item\n  > quoted\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:item");
        let s = b"- item\n  ```\n  code\n  more\n  ```\n  text b\n";
        assert_eq!(name(s, b"text b"), "item:item");
        let s = b"- item\n  <div>\n  html\n  </div>\n- next\n";
        assert_eq!(name(s, b"html"), "item:item");
    }

    #[test]
    fn invalid_utf8_bytes_become_question_marks() {
        // One invalid byte in the middle must not lose the Trails of the whole file: each
        // invalid byte becomes `?`, so offsets stay put.
        let src = b"# Before\nneedle-one\n## Bad \xff here\nneedle-two\n# After\nneedle-three\n";
        let at = |s: &[u8]| src.windows(s.len()).position(|w| w == s).unwrap();
        let t = Markdown
            .trails(
                &input(src),
                &toml::Table::new(),
                &[at(b"needle-one"), at(b"needle-two"), at(b"needle-three")],
            )
            .unwrap();
        let t: Vec<Vec<String>> = t.iter().map(|x| kinds(x)).collect();
        assert_eq!(
            t,
            [
                vec!["heading:Before".to_string()],
                vec!["heading:Before".into(), "heading:Bad ? here".into()],
                vec!["heading:After".to_string()],
            ]
        );
        let n = Markdown.names(&input(src), &toml::Table::new()).unwrap();
        let texts: Vec<&str> = n.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, ["Before", "Bad ? here", "After"]);
        // The name keeps the offsets of the original bytes.
        let bad = &n[1];
        assert_eq!(
            bad.source_ranges(0..bad.text.len()),
            vec![at(b"Bad")..at(b"\nneedle-two")]
        );
        // A truncated multi-byte sequence: one `?` per byte.
        let n = names_bytes(b"# a\xe2\x82b\n");
        assert_eq!(n[0].text, "a??b");
    }

    #[test]
    fn sections_and_table_lines() {
        let linux = segments_with(&toml::Table::new(), "needle-pkg", 0);
        assert_eq!(
            (linux[2].line, linux[2].end_line, linux[2].level),
            (9, 18, Some(2))
        );
        let price = segments_with(&toml::Table::new(), "needle-10", 0);
        assert_eq!((price[3].line, price[3].end_line), (21, 24));
    }

    #[test]
    fn table_cells_sit_between_the_pipes() {
        let o = Markdown
            .outline(GUIDE.as_bytes(), &toml::Table::new())
            .unwrap();
        let header = byte_of("| Plan | Price |", 0);
        let cells: Vec<_> = o
            .segments
            .iter()
            .filter(|s| {
                s.segment.kind == "column" && s.range.start < header + 16 && s.range.start > header
            })
            .map(|s| s.range.clone())
            .collect();
        assert_eq!(cells, vec![header + 1..header + 7, header + 8..header + 15]);
    }

    #[test]
    fn names_in_the_guide() {
        let n = names(GUIDE);
        let texts: Vec<&str> = n.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "Deploy guide",
                "Install",
                "Linux",
                "Plans",
                "Heading in item",
                "Setext Title"
            ]
        );
        for x in &n {
            let joined: String = x.pieces.iter().map(|(_, s)| &GUIDE[s.clone()]).collect();
            assert_eq!(joined, x.text);
        }
        let plans = n.iter().find(|x| x.text == "Plans").unwrap();
        assert_eq!(kinds(&plans.trail), [D, I, "heading:Plans"]);
    }

    #[test]
    fn setext_heading_over_two_lines() {
        let src = "First\nSecond\n======\n";
        let n = names(src);
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].text, "First Second");
        // The space that joins the lines is a piece over the `\n` it replaces (code review r1,
        // Codex #10), so a Match on it has a place in the Source.
        assert_eq!(
            n[0].pieces,
            vec![(0..5, 0..5), (5..6, 5..6), (6..12, 6..12)]
        );
        assert_eq!(n[0].source_ranges(6..12), vec![6..12]);
        assert_eq!(n[0].source_ranges(5..6), vec![5..6]);
        assert_eq!(n[0].source_ranges(4..7), vec![4..5, 5..6, 6..7]);
        assert_eq!(LineIndex::new(src.as_bytes()).line_of(6), 2);
    }

    #[test]
    fn inline_code_piece_points_inside_the_backticks() {
        let src = "# A `code` B\n";
        let n = names(src);
        assert_eq!(n[0].text, "A code B");
        let code = n[0]
            .pieces
            .iter()
            .find(|(t, _)| &n[0].text[t.clone()] == "code")
            .unwrap();
        assert_eq!(code.1, 5..9);
        assert_eq!(&src[code.1.clone()], "code");
    }

    #[test]
    fn decoded_entity_pieces() {
        let n = names("# &lt; B\n");
        assert_eq!(n[0].text, "< B");
        assert_eq!(n[0].pieces, vec![(0..1, 2..6), (1..3, 6..8)]);
        assert_eq!(n[0].source_ranges(2..3), vec![7..8]);
        assert_eq!(n[0].source_ranges(0..1), vec![2..6]);
        assert_eq!(n[0].source_ranges(0..0), vec![2..2]);
        assert_eq!(n[0].source_ranges(3..3), vec![8..8]);

        let n = names("# &lt;\n");
        assert_eq!(n[0].source_ranges(1..1), vec![6..6]);

        let n = names("# &amp;\n");
        assert_eq!(n[0].text, "&");
        assert_eq!(n[0].pieces, vec![(0..1, 2..7)]);
    }

    #[test]
    fn frontmatter_title_is_read_as_yaml() {
        let n = names("---\ntitle: Hello # comment\n---\n# A\n");
        assert_eq!(n[0].text, "Hello");
        let src = "---\ntitle: |\n  Hello\n  World\n---\n# A\n";
        let n = names(src);
        assert_eq!(n[0].text, "Hello World");
        let at = n[0].text.find("World").unwrap();
        let hit: Vec<&str> = n[0]
            .source_ranges(at..at + 5)
            .into_iter()
            .map(|r| &src[r])
            .collect();
        assert_eq!(hit, ["World"]);
        let n = names("---\ntitle: 'It''s'\n---\n");
        assert_eq!(n[0].text, "It's");
    }

    #[test]
    fn empty_heading_has_no_pieces_and_an_anchor() {
        let n = names("###\n");
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].text, "");
        assert!(n[0].pieces.is_empty());
        assert_eq!(n[0].source_ranges(0..0), vec![3..3]);
    }

    /// The quadratic closing `close_sections` replaced, kept as the reference.
    fn close_sections_naive(
        segs: &mut [OutlineSegment],
        headings: &[(usize, u8)],
        containers: &[Range<usize>],
        file_end: usize,
    ) {
        let container_of = |pos: usize| {
            containers
                .iter()
                .filter(|c| c.start <= pos && pos < c.end)
                .min_by_key(|c| c.end - c.start)
                .cloned()
        };
        let starts: Vec<(usize, u8, Option<Range<usize>>)> = headings
            .iter()
            .map(|&(i, l)| (i, l, container_of(segs[i].range.start)))
            .collect();
        for (k, (i, lvl, cont)) in starts.iter().enumerate() {
            let limit = cont.as_ref().map(|c| c.end).unwrap_or(file_end);
            let next = starts[k + 1..]
                .iter()
                .find(|(j, l, c)| l <= lvl && c == cont && segs[*j].range.start < limit)
                .map(|(j, _, _)| segs[*j].range.start);
            segs[*i].range.end = next.unwrap_or(limit);
        }
    }

    /// Nested containers in document order inside `range`, from a small deterministic generator.
    fn nest(range: Range<usize>, seed: &mut u64, depth: u32, out: &mut Vec<Range<usize>>) {
        let mut at = range.start;
        while depth < 4 && at + 4 < range.end {
            *seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let len = 2 + (*seed >> 33) as usize % (range.end - at).min(60);
            let c = at + 1..(at + 1 + len).min(range.end);
            out.push(c.clone());
            nest(c.clone(), seed, depth + 1, out);
            at = c.end + (*seed >> 40) as usize % 5;
        }
    }

    #[test]
    fn sections_close_as_the_naive_closing_did() {
        for round in 0..200u64 {
            let mut seed = round;
            let mut containers = Vec::new();
            nest(0..400, &mut seed, 0, &mut containers);
            let mut segs = Vec::new();
            let mut headings = Vec::new();
            for pos in (0..400).step_by(3) {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                if (seed >> 35) % 3 == 0 {
                    let s = OutlineSegment::new("heading", "h", pos..pos + 1, pos..pos + 1);
                    segs.push(s);
                    headings.push((segs.len() - 1, 1 + ((seed >> 50) % 4) as u8));
                }
            }
            let mut want = segs.clone();
            close_sections_naive(&mut want, &headings, &containers, 400);
            close_sections(&mut segs, &headings, &containers, 400);
            let ends = |v: &[OutlineSegment]| v.iter().map(|s| s.range.end).collect::<Vec<_>>();
            assert_eq!(ends(&segs), ends(&want), "round {round}");
        }
    }
}
