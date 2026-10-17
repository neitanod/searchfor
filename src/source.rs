//! A file's bytes, read once and decoded as ripgrep decodes them (design note 1).
use std::path::{Path, PathBuf};

/// Files at least this large are memory-mapped; smaller ones are read. (A memory-mapped file
/// truncated by another process while we search it can crash the process; ripgrep accepts the
/// same risk.)
pub const MMAP_THRESHOLD: u64 = 8 * 1024 * 1024;

enum Data {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl Data {
    fn raw(&self) -> &[u8] {
        match self {
            Data::Owned(v) => v,
            Data::Mapped(m) => m,
        }
    }
}

pub struct Source {
    /// The path as printed.
    pub path: PathBuf,
    /// Named on the command line, or stdin (binary policy, design note 3).
    pub explicit: bool,
    data: Data,
    /// Bytes to skip at the start (a UTF-8 BOM).
    skip: usize,
    /// The text of a UTF-16 file, transcoded to UTF-8.
    decoded: Option<Vec<u8>>,
    /// The raw file started with a BOM (UTF-8 or UTF-16). grep-searcher then searches with a
    /// reader even when memory maps are on (grep-searcher 0.1.14, searcher/mod.rs:762), so C9 does the same.
    pub had_bom: bool,
}

impl Source {
    pub fn open(path: &Path, explicit: bool) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        let data = if len >= MMAP_THRESHOLD {
            // SAFETY: read-only map; see MMAP_THRESHOLD about concurrent truncation.
            Data::Mapped(unsafe { memmap2::Mmap::map(&file)? })
        } else {
            let mut v = Vec::with_capacity(len as usize);
            std::io::Read::read_to_end(&mut &file, &mut v)?;
            Data::Owned(v)
        };
        Ok(Self::decode(path.to_path_buf(), explicit, data))
    }

    /// For stdin and, from M4, Container Members.
    pub fn from_bytes(path: PathBuf, bytes: Vec<u8>, explicit: bool) -> Self {
        Self::decode(path, explicit, Data::Owned(bytes))
    }

    fn decode(path: PathBuf, explicit: bool, data: Data) -> Self {
        let raw = data.raw();
        if raw.starts_with(b"\xEF\xBB\xBF") {
            return Self {
                path,
                explicit,
                data,
                skip: 3,
                decoded: None,
                had_bom: true,
            };
        }
        let utf16 = if raw.starts_with(b"\xFF\xFE") {
            Some(encoding_rs::UTF_16LE)
        } else if raw.starts_with(b"\xFE\xFF") {
            Some(encoding_rs::UTF_16BE)
        } else {
            None
        };
        if let Some(enc) = utf16 {
            let (text, _) = enc.decode_without_bom_handling(&raw[2..]);
            let decoded = Some(text.into_owned().into_bytes());
            return Self {
                path,
                explicit,
                data,
                skip: 0,
                decoded,
                had_bom: true,
            };
        }
        Self {
            path,
            explicit,
            data,
            skip: 0,
            decoded: None,
            had_bom: false,
        }
    }

    /// The decoded bytes every stage works on.
    pub fn bytes(&self) -> &[u8] {
        match &self.decoded {
            Some(d) => d,
            None => &self.data.raw()[self.skip..],
        }
    }

    /// The bytes as read, BOM included: what a reader search reads. With a BOM, it needs a
    /// searcher that sniffs it, which decodes them into `bytes` as ripgrep does, filling its
    /// buffers as ripgrep's (code review r4, Codex #2).
    pub fn as_read(&self) -> &[u8] {
        self.data.raw()
    }

    /// The first 512 bytes, for the Detector.
    pub fn head(&self) -> &[u8] {
        let b = self.bytes();
        &b[..b.len().min(512)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(p: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(p)
    }

    #[test]
    fn utf8_bom_is_skipped() {
        let src = Source::open(&fixture("misc/bom.md"), true).unwrap();
        assert!(src.bytes().starts_with(b"# T"));
        assert!(src.had_bom);
        assert!(src.explicit);
    }

    #[test]
    fn utf16_is_transcoded() {
        let src = Source::open(&fixture("misc/utf16.md"), false).unwrap();
        assert_eq!(src.bytes(), b"# T\nneedle\n");
        assert!(src.had_bom);
    }

    #[test]
    fn plain_file_is_unchanged() {
        let src = Source::open(&fixture("misc/bin.dat"), false).unwrap();
        assert_eq!(src.bytes(), b"needle first\nmore\0binary needle\n");
        assert!(!src.had_bom);
    }

    #[test]
    fn large_files_are_mapped() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("big.txt");
        let size = 9 * 1024 * 1024;
        std::fs::write(&p, vec![b'x'; size]).unwrap();
        let src = Source::open(&p, false).unwrap();
        assert!(matches!(src.data, Data::Mapped(_)));
        assert_eq!(src.bytes().len(), size);
    }

    #[test]
    fn from_bytes_strips_a_utf8_bom() {
        let src = Source::from_bytes("<stdin>".into(), b"\xEF\xBB\xBFhi\n".to_vec(), true);
        assert_eq!(src.bytes(), b"hi\n");
        assert_eq!(src.path, PathBuf::from("<stdin>"));
    }

    #[test]
    fn head_is_at_most_512_bytes() {
        let small = Source::from_bytes("a".into(), b"ab".to_vec(), false);
        assert_eq!(small.head(), b"ab");
        let big = Source::from_bytes("a".into(), vec![b'x'; 1000], false);
        assert_eq!(big.head().len(), 512);
    }
}
