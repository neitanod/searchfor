//! markup: HTML and XML element paths (spec §5.4, M3 plan design note 3). One walk over the
//! tags feeds keypath's Collector the elements that hold an anchor, so a big file with three
//! Matches keeps three short stacks.
pub mod html;
pub mod scan;

use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, Segment, SharedTrail, Trail};
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

#[derive(Clone, Copy)]
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
    /// Its place among the file's elements that may be Segments, in the order their start tags
    /// come (`Census`); `NO_SEQ` for the others.
    seq: u32,
    /// Its children that may be Segments, from the first one on: a leaf, and every element
    /// past MAX_DEPTH, costs only the Frame (M3 code review r1, Fable #4).
    children: Option<Box<Children>>,
}

/// The children of an open element, counted for their `[n]`.
#[derive(Default)]
struct Children {
    /// Children seen so far, by tag name: (a name of that tag, count).
    counts: Vec<(Range<usize>, u32)>,
    /// The `seq` of the first child counted in each slot of `counts`.
    first: Vec<u32>,
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
            seq: NO_SEQ,
            children: None,
        }
    }
}

/// `Frame::seq` of an element that is no Segment.
const NO_SEQ: u32 = u32::MAX;

/// The walk builds a Segment only when its element closes and holds an anchor (`keeps`), so
/// the elements without Matches cost no allocation (M3 plan, design note 3).
struct Walk<'s, 'c, 'a> {
    src: &'s [u8],
    p: Params,
    mode: Mode<'c, 'a>,
    stack: Vec<Frame>,
    /// The `seq` of the next element that may be a Segment.
    next_seq: u32,
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

/// What a walk does with the elements it closes.
enum Mode<'c, 'a> {
    /// Hands the Segments to a Collector (`Processor::trails`, `names`).
    Collect(&'c mut Collector<'a>),
    /// The preparation of a cursor: records each element's end and whether its tag repeats
    /// among its siblings (`Census`).
    Census(Census),
    /// The walk of a cursor: keeps the open elements only (`MarkupCursor`).
    Follow,
}

/// Per element that may be a Segment, by its `seq`: where it ends, with `REPEATS` set when
/// another sibling has its tag (its name then ends in `[n]`). 8 bytes per element, the
/// preparation of a markup file (M5b spec §6.1).
type Census = Vec<u64>;
const REPEATS: u64 = 1 << 63;

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

