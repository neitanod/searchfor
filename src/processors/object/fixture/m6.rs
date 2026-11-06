//! Small metadata files of the M6 binary layouts (M6 spec §9), written byte by byte for the
//! tests of `tests/cli_m6_object.rs`: each holds `text` where its layout's example Trail
//! says, and nothing a decoder would need (no pixels, no audio).

/// A little-endian TIFF whose IFD0 has one entry, `Model` (ASCII), holding `text` and its NUL:
/// inline when it fits in four bytes, else right after the IFD at offset 26.
pub fn tiff(text: &[u8]) -> Vec<u8> {
    let mut b = b"II\x2a\0\x08\0\0\0".to_vec();
    b.extend(1u16.to_le_bytes()); // IFD0 entry count
    b.extend(0x0110u16.to_le_bytes()); // Model, ASCII
    b.extend(2u16.to_le_bytes());
    b.extend(((text.len() + 1) as u32).to_le_bytes());
    if text.len() + 1 <= 4 {
        let mut value = [0; 4];
        value[..text.len()].copy_from_slice(text);
        b.extend(value);
    } else {
        b.extend(26u32.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes()); // next IFD
    if text.len() + 1 > 4 {
        b.extend(text);
        b.push(0);
    }
    b
}

/// A JPEG with one APP1 Exif segment, the `tiff` of `text`, and no image data.
pub fn jpeg(text: &[u8]) -> Vec<u8> {
    let payload = [b"Exif\0\0".as_slice(), &tiff(text)].concat();
    let mut b = vec![0xff, 0xd8, 0xff, 0xe1];
    b.extend(((payload.len() + 2) as u16).to_be_bytes());
    b.extend(payload);
    b.extend([0xff, 0xd9]);
    b
}

/// The CRC-32 of PNG's chunks (ISO 3309, the reflected 0xedb88320 polynomial).
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xedb88320 } else { 0 };
        }
    }
    !crc
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut b = (data.len() as u32).to_be_bytes().to_vec();
    b.extend(kind);
    b.extend(data);
    b.extend(crc32(&b[4..]).to_be_bytes());
    b
}

/// A 1×1 PNG header, a `tEXt` chunk `Comment` holding `text`, and IEND, without image data.
pub fn png(text: &[u8]) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    b.extend(png_chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]));
    b.extend(png_chunk(b"tEXt", &[b"Comment\0".as_slice(), text].concat()));
    b.extend(png_chunk(b"IEND", b""));
    b
}

/// A RIFF WebP with one `EXIF` chunk, the `tiff` of `text`.
pub fn webp(text: &[u8]) -> Vec<u8> {
    let data = tiff(text);
    let mut body = b"WEBP".to_vec();
    body.extend(b"EXIF");
    body.extend((data.len() as u32).to_le_bytes());
    body.extend(data);
    if body.len() % 2 != 0 {
        body.push(0);
    }
    let mut b = b"RIFF".to_vec();
    b.extend((body.len() as u32).to_le_bytes());
    b.extend(body);
    b
}

/// A JPEG with one APP1 XMP segment holding `packet`.
pub fn jpeg_xmp(packet: &[u8]) -> Vec<u8> {
    let data = [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), packet].concat();
    let mut b = vec![0xff, 0xd8, 0xff, 0xe1];
    b.extend(((data.len() + 2) as u16).to_be_bytes());
    b.extend(data);
    b.extend([0xff, 0xd9]);
    b
}

fn syncsafe(n: usize) -> [u8; 4] {
    [
        ((n >> 21) & 127) as u8,
        ((n >> 14) & 127) as u8,
        ((n >> 7) & 127) as u8,
        (n & 127) as u8,
    ]
}

/// An ID3v2.4 tag with one TIT2 frame (UTF-8) holding `text`, and no audio.
pub fn id3(text: &[u8]) -> Vec<u8> {
    let mut frame = b"TIT2".to_vec();
    frame.extend(syncsafe(text.len() + 1));
    frame.extend([0, 0, 3]);
    frame.extend(text);
    let mut b = b"ID3\x04\0\0".to_vec();
    b.extend(syncsafe(frame.len()));
    b.extend(frame);
    b
}

fn box_bytes(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut b = ((data.len() + 8) as u32).to_be_bytes().to_vec();
    b.extend(kind);
    b.extend(data);
    b
}

/// An M4A whose `©nam` item holds `text`: `ftyp`, then `moov › udta › meta › ilst › ©nam ›
/// data`, and no media.
pub fn mp4(text: &[u8]) -> Vec<u8> {
    let data = box_bytes(b"data", &[&[0, 0, 0, 1, 0, 0, 0, 0][..], text].concat());
    let item = box_bytes(&[0xa9, b'n', b'a', b'm'], &data);
    let ilst = box_bytes(b"ilst", &item);
    let meta = box_bytes(b"meta", &[&[0, 0, 0, 0][..], &ilst].concat());
    let udta = box_bytes(b"udta", &meta);
    [
        box_bytes(b"ftyp", b"M4A \0\0\0\0M4A "),
        box_bytes(b"moov", &udta),
    ]
    .concat()
}
