//! buildfile: the stages and instructions of a Dockerfile, the targets, variables and
//! conditionals of a Makefile (M6 spec §7.9–§7.10), with one line scanner per `syntax`. A scanner gives a compact index of Source ranges, with no text; the Outline is
//! built from it once per file, and its cursor streams the Trails (M5b spec §6.1).
pub mod dockerfile;
pub mod make;

use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, SharedTrail, Trail};
use crate::trail::{Outline, OutlineSegment, TrailCursor, names_of};
use std::ops::Range;
use std::rc::Rc;

pub struct Buildfile;

/// Characters of a name before it is cut with `…`, when the Definition sets no `name_chars`.
const NAME_CHARS: usize = 40;
/// Characters of a signature: an opening line, as treesitter's.
const SIGNATURE_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Syntax {
    Dockerfile,
    Make,
}

fn syntax(params: &toml::Table) -> anyhow::Result<Syntax> {
    match params.get("syntax").and_then(|v| v.as_str()) {
        Some("dockerfile") => Ok(Syntax::Dockerfile),
        Some("make") => Ok(Syntax::Make),
        _ => anyhow::bail!("buildfile needs `syntax` = \"dockerfile\" or \"make\" in [params]"),
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

/// One Segment of the index: Source ranges only.
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

/// The Segments a scanner found, in any order.
pub(crate) type BuildIndex = Vec<Item>;

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

fn scan(src: &[u8], syntax: Syntax) -> BuildIndex {
    match syntax {
        Syntax::Dockerfile => dockerfile::scan(src),
        Syntax::Make => make::scan(src),
    }
}

/// The text of a signature: the opening line, invalid UTF-8 as U+FFFD, cut at
/// `SIGNATURE_CHARS` characters. Only that many bytes and a few more are decoded.
fn signature(src: &[u8], r: Range<usize>) -> String {
    let r = r.start..r.end.min(r.start.saturating_add(4 * SIGNATURE_CHARS + 4));
    let text = String::from_utf8_lossy(&src[r]);
    text.trim_end().chars().take(SIGNATURE_CHARS).collect()
}

/// The Outline of an index: the names and signatures decoded, cut at `name_chars`.
fn outline(src: &[u8], index: BuildIndex, name_chars: usize) -> Outline {
    let segments = index
        .into_iter()
        .map(|item| {
            let (text, pieces) = match &item.name {
                Named::Source(r) => {
                    let mut n = crate::text::CutName::new(name_chars);
                    n.push_lossy(src, r.clone());
                    let (t, p) = n.finish();
                    (t, Some((p, r.start)))
                }
                Named::Ordinal(word, n) => (format!("{word} {n}"), None),
            };
            let mut s = OutlineSegment::new(item.kind, text, item.range, item.label);
            if let Some((pieces, anchor)) = pieces {
                s.name = Some(pieces);
                s.name_anchor = anchor;
            }
            s.segment.signature = item.signature.map(|r| signature(src, r));
            s
        })
        .collect();
    Outline::build(segments, src)
}

fn outline_of(src: &[u8], params: &toml::Table) -> anyhow::Result<Outline> {
    let syntax = syntax(params)?;
    Ok(outline(src, scan(src, syntax), name_chars(params)))
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
            outline: outline_of(input.bytes, params)?,
        }))
    }

    fn capability(&self, params: &toml::Table) -> Capability {
        match syntax(params) {
            Ok(_) => Capability::Native,
            Err(_) => Capability::Model,
        }
    }
}

/// A build file prepared once: the Outline of its index.
pub struct BuildPrepared {
    outline: Outline,
}

impl Prepared for BuildPrepared {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(BuildCursor {
            trails: TrailCursor::new(&self.outline),
            indices: Vec::new(),
            last: None,
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.outline))
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of a prepared build file in file order: a Trail is cloned out of the Outline only
/// when the indices of its Segments change.
pub struct BuildCursor<'o> {
    trails: TrailCursor<'o>,
    indices: Vec<usize>,
    last: Option<(Vec<usize>, SharedTrail)>,
}

impl Cursor for BuildCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        self.trails.indices(byte, &mut self.indices);
        match &self.last {
            Some((ix, t)) if *ix == self.indices => Ok(t.clone()),
            _ => {
                let t: SharedTrail = Rc::from(self.trails.segments(&self.indices));
                self.last = Some((self.indices.clone(), t.clone()));
                Ok(t)
            }
        }
    }

    fn open_from(&mut self) -> Option<usize> {
        Some(self.trails.open_from())
    }

    fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        Some(self.trails.open_from_with(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
}
