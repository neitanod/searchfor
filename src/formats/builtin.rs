//! Built-in Format Definitions, embedded at compile time (ADR 0002).
/// (file name, TOML source). Add one line per file in formats/.
pub const BUILTIN: &[(&str, &str)] = &[
    ("markdown.toml", include_str!("../../formats/markdown.toml")),
];
