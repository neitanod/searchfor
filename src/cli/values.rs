//! Parsers of the values of ripgrep's flags that clap cannot check by itself, with rg 14.1.1's
//! rules (crates/core/flags/defs.rs, `convert` and the `update` of each flag). A value that does
//! not parse is a usage error, exit 2, with clap's message (rg-compat.md, «Usage errors»).

/// Bytes a flag gives, kept as a type of their own: clap reads a `Vec<u8>` field as a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

/// `--path-separator`: one byte, or none (an empty value, which rg treats as no separator).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathSeparator(pub Option<u8>);

/// rg's sizes: digits and an optional `K`, `M` or `G` (grep-cli's `parse_human_readable_size`).
pub fn human_u64(s: &str) -> Result<u64, String> {
    grep_cli::parse_human_readable_size(s).map_err(|e| format!("invalid size: {e}"))
}

/// `human_u64` for a limit that must fit a `usize` (`--dfa-size-limit`, `--regex-size-limit`).
pub fn human_usize(s: &str) -> Result<usize, String> {
    usize::try_from(human_u64(s)?).map_err(|_| "size is too big".to_string())
}

/// A separator with rg's escapes (`\t`, `\n`, `\x7F`, …), as bstr's `unescape_bytes` reads them.
pub fn separator(s: &str) -> Result<Bytes, String> {
    Ok(Bytes(<Vec<u8> as bstr::ByteVec>::unescape_bytes(s)))
}

/// `--path-separator`: exactly one byte once unescaped, or empty (rg 14.1.1's message).
pub fn path_separator(s: &str) -> Result<PathSeparator, String> {
    let raw = <Vec<u8> as bstr::ByteVec>::unescape_bytes(s);
    match raw.len() {
        0 => Ok(PathSeparator(None)),
        1 => Ok(PathSeparator(Some(raw[0]))),
        len => Err(format!(
            "A path separator must be exactly one byte, but the given separator is {len} \
             bytes: {s}\nIn some shells on Windows '/' is automatically expanded. Use '//' \
             instead."
        )),
    }
}

/// `-E`: `auto`, `none`, or a label the WHATWG Encoding Standard knows (grep-searcher's
/// `Encoding::new`, which `encoding_rs` backs).
pub fn encoding(s: &str) -> Result<String, String> {
    if s == "auto" || s == "none" {
        return Ok(s.to_string());
    }
    grep_searcher::Encoding::new(s)
        .map(|_| s.to_string())
        .map_err(|e| e.to_string())
}

/// `--colors`: a spec grep-printer reads (`{type}:{attribute}:{value}`), checked here and kept
/// as written; s4 also takes the type `trail` (M5a plan, D11), which grep-printer does not know.
pub fn color_spec(s: &str) -> Result<String, String> {
    if let Some(rest) = s.strip_prefix("trail:") {
        // Checked as the same spec for a type grep-printer knows.
        return format!("path:{rest}")
            .parse::<grep_printer::UserColorSpec>()
            .map(|_| s.to_string())
            .map_err(|e| e.to_string());
    }
    s.parse::<grep_printer::UserColorSpec>()
        .map(|_| s.to_string())
        .map_err(|e| e.to_string())
}

/// `--hyperlink-format`: a format or an alias grep-printer knows (`default`, `vscode`, …),
/// kept as written.
pub fn hyperlink_format(s: &str) -> Result<String, String> {
    s.parse::<grep_printer::HyperlinkFormat>()
        .map(|_| s.to_string())
        .map_err(|e| format!("invalid hyperlink format: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_as_ripgrep() {
        assert_eq!(human_u64("123"), Ok(123));
        assert_eq!(human_u64("2K"), Ok(2048));
        assert_eq!(human_u64("3M"), Ok(3 << 20));
        assert_eq!(human_u64("1G"), Ok(1 << 30));
        assert!(human_u64("1k").is_err());
        assert!(human_u64("").is_err());
        assert!(human_u64("K").is_err());
        assert!(human_u64("99999999999999999999G").is_err());
    }

    #[test]
    fn separators_read_escapes() {
        assert_eq!(separator("--"), Ok(Bytes(b"--".to_vec())));
        assert_eq!(separator(r"\t\x00"), Ok(Bytes(b"\t\0".to_vec())));
        assert_eq!(separator(""), Ok(Bytes(Vec::new())));
    }

    #[test]
    fn path_separator_is_one_byte_or_none() {
        assert_eq!(path_separator("/"), Ok(PathSeparator(Some(b'/'))));
        assert_eq!(path_separator(r"\\"), Ok(PathSeparator(Some(b'\\'))));
        assert_eq!(path_separator(""), Ok(PathSeparator(None)));
        let e = path_separator("ab").unwrap_err();
        assert!(e.starts_with("A path separator must be exactly one byte, but the given"));
        assert!(e.contains("is 2 bytes: ab"));
    }

    #[test]
    fn encodings_are_whatwg_labels() {
        assert_eq!(encoding("utf-16le").as_deref(), Ok("utf-16le"));
        assert_eq!(encoding("latin1").as_deref(), Ok("latin1"));
        assert_eq!(encoding("auto").as_deref(), Ok("auto"));
        assert_eq!(encoding("none").as_deref(), Ok("none"));
        assert!(encoding("klingon").is_err());
    }

    #[test]
    fn color_specs_and_the_trail_type() {
        assert!(color_spec("match:fg:blue").is_ok());
        assert!(color_spec("path:none").is_ok());
        assert!(color_spec("line:style:bold").is_ok());
        assert!(color_spec("trail:fg:yellow").is_ok());
        assert!(color_spec("nope:fg:blue").is_err());
        assert!(color_spec("match:fg:nocolor").is_err());
    }

    #[test]
    fn hyperlink_formats_and_aliases() {
        assert!(hyperlink_format("default").is_ok());
        assert!(hyperlink_format("vscode").is_ok());
        assert!(hyperlink_format("file://{host}{path}:{line}").is_ok());
        assert!(hyperlink_format("{nope}").is_err());
    }
}
