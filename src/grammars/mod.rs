//! Grammar Helpers on disk (spec §7): where they live, the index, install and list.
pub mod index;
pub mod install;

use std::path::{Path, PathBuf};

/// `SEARCHFOR_GRAMMARS_DIR` if set (tests), else `<data dir>/searchfor/grammars`.
pub fn grammars_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("SEARCHFOR_GRAMMARS_DIR") {
        return Some(PathBuf::from(d));
    }
    dirs::data_dir().map(|d| d.join("searchfor").join("grammars"))
}

/// `SEARCHFOR_GRAMMAR_INDEX` if set, else the index in the repository.
pub fn index_url() -> String {
    std::env::var("SEARCHFOR_GRAMMAR_INDEX").unwrap_or_else(|_| index::DEFAULT_INDEX.to_string())
}

pub fn binary_name(lang: &str) -> String {
    format!("sfor-grammar-{lang}{}", std::env::consts::EXE_SUFFIX)
}

/// An installed Grammar Helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    pub binary: PathBuf,
    pub definition: PathBuf,
}

/// The installed Grammar Helpers under `dir`, sorted by name; a broken one is skipped. Names
/// that `valid_name` refuses (locks, staging and backup directories start with `.`) never list.
pub fn installed(dir: &Path) -> Vec<Installed> {
    let mut out: Vec<Installed> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            if !index::valid_name(&name) {
                return None;
            }
            let d = e.path();
            let meta: serde_json::Value =
                serde_json::from_slice(&std::fs::read(d.join("installed.json")).ok()?).ok()?;
            let binary = d.join(binary_name(&name));
            let definition = d.join(format!("{name}.toml"));
            (binary.is_file() && definition.is_file()).then(|| Installed {
                version: meta["version"].as_str().unwrap_or("?").to_string(),
                name,
                dir: d.clone(),
                binary,
                definition,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn installed_binary(dir: &Path, lang: &str) -> Option<PathBuf> {
    installed(dir)
        .into_iter()
        .find(|i| i.name == lang)
        .map(|i| i.binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A complete install of `name` under `dir`, as `install` writes it.
    fn put(dir: &Path, name: &str, version: &str) {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(binary_name(name)), b"bin").unwrap();
        std::fs::write(d.join(format!("{name}.toml")), b"name = 'x'").unwrap();
        std::fs::write(
            d.join("installed.json"),
            serde_json::json!({ "version": version }).to_string(),
        )
        .unwrap();
    }

    #[test]
    fn installed_lists_complete_installs_by_name_and_skips_the_rest() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "zig", "1.0.0");
        put(t.path(), "lua", "0.2.0");
        // A staging directory, an invalid name, a missing binary and a missing installed.json.
        put(t.path(), ".lua.part-abc", "9");
        put(t.path(), "Bad-Name", "9");
        put(t.path(), "nobin", "9");
        std::fs::remove_file(t.path().join("nobin").join(binary_name("nobin"))).unwrap();
        put(t.path(), "nometa", "9");
        std::fs::remove_file(t.path().join("nometa").join("installed.json")).unwrap();
        std::fs::write(t.path().join(".lua.lock"), b"").unwrap();

        let got = installed(t.path());
        let names: Vec<_> = got
            .iter()
            .map(|i| (i.name.as_str(), i.version.as_str()))
            .collect();
        assert_eq!(names, [("lua", "0.2.0"), ("zig", "1.0.0")]);
        assert_eq!(got[0].dir, t.path().join("lua"));
        assert_eq!(got[0].binary, t.path().join("lua").join(binary_name("lua")));
        assert_eq!(got[0].definition, t.path().join("lua").join("lua.toml"));
        assert_eq!(
            installed_binary(t.path(), "zig"),
            Some(got[1].binary.clone())
        );
        assert_eq!(installed_binary(t.path(), "nobin"), None);
    }

    #[test]
    fn a_missing_directory_has_nothing_installed() {
        let t = tempfile::tempdir().unwrap();
        assert!(installed(&t.path().join("absent")).is_empty());
    }

    #[test]
    fn the_binary_name_carries_the_platform_suffix() {
        assert_eq!(
            binary_name("lua"),
            format!("sfor-grammar-lua{}", std::env::consts::EXE_SUFFIX)
        );
    }
}
