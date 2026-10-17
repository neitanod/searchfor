//! subtitles: the cues of SubRip and WebVTT files (spec §5.6). Filled by C4 (M3 plan).
use super::Processor;
use crate::model::{Input, Name, Trail};

pub struct Subtitles;

impl Processor for Subtitles {
    fn trails(&self, _: &Input, _: &toml::Table, _: &[usize]) -> anyhow::Result<Vec<Trail>> {
        Err(crate::helper::Unavailable.into())
    }

    fn names(&self, _: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Err(crate::helper::Unavailable.into())
    }
}
