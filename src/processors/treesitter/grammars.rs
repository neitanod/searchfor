//! The tree-sitter Grammars compiled into sfor (spec §7, ADR 0005). C6 fills it.

/// The Grammars compiled into sfor (spec §7).
pub const COMPILED_IN: &[&str] = &[];

pub fn language(_name: &str) -> Option<tree_sitter::Language> {
    None
}
