//! markup: HTML and XML element paths (spec §5.4). Filled by C2 (M3 plan).
pub mod html;
pub mod scan;

use super::Processor;
use crate::model::{Input, Name, Trail};

pub struct Markup;

impl Processor for Markup {
    fn trails(&self, _: &Input, _: &toml::Table, _: &[usize]) -> anyhow::Result<Vec<Trail>> {
        Err(crate::helper::Unavailable.into())
    }

    fn names(&self, _: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Err(crate::helper::Unavailable.into())
    }
}