impl<'a> Walk<'_, '_, 'a> {
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
        // Built only up to its cut (`text::CutName`): the words of a `class` past it are not
        // even split (M3 code review r2, Codex #1).
        let mut n = crate::text::CutName::new(self.p.name_chars);
        n.push_lossy(src, f.name.clone());
        let mut has_id = false;
        if self.p.syntax == Syntax::Html {
            if let Some(id) = scan::attribute(src, &f.tag, f.name.end, b"id", false) {
                has_id = true;
                n.push_str("#");
                n.push_lossy(src, id);
            }
            if !n.full()
                && let Some(cls) = scan::attribute(src, &f.tag, f.name.end, b"class", false)
            {
                let mut i = cls.start;
                while i < cls.end && !n.full() {
                    while i < cls.end && src[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let s = i;
                    while i < cls.end && !src[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if s < i {
                        n.push_str(".");
                        n.push_lossy(src, s..i);
                    }
                }
            }
        }
        let (text, pieces) = n.finish();
        (text, pieces, has_id)
    }

    /// Counts a child named `name` in the current element; returns its slot in `counts` and
    /// its 1-based ordinal among its siblings of that tag.
    fn slot(&mut self, name: &Range<usize>, seq: u32) -> (usize, u32) {
        let src = self.src;
        let syntax = self.p.syntax;
        let parent = self.stack.last_mut().unwrap();
        let ch = parent.children.get_or_insert_with(Default::default);
        let indexed = ch.counts.len() >= LINEAR;
        if indexed {
            fill_key(&mut self.buf, &src[name.clone()], syntax);
        }
        let buf = &self.buf;
        let found = match (indexed, &ch.index) {
            (true, Some(ix)) => ix.get(&buf[..]).copied(),
            _ => ch.counts.iter().position(|(n, _)| {
                let (a, b) = (&src[n.clone()], &src[name.clone()]);
                if syntax == Syntax::Html {
                    a.eq_ignore_ascii_case(b)
                } else {
                    a == b
                }
            }),
        };
        if let Some(k) = found {
            ch.counts[k].1 += 1;
            return (k, ch.counts[k].1);
        }
        ch.counts.push((name.clone(), 1));
        ch.first.push(seq);
        let k = ch.counts.len() - 1;
        if indexed {
            let counts = &ch.counts;
            let ix = ch.index.get_or_insert_with(|| {
                // Index the tags counted so far, then this one.
                let mut ix = Map::default();
                let mut key = Vec::new();
                for (j, (n, _)) in counts[..k].iter().enumerate() {
                    fill_key(&mut key, &src[n.clone()], syntax);
                    ix.insert(key.clone().into_boxed_slice(), j);
                }
                ix
            });
            ix.insert(buf.clone().into_boxed_slice(), k);
        }
        (k, 1)
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
        let keep = match &mut self.mode {
            Mode::Collect(c) => f.seg && c.keeps(f.tag.start, end),
            Mode::Census(census) => {
                if f.seq != NO_SEQ {
                    let e = &mut census[f.seq as usize];
                    *e = (*e & REPEATS) | end as u64;
                }
                return;
            }
            Mode::Follow => return,
        };
        self.flush(&mut f);
        if keep {
            let (text, pieces, has_id) = self.name_of(&f);
            let mut s = OutlineSegment::new("tag", text, f.tag.start..end, f.tag.clone());
            // `html` and `body` are hidden and not searchable by name (D7, M3 review r1,
            // Fable #6).
            s.name = (!f.info.hidden).then_some(pieces);
            s.name_anchor = f.name.start;
            s.hidden = f.info.hidden;
            if has_id {
                self.collector().closed(s, end);
            } else {
                // A Segment's parent counted it, so it has its children.
                let parent = self.stack.last_mut().unwrap();
                let ch = parent.children.as_mut().expect("counted by `slot`");
                ch.pending.push((s, f.slot, f.ordinal));
            }
        }
    }

    /// The Collector of a `Mode::Collect` walk, the only one with pending children.
    fn collector(&mut self) -> &mut Collector<'a> {
        match &mut self.mode {
            Mode::Collect(c) => c,
            _ => unreachable!("only a collecting walk keeps Segments"),
        }
    }

