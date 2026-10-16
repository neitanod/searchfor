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
/// byte, so offsets into the result are offsets into `content`.
fn replace_invalid(content: &[u8]) -> String {
    let mut out = String::with_capacity(content.len());
    for chunk in content.utf8_chunks() {
        out.push_str(chunk.valid());
        out.extend(std::iter::repeat_n('?', chunk.invalid().len()));
    }
    out
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
            match ev {
                Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle)) => {
                    segs.push(OutlineSegment::new(
                        "document",
                        "",
                        0..text.len(),
                        r.clone(),
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
                        it.text.push_str(t);
                    }
                }
                Event::SoftBreak | Event::HardBreak | Event::End(TagEnd::Paragraph) => {
                    // A soft break inside a (setext) heading becomes one space in its name.
                    if let (Some(h), Event::SoftBreak) = (heading.as_mut(), &ev) {
                        h.text.push(' ');
                    }
                    if let Some(it) = items.last_mut() {
                        it.done = true;
                    }
                }
                _ => {}
            }
        }

        // Close heading Sections inside their container (spec §5.1): at the next heading of the
        // same or a higher level in the same container, never past the container's end.
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
            let limit = cont.as_ref().map(|c| c.end).unwrap_or(text.len());
            let next = starts[k + 1..]
                .iter()
                .find(|(j, l, c)| l <= lvl && c == cont && segs[*j].range.start < limit)
                .map(|(j, _, _)| segs[*j].range.start);
            segs[*i].range.end = next.unwrap_or(limit);
        }
        segs.retain(|s| !(s.segment.kind == "document" && s.segment.name.is_empty()));
        Ok(Outline::build(segs, content))
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
        assert_eq!(n[0].pieces, vec![(0..5, 0..5), (6..12, 6..12)]);
        assert_eq!(n[0].source_ranges(6..12), vec![6..12]);
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
}
