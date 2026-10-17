//! markup: HTML and XML element paths (spec §5.4, M3 plan design note 3). One walk over the
//! tags feeds keypath's Collector the elements that hold an anchor, so a big file with three
//! Matches keeps three short stacks.
pub mod html;
pub mod scan;

use super::Processor;
use crate::model::{Input, Name, Trail};
use crate::processors::keypath::collect::Collector;
use crate::trail::{Outline, OutlineSegment, TrailCursor, names_of};
use scan::{Scanner, Step, Tag};
use std::collections::HashMap;
use std::ops::Range;

use crate::text::Pieces;

pub struct Markup;

/// Past this many open elements no more Segments open (as JSON's 256, M2 plan D34).
pub const MAX_DEPTH: usize = 256;
pub const DEFAULT_NAME_CHARS: usize = 40;
/// Distinct child tags a parent compares one by one before it builds an index.
const LINEAR: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Syntax {
    Html,
    Xml,
}

struct Params {
    syntax: Syntax,
    name_chars: usize,
}

fn params(t: &toml::Table) -> anyhow::Result<Params> {
    let syntax = match t.get("syntax").and_then(|v| v.as_str()) {
        Some("html") => Syntax::Html,
        Some("xml") => Syntax::Xml,
        _ => anyhow::bail!("markup needs `syntax` = \"html\" or \"xml\" in [params]"),
    };
    let name_chars = t
        .get("name_chars")
        .and_then(|v| v.as_integer())
        .filter(|&n| n > 0)
        .map_or(DEFAULT_NAME_CHARS, |n| n as usize);
    Ok(Params { syntax, name_chars })
}

/// One open element of the walk.
struct Frame {
    /// The tag name in the Source.
    name: Range<usize>,
    /// The start tag, `<` to `>`.
    tag: Range<usize>,
    /// HTML: its tag classified once (`html::info`).
    info: html::Info,
    /// HTML: what the implied-end rules would end from here (`html::reach`).
    reach: html::Reach,
    /// HTML: the id of its tag in `Walk::ids`.
    id: u32,
    /// It may become a Segment (not the root, not past MAX_DEPTH).
    seg: bool,
    /// Its place among its parent's children of the same tag: (slot in the parent's
    /// `counts`, 1-based ordinal).
    slot: usize,
    ordinal: u32,
    /// Children seen so far, by tag name: (a name of that tag, count).
    counts: Vec<(Range<usize>, u32)>,
    /// Past `LINEAR` distinct tags, the slot in `counts` of each tag's key (`fill_key`), so a
    /// parent with thousands of distinct child tags costs constant time per child (M3 review
    /// r1, Codex #1). Memory: one entry per distinct child tag of each open element.
    index: Option<Map<usize>>,
    /// Closed children kept as Segments and without an id, which get `[n]` when their tag
    /// repeats: they reach the Collector when this element closes and the counts are final.
    pending: Vec<(OutlineSegment, usize, u32)>,
}

impl Frame {
    fn root() -> Self {
        Frame {
            name: 0..0,
            tag: 0..0,
            info: html::info(b""),
            reach: html::NO_REACH,
            id: 0,
            seg: false,
            slot: 0,
            ordinal: 0,
            counts: Vec::new(),
            index: None,
            pending: Vec::new(),
        }
    }
}

/// A name's text and pieces from the Source bytes `r` (`text::push_lossy`).
fn push_piece(src: &[u8], r: Range<usize>, text: &mut String, pieces: &mut Pieces) {
    crate::text::push_lossy(text, pieces, src, r);
}

/// The walk builds a Segment only when its element closes and holds an anchor (`keeps`), so
/// the elements without Matches cost no allocation (M3 plan, design note 3).
struct Walk<'s, 'c, 'a> {
    src: &'s [u8],
    p: Params,
    c: &'c mut Collector<'a>,
    stack: Vec<Frame>,
    /// HTML: an id per distinct tag (`fill_key`) of the file, and how many elements of each are
    /// open: an end tag with none open is ignored at once instead of searching the stack (M3
    /// review r2, Codex #1). One allocation per distinct tag; a closing element finds its count
    /// by the id its Frame keeps.
    ids: Map<u32>,
    open: Vec<u32>,
    /// The key being looked up, reused.
    buf: Vec<u8>,
    /// The last `at` that `close_implied` trimmed back from, and where white space before it
    /// starts: many elements ended at one byte trim its white space once.
    trimmed: (usize, usize),
}

/// FNV-1a: the maps of the walk hash short tag names, where SipHash's cost per lookup showed in
/// the walk's time; nothing in them comes from an attacker who can choose collisions that
/// matter more than a slower walk.
#[derive(Default)]
struct Fnv(u64);

