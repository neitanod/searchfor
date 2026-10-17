//! The Grammars compiled into sfor (spec §7, ADR 0005).
use tree_sitter::Language;

pub const COMPILED_IN: &[&str] = &[
    "php",
    "javascript",
    "typescript",
    "tsx",
    "bash",
    "go",
    "rust",
    "python",
];

pub fn language(name: &str) -> Option<Language> {
    Some(match name {
        "php" => tree_sitter_php::LANGUAGE_PHP.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        _ => return None,
    })
}
