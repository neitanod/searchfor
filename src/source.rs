//! A file's bytes, read once and decoded as ripgrep decodes them (design note 1).
use std::path::{Path, PathBuf};

/// Files at least this large are memory-mapped; smaller ones are read. (A memory-mapped file
/// truncated by another process while we search it can crash the process; ripgrep accepts the
/// same risk, and `--no-mmap` avoids it: `Mapping::Never`.)
pub const MMAP_THRESHOLD: u64 = 8 * 1024 * 1024;

/// Whether `Source::open_as` maps a file of `MMAP_THRESHOLD` bytes or more (`Large`) or reads
/// every file (`Never`, `--no-mmap`), as ripgrep reads every file then (review r2 of the M5a
/// plan, Codex #1). `--mmap` changes nothing here: a smaller file is read either way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mapping {
    #[default]
    Large,
    Never,
}

impl Mapping {
    /// The mapping `--mmap`/`--no-mmap` ask for.
    pub fn of(args: &crate::cli::Args) -> Mapping {
        match args.mmap_mode {
            crate::cli::MmapMode::Never => Mapping::Never,
            _ => Mapping::Large,
        }
    }
}

/// What `Source::head` gives the Detector.
const HEAD: usize = 512;

/// How much of a file's bytes as read decodes to its whole `head`: a BOM, then at most four
/// bytes per byte of `head` (two UTF-16 code units, one of them maybe cut).
pub const READ_HEAD: usize = 3 + 4 * HEAD;

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

/// How a file's bytes become the text every stage reads (M5a plan, D10), as grep-searcher
/// decodes them for rg 14.1.1: by its BOM (`Auto`), with `-E`'s encoding unless a BOM says
/// another, or as they are with `-E none`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Decoding {
    #[default]
    Auto,
    Label(&'static encoding_rs::Encoding),
    Raw,
}

