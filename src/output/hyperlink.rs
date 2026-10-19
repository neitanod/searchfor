//! `--hyperlink-format` in the Trail layouts (M5a plan, D12): a path, and with it the line and
//! column numbers of a row, as an OSC 8 link on a terminal with colour, the URL built as
//! grep-printer 0.2.2 builds it (hyperlink.rs): `{path}` the file's canonical path,
//! percent-encoded, `{host}` the hostname (`--hostname-bin`, else the system's), `{wslprefix}`
//! `wsl$/<distro>` under WSL, `{line}` and `{column}` 1 when the row has none.
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(Vec<u8>),
    Host,
    WslPrefix,
    Path,
    Line,
    Column,
}

/// The run's hyperlink format with the values it reads from the system.
#[derive(Debug)]
pub struct Hyperlinks {
    parts: Vec<Part>,
    host: Option<String>,
    wsl_prefix: Option<String>,
    /// The format names `{line}` or `{column}`: a row's numbers go inside its link.
    line_dependent: bool,
}

impl Hyperlinks {
    /// None when the format is empty (`none`, or no `--hyperlink-format`): no links. `format`
    /// is one `--hyperlink-format` accepted (`cli::values::hyperlink_format`).
    pub fn new(format: &str, hostname_bin: Option<&Path>) -> Option<Hyperlinks> {
        let parts = parse(&expand_alias(format)?)?;
        if parts.is_empty() {
            return None;
        }
        let line_dependent = parts.iter().any(|p| matches!(p, Part::Line | Part::Column));
        Some(Hyperlinks {
            host: parts
                .contains(&Part::Host)
                .then(|| hostname(hostname_bin))
                .flatten(),
            wsl_prefix: parts.contains(&Part::WslPrefix).then(wsl_prefix).flatten(),
            parts,
            line_dependent,
        })
    }

    /// Whether a row's line and column go in its link (grep-printer's `is_line_dependent`).
    pub fn line_dependent(&self) -> bool {
        self.line_dependent
    }

    /// The URL of `path` at `line` and `column`, or None when the path cannot be made absolute.
    pub fn url(&self, path: &Path, line: Option<u64>, column: Option<u64>) -> Option<Vec<u8>> {
        let encoded = encode_path(path)?;
        let mut url = Vec::new();
        for part in &self.parts {
            match part {
                Part::Text(t) => url.extend_from_slice(t),
                Part::Host => url.extend_from_slice(self.host.as_deref().unwrap_or("").as_bytes()),
                Part::WslPrefix => {
                    url.extend_from_slice(self.wsl_prefix.as_deref().unwrap_or("").as_bytes())
                }
                Part::Path => url.extend_from_slice(&encoded),
                Part::Line => url.extend_from_slice(line.unwrap_or(1).to_string().as_bytes()),
                Part::Column => url.extend_from_slice(column.unwrap_or(1).to_string().as_bytes()),
            }
        }
        Some(url)
    }
}

/// An alias (`default`, `vscode`, `none`, …) as the format grep-printer gives it; a format of
/// the user's as it is. An alias has no brace, and its format no literal brace, so grep-printer's
/// display of it reads back the same.
fn expand_alias(format: &str) -> Option<String> {
    let parsed: grep_printer::HyperlinkFormat = format.parse().ok()?;
    Some(if format.contains('{') {
        format.to_string()
    } else {
        parsed.to_string()
    })
}

/// grep-printer's `HyperlinkFormat::from_str`: `{name}` a variable, `{{` and `}}` a brace. A
/// format must have `{path}` (which grep-printer checked already: `values::hyperlink_format`).
fn parse(s: &str) -> Option<Vec<Part>> {
    let mut parts: Vec<Part> = Vec::new();
    let mut text = Vec::new();
    let mut chars = s.chars().peekable();
    let flush = |text: &mut Vec<u8>, parts: &mut Vec<Part>| {
        if !text.is_empty() {
            parts.push(Part::Text(std::mem::take(text)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                text.push(b'{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                text.push(b'}');
            }
            '{' => {
                let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                flush(&mut text, &mut parts);
                parts.push(match name.as_str() {
                    "host" => Part::Host,
                    "wslprefix" => Part::WslPrefix,
                    "path" => Part::Path,
                    "line" => Part::Line,
                    "column" => Part::Column,
                    _ => return None,
                });
            }
            c => {
                let mut b = [0; 4];
                text.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            }
        }
    }
    flush(&mut text, &mut parts);
    Some(parts)
}

/// grep-printer's `HyperlinkPath::from_path` on Unix: the canonical path, which must start with
/// `/`, percent-encoded but for RFC 3986's unreserved characters, `/`, `:` and non-ASCII bytes.
#[cfg(unix)]
fn encode_path(path: &Path) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;
    let path = path.canonicalize().ok()?;
    let bytes = path.as_os_str().as_bytes();
    if !bytes.starts_with(b"/") {
        return None;
    }
    Some(encode(bytes))
}

