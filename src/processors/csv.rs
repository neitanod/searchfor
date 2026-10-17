//! csv: rows and columns of CSV and TSV files (spec §5.5). Filled by C3 (M3 plan).
use super::Processor;
use crate::model::{Input, Name, Trail};

pub struct Csv;

impl Processor for Csv {
    fn trails(&self, _: &Input, _: &toml::Table, _: &[usize]) -> anyhow::Result<Vec<Trail>> {
        Err(crate::helper::Unavailable.into())
    }

    fn names(&self, _: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Err(crate::helper::Unavailable.into())
    }
}
