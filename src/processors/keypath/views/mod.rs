//! JSON views (M6 spec §5.1, plan D9): keypath's JSON Outline of one known shape, relabeled.
//! A view runs the JSON walker and its streaming cursor unchanged (`json::JsonCursor`) and
//! projects the members holding each Match: it hides the shape's wrappers, names the units a
//! person reads (a notebook's cells, a HAR entry, a chat's messages) and, for Jupyter, adds the
//! Markdown headings above them. The values a name needs are read by a lookahead over the unit
//! that holds them (`find`), the first time a Trail holds it, so a name may come from bytes past
//! the Match and the preparation keeps no table of every unit. A file without the shape at its
//! first value keeps keypath's JSON Trails.
pub(crate) mod har;
pub(crate) mod jupyter;

use super::json::{self, JsonCursor, Met, Open, Step};
use crate::model::{Input, Name, Segment, SharedTrail, Trail};
use crate::processors::{Cursor, Prepared};
use crate::text::{Pieces, cut_name};
use crate::trail::{Outline, TrailCursor, names_of};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Jupyter,
    Har,
}

impl View {
    /// The `view` of a Definition's `[params]`, if any.
    pub(crate) fn of(params: &::toml::Table) -> anyhow::Result<Option<View>> {
        match params.get("view") {
            None => Ok(None),
            Some(v) => match v.as_str() {
                Some("jupyter") => Ok(Some(View::Jupyter)),
                Some("har") => Ok(Some(View::Har)),
                _ => anyhow::bail!("keypath's `view` is \"jupyter\" or \"har\""),
            },
        }
    }
}

/// Where a member sits in its view's shape: what its own members are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ctx {
    /// Outside the shape: keypath's JSON.
    Plain,
    /// The elements hide: a cell's source lines, an output's text, a MIME key's lines.
    Lines,
    // Jupyter (M6 spec §6.5).
    Notebook,
    Cells,
    Cell,
    Outputs,
    Output,
    Data,
    // HAR (M6 spec §8.8).
    Har,
    Log,
    Entries,
    Pages,
    /// An element of `entries`.
    Entry,
    /// A `request` or a `response`.
    Exchange,
    PostData,
    /// Elements named by their `name`, after this word.
    Named(&'static str),
}

/// What a view makes of one member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    /// keypath's member: a key, or an element named `holder[n]`, hidden when its value is an
    /// array.
    Plain,
    /// A wrapper of the shape (`cells`, `source`, a source line): in no Trail and no name.
    Hidden,
    /// A key shown even when its value is an array (an output's MIME key).
    Shown,
    /// A Segment the view names.
    Node(Node),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Node {
    /// `cell N`, 1-based.
    Cell(usize),
    /// `output N`, 1-based within its cell.
    Output(usize),
    /// A HAR entry: `GET host/path`.
    Entry,
    /// A HAR page, by its `title`.
    Page,
    /// An element named by its `name`: `header Authorization`.
    Named(&'static str),
}

/// A member, as a view's `project` sees it.
pub(super) struct Probe<'a> {
    src: &'a [u8],
    key: Option<(Range<usize>, bool)>,
    /// An element's index.
    pub index: usize,
    /// Its value's first byte.
    value: u8,
}

impl Probe<'_> {
    /// Whether it is the key `name`.
    pub fn is(&self, name: &str) -> bool {
        self.key
            .as_ref()
            .is_some_and(|(r, e)| Step::Key(r.clone(), *e).is(self.src, name))
    }

    pub fn element(&self) -> bool {
        self.key.is_none()
    }

    pub fn object(&self) -> bool {
        self.value == b'{'
    }

    pub fn array(&self) -> bool {
        self.value == b'['
    }
}

/// The shape of the file's first value, by view: what its members are, and where the value the
/// shape was recognized by starts (Jupyter's `cells`).
fn shape(view: View, src: &[u8], root: usize) -> Option<(Ctx, usize)> {
    if src.get(root) != Some(&b'{') {
        return None;
    }
    let mut found = None;
    json::visit(src, root, &mut |m: &Met| {
        if m.path.len() != 1 {
            return true;
        }
        let p = probe(src, &m.path[0], m.value.start);
        found = match view {
            View::Jupyter => (p.is("cells") && p.array()).then_some((Ctx::Notebook, m.value.start)),
            View::Har => (p.is("log") && p.object()).then_some((Ctx::Har, m.value.start)),
        };
        found.is_none()
    });
    found
}