impl Decoding {
    /// The decoding `-E` asks for.
    pub fn of(args: &crate::cli::Args) -> Decoding {
        match &args.encoding {
            crate::cli::EncodingMode::Auto => Decoding::Auto,
            crate::cli::EncodingMode::Disabled => Decoding::Raw,
            crate::cli::EncodingMode::Label(l) => encoding_rs::Encoding::for_label(l.as_bytes())
                .map_or(Decoding::Auto, Decoding::Label),
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
    /// The raw file started with a BOM (UTF-8 or UTF-16), or `-E` named an encoding: grep-searcher
    /// then transcodes, through a reader even when memory maps are on (grep-searcher 0.1.14,
    /// searcher/mod.rs:762 and 804), so C9 does the same.
    pub had_bom: bool,
}

impl Source {
    pub fn open(path: &Path, explicit: bool) -> std::io::Result<Self> {
        Self::open_as(path, explicit, Decoding::Auto, Mapping::Large)
    }

    /// `open`, decoded as `-E` asks (`Decoding`) and mapped as `--no-mmap` allows (`Mapping`).
    pub fn open_as(
        path: &Path,
        explicit: bool,
        decoding: Decoding,
        mapping: Mapping,
    ) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        let data = if len >= MMAP_THRESHOLD && mapping == Mapping::Large {
            // SAFETY: read-only map; see MMAP_THRESHOLD about concurrent truncation.
            Data::Mapped(unsafe { memmap2::Mmap::map(&file)? })
        } else {
            let mut v = Vec::with_capacity(len as usize);
            std::io::Read::read_to_end(&mut &file, &mut v)?;
            Data::Owned(v)
        };
        Ok(Self::decode(path.to_path_buf(), explicit, data, decoding))
    }

    /// For stdin and, from M4, Container Members.
    pub fn from_bytes(path: PathBuf, bytes: Vec<u8>, explicit: bool) -> Self {
        Self::from_bytes_as(path, bytes, explicit, Decoding::Auto)
    }

    /// `from_bytes`, decoded as `-E` asks (`Decoding`).
    pub fn from_bytes_as(
        path: PathBuf,
        bytes: Vec<u8>,
        explicit: bool,
        decoding: Decoding,
    ) -> Self {
        Self::decode(path, explicit, Data::Owned(bytes), decoding)
    }

    fn decode(path: PathBuf, explicit: bool, data: Data, decoding: Decoding) -> Self {
        let raw = data.raw();
        // `-E none`: no BOM is sniffed, and the bytes are searched as they are.
        if decoding == Decoding::Raw {
            return Self {
                path,
                explicit,
                data,
                skip: 0,
                decoded: None,
                had_bom: false,
            };
        }
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
        // `-E LABEL` without a BOM: the label's encoding, UTF-8 passed through as it is
        // (encoding_rs_io's `utf8_passthru`), a sequence it cannot decode as U+FFFD.
        if let Decoding::Label(enc) = decoding {
            let decoded = (enc != encoding_rs::UTF_8).then(|| {
                enc.decode_without_bom_handling(raw)
                    .0
                    .into_owned()
                    .into_bytes()
            });
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

    /// The first `HEAD` bytes, for the Detector.
    pub fn head(&self) -> &[u8] {
        let b = self.bytes();
        &b[..b.len().min(HEAD)]
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

    /// Code review r4, Fable #6: the first `READ_HEAD` bytes as read give the whole head.
    #[test]
    fn the_head_of_the_read_head_is_the_head() {
        let text = "é𝄞a".repeat(400);
        let mut utf16le = b"\xFF\xFE".to_vec();
        utf16le.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        let mut utf16be = b"\xFE\xFF".to_vec();
        utf16be.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend(text.as_bytes());
        let ascii = b"\xFF\xFE"
            .iter()
            .copied()
            .chain("a".repeat(3000).encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        for raw in [utf16le, utf16be, bom, text.clone().into_bytes(), ascii] {
            let whole = Source::from_bytes(PathBuf::from("x"), raw.clone(), false);
            let cut = Source::from_bytes(PathBuf::from("x"), raw[..READ_HEAD].to_vec(), false);
            assert_eq!(cut.head(), whole.head());
            assert_eq!(whole.head().len(), HEAD);
        }
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

    /// M5a plan, D10: `-E` as grep-searcher decodes for rg 14.1.1.
    #[test]
    fn explicit_encodings_and_none() {
        let latin1 = Decoding::Label(encoding_rs::WINDOWS_1252);
        let s = Source::from_bytes_as("a".into(), b"caf\xe9 needle\n".to_vec(), false, latin1);
        assert_eq!(s.bytes(), "café needle\n".as_bytes());
        assert!(
            s.had_bom,
            "an explicit encoding is searched through a reader"
        );
        // A BOM wins over the label.
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice("é".as_bytes());
        let s = Source::from_bytes_as("a".into(), bom.clone(), false, latin1);
        assert_eq!(s.bytes(), "é".as_bytes());
        // UTF-8 passes through, invalid bytes included.
        let utf8 = Decoding::Label(encoding_rs::UTF_8);
        let s = Source::from_bytes_as("a".into(), b"a\xffb".to_vec(), false, utf8);
        assert_eq!(s.bytes(), b"a\xffb");
        // UTF-16 without a BOM.
        let le: Vec<u8> = "né".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let s = Source::from_bytes_as(
            "a".into(),
            le,
            false,
            Decoding::Label(encoding_rs::UTF_16LE),
        );
        assert_eq!(s.bytes(), "né".as_bytes());
        // -E none: the BOM stays, nothing is transcoded.
        let s = Source::from_bytes_as("a".into(), bom.clone(), false, Decoding::Raw);
        assert_eq!(s.bytes(), &bom[..]);
        assert!(!s.had_bom);
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

    /// Review r2 of the M5a plan, Codex #1: with `--no-mmap` no file is mapped, however large,
    /// as ripgrep reads every file then; a truncation while s4 reads it cannot end the process.
    #[test]
    fn no_mmap_maps_no_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("big.txt");
        let size = MMAP_THRESHOLD as usize;
        std::fs::write(&p, vec![b'x'; size]).unwrap();
        let read = Source::open_as(&p, false, Decoding::Auto, Mapping::Never).unwrap();
        assert!(matches!(read.data, Data::Owned(_)));
        assert_eq!(read.bytes().len(), size);
        let mapped = Source::open_as(&p, false, Decoding::Auto, Mapping::Large).unwrap();
        assert!(matches!(mapped.data, Data::Mapped(_)));
        let args = |v: &[&str]| crate::cli::parse(v.iter().map(Into::into).collect()).unwrap();
        assert_eq!(
            Mapping::of(&args(&["s4", "--no-mmap", "x"])),
            Mapping::Never
        );
        assert_eq!(Mapping::of(&args(&["s4", "--mmap", "x"])), Mapping::Large);
        assert_eq!(Mapping::of(&args(&["s4", "x"])), Mapping::Large);
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
