//! WebP files (M6 spec §9): `WebP › EXIF › IFD0 › Artist`, `WebP › XMP › …`. A walk over the
//! RIFF chunks inside the RIFF size and the file, each padded to an even length; the EXIF and
//! XMP payloads go to the shared TIFF and XMP readers.
use super::layouts::{Builder, latin1, sub, u32_at};

/// None when the file is no RIFF `WEBP` with at least one chunk that fits in it.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let bytes = b.bytes;
    if !bytes.starts_with(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
        return None;
    }
    let size = u32_at(bytes, 4, true)? as usize;
    let body = 0..bytes.len().min(size.saturating_add(8));
    let mut chunks = Vec::new();
    let mut at = 12;
    while let Some(len) = u32_at(bytes, at + 4, true) {
        let Some(chunk) = at
            .checked_add(8)
            .and_then(|_| sub(&body, at, 8 + len as usize))
        else {
            break;
        };
        chunks.push(chunk.clone());
        at = chunk.end + (len as usize & 1);
    }
    if chunks.is_empty() {
        return None;
    }
    let image = b.node(None, "object", "WebP".into(), 0..bytes.len());
    for chunk in chunks {
        let kind = &bytes[chunk.start..chunk.start + 4];
        let data = chunk.start + 8..chunk.end;
        let name = latin1(kind.trim_ascii_end());
        let node = b.node(Some(image), "section", name, chunk.clone());
        match kind {
            b"EXIF" => {
                let skip = if bytes[data.clone()].starts_with(b"Exif\0\0") {
                    6
                } else {
                    0
                };
                super::tiff::read(b, data.start + skip..data.end, node);
            }
            b"XMP " => super::xmp::read(b, data, node),
            _ => {}
        }
    }
    Some(())
}