fn probe<'a>(src: &'a [u8], step: &Step, value: usize) -> Probe<'a> {
    let (key, index) = match step {
        Step::Key(r, e) => (Some((r.clone(), *e)), 0),
        Step::Index(n) => (None, *n),
    };
    Probe {
        src,
        key,
        index,
        value: src.get(value).copied().unwrap_or(0),
    }
}

fn project(view: View, ctx: Ctx, p: &Probe) -> (Role, Ctx) {
    match (ctx, view) {
        (Ctx::Plain, _) => (Role::Plain, Ctx::Plain),
        (Ctx::Lines, _) => (Role::Hidden, Ctx::Plain),
        (_, View::Jupyter) => jupyter::project(ctx, p),
        (_, View::Har) => har::project(ctx, p),
    }
}

/// The values at `paths` under the value that starts at `start` (each path a list of keys from
/// it), read until all are found or the value ends: the lookahead of a name (M6 spec §11).
pub(super) fn find(src: &[u8], start: usize, paths: &[&[&str]]) -> Vec<Option<Range<usize>>> {
    let mut out = vec![None; paths.len()];
    let mut left = paths.len();
    json::visit(src, start, &mut |m: &Met| {
        for (i, want) in paths.iter().enumerate() {
            if out[i].is_none()
                && m.path.len() == want.len()
                && m.path.iter().zip(want.iter()).all(|(s, k)| s.is(src, k))
            {
                out[i] = Some(m.value.clone());
                left -= 1;
            }
        }
        left > 0
    });
    out
}

/// The text of a scalar value at `v` and its pieces: a string decoded, a number, `true` or
/// `false` as written. None for `null` and for an object or an array.
pub(super) fn text_of(src: &[u8], v: &Range<usize>) -> Option<(String, Pieces)> {
    let raw = src.get(v.clone())?;
    match raw.first()? {
        b'"' if raw.len() >= 2 => {
            let inner = v.start + 1..v.end - 1;
            let escaped = memchr::memchr(b'\\', &src[inner.clone()]).is_some();
            Some(json::decode(src, inner, escaped))
        }
        b'-' | b'0'..=b'9' | b't' | b'f' => {
            let text = String::from_utf8_lossy(raw).into_owned();
            Some((text, vec![(0..raw.len(), v.clone())]))
        }
        _ => None,
    }
}

/// A name made of the Source's text and of words s4 adds, which have no piece (M6 spec §3
/// rule 4).
#[derive(Default)]
pub(super) struct NameBuf {
    text: String,
    pieces: Pieces,
}

impl NameBuf {
    pub fn word(&mut self, s: &str) {
        self.text.push_str(s);
    }

    /// The text of `(text, pieces)` from its byte `skip` on.
    pub fn source(&mut self, (text, pieces): &(String, Pieces), skip: usize) {
        let base = self.text.len();
        self.text.push_str(&text[skip..]);
        for (t, s) in pieces {
            if t.end <= skip {
                continue;
            }
            let (mut t, mut s) = (t.clone(), s.clone());
            if t.start < skip {
                if t.len() == s.len() {
                    s.start += skip - t.start;
                }
                t.start = skip;
            }
            self.pieces
                .push((t.start - skip + base..t.end - skip + base, s));
        }
    }

    /// The name, cut at `max` characters.
    pub fn finish(mut self, max: usize) -> (String, Pieces) {
        cut_name(&mut self.text, &mut self.pieces, max);
        (self.text, self.pieces)
    }
}

/// A Segment a view names, with what `--in-headings` needs of it.
#[derive(Debug, Clone)]
pub(super) struct Built {
    pub seg: Segment,
    /// The bytes that name it: a Match there is outside it (M6 spec §3 rule 5).
    pub label: Range<usize>,
    /// Its name's pieces in the Source; None when every word of it is s4's.
    pub pieces: Option<Pieces>,
}

/// A file of a JSON view, prepared once: the shape of its first value and, for Jupyter, the
/// Outline of its Markdown headings.
pub(crate) struct ViewPrepared<'a> {
    src: &'a [u8],
    view: View,
    name_chars: usize,
    /// What the first value's members are.
    root: Ctx,
    /// Jupyter's headings, nested by level.
    headings: Outline,
}

