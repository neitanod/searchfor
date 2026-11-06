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