    /// Hands `f`'s pending children to the Collector, with `[n]` on those whose tag repeats
    /// among their siblings (spec §5.4).
    fn flush(&mut self, f: &mut Frame) {
        let Some(ch) = f.children.as_mut() else {
            return;
        };
        for (mut s, slot, ordinal) in std::mem::take(&mut ch.pending) {
            if ch.counts[slot].1 >= 2 {
                s.segment.name.push_str(&format!("[{ordinal}]"));
            }
            let end = s.range.end;
            self.collector().closed(s, end);
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
        // Only a child that may be a Segment needs its place among its siblings.
        let seg = self.stack.len() <= MAX_DEPTH;
        // An empty element (`<x/>`, a void element) holds nothing but its tag: no Segment, but
        // it counts among its siblings.
        let empty = self_closing || info.void;
        let seq = if seg && !empty {
            self.next_seq += 1;
            self.next_seq - 1
        } else {
            NO_SEQ
        };
        let (slot, ordinal) = if seg { self.slot(&name, seq) } else { (0, 0) };
        if let Mode::Census(census) = &mut self.mode
            && seg
        {
            if seq != NO_SEQ {
                census.push(if ordinal >= 2 { REPEATS } else { 0 });
            }
            if ordinal == 2 {
                // The first sibling of this tag repeats too, now that there is a second.
                let parent = self.stack.last().unwrap();
                let first = parent.children.as_ref().expect("counted by `slot`").first[slot];
                if first != NO_SEQ {
                    census[first as usize] |= REPEATS;
                }
            }
        }
        if empty {
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
            seg,
            slot,
            ordinal,
            seq,
            children: None,
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
            if let Some(end) = self.apply(sc.step(), &mut sc) {
                break end;
            }
        };
        self.finish(end);
        end
    }

    /// One step of the scanner: Some with where the structure ends once it ends.
    fn apply(&mut self, step: Step, sc: &mut Scanner) -> Option<usize> {
        match step {
            Step::Tag(Tag::Start {
                name,
                tag,
                self_closing,
            }) => self.start(name, tag, self_closing, sc),
            Step::Tag(Tag::End { name, tag }) => {
                if let Err(at) = self.end(name, tag) {
                    return Some(at);
                }
            }
            Step::Eof => return Some(self.src.len()),
            Step::Error(at) => return Some(at),
        }
        None
    }

    /// Closes what is still open where the structure ends.
    fn finish(&mut self, end: usize) {
        while self.stack.len() > 1 {
            self.close_implied(end);
        }
        let mut root = self.stack.pop().unwrap();
        self.flush(&mut root);
    }
}

impl<'s, 'c, 'a> Walk<'s, 'c, 'a> {
    fn new(src: &'s [u8], p: Params, mode: Mode<'c, 'a>) -> Self {
        Walk {
            src,
            p,
            mode,
            stack: vec![Frame::root()],
            next_seq: 0,
            ids: Map::default(),
            open: Vec::new(),
            buf: Vec::new(),
            trimmed: (usize::MAX, 0),
        }
    }
}

fn outline(src: &[u8], p: Params, mut c: Collector<'_>) -> Outline {
    let end = Walk::new(src, p, Mode::Collect(&mut c)).run();
    c.finish(end, src)
}

/// A markup file prepared for its cursor (M5b spec §6.1): one walk over the whole file
/// records where each element ends and whether its tag repeats among its siblings, which a
/// Trail needs before the walk that follows the Matches reaches them.
struct MarkupPrepared<'a> {
    input: Input<'a>,
    params: &'a toml::Table,
    p: Params,
    census: Census,
}

fn census(src: &[u8], p: Params) -> Census {
    let mut w = Walk::new(src, p, Mode::Census(Vec::new()));
    w.run();
    match w.mode {
        Mode::Census(c) => c,
        _ => unreachable!("a census walk"),
    }
}

impl Prepared for MarkupPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(MarkupCursor::new(self.input.bytes, self.p, &self.census))
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Markup.names(&self.input, self.params)
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a markup file in file order: a second walk follows the Matches, and the open
/// elements at a Match, with their ends and `[n]` from the census, are its Trail.
struct MarkupCursor<'s> {
    src: &'s [u8],
    p: Params,
    census: &'s [u64],
    walk: Walk<'s, 'static, 'static>,
    sc: Scanner<'s>,
    /// The step read and not applied yet: a tag past the last offset asked.
    pending: Option<Step>,
    done: bool,
    at: usize,
    /// The line of byte `.0`, the last Segment start counted.
    lines: (usize, u64),
    /// The last Trail given, by the `seq` of its elements, and its Segments.
    last: Option<(Vec<u32>, SharedTrail)>,
    seqs: Vec<u32>,
}

impl<'s> MarkupCursor<'s> {
    fn new(src: &'s [u8], p: Params, census: &'s [u64]) -> Self {
        MarkupCursor {
            src,
            p,
            census,
            walk: Walk::new(src, p, Mode::Follow),
            sc: Scanner::new(src, p.syntax == Syntax::Xml),
            pending: None,
            done: false,
            at: 0,
            lines: (0, 1),
            last: None,
            seqs: Vec::new(),
        }
    }

