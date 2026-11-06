//! PNG files (M6 spec §9): `PNG › tEXt Comment` (the chunk type and keyword),
//! `PNG › iTXt XML:com.adobe.xmp › …`, `PNG › eXIf › IFD0 › Artist`. A walk over the chunks'
//! lengths; compressed text (`zTXt`, a compressed `iTXt`) is never inflated, and no CRC or
//! pixel is checked: the metadata is read, not the image.
use super::layouts::{Builder, latin1, sub, u32_at};
use std::borrow::Cow;

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// None when the file has no PNG signature followed by an IHDR chunk that fits in it. Past
/// it, the walk stops at the first chunk that does not fit, keeping those before it.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let bytes = b.bytes;
    let all = 0..bytes.len();
    if !bytes.starts_with(SIGNATURE) || bytes.get(12..16) != Some(b"IHDR") {
        return None;
    }
    let mut chunks = Vec::new();
    let mut at = SIGNATURE.len();
    while let Some(len) = u32_at(bytes, at, false).filter(|&n| n <= i32::MAX as u32) {
        let Some(chunk) = sub(&all, at, 12 + len as usize) else {
            break;
        };
        let kind = &bytes[at + 4..at + 8];
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            break;
        }
        chunks.push(chunk.clone());
        at = chunk.end;
        if kind == b"IEND" {
            break;
        }
    }
    if chunks.is_empty() {
        return None;
    }
    let image = b.node(None, "object", "PNG".into(), all);
    for chunk in chunks {
        let kind = &bytes[chunk.start + 4..chunk.start + 8];
        let data = chunk.start + 8..chunk.end - 4;
        let payload = &bytes[data.clone()];
        // The keyword of the text chunks: Latin-1, up to its NUL.
        let keyword = memchr::memchr(0, payload).map(|k| &payload[..k]);
        let name: Cow<str> = match (kind, keyword) {
            (b"tEXt" | b"zTXt" | b"iTXt", Some(k)) => {
                format!("{} {}", latin1(kind), latin1(k)).into()
            }
            _ => latin1(kind),
        };
        let node = b.node(Some(image), "section", name, chunk.clone());
        // A Match in the chunk's own name, its type or its keyword, is outside it (M6 spec §3
        // rule 5).
        b.own(image, chunk.start + 4..chunk.start + 8);
        if matches!(kind, b"tEXt" | b"zTXt" | b"iTXt")
            && let Some(k) = keyword
        {
            b.own(image, data.start..data.start + k.len());
        }
        match kind {
            b"eXIf" => {
                let skip = if payload.starts_with(b"Exif\0\0") {
                    6
                } else {
                    0
                };
                super::tiff::read(b, data.start + skip..data.end, node);
            }
            b"iTXt" if keyword == Some(b"XML:com.adobe.xmp") => {
                // keyword NUL, compression flag and method, language NUL, translated keyword
                // NUL, then the text: XML only when it is not compressed.
                let k = keyword.map_or(0, <[u8]>::len) + 1;
                if payload.get(k) == Some(&0)
                    && let Some(lang) = payload.get(k + 2..).and_then(|r| memchr::memchr(0, r))
                    && let Some(tr) = payload
                        .get(k + 3 + lang..)
                        .and_then(|r| memchr::memchr(0, r))
                {
                    let text = data.start + k + 3 + lang + tr + 1;
                    super::xmp::read(b, text..data.end, node);
                }
            }
            _ => {}
        }
    }
    Some(())
}
