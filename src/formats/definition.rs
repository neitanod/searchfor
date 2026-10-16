//! Format Definitions (spec §4.2, ADR 0002).
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    User(PathBuf),
    /// A user file that replaced the built-in with the same name.
    Overridden(PathBuf),
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Magic {
    #[serde(default)]
    pub offset: usize,
    /// Hex string, e.g. "7f454c46" for ELF.
    pub hex: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FormatDefinition {
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub shebangs: Vec<String>,
    #[serde(default)]
    pub magic: Vec<Magic>,
    pub processor: String,
    #[serde(default)]
    pub params: toml::Table,
    #[serde(skip, default = "builtin")]
    pub origin: Origin,
}

fn builtin() -> Origin {
    Origin::BuiltIn
}

impl FormatDefinition {
    /// `known_processor` says whether a Processor name exists. C9 passes
    /// `|p| crate::processors::by_name(p).is_some()`; tests pass `|p| p == "markdown"`. Taking
    /// it as a parameter keeps this chunk independent of the Processor registry (C5).
    pub fn parse(src: &str, known_processor: impl Fn(&str) -> bool) -> Result<Self, String> {
        let d: Self = toml::from_str(src).map_err(|e| e.to_string())?;
        if d.name.is_empty() {
            return Err("`name` is empty".into());
        }
        if !known_processor(&d.processor) {
            return Err(format!("unknown processor `{}`", d.processor));
        }
        for m in &d.magic {
            if m.hex.is_empty()
                || m.hex.len() % 2 != 0
                || !m.hex.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(format!(
                    "magic `{}` is not an even-length hex string",
                    m.hex
                ));
            }
        }
        Ok(d)
    }

    /// Each magic entry as (offset, bytes). `parse` has already checked the hex strings.
    pub fn magic_bytes(&self) -> Vec<(usize, Vec<u8>)> {
        self.magic
            .iter()
            .map(|m| {
                let bytes = (0..m.hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&m.hex[i..i + 2], 16).unwrap())
                    .collect();
                (m.offset, bytes)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn known(p: &str) -> bool {
        p == "markdown"
    }

    #[test]
    fn builtin_markdown_parses() {
        let d = FormatDefinition::parse(crate::formats::builtin::BUILTIN[0].1, known).unwrap();
        assert_eq!(d.name, "markdown");
        assert!(d.extensions.contains(&"md".to_string()));
        assert_eq!(d.params["list_items"].as_bool(), Some(true));
    }

    #[test]
    fn errors_name_the_field() {
        assert!(
            FormatDefinition::parse("name='x'\nprocessor='nope'", known)
                .unwrap_err()
                .contains("unknown processor `nope`")
        );
        assert!(
            FormatDefinition::parse("name='x'\nprocessor='markdown'\nmagic=[{hex='7'}]", known)
                .unwrap_err()
                .contains("magic")
        );
        assert!(
            FormatDefinition::parse("name='x'\nprocessor='markdown'\nbogus=1", known)
                .unwrap_err()
                .contains("bogus")
        );
    }

    #[test]
    fn magic_decodes() {
        let d = FormatDefinition::parse(
            "name='elf'\nprocessor='markdown'\nmagic=[{hex='7f454c46'}]",
            known,
        )
        .unwrap();
        assert_eq!(d.magic_bytes(), vec![(0, vec![0x7f, b'E', b'L', b'F'])]);
    }
}
