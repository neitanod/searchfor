//! buildfile: the stages and instructions of a Dockerfile, the targets, variables and
//! conditionals of a Makefile, the recipes and variables of a justfile (M6 spec §7.9–§7.11),
//! with one line scanner per `syntax`. A scanner gives a compact index of Source ranges, with no
//! text (headings' `compact`), whose cursor builds a Segment only when a Trail holds it and
//! streams the Trails (M5b spec §6.1); the model path builds the Outline from the same index.
pub mod dockerfile;
pub mod just;
pub mod make;

use super::headings::compact::{Index, IndexCursor, Spec, SpecName};
use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, Trail};
use crate::trail::{Outline, TrailCursor, names_of};
use std::ops::Range;

pub struct Buildfile;

/// Characters of a name before it is cut with `…`, when the Definition sets no `name_chars`.
const NAME_CHARS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Dockerfile,
    Make,
    Just,
}

fn syntax(params: &toml::Table) -> anyhow::Result<Syntax> {
    match params.get("syntax").and_then(|v| v.as_str()) {
        Some("dockerfile") => Ok(Syntax::Dockerfile),
        Some("make") => Ok(Syntax::Make),
        Some("just") => Ok(Syntax::Just),
        _ => anyhow::bail!(
            "buildfile needs `syntax` = \"dockerfile\", \"make\" or \"just\" in [params]"
        ),
    }
}

fn name_chars(params: &toml::Table) -> usize {
    params
        .get("name_chars")
        .and_then(|v| v.as_integer())
        .map_or(NAME_CHARS, |n| n.max(0) as usize)
}

/// What names an indexed Segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Named {
    /// Source bytes, whose text is the name: `--in-headings` searches it.
    Source(Range<usize>),
    /// A name the scanner makes, `stage 1`: it has no Source piece, so no search finds it.
    Ordinal(&'static str, usize),
}

/// One Segment as a scanner finds it: Source ranges only, kept as `BuildIndex` keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub kind: &'static str,
    /// The Section.
    pub range: Range<usize>,
    /// The bytes that name it: a Match there leaves it out of its own Trail.
    pub label: Range<usize>,
    pub name: Named,
    /// The opening line the tree shows, when the kind has one.
    pub signature: Option<Range<usize>>,
}

/// The Segments a scanner found, in any order, in the compact index (`compact::Index`): a
/// Segment's position stays the one `push` gave until the scanner is done.
#[derive(Debug, Default)]
pub(crate) struct BuildIndex {
    index: Index,
    /// The items as the scanner gave them, for the scanners' tests.
    #[cfg(test)]
    items: Vec<Item>,
}

impl BuildIndex {
    pub fn push(&mut self, item: Item) {
        let one;
        let (name, anchor) = match &item.name {
            Named::Source(r) => {
                one = [r.clone()];
                (SpecName::Source(&one), r.start)
            }
            Named::Ordinal(w, n) => (SpecName::Ordinal(w, *n), item.range.start),
        };
        self.index.push(Spec {
            kind: item.kind,
            range: item.range.clone(),
            label: item.label.clone(),
            name,
            anchor,
            signature: item.signature.clone(),
            level: None,
            search: true,
            lines: None,
            hidden: false,
        });
        #[cfg(test)]
        self.items.push(item);
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Closes Segment `i` at `end`.
    pub fn set_end(&mut self, i: usize, end: usize) {
        self.index.set_end(i, end);
        #[cfg(test)]
        {
            self.items[i].range.end = end;
        }
    }

    /// The index, sorted and each Segment within its parent.
    pub fn finish(self) -> Index {
        self.index.finish()
    }
}

/// The items as the scanner gave them, with their ends: what the index holds before `finish`.
#[cfg(test)]
impl std::ops::Index<usize> for BuildIndex {
    type Output = Item;
    fn index(&self, i: usize) -> &Item {
        assert_eq!(self.index.range(i), self.items[i].range);
        assert_eq!(self.index.kind(i), self.items[i].kind);
        &self.items[i]
    }
}

#[cfg(test)]
impl IntoIterator for BuildIndex {
    type Item = Item;
    type IntoIter = std::vec::IntoIter<Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

#[cfg(test)]
impl BuildIndex {
    pub fn iter(&self) -> std::slice::Iter<'_, Item> {
        self.items.iter()
    }
}

/// One physical line: `start..end` without its `\n` or `\r\n`, and `next`, where the following
/// one starts. A lone `\r` breaks no line (M6 spec §3, point 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Line {
    pub start: usize,
    pub end: usize,
    pub next: usize,
}

