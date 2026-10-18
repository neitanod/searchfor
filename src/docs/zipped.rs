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

    /// The part `name`, as text (UTF-8, invalid bytes replaced). None when it is missing.
    pub fn text(&mut self, name: &str) -> Result<Option<String>, DocError> {
        Ok(self.bytes(name)?.map(|b| {
            String::from_utf8(b)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
        }))
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
