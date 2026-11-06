//! ID3 tags (M6 spec §9): `ID3v2.4 › TIT2`, `› TXXX MusicBrainz Album Id`, `› COMM`, and
//! `ID3v1 › title` for the 128-byte tag at the end of a file whose ID3v2 tag starts it. Frames
//! are named by their id (v2.2's three characters read the same); their bytes are never
//! transformed, so a frame that is unsynchronised, compressed or encrypted keeps its scope and
//! names no description.
use super::layouts::{Builder, latin1, sub, u16_at, u32_at};
use std::borrow::Cow;

/// A 28-bit syncsafe integer: four bytes of seven bits each; None when a byte has its top bit.
fn syncsafe(b: &[u8], at: usize) -> Option<usize> {
    let v = b.get(at..at.checked_add(4)?)?;
    v.iter()
        .try_fold(0usize, |n, &x| (x < 0x80).then_some(n << 7 | x as usize))
}

/// None when the file does not start with an ID3v2 tag that fits in it.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let bytes = b.bytes;
    let all = 0..bytes.len();
    if !bytes.starts_with(b"ID3") {
        return None;
    }
    let major = *bytes.get(3)?;
    if !(2..=4).contains(&major) || *bytes.get(4)? == 0xff {
        return None;
    }
    let flags = bytes[5];
    let size = syncsafe(bytes, 6)?;
    let footer = if major == 4 && flags & 0x10 != 0 {
        10
    } else {
        0
    };
    let tag = sub(&all, 0, 10 + size + footer)?;
    let frames = 10..10 + size;
    let image = b.node(None, "object", format!("ID3v2.{major}").into(), tag.clone());
    let mut at = 10;
    // v2.2's bit 6 is compression, of the whole tag: its frames are not read.
    if major == 2 && flags & 0x40 != 0 {
        at = frames.end;
    }
    if major >= 3 && flags & 0x40 != 0 {
        let ext = if major == 3 {
            u32_at(bytes, at, false).map(|n| n as usize + 4)
        } else {
            syncsafe(bytes, at)
        };
        at = ext
            .and_then(|n| sub(&frames, at, n))
            .map_or(frames.end, |r| r.end);
    }
    let tag_unsync = flags & 0x80 != 0;
    let (id_len, header) = if major == 2 { (3, 6) } else { (4, 10) };
    while at + header <= frames.end {
        let id = &bytes[at..at + id_len];
        if !id
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            break;
        }
        let len = match major {
            2 => Some(
                (bytes[at + 3] as usize) << 16
                    | (bytes[at + 4] as usize) << 8
                    | bytes[at + 5] as usize,
            ),
            3 => u32_at(bytes, at + 4, false).map(|n| n as usize),
            _ => syncsafe(bytes, at + 4),
        };
        let Some(frame) = len.and_then(|n| sub(&frames, at, header + n)) else {
            break;
        };
        let format = if major == 2 {
            0
        } else {
            u16_at(bytes, at + 8, false).unwrap_or(0) & 0xff
        };
        let opaque = tag_unsync
            || match major {
                3 => format & 0xc0 != 0,
                4 => format & 0x0f != 0,
                _ => false,
            };
        let data = &bytes[at + header..frame.end];
        let id_name = latin1(id);
        let mut described = 0;
        let name: Cow<str> = match id {
            b"TXXX" | b"WXXX" | b"TXX" | b"WXX" if !opaque => match description(data) {
                Some((d, n)) if !d.is_empty() => {
                    described = n;
                    format!("{id_name} {d}").into()
                }
                _ => id_name,
            },
            _ => id_name,
        };
        b.node(Some(image), "section", name, frame.clone());
        // A Match in the frame's own name, its id or its description, is outside it (M6 spec
        // §3 rule 5).
        b.own(image, at..at + id_len);
        let text = at + header + 1;
        b.own(image, text..text + described);
        at = frame.end;
    }
    // ID3v1: the last 128 bytes, past the ID3v2 tag.
    let n = bytes.len();
    if n >= tag.end + 128 && bytes[n - 128..].starts_with(b"TAG") {
        let v1 = n - 128;
        let node = b.node(None, "object", "ID3v1".into(), v1..n);
        let track = bytes[v1 + 125] == 0 && bytes[v1 + 126] != 0;
        let comment = if track { 97..125 } else { 97..127 };
        let mut fields = vec![
            ("title", 3..33),
            ("artist", 33..63),
            ("album", 63..93),
            ("year", 93..97),
            ("comment", comment),
        ];
        if track {
            fields.push(("track", 126..127));
        }
        fields.push(("genre", 127..128));
        for (name, r) in fields {
            b.node(Some(node), "key", name.into(), v1 + r.start..v1 + r.end);
        }
    }
    Some(())
}