/// The line that starts at `pos`, or None at the end of `src`.
pub(crate) fn line_at(src: &[u8], pos: usize) -> Option<Line> {
    if pos >= src.len() {
        return None;
    }
    let (end, next) = match memchr::memchr(b'\n', &src[pos..]) {
        Some(k) => (pos + k, pos + k + 1),
        None => (src.len(), src.len()),
    };
    let end = if end > pos && src[end - 1] == b'\r' && next > end {
        end - 1
    } else {
        end
    };
    Some(Line {
        start: pos,
        end,
        next,
    })
}

/// `src[r]` without the spaces and tabs around it, as a range of `src`.
pub(crate) fn trim(src: &[u8], r: Range<usize>) -> Range<usize> {
    let mut a = r.start;
    let mut b = r.end;
    while a < b && matches!(src[a], b' ' | b'\t') {
        a += 1;
    }
    while b > a && matches!(src[b - 1], b' ' | b'\t') {
        b -= 1;
    }
    a..b
}

fn scan(src: &[u8], syntax: Syntax) -> Index {
    let items = match syntax {
        Syntax::Dockerfile => dockerfile::scan(src),
        Syntax::Make => make::scan(src),
        Syntax::Just => just::scan(src),
    };
    items.finish()
}

fn index_of(src: &[u8], params: &toml::Table) -> anyhow::Result<Index> {
    Ok(scan(src, syntax(params)?))
}

fn outline_of(src: &[u8], params: &toml::Table) -> anyhow::Result<Outline> {
    Ok(index_of(src, params)?.outline(src, name_chars(params)))
}

impl Processor for Buildfile {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let o = outline_of(input.bytes, params)?;
        let mut c = TrailCursor::new(&o);
        // An anchor before the last one starts the cursor's walk over: any order is answered.
        Ok(anchors.iter().map(|&a| c.trail(a)).collect())
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&outline_of(input.bytes, params)?))
    }

    fn outline_of(&self, input: &Input, params: &toml::Table) -> Option<anyhow::Result<Outline>> {
        Some(outline_of(input.bytes, params))
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        Ok(Box::new(BuildPrepared {
            src: input.bytes,
            index: index_of(input.bytes, params)?,
            name_chars: name_chars(params),
        }))
    }

    fn capability(&self, params: &toml::Table) -> Capability {
        match syntax(params) {
            Ok(_) => Capability::Native,
            Err(_) => Capability::Model,
        }
    }
}

/// A build file prepared once: its compact index, which the cursors borrow; the Segments of a
/// Trail are built when it needs them.
pub struct BuildPrepared<'a> {
    src: &'a [u8],
    index: Index,
    name_chars: usize,
}

