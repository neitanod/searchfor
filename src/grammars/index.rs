//! The Grammar index (spec §7): what `--install-grammar` can install, per system.
use serde::Deserialize;
use std::collections::BTreeMap;

pub const INDEX_VERSION: u32 = 1;
/// Replaced by `SFOR_GRAMMAR_INDEX` (spec §7).
pub const DEFAULT_INDEX: &str =
    "https://raw.githubusercontent.com/neitanod/searchfor/main/grammars/index.json";

#[derive(Debug, Clone, Deserialize)]
pub struct Index {
    pub version: u32,
    pub grammars: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Entry {
    pub name: String,
    pub version: String,
    pub source: String,
    pub license: String,
    /// The Format Definition installed with the Grammar Helper.
    pub definition: String,
    /// By `system()`.
    pub binaries: BTreeMap<String, Binary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Binary {
    pub url: String,
    pub sha256: String,
}

/// D30: Grammar names become directory and file names.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// D14: `linux-x86_64`, `macos-aarch64`, `windows-x86_64`, …
pub fn system() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

impl Index {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let i: Index = serde_json::from_slice(bytes)?;
        if i.version != INDEX_VERSION {
            anyhow::bail!(
                "the Grammar index is version {}; this sfor reads version {INDEX_VERSION}",
                i.version
            );
        }
        if let Some(e) = i.grammars.iter().find(|e| !valid_name(&e.name)) {
            anyhow::bail!("the Grammar index has an invalid name `{}`", e.name);
        }
        Ok(i)
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.grammars.iter().find(|e| e.name == name)
    }
}

/// The HTTP client of `--install-grammar` and `--list-grammars` (D38): HTTPS on every hop,
/// redirects included, a 10 s connect timeout and a 300 s ceiling for the whole download.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(std::time::Duration::from_secs(10)))
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .build()
        .into()
}

/// The bytes at `url`: `https://` through `agent()`, `file://` from disk; anything else is
/// refused.
pub fn fetch(url: &str) -> anyhow::Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        return std::fs::read(path).map_err(|e| anyhow::anyhow!("{url}: {e}"));
    }
    if !url.starts_with("https://") {
        anyhow::bail!("{url}: only https:// and file:// URLs are accepted");
    }
    let mut r = agent()
        .get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("{url}: {e}"))?;
    // ureq's default body limit is 10 MB; a Grammar Helper may be larger.
    r.body_mut()
        .with_config()
        .limit(256 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| anyhow::anyhow!("{url}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{"version":1,"grammars":[{"name":"lua","version":"0.2.0","source":"https://example.org/lua","license":"MIT","definition":"name = 'lua'\nprocessor = 'treesitter'\n","binaries":{"linux-x86_64":{"url":"file:///tmp/x","sha256":"00"}},"future_field":1}]}"#;

    #[test]
    fn parses_and_ignores_unknown_fields() {
        let i = Index::parse(INDEX.as_bytes()).unwrap();
        assert_eq!(i.grammars[0].name, "lua");
        assert_eq!(i.grammars[0].binaries["linux-x86_64"].sha256, "00");
    }

    #[test]
    fn refuses_another_version_and_bad_names() {
        assert!(
            Index::parse(br#"{"version":2,"grammars":[]}"#)
                .unwrap_err()
                .to_string()
                .contains("version")
        );
        let bad = INDEX.replace("\"name\":\"lua\"", "\"name\":\"../lua\"");
        assert!(
            Index::parse(bad.as_bytes())
                .unwrap_err()
                .to_string()
                .contains("../lua")
        );
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn the_system_key_is_os_dash_arch() {
        assert_eq!(system(), "linux-x86_64");
    }

    #[test]
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn the_system_key_on_apple_silicon() {
        assert_eq!(system(), "macos-aarch64");
    }

    #[test]
    fn the_agent_refuses_http_even_after_a_redirect() {
        // D38: `https_only` is checked on every hop (ureq 3.4.2, run.rs:149), before any
        // connection, so this needs no network. A redirect from https:// to http:// reaches
        // the same check; a test against a live redirect is out of reach without a TLS server.
        let e = agent()
            .get("http://127.0.0.1:9/index.json")
            .call()
            .unwrap_err();
        assert!(e.to_string().contains("https only"), "{e}");
    }

    #[test]
    fn the_agent_refuses_http_before_connecting() {
        // A local listener the test owns: the refused request must not even reach it.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.set_nonblocking(true).unwrap();
        let url = format!("http://{}/index.json", l.local_addr().unwrap());
        assert!(agent().get(&url).call().is_err());
        assert!(fetch(&url).is_err());
        let e = l.accept().unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::WouldBlock, "{e}");
    }

    #[test]
    fn fetch_reads_files_and_refuses_plain_http() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("i.json");
        std::fs::write(&p, "hello").unwrap();
        assert_eq!(fetch(&format!("file://{}", p.display())).unwrap(), b"hello");
        assert!(
            fetch("http://example.org/i.json")
                .unwrap_err()
                .to_string()
                .contains("https")
        );
        assert!(fetch("ftp://example.org/i.json").is_err());
    }

    #[test]
    fn names_are_checked() {
        for ok in ["lua", "c_sharp", "x86asm"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "Lua", "../x", "a/b", "a.b", "a-b"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }
}
