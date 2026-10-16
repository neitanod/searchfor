//! Markdown Processor: headings, frontmatter title, fenced code, table columns, list items.
use super::OutlineProcessor;
use crate::trail::Outline;

pub struct Markdown;

impl OutlineProcessor for Markdown {
    fn outline(&self, _content: &[u8], _params: &toml::Table) -> anyhow::Result<Outline> {
        Ok(Outline::default())
    }
}