/// The prepared file of a JSON `view` (M6 plan, Task 6.1): a file whose first value does not
/// have the view's shape is keypath's JSON.
pub(crate) fn prepare<'a>(
    input: Input<'a>,
    params: &'a ::toml::Table,
    view: View,
) -> anyhow::Result<Box<dyn Prepared + 'a>> {
    let src = input.bytes;
    let root = json::value_start(src, 0);
    let Some((ctx, at)) = shape(view, src, root) else {
        return Ok(Box::new(json::prepare(src)));
    };
    let name_chars = super::lines::name_chars(params);
    let headings = match view {
        View::Jupyter => jupyter::headings(src, at, name_chars),
        View::Har => Outline::default(),
    };
    Ok(Box::new(ViewPrepared {
        src,
        view,
        name_chars,
        root: ctx,
        headings,
    }))
}

/// The model's Trails of a view (`Processor::trails`): its cursor over the anchors in order.
pub(crate) fn trails(
    input: &Input,
    params: &::toml::Table,
    view: View,
    anchors: &[usize],
) -> anyhow::Result<Vec<Trail>> {
    let input = Input { ..*input };
    let prepared = prepare(input, params, view)?;
    let mut order: Vec<usize> = (0..anchors.len()).collect();
    order.sort_by_key(|&i| anchors[i]);
    let mut cursor = prepared.cursor();
    let mut out = vec![Vec::new(); anchors.len()];
    for i in order {
        out[i] = cursor.trail_at(anchors[i])?.to_vec();
    }
    Ok(out)
}

/// The names of a view (`Processor::names`).
pub(crate) fn names(
    input: &Input,
    params: &::toml::Table,
    view: View,
) -> anyhow::Result<Vec<Name>> {
    prepare(Input { ..*input }, params, view)?.names()
}

impl<'a> ViewPrepared<'a> {
    fn view_cursor(&self) -> ViewCursor<'_> {
        ViewCursor {
            p: self,
            json: JsonCursor::new(self.src),
            open: Vec::new(),
            links: Vec::new(),
            headings: TrailCursor::new(&self.headings),
            hix: Vec::new(),
            ids: Vec::new(),
            last: None,
            nodes: Vec::new(),
        }
    }

    /// The Segment of `node`, a member whose value starts at `value`, over keypath's `base`
    /// (its lines). None when the values its name needs are absent: the member keeps keypath's
    /// Segment then.
    fn node(&self, node: Node, value: usize, base: Segment) -> Option<Built> {
        match self.view {
            View::Jupyter => jupyter::node(self.src, node, value, base, self.name_chars),
            View::Har => har::node(self.src, node, value, base, self.name_chars),
        }
    }
}

impl Prepared for ViewPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(self.view_cursor())
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        let mut out = names_of(&self.headings);
        let mut c = self.view_cursor();
        let mut found: Vec<(u64, usize)> = Vec::new();
        json::members(self.src, &mut |id, start| found.push((id, start)));
        for (id, start) in found {
            c.links(start);
            if let Some(n) = c.name_of(id) {
                out.push(n);
            }
        }
        Ok(out)
    }

    fn streams(&self) -> bool {
        true
    }
}

/// One Segment of a view that holds an offset.
#[derive(Debug, Clone)]
struct Link {
    /// Index in `ViewCursor::open`.
    open: usize,
    role: Role,
}

/// A heading's identity among the members' (`ViewCursor::ids`).
const HEADING: u64 = 1 << 62;

/// The Trails of a view in file order: keypath's JSON cursor finds the members holding each
/// offset, and the view projects them.
struct ViewCursor<'p> {
    p: &'p ViewPrepared<'p>,
    json: JsonCursor<'p>,
    open: Vec<Open>,
    links: Vec<Link>,
    headings: TrailCursor<'p>,
    hix: Vec<usize>,
    ids: Vec<u64>,
    last: Option<(Vec<u64>, SharedTrail)>,
    /// The view's Segments of the last offset's members, by member: each named once.
    nodes: Vec<(u64, Option<Built>)>,
}

