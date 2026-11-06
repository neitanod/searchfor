//! JPEG files (M6 spec §9): `JPEG › APP1 Exif › IFD0 › Model`, `JPEG › APP1 XMP › …`,
//! `JPEG › COM`. A walk over the marker segments, their lengths checked against the file;
//! the entropy-coded data after a scan header is skipped to the next marker, never decoded.
use super::layouts::{Builder, sub, u16_at};
use std::borrow::Cow;

/// The identifier of an APP1 segment of Exif and of XMP.
const EXIF: &[u8] = b"Exif\0\0";
const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";

const APP: [&str; 16] = [
    "APP0", "APP1", "APP2", "APP3", "APP4", "APP5", "APP6", "APP7", "APP8", "APP9", "APP10",
    "APP11", "APP12", "APP13", "APP14", "APP15",
];

/// None when the file does not start with SOI and a marker, or its first segment does not
/// fit in it. Past the first segment, the walk stops at the first marker that does not read,
/// keeping the segments before it.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let bytes = b.bytes;
    let all = 0..bytes.len();
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return None;
    }
    let mut segments = Vec::new();
    let mut at = 2;
    while at < bytes.len() {
        if bytes[at] != 0xff {
            break;
        }
        // Fill bytes before a marker.
        while bytes.get(at + 1) == Some(&0xff) {
            at += 1;
        }
        let Some(&m) = bytes.get(at + 1) else { break };
        if m == 0xd9 {
            break;
        }
        // Markers without a length: TEM and the restart markers.
        if m == 0x01 || (0xd0..=0xd7).contains(&m) {
            at += 2;
            continue;
        }
        let Some(seg) = u16_at(bytes, at + 2, false)
            .filter(|&n| n >= 2)
            .and_then(|n| sub(&all, at, 2 + n as usize))
        else {
            break;
        };
        let mut owned = seg.clone();
        if m == 0xda {
            // A scan's entropy-coded data runs to the next marker: an 0xff not followed by a
            // stuffed 0x00 or a restart marker.
            let mut i = seg.end;
            owned.end = loop {
                match memchr::memchr(0xff, &bytes[i..]) {
                    None => break bytes.len(),
                    Some(k) => match bytes.get(i + k + 1) {
                        Some(0x00 | 0xd0..=0xd7) => i += k + 2,
                        _ => break i + k,
                    },
                }
            };
        }
        segments.push((m, seg, owned.end));
        at = owned.end;
    }
    if segments.is_empty() {
        return None;
    }
    let image = b.node(None, "object", "JPEG".into(), all);
    for (m, seg, end) in segments {
        let data = seg.start + 4..seg.end;
        let payload = &bytes[data.clone()];
        let name: Cow<str> = match m {
            0xe1 if payload.starts_with(EXIF) => "APP1 Exif".into(),
            0xe1 if payload.starts_with(XMP) => "APP1 XMP".into(),
            0xe0..=0xef => APP[(m - 0xe0) as usize].into(),
            0xfe => "COM".into(),
            0xc4 => "DHT".into(),
            0xcc => "DAC".into(),
            0xc0..=0xcf if m != 0xc8 => format!("SOF{}", m - 0xc0).into(),
            0xda => "SOS".into(),
            0xdb => "DQT".into(),
            0xdc => "DNL".into(),
            0xdd => "DRI".into(),
            _ => format!("marker 0x{m:02x}").into(),
        };
        let node = b.node(Some(image), "section", name, seg.start..end);
        if m == 0xe1 && payload.starts_with(EXIF) {
            super::tiff::read(b, data.start + EXIF.len()..data.end, node);
        } else if m == 0xe1 && payload.starts_with(XMP) {
            super::xmp::read(b, data.start + XMP.len()..data.end, node);
        }
    }
    Some(())
}
