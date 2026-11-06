//! TIFF structures (M6 spec §9), wherever they sit: a TIFF file, JPEG's APP1 Exif, PNG's
//! `eXIf`, WebP's `EXIF`. IFD0, IFD1, the Exif, GPS and Interoperability IFDs are `section`
//! nodes, and each entry a `key` named from the standard tags, or `tag 0xNNNN`. An entry owns
//! its twelve bytes, and its value when the value lies elsewhere in the packet.
use super::layouts::{Builder, sub, u16_at, u32_at};
use std::borrow::Cow;
use std::ops::Range;

/// A TIFF file: `TIFF › IFD0 › ImageDescription`. None when its header or IFD0 does not read.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let all = 0..b.bytes.len();
    // Checked before the image node is added, so a file that does not read adds nothing.
    header(b.bytes, &all)?;
    let image = b.node(None, "object", "TIFF".into(), all.clone());
    read(b, all, image)
}

/// Which tag table names an IFD's entries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ifd {
    /// IFD0, IFD1, …: TIFF's tags, and Exif's in the Exif IFD.
    Main(u32),
    Exif,
    Gps,
    Interop,
}

/// (little-endian, IFD0's offset) of the TIFF header at `packet.start`.
fn header(bytes: &[u8], packet: &Range<usize>) -> Option<(bool, u32)> {
    let at = packet.start;
    sub(packet, at, 8)?;
    let le = match bytes.get(at..at + 2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    (u16_at(bytes, at + 2, le)? == 42).then_some(())?;
    Some((le, u32_at(bytes, at + 4, le)?))
}

/// The IFDs of the TIFF packet `packet` (offsets relative to its start) under `parent`. None
/// when its header or IFD0 does not read; an inner IFD that does not read is left out. Each
/// IFD is read once, however many pointers name it, and all of them together have at most the
/// entries the packet has room for.
pub(super) fn read(b: &mut Builder, packet: Range<usize>, parent: usize) -> Option<()> {
    let bytes = b.bytes;
    let (le, ifd0) = header(bytes, &packet)?;
    let base = packet.start;
    let mut budget = packet.len() / 12 + 1;
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![(ifd0 as usize, Ifd::Main(0), parent)];
    let mut first = true;
    while let Some((offset, kind, under)) = queue.pop() {
        if !visited.insert(offset) {
            continue;
        }
        let Some(table) = base
            .checked_add(offset)
            .and_then(|at| Some((at, u16_at(bytes, at, le)? as usize)))
            .and_then(|(at, n)| sub(&packet, at, 2 + 12 * n).map(|r| (r, n)))
            .filter(|(_, n)| *n <= budget)
        else {
            if first {
                return None;
            }
            continue;
        };
        first = false;
        let (table, count) = table;
        budget -= count;
        let next_at = table.end;
        let span = table.start..next_at.saturating_add(4).min(packet.end);
        let name: Cow<str> = match kind {
            Ifd::Main(0) => "IFD0".into(),
            Ifd::Main(1) => "IFD1".into(),
            Ifd::Main(n) => format!("IFD{n}").into(),
            Ifd::Exif => "Exif IFD".into(),
            Ifd::Gps => "GPS IFD".into(),
            Ifd::Interop => "Interop IFD".into(),
        };
        let node = b.node(Some(under), "section", name, span);
        for e in 0..count {
            let at = table.start + 2 + 12 * e;
            let (Some(tag), Some(typ), Some(n)) = (
                u16_at(bytes, at, le),
                u16_at(bytes, at + 2, le),
                u32_at(bytes, at + 4, le),
            ) else {
                break;
            };
            let key = b.node(Some(node), "key", tag_name(kind, tag), at..at + 12);
            let size = type_size(typ).and_then(|s| s.checked_mul(n as usize));
            if let Some(size) = size.filter(|&s| s > 4)
                && let Some(off) = u32_at(bytes, at + 8, le)
                && let Some(value) = base
                    .checked_add(off as usize)
                    .and_then(|v| sub(&packet, v, size))
            {
                b.own(key, value.clone());
                if tag == 0x02bc && matches!(kind, Ifd::Main(_)) {
                    super::xmp::read(b, value, key);
                }
            }
            let pointer = match (kind, tag) {
                (Ifd::Main(_), 0x8769) => Some(Ifd::Exif),
                (Ifd::Main(_), 0x8825) => Some(Ifd::Gps),
                (Ifd::Exif, 0xa005) => Some(Ifd::Interop),
                _ => None,
            };
            if let Some(p) = pointer
                && n >= 1
                && matches!(typ, 4 | 13)
                && let Some(off) = u32_at(bytes, at + 8, le)
            {
                queue.push((off as usize, p, node));
            }
        }
        if let Ifd::Main(i) = kind
            && let Some(next) = u32_at(bytes, next_at, le).filter(|&n| n != 0)
            && next_at + 4 <= packet.end
        {
            queue.push((next as usize, Ifd::Main(i.saturating_add(1)), under));
        }
    }
    Some(())
}

/// The bytes of one value of a field type; None for a type TIFF 6.0 and Exif do not define.
fn type_size(t: u16) -> Option<usize> {
    Some(match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => return None,
    })
}

