//! The `object` Processor (spec §5.7). Stub of C1; Chunk 2 writes it.
#[doc(hidden)]
pub mod fixture;
pub mod rows;

use crate::model::{Input, Name, Trail};
use crate::trail::LineIndex;

pub struct Object;

impl super::Processor for Object {
    fn trails(
        &self,
        _input: &Input,
        _params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        Ok(vec![Vec::new(); anchors.len()])
    }

    fn names(&self, _input: &Input, _params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(Vec::new())
    }
}

/// What the `object` Processor knows of a file. The stub reads nothing.
pub struct Map;

impl Map {
    pub fn parse(_bytes: &[u8]) -> Option<Map> {
        None
    }

    pub fn trail(&self, _at: usize, _lines: &LineIndex) -> Trail {
        Vec::new()
    }
}
