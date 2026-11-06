//! MP4 files and the ISO base media family (M6 spec §9): `MP4 M4A › moov › udta › meta › ilst
//! › ©nam`, the box path, the image named by the major brand. Box types are four bytes shown as
//! Latin-1. The walk descends only into the container boxes, `meta` with its 4-byte header;
//! `mdat` and every other box are sections with nothing inside. The `data` boxes under an
//! `ilst` item carry its value and are the item's: the Trail ends at the item.
use super::layouts::{Builder, latin1, sub, u32_at, u64_at};
use std::borrow::Cow;
use std::ops::Range;

const CONTAINERS: &[&[u8; 4]] = &[
    b"moov", b"trak", b"mdia", b"minf", b"stbl", b"udta", b"edts", b"dinf", b"mvex", b"moof",
    b"traf", b"mfra", b"tref", b"meta", b"ilst", b"iprp", b"ipco",
];

/// None when the file has no `ftyp` box at its start that fits in it.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let bytes = b.bytes;
    let all = 0..bytes.len();
    if bytes.get(4..8) != Some(b"ftyp") {
        return None;
    }
    let ftyp = u32_at(bytes, 0, false)? as usize;
    if ftyp < 16 {
        return None;
    }
    sub(&all, 0, ftyp)?;
    let brand = bytes[8..12].trim_ascii_end();
    let brand = &brand[..brand.iter().position(|&c| c == 0).unwrap_or(brand.len())];
    let image = b.node(
        None,
        "object",
        format!("MP4 {}", latin1(brand)).into(),
        all.clone(),
    );
    // (the bytes holding boxes, their parent node, whether they are ilst items)
    let mut levels: Vec<(Range<usize>, usize, bool)> = vec![(all, image, false)];
    while let Some((range, parent, items)) = levels.pop() {
        let mut at = range.start;
        while at + 8 <= range.end {
            let Some(size) = u32_at(bytes, at, false) else {
                break;
            };
            let (header, size) = match size {
                0 => (8, range.end - at),
                1 => match u64_at(bytes, at + 8, false).and_then(|n| usize::try_from(n).ok()) {
                    Some(n) => (16, n),
                    None => break,
                },
                n => (8, n as usize),
            };
            if size < header {
                break;
            }
            let Some(boxed) = sub(&range, at, size) else {
                break;
            };
            let kind: &[u8; 4] = bytes[at + 4..at + 8].try_into().unwrap_or(&[0; 4]);
            let name: Cow<str> = latin1(kind);
            let node = b.node(Some(parent), "section", name, boxed.clone());
            // A Match in the box's own name is outside it (M6 spec §3 rule 5).
            b.own(parent, at + 4..at + 8);
            let mut inner = boxed.start + header;
            if kind == b"meta" && bytes.get(inner + 4..inner + 8) != Some(b"hdlr") {
                // ISO's meta is a full box (version and flags); QuickTime's has no such header.
                inner += 4;
            }
            if !items && inner <= boxed.end {
                if kind == b"ilst" {
                    levels.push((inner..boxed.end, node, true));
                } else if CONTAINERS.contains(&kind) {
                    levels.push((inner..boxed.end, node, false));
                }
            }
            at = boxed.end;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::super::layouts::BinaryMap;
    use crate::trail::LineIndex;

    /// A 64-bit size, a size of 0 (to the end of its parent), and `mdat` with nothing inside.
    #[test]
    fn large_and_open_ended_boxes() {
        let mut b = b"\0\0\0\x10ftypheic\0\0\0\0".to_vec();
        b.extend(b"\0\0\0\x01mdat");
        b.extend(24u64.to_be_bytes());
        b.extend(b"moovneed");
        b.extend(b"\0\0\0\0udta");
        b.extend(b"needle");
        let m = BinaryMap::parse(&b, "mp4").unwrap();
        let lines = LineIndex::new(&b);
        let names = |at| {
            m.trail(at, &lines)
                .into_iter()
                .map(|s| s.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(32), ["MP4 heic", "mdat"]);
        assert_eq!(names(b.len() - 1), ["MP4 heic", "udta"]);
    }
}
