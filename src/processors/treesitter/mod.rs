//! treesitter: code Trails from the query of a Format Definition (spec §5.3). C6 fills it.
pub mod grammars;

/// treesitter: code Trails from a query (spec §5.3). Per-run state (D39): `Processors` owns
/// one instance per run.
#[derive(Default)]
pub struct TreeSitter {}

impl super::Processor for TreeSitter {
    fn trails(
        &self,
        _: &crate::model::Input,
        _: &toml::Table,
        _: &[usize],
    ) -> anyhow::Result<Vec<crate::model::Trail>> {
        anyhow::bail!("treesitter is not implemented yet (C6)")
    }

    fn names(
        &self,
        _: &crate::model::Input,
        _: &toml::Table,
    ) -> anyhow::Result<Vec<crate::model::Name>> {
        anyhow::bail!("treesitter is not implemented yet (C6)")
    }
}