/// The standard name of `tag` in an IFD of `kind`, else `tag 0xNNNN`.
fn tag_name(kind: Ifd, tag: u16) -> Cow<'static, str> {
    let known = match kind {
        Ifd::Main(_) | Ifd::Exif => TIFF_TAGS
            .binary_search_by_key(&tag, |&(t, _)| t)
            .ok()
            .map(|i| TIFF_TAGS[i].1),
        Ifd::Gps => GPS_TAGS
            .binary_search_by_key(&tag, |&(t, _)| t)
            .ok()
            .map(|i| GPS_TAGS[i].1),
        Ifd::Interop => match tag {
            0x0001 => Some("InteroperabilityIndex"),
            0x0002 => Some("InteroperabilityVersion"),
            _ => None,
        },
    };
    match known {
        Some(n) => Cow::Borrowed(n),
        None => Cow::Owned(format!("tag 0x{tag:04x}")),
    }
}

/// TIFF 6.0's and Exif 2.32's tags of IFD0, IFD1 and the Exif IFD, sorted by tag.
const TIFF_TAGS: &[(u16, &str)] = &[
    (0x00fe, "NewSubfileType"),
    (0x0100, "ImageWidth"),
    (0x0101, "ImageLength"),
    (0x0102, "BitsPerSample"),
    (0x0103, "Compression"),
    (0x0106, "PhotometricInterpretation"),
    (0x010d, "DocumentName"),
    (0x010e, "ImageDescription"),
    (0x010f, "Make"),
    (0x0110, "Model"),
    (0x0111, "StripOffsets"),
    (0x0112, "Orientation"),
    (0x0115, "SamplesPerPixel"),
    (0x0116, "RowsPerStrip"),
    (0x0117, "StripByteCounts"),
    (0x011a, "XResolution"),
    (0x011b, "YResolution"),
    (0x011c, "PlanarConfiguration"),
    (0x011d, "PageName"),
    (0x0128, "ResolutionUnit"),
    (0x012d, "TransferFunction"),
    (0x0131, "Software"),
    (0x0132, "DateTime"),
    (0x013b, "Artist"),
    (0x013c, "HostComputer"),
    (0x013e, "WhitePoint"),
    (0x013f, "PrimaryChromaticities"),
    (0x0201, "JPEGInterchangeFormat"),
    (0x0202, "JPEGInterchangeFormatLength"),
    (0x0211, "YCbCrCoefficients"),
    (0x0212, "YCbCrSubSampling"),
    (0x0213, "YCbCrPositioning"),
    (0x0214, "ReferenceBlackWhite"),
    (0x02bc, "XMP"),
    (0x8298, "Copyright"),
    (0x829a, "ExposureTime"),
    (0x829d, "FNumber"),
    (0x83bb, "IPTC"),
    (0x8769, "ExifIFDPointer"),
    (0x8822, "ExposureProgram"),
    (0x8824, "SpectralSensitivity"),
    (0x8825, "GPSInfoIFDPointer"),
    (0x8827, "PhotographicSensitivity"),
    (0x9000, "ExifVersion"),
    (0x9003, "DateTimeOriginal"),
    (0x9004, "DateTimeDigitized"),
    (0x9010, "OffsetTime"),
    (0x9011, "OffsetTimeOriginal"),
    (0x9012, "OffsetTimeDigitized"),
    (0x9101, "ComponentsConfiguration"),
    (0x9102, "CompressedBitsPerPixel"),
    (0x9201, "ShutterSpeedValue"),
    (0x9202, "ApertureValue"),
    (0x9203, "BrightnessValue"),
    (0x9204, "ExposureBiasValue"),
    (0x9205, "MaxApertureValue"),
    (0x9206, "SubjectDistance"),
    (0x9207, "MeteringMode"),
    (0x9208, "LightSource"),
    (0x9209, "Flash"),
    (0x920a, "FocalLength"),
    (0x9214, "SubjectArea"),
    (0x927c, "MakerNote"),
    (0x9286, "UserComment"),
    (0x9290, "SubSecTime"),
    (0x9291, "SubSecTimeOriginal"),
    (0x9292, "SubSecTimeDigitized"),
    (0xa000, "FlashpixVersion"),
    (0xa001, "ColorSpace"),
    (0xa002, "PixelXDimension"),
    (0xa003, "PixelYDimension"),
    (0xa004, "RelatedSoundFile"),
    (0xa005, "InteroperabilityIFDPointer"),
    (0xa20e, "FocalPlaneXResolution"),
    (0xa20f, "FocalPlaneYResolution"),
    (0xa210, "FocalPlaneResolutionUnit"),
    (0xa217, "SensingMethod"),
    (0xa300, "FileSource"),
    (0xa301, "SceneType"),
    (0xa401, "CustomRendered"),
    (0xa402, "ExposureMode"),
    (0xa403, "WhiteBalance"),
    (0xa404, "DigitalZoomRatio"),
    (0xa405, "FocalLengthIn35mmFilm"),
    (0xa406, "SceneCaptureType"),
    (0xa408, "Contrast"),
    (0xa409, "Saturation"),
    (0xa40a, "Sharpness"),
    (0xa40c, "SubjectDistanceRange"),
    (0xa420, "ImageUniqueID"),
    (0xa430, "CameraOwnerName"),
    (0xa431, "BodySerialNumber"),
    (0xa432, "LensSpecification"),
    (0xa433, "LensMake"),
    (0xa434, "LensModel"),
    (0xa435, "LensSerialNumber"),
];

