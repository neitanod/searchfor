//! Built-in Format Definitions, embedded at compile time (ADR 0002).
/// (file name, TOML source). Add one line per file in formats/.
pub const BUILTIN: &[(&str, &str)] = &[
    ("markdown.toml", include_str!("../../formats/markdown.toml")),
    ("php.toml", include_str!("../../formats/php.toml")),
    (
        "javascript.toml",
        include_str!("../../formats/javascript.toml"),
    ),
    (
        "typescript.toml",
        include_str!("../../formats/typescript.toml"),
    ),
    ("tsx.toml", include_str!("../../formats/tsx.toml")),
    ("bash.toml", include_str!("../../formats/bash.toml")),
    ("go.toml", include_str!("../../formats/go.toml")),
    ("rust.toml", include_str!("../../formats/rust.toml")),
    ("python.toml", include_str!("../../formats/python.toml")),
    ("json.toml", include_str!("../../formats/json.toml")),
    ("toml.toml", include_str!("../../formats/toml.toml")),
    ("yaml.toml", include_str!("../../formats/yaml.toml")),
];