    /// The walk up to `byte`: every start tag that starts at or before it, and every end tag
    /// that ends at or before it, so an element whose end tag holds `byte` is still open.
    fn reach(&mut self, byte: usize) {
        while !self.done {
            let step = match self.pending.take() {
                Some(s) => s,
                None => self.sc.step(),
            };
            let apply = match &step {
                Step::Tag(Tag::Start { tag, .. }) => tag.start <= byte,
                Step::Tag(Tag::End { tag, .. }) => tag.end <= byte,
                Step::Eof | Step::Error(_) => true,
            };
            if !apply {
                self.pending = Some(step);
                return;
            }
            // Where the structure ends, the census closed what is open: its ends say so.
            self.done = self.walk.apply(step, &mut self.sc).is_some();
        }
    }

    fn end_of(&self, seq: u32) -> usize {
        (self.census[seq as usize] & !REPEATS) as usize
    }

    /// The line of `byte`, counted on from the last one asked.
    fn line_of(&mut self, byte: usize) -> u64 {
        let (at, line) = self.lines;
        let line = if byte >= at {
            line + memchr::memchr_iter(b'\n', &self.src[at..byte]).count() as u64
        } else {
            line - memchr::memchr_iter(b'\n', &self.src[byte..at]).count() as u64
        };
        self.lines = (byte, line);
        line
    }

    /// The Segment of the open element at `k` in the walk's stack.
    fn segment(&mut self, k: usize) -> Segment {
        let f = &self.walk.stack[k];
        let (start, seq, ordinal) = (f.tag.start, f.seq, f.ordinal);
        let (mut name, _, has_id) = self.walk.name_of(f);
        if !has_id && self.census[seq as usize] & REPEATS != 0 {
            name.push_str(&format!("[{ordinal}]"));
        }
        let end = self.end_of(seq);
        let line = self.line_of(start);
        let last = end.saturating_sub(1).max(start).min(self.src.len());
        let end_line = line + memchr::memchr_iter(b'\n', &self.src[start..last]).count() as u64;
        Segment {
            kind: "tag".into(),
            name,
            line,
            end_line,
            level: None,
            signature: None,
        }
    }
}