/// Exif 2.32's GPS tags, sorted by tag.
const GPS_TAGS: &[(u16, &str)] = &[
    (0x0000, "GPSVersionID"),
    (0x0001, "GPSLatitudeRef"),
    (0x0002, "GPSLatitude"),
    (0x0003, "GPSLongitudeRef"),
    (0x0004, "GPSLongitude"),
    (0x0005, "GPSAltitudeRef"),
    (0x0006, "GPSAltitude"),
    (0x0007, "GPSTimeStamp"),
    (0x0008, "GPSSatellites"),
    (0x0009, "GPSStatus"),
    (0x000a, "GPSMeasureMode"),
    (0x000b, "GPSDOP"),
    (0x000c, "GPSSpeedRef"),
    (0x000d, "GPSSpeed"),
    (0x000e, "GPSTrackRef"),
    (0x000f, "GPSTrack"),
    (0x0010, "GPSImgDirectionRef"),
    (0x0011, "GPSImgDirection"),
    (0x0012, "GPSMapDatum"),
    (0x0013, "GPSDestLatitudeRef"),
    (0x0014, "GPSDestLatitude"),
    (0x0015, "GPSDestLongitudeRef"),
    (0x0016, "GPSDestLongitude"),
    (0x0017, "GPSDestBearingRef"),
    (0x0018, "GPSDestBearing"),
    (0x0019, "GPSDestDistanceRef"),
    (0x001a, "GPSDestDistance"),
    (0x001b, "GPSProcessingMethod"),
    (0x001c, "GPSAreaInformation"),
    (0x001d, "GPSDateStamp"),
    (0x001e, "GPSDifferential"),
    (0x001f, "GPSHPositioningError"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_tables_are_sorted() {
        assert!(TIFF_TAGS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(GPS_TAGS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(tag_name(Ifd::Main(0), 0x9c9b), "tag 0x9c9b");
        assert_eq!(tag_name(Ifd::Gps, 0x0002), "GPSLatitude");
    }
}
