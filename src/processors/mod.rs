//! Processors turn Match anchors into Trails (design note 6).
pub mod external;
pub mod frontmatter;
pub mod keypath;
pub mod markdown;
pub mod treesitter;

use crate::model::{Input, Name, Trail};
use crate::trail::{Outline, TrailCursor, names_of, trail_for};

/// The public contract. M5's External Processors implement it over the protocol of spec §4.4,
/// which C10 extends with an `"op"` field: `"trails"` (the request of §4.4) and `"names"`
/// (no `matches`; the answer is
/// `{"names":[{"text","pieces":[[text_start,text_end,src_start,src_end],…],"anchor","trail":[…]}]}`).
/// An External Processor that answers `names` with an error makes `--in-headings` skip its
/// files with one warning.
pub trait Processor: Send + Sync {
    /// One Trail per anchor (absolute byte offset in `input.bytes`), in the same order.
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>>;
    /// Every searchable Segment name with the Source pieces of its text (`--in-headings`).
    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>>;
    /// The file's Outline, for a Processor that builds one: then the Trails can be computed
    /// while the rows are printed. None: only `trails`.
    fn outline_of(&self, _input: &Input, _params: &toml::Table) -> Option<anyhow::Result<Outline>> {
        None
    }
}

/// Built-in Processors that can describe a whole file as an Outline.
pub trait OutlineProcessor: Send + Sync {
    fn outline(&self, src: &[u8], params: &toml::Table) -> anyhow::Result<Outline>;
}

impl<T: OutlineProcessor> Processor for T {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let o = self.outline(input.bytes, params)?;
        // Anchors come in file order: one pass over the Outline.
        if anchors.is_sorted() {
            let mut c = TrailCursor::new(&o);
            return Ok(anchors.iter().map(|&a| c.trail(a)).collect());
        }
        Ok(anchors.iter().map(|&a| trail_for(&o, a)).collect())
    }

    fn outline_of(&self, input: &Input, params: &toml::Table) -> Option<anyhow::Result<Outline>> {
        Some(self.outline(input.bytes, params))
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.outline(input.bytes, params)?))
    }
}

/// The Processor named in a Format Definition's `processor` field.
pub fn by_name(name: &str) -> Option<&'static dyn Processor> {
    match name {
        "markdown" => Some(&markdown::Markdown),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trail::OutlineSegment;
    use std::path::Path;

    /// One heading named "A" over the first 10 bytes, its label the first 4.
    struct Fixed;

    impl OutlineProcessor for Fixed {
        fn outline(&self, src: &[u8], _: &toml::Table) -> anyhow::Result<Outline> {
            let mut s = OutlineSegment::new("heading", "A", 0..10, 0..4);
            s.name = Some(vec![(0..1, 2..3)]);
            Ok(Outline::build(vec![s], src))
        }
    }

    fn input(bytes: &[u8]) -> Input<'_> {
        Input {
            bytes,
            path: Path::new("x.md"),
            format: "markdown",
        }
    }

    #[test]
    fn outline_processors_give_one_trail_per_anchor_in_order() {
        let src = b"# A\nbody\n\nrest\n";
        let t = Fixed
            .trails(&input(src), &toml::Table::new(), &[5, 12, 2])
            .unwrap();
        let names: Vec<Vec<&str>> = t
            .iter()
            .map(|t| t.iter().map(|s| s.name.as_str()).collect())
            .collect();
        assert_eq!(names, vec![vec!["A"], vec![], vec![]]);
    }

    #[test]
    fn outline_processors_report_names() {
        let n = Fixed
            .names(&input(b"# A\nbody\n\nrest\n"), &toml::Table::new())
            .unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].text, "A");
        assert_eq!(n[0].pieces, vec![(0..1, 2..3)]);
    }

    #[test]
    fn by_name_knows_markdown_only() {
        assert!(by_name("markdown").is_some());
        assert!(by_name("plain").is_none());
        assert!(by_name("").is_none());
    }
}
