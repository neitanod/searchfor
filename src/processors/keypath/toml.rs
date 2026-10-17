//! keypath for TOML, over toml_edit (spec §5.2, D36). C3 fills it.

pub fn outline(_src: &[u8]) -> crate::trail::Outline {
    crate::trail::Outline::default()
}