impl ViewCursor<'_> {
    /// The members holding `at`, each with its role (`links`), and the headings over it.
    fn links(&mut self, at: usize) {
        self.json.holding(at, &mut self.open);
        self.links.clear();
        let src = self.p.src;
        let mut ctx = if self.json.roots() == 1 {
            self.p.root
        } else {
            Ctx::Plain
        };
        for (i, o) in self.open.iter().enumerate() {
            let step = match &o.key {
                Some((r, e)) => Step::Key(r.clone(), *e),
                None => Step::Index(o.index),
            };
            let (role, next) = project(self.p.view, ctx, &probe(src, &step, o.value));
            ctx = next;
            self.links.push(Link { open: i, role });
        }
        self.headings.indices(at, &mut self.hix);
        let ids: Vec<u64> = self.links.iter().map(|l| self.open[l.open].id).collect();
        self.nodes.retain(|(id, _)| ids.contains(id));
    }

    /// Whether a heading's own text holds `at`: its parents' Trail, and nothing under it.
    fn on_heading(&self, at: usize) -> bool {
        let h = &self.p.headings.segments;
        let i = h.partition_point(|s| s.label.start <= at);
        i > 0 && h[i - 1].label.contains(&at)
    }

    /// The view's Segment of link `k`, if it names one, made once.
    fn built(&mut self, k: usize) -> Option<Built> {
        let Role::Node(node) = self.links[k].role else {
            return None;
        };
        let o = self.open[self.links[k].open].clone();
        if let Some((_, b)) = self.nodes.iter().find(|(id, _)| *id == o.id) {
            return b.clone();
        }
        let base = self.json.plain(&o);
        let b = self.p.node(node, o.value, base);
        self.nodes.push((o.id, b.clone()));
        b
    }

    /// The Segments of the Trail of `at`, as links: (link, label) of each shown one, before the
    /// first whose label holds `at`.
    fn shown(&mut self, at: usize) -> Vec<(usize, Option<Built>)> {
        let mut out = Vec::new();
        if self.on_heading(at) {
            return out;
        }
        for k in 0..self.links.len() {
            let o = &self.open[self.links[k].open];
            let (label, array) = (o.label.clone(), o.array);
            let entry = match self.links[k].role {
                Role::Hidden => continue,
                Role::Plain if array => continue,
                Role::Plain | Role::Shown => (k, None, label),
                Role::Node(_) => match self.built(k) {
                    Some(b) => {
                        let label = b.label.clone();
                        (k, Some(b), label)
                    }
                    None if array => continue,
                    None => (k, None, label),
                },
            };
            if entry.2.contains(&at) {
                break;
            }
            out.push((entry.0, entry.1));
        }
        out
    }

    /// The Name of member `id`, after `links` at its start: None when the view hides it or
    /// its name has no piece of the Source.
    fn name_of(&mut self, id: u64) -> Option<Name> {
        let k = self.links.iter().position(|l| self.open[l.open].id == id)?;
        let start = self.open[self.links[k].open].start;
        let mut trail = Vec::new();
        let mut labels = Vec::new();
        for &h in &self.hix {
            trail.push(self.p.headings.segments[h].segment.clone());
            labels.push(self.p.headings.segments[h].label.clone());
        }
        let shown = self.shown(start);
        for (j, built) in shown {
            if j >= k {
                break;
            }
            let (seg, label) = self.segment(j, built);
            trail.push(seg);
            labels.push(label);
        }
        let o = self.open[self.links[k].open].clone();
        let (seg, label, pieces) = match self.links[k].role {
            Role::Hidden => return None,
            Role::Plain | Role::Shown => {
                let (r, e) = o.key.clone()?;
                let (_, pieces) = json::decode(self.p.src, r, e);
                (self.json.plain(&o), o.label.clone(), pieces)
            }
            Role::Node(_) => match self.built(k) {
                Some(b) => (b.seg, b.label, b.pieces?),
                None => {
                    let (r, e) = o.key.clone()?;
                    let (_, pieces) = json::decode(self.p.src, r, e);
                    (self.json.plain(&o), o.label.clone(), pieces)
                }
            },
        };
        let anchor = pieces.first().map_or(label.start, |p| p.1.start);
        trail.push(seg);
        labels.push(label);
        Some(Name {
            text: trail.last().expect("pushed").name.clone(),
            pieces,
            anchor,
            trail,
            labels,
        })
    }

    /// The Segment of link `k` and its label.
    fn segment(&mut self, k: usize, built: Option<Built>) -> (Segment, Range<usize>) {
        match built {
            Some(b) => (b.seg, b.label),
            None => {
                let o = self.open[self.links[k].open].clone();
                (self.json.plain(&o), o.label)
            }
        }
    }
}

