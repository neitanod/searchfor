//! Prepared files and their ordered Trail cursors (M5b spec §6.1, plan Task 9.1): a Processor
//! prepares what it needs of a file once, and a cursor gives the Trail of each Match in file
//! order, consecutive equal Trails shared. `ModelPrepared` is the compatibility adapter for
//! Processors that only answer the model contract (`Processor::trails`); `OutlinePrepared` is
//! the native cursor of the Processors that build an Outline.
use super::{Cursor, Prepared, Processor};
use crate::model::{Input, Name, Segment, SharedTrail};
use crate::trail::{Outline, TrailCursor, names_of};
use std::rc::Rc;

/// The model fallback (plan D11): each `trail_at` asks the Processor's `trails` for one anchor.
/// It is compatibility only: the pipeline never streams a file through it (`streams` is false),
/// it takes the one-shot whole-file model instead, so a Helper Processor never gets one request
/// per anchor from it.
pub struct ModelPrepared<'a, P: Processor + ?Sized> {
    processor: &'a P,
    input: Input<'a>,
    params: &'a toml::Table,
}

impl<'a, P: Processor + ?Sized> ModelPrepared<'a, P> {
    pub fn new(processor: &'a P, input: Input<'a>, params: &'a toml::Table) -> Self {
        Self {
            processor,
            input,
            params,
        }
    }
}

impl<P: Processor + ?Sized> Prepared for ModelPrepared<'_, P> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(ModelCursor {
            prepared: self,
            last: None,
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        self.processor.names(&self.input, self.params)
    }
}

struct ModelCursor<'p, 'a, P: Processor + ?Sized> {
    prepared: &'p ModelPrepared<'a, P>,
    /// The last Trail given, shared with the next one when its Segments are all equal.
    last: Option<SharedTrail>,
}

impl<P: Processor + ?Sized> Cursor for ModelCursor<'_, '_, P> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        let p = self.prepared;
        let mut trails = p.processor.trails(&p.input, p.params, &[byte])?;
        if trails.len() != 1 {
            anyhow::bail!("the Processor returned the wrong number of Trails");
        }
        let trail = trails.pop().expect("one Trail");
        Ok(share(&mut self.last, trail))
    }
}

/// `trail` as a shared Trail: `last` when every Segment is equal (kind, name, lines, level and
/// signature: a Segment is never shared by its name alone), else a new one that becomes `last`.
pub(crate) fn share(last: &mut Option<SharedTrail>, trail: Vec<Segment>) -> SharedTrail {
    match last {
        Some(l) if l.as_ref() == trail.as_slice() => l.clone(),
        _ => last.insert(Rc::from(trail)).clone(),
    }
}

/// The native cursor of an Outline Processor: the Outline is built once, and owned here, so
/// the cursor borrows it (plan D10).
pub struct OutlinePrepared {
    outline: Outline,
}

impl OutlinePrepared {
    pub fn new(outline: Outline) -> Self {
        Self { outline }
    }
}

impl Prepared for OutlinePrepared {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(OutlineCursor {
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

/// Trails from an Outline by the identity of their Segments: a Trail is cloned out of the
/// Outline only when its Segments' indices change.
struct OutlineCursor<'o> {
    trails: TrailCursor<'o>,
    indices: Vec<usize>,
    last: Option<(Vec<usize>, SharedTrail)>,
}

impl Cursor for OutlineCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        // An offset before the last one starts the walk over (`TrailCursor::indices`).
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processors::markdown::Markdown;
    use std::path::Path;

    fn input(bytes: &[u8]) -> Input<'_> {
        Input {
            bytes,
            path: Path::new("x.md"),
            format: "markdown",
            disk: None,
        }
    }

    #[test]
    fn the_markdown_cursor_equals_its_trails_and_shares_equal_ones() {
        let src = b"# A\none needle\ntwo needle\n## B\nneedle\n# C\nneedle\n";
        let params = toml::Table::new();
        let anchors: Vec<usize> = memchr::memmem::find_iter(src, b"needle").collect();
        let expected = Markdown.trails(&input(src), &params, &anchors).unwrap();
        let prepared = Markdown.prepare(input(src), &params).unwrap();
        assert!(prepared.streams());
        let mut c = prepared.cursor();
        let got: Vec<SharedTrail> = anchors.iter().map(|&a| c.trail_at(a).unwrap()).collect();
        for (g, e) in got.iter().zip(&expected) {
            assert_eq!(g.as_ref(), e.as_slice());
        }
        assert!(Rc::ptr_eq(&got[0], &got[1]));
        assert!(!Rc::ptr_eq(&got[1], &got[2]));
        // An offset before the last one is answered, not given a stale Trail.
        assert_eq!(
            c.trail_at(anchors[0]).unwrap().as_ref(),
            expected[0].as_slice()
        );
    }

    #[test]
    fn sharing_compares_whole_segments_not_names() {
        let seg = |line| Segment {
            kind: "heading".into(),
            name: "Same".into(),
            line,
            end_line: line + 1,
            level: Some(1),
            signature: None,
        };
        let mut last = None;
        let a = share(&mut last, vec![seg(1)]);
        let b = share(&mut last, vec![seg(1)]);
        let c = share(&mut last, vec![seg(5)]);
        assert!(Rc::ptr_eq(&a, &b));
        assert!(!Rc::ptr_eq(&b, &c));
        assert_eq!(c[0].line, 5);
    }
}
