//! The zip container of the Office, OpenDocument and EPUB formats: its parts, read whole and
//! never more than a limit each and never more than a limit all together, so a zip bomb fails
//! instead of filling the memory or taking minutes (D17).
//! Only the parts a format names are read; nothing else in the zip is opened (8390).
use super::DocError;
use std::io::Read;

/// The most bytes one part may decompress to (256 MiB).
pub const MAX_PART: u64 = 256 << 20;
/// The most bytes all the parts one document reads may decompress to, together (1 GiB).
pub const MAX_DOCUMENT: u64 = 1 << 30;

pub struct Zip<'a> {
    archive: zip::ZipArchive<std::io::Cursor<&'a [u8]>>,
    /// What is left of `MAX_DOCUMENT`.
    left: u64,
}

impl<'a> Zip<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Zip<'a>, DocError> {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| DocError::broken(format!("not a zip file the reader can open ({e})")))?;
        Ok(Zip {
            archive,
            left: MAX_DOCUMENT,
        })
    }

    /// The part `name`, as text in its XML encoding (invalid bytes replaced). None when it is
    /// missing.
    pub fn text(&mut self, name: &str) -> Result<Option<String>, DocError> {
        match self.bytes(name)? {
            Some(b) => decode(name, b).map(Some),
            None => Ok(None),
        }
    }

    /// The part `name`, decompressed. None when it is missing.
    pub fn bytes(&mut self, name: &str) -> Result<Option<Vec<u8>>, DocError> {
        let mut f = match self.archive.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(
                zip::result::ZipError::UnsupportedArchive(zip::result::ZipError::PASSWORD_REQUIRED)
                | zip::result::ZipError::InvalidPassword,
            ) => return Err(DocError::Encrypted),
            Err(e) => return Err(DocError::broken(format!("{name}: {e}"))),
        };
        if f.encrypted() {
            return Err(DocError::Encrypted);
        }
        let mut out = Vec::with_capacity(f.size().min(1 << 20) as usize);
        let read = (&mut f)
            .take(MAX_PART.min(self.left) + 1)
            .read_to_end(&mut out);
        read.map_err(|e| DocError::broken(format!("{name}: {e}")))?;
        let n = out.len() as u64;
        if n > MAX_PART {
            return Err(DocError::TooLarge(
                name.to_string(),
                (MAX_PART >> 20) as usize,
            ));
        }
        if n > self.left {
            return Err(DocError::TooLarge(
                "the whole document, decompressed,".into(),
                (MAX_DOCUMENT >> 20) as usize,
            ));
        }
        self.left -= n;
        Ok(Some(out))
    }
}