impl Cursor for ViewCursor<'_> {
    fn trail_at(&mut self, at: usize) -> anyhow::Result<SharedTrail> {
        self.links(at);
        let shown = self.shown(at);
        self.ids.clear();
        self.ids
            .extend(self.hix.iter().map(|&h| HEADING | h as u64));
        self.ids
            .extend(shown.iter().map(|(k, _)| self.open[self.links[*k].open].id));
        if let Some((ids, t)) = &self.last
            && *ids == self.ids
        {
            return Ok(t.clone());
        }
        let mut trail: Vec<Segment> = self
            .hix
            .iter()
            .map(|&h| self.p.headings.segments[h].segment.clone())
            .collect();
        for (k, built) in shown {
            trail.push(self.segment(k, built).0);
        }
        let t: SharedTrail = trail.into();
        self.last = Some((self.ids.clone(), t.clone()));
        Ok(t)
    }

    /// The outermost member's start or heading's, whichever is first.
    fn open_from(&mut self) -> Option<usize> {
        let json = self.json.open_from()?;
        Some(json.min(self.headings.open_from()))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Input;
    use crate::processors::Processor;
    use crate::processors::keypath::Keypath;
    use std::path::Path;

    pub(super) fn params(view: &str) -> ::toml::Table {
        ::toml::from_str(&format!("syntax = 'json'\nview = '{view}'")).unwrap()
    }

    fn input(src: &[u8]) -> Input<'_> {
        Input {
            bytes: src,
            path: Path::new("x"),
            format: "x",
            disk: None,
        }
    }

    /// The Trail names of every `needle` in `src`.
    pub(super) fn needles(src: &[u8], view: &str) -> Vec<String> {
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        Keypath
            .trails(&input(src), &params(view), &anchors)
            .unwrap()
            .iter()
            .map(|t| {
                t.iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" › ")
            })
            .collect()
    }

    /// The cursor asked at every byte in order, and once more from the start, gives what a new
    /// cursor gives at each byte alone.
    pub(super) fn every_byte(src: &[u8], view: &str) {
        let params = params(view);
        let prepared = Keypath.prepare(input(src), &params).unwrap();
        assert!(prepared.streams());
        let mut cursor = prepared.cursor();
        for at in (0..=src.len()).chain([0]) {
            let alone = prepared.cursor().trail_at(at).unwrap();
            assert_eq!(
                cursor.trail_at(at).unwrap(),
                alone,
                "{} at {at}",
                String::from_utf8_lossy(src)
            );
        }
    }

    #[test]
    fn jupyter_headings_cross_cells_and_skip_fences() {
        let src = br###"{"cells":[
{"cell_type":"markdown","source":"# A\n```\n# not\n```\n## B\u000a  ### C ##\n"},
{"source":["needle"],"cell_type":"code"},
{"cell_type":"markdown","source":["#no\n","# D\r\n"]},
{"cell_type":"code","source":["needle"],"outputs":[{"output_type":"stream","text":["needle"]}]}
],"metadata":{"k":"needle"}}"###;
        assert_eq!(
            needles(src, "jupyter"),
            [
                "A › B › C › cell 2",
                "D › cell 4",
                "D › cell 4 › output 1",
                "metadata › k"
            ]
        );
        every_byte(src, "jupyter");
        // A Match in a heading's own text has its parents' Trail only.
        let names = Keypath.names(&input(src), &params("jupyter")).unwrap();
        let c = names.iter().find(|n| n.text == "C").unwrap();
        let t: Vec<&str> = c.trail.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(t, ["A", "B", "C"]);
        let at = c.pieces[0].1.start;
        let anchors = [at];
        let trail = &Keypath
            .trails(&input(src), &params("jupyter"), &anchors)
            .unwrap()[0];
        let t: Vec<&str> = trail.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(t, ["A", "B"]);
        // Kept keys are names; the hidden wrappers and the generated `cell N` are not.
        assert!(names.iter().any(|n| n.text == "k"));
        assert!(names.iter().any(|n| n.text == "cell_type"));
        assert!(!names.iter().any(|n| {
            ["cells", "source", "outputs", "text"].contains(&n.text.as_str())
                || n.text.starts_with("cell ")
        }));
    }
}
