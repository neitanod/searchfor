//! The Grammar index (spec §7): what `--install-grammar` can install, per system.
use serde::Deserialize;
use std::collections::BTreeMap;

pub const INDEX_VERSION: u32 = 1;
/// Replaced by `SEARCHFOR_GRAMMAR_INDEX` (spec §7).
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

/// D30: Grammar names become directory and file names: runs of lowercase letters and digits,
/// joined by single `_` or `-` (`c-sharp`, `old_name`; M5b plan, D17). No dot, separator or
/// leading or trailing `-`/`_` can make one a path.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.split(['_', '-']).all(|run| {
            !run.is_empty()
                && run
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
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
                "the Grammar index is version {}; this s4 reads version {INDEX_VERSION}",
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

/// The local path of a `file://` URL. Accepts the unescaped local paths tests and older
/// indexes write (`file:///tmp/a b`, `file://C:\dir\i.json`) as well as standard URLs, whose
/// path is percent-decoded once: an empty or `localhost` host, `/C:/…` for a Windows drive, and
/// on Windows another host as a UNC share. Unix keeps the decoded bytes as they are; Windows
/// needs them to be UTF-8. A malformed escape, a NUL or a host on Unix is refused.
fn file_url_path(url: &str) -> anyhow::Result<std::path::PathBuf> {
    let bad = |why: &str| anyhow::anyhow!("{url}: {why}");
    let rest = url
        .strip_prefix("file://")
        .ok_or_else(|| bad("not a file:// URL"))?;
    let b = rest.as_bytes();
    // An unescaped Windows path right after `file://`: a drive, no host.
    let drive = b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && b.get(2).is_none_or(|c| *c == b'/' || *c == b'\\');
    let (host, path) = if rest.starts_with('/') || drive {
        ("", rest)
    } else {
        rest.split_at(rest.find('/').unwrap_or(rest.len()))
    };
    let mut bytes = Vec::with_capacity(path.len());
    let mut it = path.bytes();
    while let Some(c) = it.next() {
        if c != b'%' {
            bytes.push(c);
            continue;
        }
        let hex = |c: Option<u8>| c.and_then(|c| (c as char).to_digit(16));
        match (hex(it.next()), hex(it.next())) {
            (Some(h), Some(l)) => bytes.push((h * 16 + l) as u8),
            _ => return Err(bad("a `%` that is not followed by two hex digits")),
        }
    }
    if bytes.contains(&0) {
        return Err(bad("the path holds a NUL"));
    }
    let local = host.is_empty() || host.eq_ignore_ascii_case("localhost");
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        if !local {
            return Err(bad("a file:// URL with a host other than localhost"));
        }
        Ok(std::ffi::OsString::from_vec(bytes).into())
    }
    #[cfg(not(unix))]
    {
        let path = String::from_utf8(bytes).map_err(|_| bad("the path is not UTF-8"))?;
        let path = path.replace('/', "\\");
        if !local {
            return Ok(format!(r"\\{host}{path}").into());
        }
        // `/C:/x` is the drive path `C:/x`.
        let p = path.as_bytes();
        let drive = p.len() >= 3 && p[0] == b'\\' && p[1].is_ascii_alphabetic() && p[2] == b':';
        Ok(if drive { &path[1..] } else { &path[..] }.into())
    }
}

/// The bytes at `url`: `https://` through `agent()`, `file://` from disk; anything else is
/// refused.
pub fn fetch(url: &str) -> anyhow::Result<Vec<u8>> {
    if url.starts_with("file://") {
        let path = file_url_path(url)?;
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
    use std::path::PathBuf;

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
        for ok in ["lua", "c_sharp", "x86asm", "a-b"] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "Lua", "../x", "a/b", "a.b", "a--b", "a-_b"] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn grammar_identifiers_include_c_sharp_without_traversal() {
        for name in ["c", "cpp", "java", "ruby", "c-sharp", "elixir", "old_name"] {
            assert!(valid_name(name), "{name}");
        }
        for name in [
            "", ".", "..", "../c", "c/sharp", "c\\sharp", "-c", "c-", "C",
        ] {
            assert!(!valid_name(name), "{name}");
        }
    }

    #[test]
    fn file_urls_decode_spaces_and_percent_once() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a b%20");
        std::fs::write(&p, b"local index").unwrap();
        let raw = p.to_str().unwrap().replace('\\', "/");
        let raw = if cfg!(windows) {
            format!("/{raw}")
        } else {
            raw
        };
        let url = format!("file://{}", raw.replace('%', "%25").replace(' ', "%20"));
        assert_eq!(fetch(&url).unwrap(), b"local index");
        assert!(file_url_path("file:///x%00y").is_err());
        assert!(file_url_path("file:///x%Q0").is_err());
    }

    #[test]
    fn index_schema_accepts_required_c_sharp_identity() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../docs/grammar-index.schema.json")).unwrap();
        let doc = serde_json::json!({"version":1,"grammars":[{
            "name":"c-sharp","version":"0.23.1","source":"https://example.org/csharp",
            "license":"MIT","definition":"name = 'c-sharp'",
            "binaries":{"windows-x86_64":{"url":"file:///C:/g.exe",
                "sha256":"a".repeat(64)}}}]});
        assert!(jsonschema::is_valid(&schema, &doc));
    }

    /// The forms of `file://` URL an index may hold: an unescaped local path (what the tests
    /// and older indexes write), `localhost`, a percent-encoded path, and on Windows a drive
    /// (`/C:/…`) or a UNC host. A host elsewhere, NUL and a broken escape are refused.
    #[test]
    fn file_url_forms() {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let p = |u: &str| file_url_path(u).unwrap();
            assert_eq!(p("file:///tmp/a b"), PathBuf::from("/tmp/a b"));
            assert_eq!(p("file://localhost/tmp/x"), PathBuf::from("/tmp/x"));
            assert_eq!(p("file:///tmp/%C3%A9%2Fx"), PathBuf::from("/tmp/é/x"));
            // Bytes that are not UTF-8 survive on Unix.
            assert_eq!(p("file:///tmp/%FF").as_os_str().as_bytes(), b"/tmp/\xff");
            assert!(file_url_path("file://server/share/x").is_err());
        }
        #[cfg(windows)]
        {
            let p = |u: &str| file_url_path(u).unwrap();
            assert_eq!(
                p("file:///C:/a%20b/i.json"),
                PathBuf::from(r"C:\a b\i.json")
            );
            assert_eq!(p("file://localhost/C:/x"), PathBuf::from(r"C:\x"));
            assert_eq!(
                p(r"file://C:\Users\x y\i.json"),
                PathBuf::from(r"C:\Users\x y\i.json")
            );
            assert_eq!(
                p("file://server/share/i.json"),
                PathBuf::from(r"\\server\share\i.json")
            );
            assert!(file_url_path("file:///C:/%FF").is_err());
        }
        assert!(file_url_path("file:///x%0").is_err());
        assert!(file_url_path("file:///x\0y").is_err());
        assert!(file_url_path("https://example.org/x").is_err());
    }
}