/// The text of the XML part `name`, by its encoding (Codex r1 #5): UTF-16 when its BOM or
/// its first bytes (`<?` in UTF-16) say so, else the encoding its declaration names, UTF-8
/// when it names none. UTF-8, UTF-16, US-ASCII, ISO-8859-1 and Windows-1252 are read; a part
/// in another encoding is an error that names it.
fn decode(name: &str, b: Vec<u8>) -> Result<String, DocError> {
    let utf16 = |b: &[u8], big: bool| {
        let units: Vec<u16> = b
            .chunks(2)
            .map(|c| {
                let pair = [c[0], c.get(1).copied().unwrap_or(0)];
                if big {
                    u16::from_be_bytes(pair)
                } else {
                    u16::from_le_bytes(pair)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    let unread = |encoding: &str| {
        DocError::broken(format!(
            "{name} is in {encoding}, an encoding the reader does not read"
        ))
    };
    let starts = |p: &[u8]| b.starts_with(p);
    if starts(&[0, 0, 0xfe, 0xff])
        || starts(&[0xff, 0xfe, 0, 0])
        || starts(&[0, 0, 0, b'<'])
        || starts(&[b'<', 0, 0, 0])
    {
        return Err(unread("UTF-32"));
    }
    if starts(&[0xff, 0xfe]) {
        return Ok(utf16(&b[2..], false));
    }
    if starts(&[0xfe, 0xff]) {
        return Ok(utf16(&b[2..], true));
    }
    if starts(&[b'<', 0, b'?', 0]) {
        return Ok(utf16(&b, false));
    }
    if starts(&[0, b'<', 0, b'?']) {
        return Ok(utf16(&b, true));
    }
    let encoding = declared(&b).map(|e| e.to_ascii_lowercase());
    match encoding.as_deref() {
        // A part that declares UTF-16 but whose bytes are not is read as its bytes are.
        None | Some("utf-8" | "utf8" | "us-ascii" | "ascii" | "utf-16" | "utf16") => {
            Ok(String::from_utf8(b)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
        }
        Some("iso-8859-1" | "latin1" | "latin-1" | "l1" | "iso_8859-1" | "iso8859-1") => {
            Ok(b.iter().map(|&c| char::from(c)).collect())
        }
        Some("windows-1252" | "cp1252") => Ok(b.iter().map(|&c| cp1252(c)).collect()),
        Some(_) => Err(unread(&declared(&b).unwrap_or_default())),
    }
}

/// The encoding an XML declaration at the start of `b` names.
fn declared(b: &[u8]) -> Option<String> {
    let b = b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b);
    let head = b.strip_prefix(b"<?xml")?;
    let end = memchr::memmem::find(head, b"?>")?;
    let decl = std::str::from_utf8(&head[..end]).ok()?;
    super::xml::attr(decl, "encoding").map(|e| e.trim().to_string())
}

/// A byte of Windows-1252: Latin-1, but for the printable characters of 0x80–0x9f.
fn cp1252(c: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    match c {
        0x80..=0x9f => HIGH[usize::from(c - 0x80)],
        c => char::from(c),
    }
}

/// Resolves `href`, relative to the part `base` (`OEBPS/content.opf`), to a part name:
/// percent escapes decoded, `.` and `..` folded, a fragment dropped.
pub fn resolve(base: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let href = percent(href);
    let mut parts: Vec<&str> = if href.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map_or(Vec::new(), |(d, _)| d.split('/').collect())
    };
    for p in href.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn percent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fable r1 #3: every part a document reads spends one budget, so many parts under the
    /// limit of one are refused too.
    #[test]
    fn the_parts_of_a_document_share_a_budget() {
        let parts: Vec<(&str, Vec<u8>)> = ["a.xml", "b.xml", "c.xml"]
            .iter()
            .map(|n| (*n, vec![b' '; 400]))
            .collect();
        let bytes = super::super::fixture::zip(&parts, &[]);
        let mut z = Zip::open(&bytes).unwrap();
        z.left = 1000;
        assert_eq!(z.bytes("a.xml").unwrap().unwrap().len(), 400);
        assert_eq!(z.bytes("b.xml").unwrap().unwrap().len(), 400);
        let e = z.bytes("c.xml").unwrap_err();
        assert!(
            matches!(&e, DocError::TooLarge(what, 1024) if what == "the whole document, decompressed,"),
            "{e}"
        );
    }

    fn utf16(s: &str, big: bool) -> Vec<u8> {
        let mut out = if big {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for u in s.encode_utf16() {
            out.extend(if big {
                u.to_be_bytes()
            } else {
                u.to_le_bytes()
            });
        }
        out
    }

    /// Codex r1 #5: a part in UTF-16, as its BOM or its first bytes say, is transcoded, and so
    /// is one that declares Latin-1; a document whose part is in an encoding the reader does
    /// not read fails with a warning that names it.
    #[test]
    fn parts_are_read_in_their_xml_encoding() {
        use super::super::{Kind, extract};
        let doc = super::super::fixture::docx(&[super::super::fixture::p("the needle é")]);
        let reencode = |f: &dyn Fn(&str) -> Vec<u8>| {
            super::super::fixture::rezip(&doc, |name, data| {
                if name != "word/document.xml" {
                    return data;
                }
                let x = String::from_utf8(data).unwrap();
                f(&x.replace(r#"encoding="UTF-8""#, r#"encoding="UTF-16""#))
            })
        };
        for big in [false, true] {
            let d = reencode(&|x| utf16(x, big));
            let e = extract(Kind::Docx, &d).unwrap();
            assert_eq!(e.text, "the needle é\n".as_bytes(), "big endian: {big}");
            // Without the BOM, its first bytes tell it.
            let d = reencode(&|x| utf16(x, big)[2..].to_vec());
            let e = extract(Kind::Docx, &d).unwrap();
            assert_eq!(e.text, "the needle é\n".as_bytes(), "big endian: {big}");
        }
        let latin1 = |x: &str| {
            x.replace(r#"encoding="UTF-16""#, r#"encoding="ISO-8859-1""#)
                .chars()
                .map(|c| c as u32 as u8)
                .collect()
        };
        let e = extract(Kind::Docx, &reencode(&latin1)).unwrap();
        assert_eq!(e.text, "the needle é\n".as_bytes());
        let sjis = |x: &str| x.replace("UTF-16", "Shift_JIS").into_bytes();
        let e = extract(Kind::Docx, &reencode(&sjis)).unwrap_err();
        assert_eq!(
            e.to_string(),
            "word/document.xml is in Shift_JIS, an encoding the reader does not read"
        );
    }

    #[test]
    fn hrefs_resolve_against_their_part() {
        assert_eq!(
            resolve("OEBPS/content.opf", "text/ch%201.xhtml#x"),
            "OEBPS/text/ch 1.xhtml"
        );
        assert_eq!(
            resolve("OEBPS/text/nav.xhtml", "../text/c2.xhtml"),
            "OEBPS/text/c2.xhtml"
        );
        assert_eq!(resolve("content.opf", "./c.xhtml"), "c.xhtml");
        assert_eq!(
            resolve("ppt/presentation.xml", "slides/slide1.xml"),
            "ppt/slides/slide1.xml"
        );
        assert_eq!(
            resolve("ppt/slides/slide1.xml", "/ppt/notesSlides/n1.xml"),
            "ppt/notesSlides/n1.xml"
        );
    }
}