impl Cursor for MarkupCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        if byte < self.at {
            // An offset before the last one walks again from the start.
            self.walk = Walk::new(self.src, self.p, Mode::Follow);
            self.sc = Scanner::new(self.src, self.p.syntax == Syntax::Xml);
            self.pending = None;
            self.done = false;
        }
        self.at = byte;
        self.reach(byte);
        // The open elements hold `byte` unless they ended before it (an implied end the walk has
        // not reached); a hidden element and one whose start tag holds `byte` are left out, as
        // `TrailCursor` leaves them.
        let mut seqs = std::mem::take(&mut self.seqs);
        seqs.clear();
        let mut ks = Vec::new();
        for (k, f) in self.walk.stack.iter().enumerate().skip(1) {
            if f.seq != NO_SEQ
                && !f.info.hidden
                && !f.tag.contains(&byte)
                && self.end_of(f.seq) > byte
            {
                seqs.push(f.seq);
                ks.push(k);
            }
        }
        if let Some((s, t)) = &self.last
            && *s == seqs
        {
            let t = t.clone();
            self.seqs = seqs;
            return Ok(t);
        }
        let mut trail = Vec::with_capacity(ks.len());
        for (i, &k) in ks.iter().enumerate() {
            // A Segment of the last Trail is the same element: its Segment is copied.
            let known = self
                .last
                .as_ref()
                .and_then(|(s, t)| s.iter().position(|&q| q == seqs[i]).map(|j| t[j].clone()));
            trail.push(match known {
                Some(seg) => seg,
                None => self.segment(k),
            });
        }
        let t: SharedTrail = std::rc::Rc::from(trail);
        self.last = Some((seqs.clone(), t.clone()));
        self.seqs = seqs;
        Ok(t)
    }
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

    /// The census of the file's elements, then a cursor that follows the Matches.
    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        let p = self::params(params)?;
        Ok(Box::new(MarkupPrepared {
            census: census(input.bytes, p),
            input,
            params,
            p,
        }))
    }

    fn capability(&self, _: &toml::Table) -> Capability {
        Capability::Native
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
    fn xml_keeps_the_trails_before_a_malformed_comment_or_pi() {
        // M3 code review r1, Codex #3.
        for bad in ["<!-- bad -- comment -->", "<??>"] {
            let src = format!("<r><ok>needle</ok>{bad}<a>needle</a></r>");
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

    #[test]
    fn markup_preparation_keeps_sibling_numbers_and_implicit_closures() {
        use crate::processors::Processor;
        let params: toml::Table = toml::from_str("syntax = 'html'").unwrap();
        for src in [
            &b"<ul><li>needle<li>needle</ul>"[..],
            &b"<nav><p>needle</p><p>needle</p></nav>"[..],
        ] {
            let input = crate::model::Input {
                bytes: src,
                path: std::path::Path::new("x.html"),
                format: "html",
                disk: None,
            };
            let anchors: Vec<_> = memchr::memmem::find_iter(src, b"needle").collect();
            let expected = Markup.trails(&input, &params, &anchors).unwrap();
            let prepared = Markup.prepare(input, &params).unwrap();
            assert!(prepared.streams());
            let mut c = prepared.cursor();
            for (at, t) in anchors.into_iter().zip(expected) {
                assert_eq!(c.trail_at(at).unwrap().as_ref(), t.as_slice());
            }
        }
    }

    /// The cursor against `trails` at every byte of `src`, then once more from the start.
    fn every_byte(src: &[u8], syntax: &str) {
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: syntax,
            disk: None,
        };
        let params = params(syntax);
        let anchors: Vec<usize> = (0..src.len()).collect();
        let expected = Markup.trails(&input, &params, &anchors).unwrap();
        let prepared = Markup.prepare(input, &params).unwrap();
        let mut c = prepared.cursor();
        for (&at, t) in anchors.iter().zip(&expected) {
            let got = c.trail_at(at).unwrap();
            assert_eq!(got.as_ref(), t.as_slice(), "byte {at} of {syntax}");
        }
        if let Some(t) = expected.first() {
            assert_eq!(c.trail_at(0).unwrap().as_ref(), t.as_slice());
        }
    }

    #[test]
    fn the_markup_cursor_gives_the_trails_of_every_byte() {
        every_byte(&fixture("page.html"), "html");
        every_byte(&fixture("feed.xml"), "xml");
        every_byte(&fixture("broken.xml"), "xml");
        // Implied ends, white space before them, ids, raw text, void and empty elements.
        every_byte(
            b"<ul>\n<li>a\n<li>b  \n  \n</ul>\n<p>x<p>y\n<div id=k><p>z</p><p>w</div>\n",
            "html",
        );
        every_byte(
            b"<html><body><table><tr><td>a<td>b<tr><td>c</table><script>if (a<b) {}</script>\n<br><br><p/><P>q</p></BODY>",
            "html",
        );
        // An XML error: what is open ends there.
        every_byte(b"<a><b>x</b><c>y</c>\n<b>z</b></x><b>tail</b>", "xml");
        // Past `LINEAR` distinct sibling tags, and past `MAX_DEPTH`.
        let mut wide = b"<r>".to_vec();
        for i in 0..12 {
            wide.extend(format!("<t{i}>a</t{i}>\n<t{i}>b</t{i}>").bytes());
        }
        wide.extend(b"</r>");
        every_byte(&wide, "xml");
        let deep = format!(
            "{}x{}",
            "<d>".repeat(MAX_DEPTH + 4),
            "</d>".repeat(MAX_DEPTH + 4)
        );
        every_byte(deep.as_bytes(), "xml");
    }
}