/// The description of a TXXX or WXXX frame: its encoding byte, then the text up to its
/// terminator (Latin-1, UTF-16 with a BOM, UTF-16BE or UTF-8). (it, its bytes after the
/// encoding byte, the terminator left out)
fn description(data: &[u8]) -> Option<(String, usize)> {
    let (&enc, rest) = data.split_first()?;
    let nul = memchr::memchr(0, rest).unwrap_or(rest.len());
    Some(match enc {
        0 => (latin1(&rest[..nul]).into_owned(), nul),
        3 => (String::from_utf8_lossy(&rest[..nul]).into_owned(), nul),
        1 | 2 => {
            let (le, text) = match rest {
                [0xff, 0xfe, t @ ..] => (true, t),
                [0xfe, 0xff, t @ ..] => (false, t),
                t => (false, t),
            };
            let bom = rest.len() - text.len();
            let n = text
                .chunks_exact(2)
                .position(|c| c == [0, 0])
                .unwrap_or(text.len() / 2);
            let units = text
                .chunks_exact(2)
                .map(|c| {
                    if le {
                        u16::from_le_bytes([c[0], c[1]])
                    } else {
                        u16::from_be_bytes([c[0], c[1]])
                    }
                })
                .take_while(|&u| u != 0);
            let d = char::decode_utf16(units)
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect();
            (d, bom + 2 * n)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::layouts::BinaryMap;
    use crate::trail::LineIndex;

    fn trail(bytes: &[u8], at: usize) -> Vec<String> {
        let m = BinaryMap::parse(bytes, "id3").unwrap();
        m.trail(at, &LineIndex::new(bytes))
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    /// v2.3's plain sizes and a TXXX description in UTF-16 with a BOM; v2.2's three-character
    /// ids and 24-bit sizes.
    #[test]
    fn txxx_descriptions_and_older_versions() {
        let mut frame = b"TXXX".to_vec();
        let data: Vec<u8> = [&[1u8, 0xff, 0xfe][..], &[b'I', 0, b'd', 0, 0, 0], b"needle"].concat();
        frame.extend((data.len() as u32).to_be_bytes());
        frame.extend([0, 0]);
        frame.extend(&data);
        let mut v3 = b"ID3\x03\0\0\0\0\0".to_vec();
        v3.push(frame.len() as u8);
        v3.extend(&frame);
        assert_eq!(trail(&v3, v3.len() - 1), ["ID3v2.3", "TXXX Id"]);

        let mut v2 = b"ID3\x02\0\0\0\0\0".to_vec();
        let frame = [b"TT2\0\0\x07\0needle".as_slice()].concat();
        v2.push(frame.len() as u8);
        v2.extend(&frame);
        assert_eq!(trail(&v2, v2.len() - 1), ["ID3v2.2", "TT2"]);
    }

    /// A frame that is compressed keeps its scope and names no description.
    #[test]
    fn a_compressed_frame_keeps_its_scope() {
        let mut frame = b"TXXX".to_vec();
        let data = b"\0desc\0needle";
        frame.extend((data.len() as u32).to_be_bytes());
        frame.extend([0, 0x80]);
        frame.extend(data);
        let mut v3 = b"ID3\x03\0\0\0\0\0".to_vec();
        v3.push(frame.len() as u8);
        v3.extend(&frame);
        assert_eq!(trail(&v3, v3.len() - 1), ["ID3v2.3", "TXXX"]);
    }
}
