//! keypath: YAML, JSON and TOML key paths (spec §5.2). C2 fills it.
pub mod collect;
pub mod json;
pub mod toml;
pub mod yaml;

/// keypath: YAML, JSON and TOML key paths (spec §5.2).
pub struct Keypath;

impl super::Processor for Keypath {
    // `toml` is the sibling module here: the crate is `::toml`.
    fn trails(
        &self,
        _: &crate::model::Input,
        _: &::toml::Table,
        _: &[usize],
    ) -> anyhow::Result<Vec<crate::model::Trail>> {
        anyhow::bail!("keypath is not implemented yet (C2)")
    }

    fn names(
        &self,
        _: &crate::model::Input,
        _: &::toml::Table,
    ) -> anyhow::Result<Vec<crate::model::Name>> {
        anyhow::bail!("keypath is not implemented yet (C2)")
    }
}