impl Prepared for BuildPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(IndexCursor::new(self.src, &self.index, self.name_chars))
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.index.outline(self.src, self.name_chars)))
    }

    fn streams(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SharedTrail;
    use std::path::Path;
    use std::rc::Rc;

    fn input<'a>(bytes: &'a [u8], format: &'a str) -> Input<'a> {
        Input {
            bytes,
            path: Path::new("x"),
            format,
            disk: None,
        }
    }

    fn params(syntax: &str) -> toml::Table {
        let mut p = toml::Table::new();
        p.insert("syntax".into(), toml::Value::String(syntax.into()));
        p
    }

    /// The native cursor gives what the model gives, at every byte, and shares equal Trails.
    pub(crate) fn cursor_equals_model(src: &[u8], syntax: &str) {
        let p = params(syntax);
        let anchors: Vec<usize> = (0..src.len()).collect();
        let model = Buildfile.trails(&input(src, syntax), &p, &anchors).unwrap();
        let prepared = Buildfile.prepare(input(src, syntax), &p).unwrap();
        assert!(prepared.streams());
        let mut c = prepared.cursor();
        let mut last: Option<SharedTrail> = None;
        for (&a, m) in anchors.iter().zip(&model) {
            let t = c.trail_at(a).unwrap();
            assert_eq!(t.as_ref(), m.as_slice(), "byte {a}");
            if let Some(l) = &last
                && l.as_ref() == t.as_ref()
            {
                assert!(Rc::ptr_eq(l, &t), "byte {a}");
            }
            assert!(c.open_from().is_some_and(|o| o <= a));
            last = Some(t);
        }
    }

    /// Fable r1 #16, in the compact index: 256 nested conditionals around a Makefile cost a
    /// Trail about what a single one does, and their lines are the model's.
    #[test]
    fn nested_sections_count_their_lines_once() {
        let body = "K := v\n".repeat(300_000);
        let nested = format!(
            "{}{body}K := ONCE\n{}",
            "ifeq (a,b)\n".repeat(256),
            "endif\n".repeat(256)
        );
        let flat = format!("ifeq (a,b)\n{body}K := ONCE\nendif\n");
        let p = params("make");
        let time = |src: &str| {
            let prepared = Buildfile
                .prepare(input(src.as_bytes(), "make"), &p)
                .unwrap();
            let at = src.find("ONCE").unwrap();
            let mut t = Vec::new();
            let mut best = std::time::Duration::MAX;
            for _ in 0..3 {
                let mut c = prepared.cursor();
                let started = std::time::Instant::now();
                t = c.trail_at(at).unwrap().to_vec();
                best = best.min(started.elapsed());
            }
            (best, t)
        };
        let (t_nested, trail) = time(&nested);
        let (t_flat, _) = time(&flat);
        assert_eq!(trail.len(), 256);
        let at = nested.find("ONCE").unwrap();
        let model = Buildfile
            .trails(&input(nested.as_bytes(), "make"), &p, &[at])
            .unwrap();
        assert_eq!(trail, model[0]);
        assert_eq!((trail[0].line, trail[0].end_line), (1, 300_513));
        assert!(
            t_nested < t_flat * 4 + std::time::Duration::from_millis(20),
            "nested {t_nested:?}, flat {t_flat:?}"
        );
    }

    #[test]
    fn the_syntax_is_required_and_decides_the_capability() {
        assert_eq!(
            Buildfile.capability(&params("dockerfile")),
            Capability::Native
        );
        assert_eq!(Buildfile.capability(&params("cobol")), Capability::Model);
        let e = Buildfile
            .names(&input(b"FROM x\n", "x"), &toml::Table::new())
            .unwrap_err();
        assert!(e.to_string().contains("syntax"), "{e}");
    }

    #[test]
    fn lines_end_at_lf_or_crlf_and_a_lone_cr_breaks_none() {
        let src = b"a\r\nb\rc\nd";
        let l = line_at(src, 0).unwrap();
        assert_eq!((l.start, l.end, l.next), (0, 1, 3));
        let l = line_at(src, 3).unwrap();
        assert_eq!((l.start, l.end, l.next), (3, 6, 7));
        let l = line_at(src, 7).unwrap();
        assert_eq!((l.start, l.end, l.next), (7, 8, 8));
        assert!(line_at(src, 8).is_none());
        // A last line that ends in `\r` keeps it: no `\n` follows.
        assert_eq!(line_at(b"x\r", 0).unwrap().end, 2);
    }

    #[test]
    fn the_dockerfile_cursor_equals_its_model() {
        cursor_equals_model(
            b"ARG A=1\nFROM a AS b\nRUN x \\\n# c\n y\nRUN <<E\nz\nE\nFROM c\nCOPY . /\n",
            "dockerfile",
        );
    }

    #[test]
    fn the_make_cursor_equals_its_model() {
        cursor_equals_model(
            b"ifeq (a,b)\nx: y\n\tz\n\n# c\n\tw\nelse\nN := 1\nendif\ndefine D\nq\nendef\nK += v \\\n u\n",
            "make",
        );
    }

    #[test]
    fn the_just_cursor_equals_its_model() {
        cursor_equals_model(
            b"# doc\n[a]\n@r x='1': d\n  y\n\n  z\nV := 'v'\nalias s := r\nstray\n  w\n",
            "just",
        );
    }
}