/// On Windows (grep-printer's `from_path`): `\\?\C:\dir` becomes `/C:/dir` and `\\?\UNC\server\x`
/// `//server/x`, separators as `/`.
#[cfg(not(unix))]
fn encode_path(path: &Path) -> Option<Vec<u8>> {
    let path = path.canonicalize().ok()?;
    let s = path.to_str()?;
    let s = if let Some(unc) = s.strip_prefix(r"\\?\UNC\") {
        format!("//{unc}")
    } else if let Some(local) = s.strip_prefix(r"\\?\") {
        format!("/{local}")
    } else {
        return None;
    };
    Some(encode(s.as_bytes()))
}

fn encode(input: &[u8]) -> Vec<u8> {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut out = Vec::with_capacity(input.len());
    for &b in input {
        match b {
            b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
            | b'/'
            | b':'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | 128.. => out.push(b),
            b'\\' if cfg!(windows) => out.push(b'/'),
            _ => out.extend_from_slice(&[b'%', HEX[(b >> 4) as usize], HEX[(b & 0xF) as usize]]),
        }
    }
    out
}

/// rg 14.1.1's `hostname`: the output of `--hostname-bin`, trimmed, when it runs and prints
/// something; else the system's.
fn hostname(bin: Option<&Path>) -> Option<String> {
    if let Some(bin) = bin
        && let Ok(bin) = grep_cli::resolve_binary(bin)
    {
        let mut cmd = std::process::Command::new(&bin);
        cmd.stdin(std::process::Stdio::null());
        if let Ok(rdr) = grep_cli::CommandReader::new(&mut cmd)
            && let Ok(out) = std::io::read_to_string(rdr)
            && !out.trim().is_empty()
        {
            return Some(out.trim().to_string());
        }
    }
    grep_cli::hostname().ok()?.to_str().map(str::to_string)
}

/// rg 14.1.1's `wsl_prefix`: `wsl$/<distro>` on Unix under WSL.
fn wsl_prefix() -> Option<String> {
    if !cfg!(unix) {
        return None;
    }
    let distro = std::env::var_os("WSL_DISTRO_NAME")?;
    Some(format!("wsl$/{}", distro.to_str()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_aliases_and_braces() {
        assert!(Hyperlinks::new("", None).is_none());
        assert!(Hyperlinks::new("none", None).is_none());
        let f = Hyperlinks::new("vscode", None).unwrap();
        assert!(f.line_dependent());
        let f = Hyperlinks::new("file://x{{{path}}}", None).unwrap();
        assert!(!f.line_dependent());
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a b.txt");
        std::fs::write(&p, "x").unwrap();
        let url = String::from_utf8(f.url(&p, None, None).unwrap()).unwrap();
        assert!(
            url.starts_with("file://x{/") && url.ends_with("/a%20b.txt}"),
            "{url}"
        );
    }

    #[test]
    fn line_and_column_default_to_one() {
        let f = Hyperlinks::new("file://{path}:{line}:{column}", None).unwrap();
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("f");
        std::fs::write(&p, "x").unwrap();
        let canon = p.canonicalize().unwrap();
        let url = String::from_utf8(f.url(&p, Some(7), None).unwrap()).unwrap();
        assert_eq!(url, format!("file://{}:7:1", canon.display()));
        assert!(f.url(&d.path().join("missing"), None, None).is_none());
    }

    #[test]
    fn the_hostname_comes_from_hostname_bin() {
        let f = Hyperlinks::new("file://{host}{path}", Some(Path::new("true"))).unwrap();
        // `true` prints nothing: the system's hostname.
        assert_eq!(
            f.host,
            grep_cli::hostname().ok().and_then(|h| h.into_string().ok())
        );
        assert!(
            Hyperlinks::new("file://{path}", None)
                .unwrap()
                .host
                .is_none()
        );
    }
}