impl std::hash::Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 {
            0xcbf2_9ce4_8422_2325
        } else {
            self.0
        };
        for &b in bytes {
            h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.0 = h;
    }
}

type Map<V> = HashMap<Box<[u8]>, V, std::hash::BuildHasherDefault<Fnv>>;

/// The key of a tag in `buf`: its bytes, lowercased in HTML.
fn fill_key(buf: &mut Vec<u8>, name: &[u8], syntax: Syntax) {
    buf.clear();
    buf.extend_from_slice(name);
    if syntax == Syntax::Html {
        buf.make_ascii_lowercase();
    }
}

impl Walk<'_, '_, '_> {
    fn same(&self, a: &Range<usize>, b: &Range<usize>) -> bool {
        let (a, b) = (&self.src[a.clone()], &self.src[b.clone()]);
        match self.p.syntax {
            Syntax::Html => a.eq_ignore_ascii_case(b),
            Syntax::Xml => a == b,
        }
    }

    /// `tag#id.class1.class2` (HTML) or the tag (XML), cut at `name_chars` characters, with
    /// its pieces (spec §5.4). Returns whether the element has an id.
    fn name_of(&self, f: &Frame) -> (String, Pieces, bool) {
        let src = self.src;
        let mut text = String::new();
        let mut pieces = Vec::new();
        push_piece(src, f.name.clone(), &mut text, &mut pieces);
        let mut has_id = false;
        if self.p.syntax == Syntax::Html {
            if let Some(id) = scan::attribute(src, &f.tag, f.name.end, b"id", false) {
                has_id = true;
                text.push('#');
                push_piece(src, id, &mut text, &mut pieces);
            }
            if let Some(cls) = scan::attribute(src, &f.tag, f.name.end, b"class", false) {
                let mut i = cls.start;
                while i < cls.end {
                    while i < cls.end && src[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let s = i;
                    while i < cls.end && !src[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if s < i {
                        text.push('.');
                        push_piece(src, s..i, &mut text, &mut pieces);
                    }
                }
            }
        }
        crate::text::cut_name(&mut text, &mut pieces, self.p.name_chars);
        (text, pieces, has_id)
    }

    /// Counts a child named `name` in the current element; returns its slot in `counts`.
    fn slot(&mut self, name: &Range<usize>) -> usize {
        let src = self.src;
        let syntax = self.p.syntax;
        let indexed = self.stack.last().unwrap().counts.len() >= LINEAR;
        if indexed {
            fill_key(&mut self.buf, &src[name.clone()], syntax);
        }
        let buf = &self.buf;
        let parent = self.stack.last_mut().unwrap();
        let found = match (indexed, &parent.index) {
            (true, Some(ix)) => ix.get(&buf[..]).copied(),
            _ => parent.counts.iter().position(|(n, _)| {
                let (a, b) = (&src[n.clone()], &src[name.clone()]);
                if syntax == Syntax::Html {
                    a.eq_ignore_ascii_case(b)
                } else {
                    a == b
                }
            }),
        };
        if let Some(k) = found {
            parent.counts[k].1 += 1;
            return k;
        }
        parent.counts.push((name.clone(), 1));
        let k = parent.counts.len() - 1;
        if indexed {
            let ix = parent.index.get_or_insert_with(|| {
                // Index the tags counted so far, then this one.
                let mut ix = Map::default();
                let mut key = Vec::new();
                for (j, (n, _)) in parent.counts[..k].iter().enumerate() {
                    fill_key(&mut key, &src[n.clone()], syntax);
                    ix.insert(key.clone().into_boxed_slice(), j);
                }
                ix
            });
            ix.insert(buf.clone().into_boxed_slice(), k);
        }
        k
    }

    /// HTML: the id of the tag `name`, given one when it is new.
    fn tag_id(&mut self, name: &Range<usize>) -> u32 {
        fill_key(&mut self.buf, &self.src[name.clone()], self.p.syntax);
        if let Some(&id) = self.ids.get(&self.buf[..]) {
            return id;
        }
        let id = self.open.len() as u32;
        self.ids.insert(self.buf.clone().into_boxed_slice(), id);
        self.open.push(0);
        id
    }

    /// Closes the innermost open element at `end` (exclusive).
    fn close(&mut self, end: usize) {
        let mut f = self.stack.pop().expect("the root is never closed here");
        if self.p.syntax == Syntax::Html {
            self.open[f.id as usize] -= 1;
        }
        self.flush(&mut f);
        if f.seg && self.c.keeps(f.tag.start, end) {
            let (text, pieces, has_id) = self.name_of(&f);
            let mut s = OutlineSegment::new("tag", text, f.tag.start..end, f.tag.clone());
            // `html` and `body` are hidden and not searchable by name (D7, M3 review r1,
            // Fable #6).
            s.name = (!f.info.hidden).then_some(pieces);
            s.name_anchor = f.name.start;
            s.hidden = f.info.hidden;
            if has_id {
                self.c.closed(s, end);
            } else {
                let parent = self.stack.last_mut().unwrap();
                parent.pending.push((s, f.slot, f.ordinal));
            }
        }
    }

    /// Hands `f`'s pending children to the Collector, with `[n]` on those whose tag repeats
    /// among their siblings (spec §5.4).
    fn flush(&mut self, f: &mut Frame) {
        for (mut s, slot, ordinal) in std::mem::take(&mut f.pending) {
            if f.counts[slot].1 >= 2 {
                s.segment.name.push_str(&format!("[{ordinal}]"));
            }
            let end = s.range.end;
            self.c.closed(s, end);
        }
    }

    /// Closes the innermost open element where something else ends it (a start tag HTML lets
    /// end it, an ancestor's end tag, the end of the file or an XML error at `at`): its Section
    /// ends after its last byte that is not white space, so it ends on its own last line.
    fn close_implied(&mut self, at: usize) {
        let floor = self.stack.last().map_or(0, |f| f.tag.end);
        if self.trimmed.0 != at {
            let mut t = at;
            while t > 0 && self.src[t - 1].is_ascii_whitespace() {
                t -= 1;
            }
            self.trimmed = (at, t);
        }
        // Trimming back from `at` and stopping at `floor` ends at the larger of the two.
        let end = if at <= floor {
            floor
        } else {
            self.trimmed.1.max(floor)
        };
        self.close(end);
    }

    fn start(
        &mut self,
        name: Range<usize>,
        tag: Range<usize>,
        self_closing: bool,
        sc: &mut Scanner,
    ) {
        let html = self.p.syntax == Syntax::Html;
        let info = html::info(if html { &self.src[name.clone()] } else { b"" });
        if html {
            let top = self.stack.last().unwrap();
            let open = self.stack.len() - 1;
            let current = (open > 0).then_some(&top.info);
            for _ in 0..html::closed_by_start(open, current, &top.reach, &info) {
                self.close_implied(tag.start);
            }
        }
        let slot = self.slot(&name);
        let ordinal = self.stack.last().unwrap().counts[slot].1;
        // An empty element (`<x/>`, a void element) holds nothing but its tag: no Segment, but
        // it counts among its siblings.
        if self_closing || info.void {
            return;
        }
        let raw = info.raw_text;
        let reach = html::reach(
            &self.stack.last().unwrap().reach,
            &info,
            self.stack.len() - 1,
        );
        let id = if html {
            let id = self.tag_id(&name);
            self.open[id as usize] += 1;
            id
        } else {
            0
        };
        self.stack.push(Frame {
            name: name.clone(),
            tag,
            info,
            reach,
            id,
            seg: self.stack.len() <= MAX_DEPTH,
            slot,
            ordinal,
            counts: Vec::new(),
            index: None,
            pending: Vec::new(),
        });
        if raw {
            sc.skip_raw_text(&self.src[name]);
        }
    }

    /// An end tag: XML closes the current element or stops (Err with the error's byte); HTML
    /// closes the innermost open element of that name and the ones inside it, or ignores it.
    /// Each search is linear only in the elements it then closes, so the walk stays linear.
    fn end(&mut self, name: Range<usize>, tag: Range<usize>) -> Result<(), usize> {
        let top = self.stack.len() - 1;
        if self.p.syntax == Syntax::Xml {
            if top > 0 && self.same(&self.stack[top].name, &name) {
                self.close(tag.end);
                return Ok(());
            }
            return Err(tag.start);
        }
        fill_key(&mut self.buf, &self.src[name.clone()], self.p.syntax);
        let Some(&id) = self.ids.get(&self.buf[..]) else {
            return Ok(());
        };
        if self.open[id as usize] == 0 {
            return Ok(());
        }
        let k = (1..self.stack.len())
            .rev()
            .find(|&k| self.stack[k].id == id)
            .expect("an element of this tag is open");
        while self.stack.len() > k + 1 {
            self.close_implied(tag.start);
        }
        self.close(tag.end);
        Ok(())
    }

    /// Walks the file; returns where the structure ends: its length, or the byte where XML
    /// stops being well formed (spec §5.4).
    fn run(&mut self) -> usize {
        let mut sc = Scanner::new(self.src, self.p.syntax == Syntax::Xml);
        let end = loop {
            match sc.step() {
                Step::Tag(Tag::Start {
                    name,
                    tag,
                    self_closing,
                }) => self.start(name, tag, self_closing, &mut sc),
                Step::Tag(Tag::End { name, tag }) => {
                    if let Err(at) = self.end(name, tag) {
                        break at;
                    }
                }
                Step::Eof => break self.src.len(),
                Step::Error(at) => break at,
            }
        };
        while self.stack.len() > 1 {
            self.close_implied(end);
        }
        let mut root = self.stack.pop().unwrap();
        self.flush(&mut root);
        end
    }
}

fn outline(src: &[u8], p: Params, mut c: Collector<'_>) -> Outline {
    let end = Walk {
        src,
        p,
        c: &mut c,
        stack: vec![Frame::root()],
        ids: Map::default(),
        open: Vec::new(),
        buf: Vec::new(),
        trimmed: (usize::MAX, 0),
    }
    .run();
    c.finish(end, src)
}

impl Processor for Markup {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let p = self::params(params)?;
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let sorted: Vec<usize> = order.iter().map(|&i| anchors[i]).collect();
        let o = outline(input.bytes, p, Collector::for_anchors(&sorted));
        let mut cursor = TrailCursor::new(&o);
        let mut out = vec![Vec::new(); anchors.len()];
        for (k, &i) in order.iter().enumerate() {
            out[i] = cursor.trail(sorted[k]);
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        let p = self::params(params)?;
        Ok(names_of(&outline(input.bytes, p, Collector::for_names())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn params(syntax: &str) -> toml::Table {
        toml::from_str(&format!("syntax = '{syntax}'")).unwrap()
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/markup")
                .join(name),
        )
        .unwrap()
    }

    /// (line, Trail names) of every `needle` in `src`.
    fn needles(src: &[u8], syntax: &str) -> Vec<(usize, String)> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: syntax,
            disk: None,
        };
        let t = Markup.trails(&input, &params(syntax), &anchors).unwrap();
        anchors
            .iter()
            .zip(t)
            .map(|(&a, t)| {
                let line = src[..a].iter().filter(|&&b| b == b'\n').count() + 1;
                (
                    line,
                    t.iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(" › "),
                )
            })
            .collect()
    }

    fn pairs(v: &[(usize, &str)]) -> Vec<(usize, String)> {
        v.iter().map(|(l, s)| (*l, s.to_string())).collect()
    }

    #[test]
    fn html_element_paths() {
        assert_eq!(
            needles(&fixture("page.html"), "html"),
            pairs(&[
                (4, "head › title"),
                (5, "head › script"),
                (12, "nav.top.menu › ul › li[3]"),
                (19, "main#content › section#pricing.card.wide › p.note[2]"),
                (
                    23,
                    "main#content › section#pricing.card.wide › table › tr[3] › td[2]"
                ),
                (26, "main#content › div › div › div"),
                (27, "main#content"),
            ])
        );
    }

    #[test]
    fn html_sections_end_where_html_ends_them() {
        let src = fixture("page.html");
        let at = memchr::memmem::find(&src, b"needle plan").unwrap();
        let input = Input {
            bytes: &src,
            path: Path::new("x"),
            format: "html",
            disk: None,
        };
        let t = Markup.trails(&input, &params("html"), &[at]).unwrap();
        let lines: Vec<(&str, &str, u64, u64)> = t[0]
            .iter()
            .map(|s| (s.kind.as_str(), s.name.as_str(), s.line, s.end_line))
            .collect();
        assert_eq!(
            lines,
            [
                ("tag", "main#content", 15, 28),
                ("tag", "section#pricing.card.wide", 16, 25),
                // The `<table>` start tag ends the open `p` (HTML's rules), on line 19.
                ("tag", "p.note[2]", 19, 19),
            ]
        );
    }

    #[test]
    fn xml_element_paths() {
        assert_eq!(
            needles(&fixture("feed.xml"), "xml"),
            pairs(&[
                (3, ""),
                (6, "catalog"),
                (12, "catalog › book[2] › title"),
                (14, "catalog › book[2] › note"),
                (17, "catalog › magazine › title"),
            ])
        );
    }

    #[test]
    fn xml_keeps_the_trails_before_a_syntax_error() {
        assert_eq!(
            needles(&fixture("broken.xml"), "xml"),
            pairs(&[(2, "root › ok"), (3, "root › a › b"), (4, "")])
        );
    }

    #[test]
    fn xml_doctypes_and_malformed_tags_do_not_invent_elements() {
        // M3 review r1, Codex #4 and #5.
        assert_eq!(
            needles(
                br#"<!DOCTYPE r [<!ENTITY e "]> <fake>">]><r>needle</r>"#,
                "xml"
            ),
            pairs(&[(1, "r")])
        );
        assert_eq!(
            needles(
                b"<r><ok>needle</ok><a=b>needle</a=b><c x=bare>needle</c></r>",
                "xml"
            ),
            pairs(&[(1, "r › ok"), (1, ""), (1, "")])
        );
        // M3 review r2, Codex #3: a name XML does not allow, a form feed, a repeated attribute.
        for bad in [
            "<©>needle</©>",
            "<a\x0cb='x'>needle</a>",
            "<a x='1' x='2'>needle</a>",
        ] {
            let src = format!("<r><ok>needle</ok>{bad}</r>");
            assert_eq!(
                needles(src.as_bytes(), "xml"),
                pairs(&[(1, "r › ok"), (1, "")]),
                "{bad}"
            );
        }
    }

    #[test]
    fn an_optgroup_ends_the_open_option() {
        // M3 review r1, Codex #9.
        let src = b"<select><option>first<optgroup label=x><option>needle</optgroup><optgroup><option>needle</select>";
        assert_eq!(
            needles(src, "html"),
            pairs(&[
                (1, "select › optgroup[1] › option"),
                (1, "select › optgroup[2] › option")
            ])
        );
    }

    #[test]
    fn many_distinct_sibling_tags_keep_their_indexes() {
        // Past LINEAR distinct tags the index finds the slot (M3 review r1, Codex #1).
        let mut src = String::from("<r>");
        for i in 0..40 {
            src.push_str(&format!("<t{i}/><T{i}/>"));
        }
        src.push_str("<t7>needle</t7><x>needle</x></r>");
        assert_eq!(
            needles(src.as_bytes(), "xml"),
            pairs(&[(1, "r › t7[2]"), (1, "r › x")])
        );
        let html = src.replace("<r>", "<div>").replace("</r>", "</div>");
        // HTML compares tags without case: t7, T7 and the last t7 are three.
        assert_eq!(
            needles(html.as_bytes(), "html"),
            pairs(&[(1, "div › t7[3]"), (1, "div › x")])
        );
    }

    #[test]
    fn names_are_searchable_with_their_pieces() {
        let src = br#"<div id="main" class="a b"><p>x</p><p>y</p></div>"#;
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: "html",
            disk: None,
        };
        let n = Markup.names(&input, &params("html")).unwrap();
        let got: Vec<(&str, Pieces)> = n
            .iter()
            .map(|n| (n.text.as_str(), n.pieces.clone()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "div#main.a.b",
                    vec![
                        (0..3, 1..4),
                        (4..8, 9..13),
                        (9..10, 22..23),
                        (11..12, 24..25)
                    ]
                ),
                ("p[1]", vec![(0..1, 28..29)]),
                ("p[2]", vec![(0..1, 36..37)]),
            ]
        );
    }

    #[test]
    fn html_and_body_are_not_names() {
        let src = b"<html><body><p>x</p></body></html>";
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: "html",
            disk: None,
        };
        let n = Markup.names(&input, &params("html")).unwrap();
        let texts: Vec<&str> = n.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(texts, ["p"]);
    }

    #[test]
    fn the_sibling_index_agrees_with_the_linear_count() {
        // Fable r1 #1: 20 distinct tags cross LINEAR, then `a` again is `a[2]`.
        let mut src = String::from("<r><a>needle</a>");
        for i in 0..20 {
            src.push_str(&format!("<t{i}/>"));
        }
        src.push_str("<a>needle</a></r>");
        assert_eq!(
            needles(src.as_bytes(), "xml"),
            pairs(&[(1, "r › a[1]"), (1, "r › a[2]")])
        );
    }

    #[test]
    fn long_names_are_cut_and_deep_nesting_stops_opening_segments() {
        let classes: String = (0..30).map(|i| format!(" c{i}")).collect();
        let src = format!("<div class=\"{classes}\">needle</div>");
        let got = needles(src.as_bytes(), "html");
        assert_eq!(got[0].1.chars().count(), 40);
        assert!(got[0].1.ends_with('…'), "{got:?}");
        let deep = format!("{}needle{}", "<x>".repeat(100_000), "</x>".repeat(100_000));
        let t = needles(deep.as_bytes(), "xml");
        assert_eq!(t[0].1.matches('›').count() + 1, MAX_DEPTH);
    }
}
