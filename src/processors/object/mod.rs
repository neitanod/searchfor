//! The `object` Processor (spec §5.7): the Trail of a Match in an ELF, PE or Mach-O file is
//! `ELF x86-64 › .rodata › main.version+0x10`: the image, the section the Match's bytes are in,
//! and the closest symbol of that section at or before them. Parsed with goblin (M4 plan, D3).
//!
//! The Processor runs in s4's process, where an allocation that fails aborts s4 whatever
//! `catch_unwind` says, so what a file costs is bounded by its size and never by a count it
//! declares (Codex M4 code r5 #1): the names of sections and symbols are borrowed from the file,
//! never copied once per symbol that names them; a fat Mach-O's slices neither overlap nor
//! repeat; a Mach-O segment has the sections its command has room for, and its symbol table the
//! symbols the file has room for; a PE's import, resource, TLS and certificate tables are not
//! read, and its sections' long names fit in the file together.
#[doc(hidden)]
pub mod fixture;
pub mod rows;

use crate::model::{Input, Name, Segment, Trail};
use crate::trail::LineIndex;
use std::borrow::Cow;
use std::ops::Range;

pub struct Object;

impl super::Processor for Object {
    fn trails(
        &self,
        input: &Input,
        _params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let Some(map) = Map::parse(input.bytes) else {
            return Ok(vec![Vec::new(); anchors.len()]);
        };
        let lines = LineIndex::new(input.bytes);
        Ok(anchors.iter().map(|&a| map.trail(a, &lines)).collect())
    }

    /// Section and symbol names are not searched by `--in-headings` in M4 (M4 plan, D11).
    fn names(&self, _input: &Input, _params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(Vec::new())
    }
}

/// `f()`, or None when it panics, without the panic's message: the panic hook stays silent on
/// the thread while `f` runs, and prints every other panic as before.
fn quietly<T>(f: impl FnOnce() -> Option<T>) -> Option<T> {
    use std::cell::Cell;
    thread_local! {
        static QUIET: Cell<bool> = const { Cell::new(false) };
    }
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                before(info);
            }
        }));
    });
    let was = QUIET.with(|q| q.replace(true));
    let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    QUIET.with(|q| q.set(was));
    got.ok().flatten()
}

/// One section: its name, the bytes of the file it holds, and its symbols, sorted by offset,
/// one per offset.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sect<'a> {
    name: Cow<'a, str>,
    file: Range<usize>,
    symbols: Vec<Sym<'a>>,
}

/// A symbol at file offset `at`; `end` is where it ends when the table says (ELF's `st_size`).
/// Its name as the table has it, demangled when it names a Match.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sym<'a> {
    at: usize,
    end: Option<usize>,
    name: &'a str,
}

/// One image: a whole ELF or PE file, or one architecture of a fat Mach-O.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Image<'a> {
    label: String,
    range: Range<usize>,
    /// Sorted by start; sections without bytes in the file are left out.
    sections: Vec<Sect<'a>>,
}

/// What the `object` Processor knows of a file: its images, which borrow their names from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map<'a> {
    images: Vec<Image<'a>>,
}

/// The arches a fat Mach-O may have: Apple's loader reads the fat header and its arch table
/// from the file's first 4 KiB (xnu's `mach_fat.c`), which hold 204 of them.
const MAX_ARCHES: usize = (4096 - 8) / 20;

impl<'a> Map<'a> {
    /// None when goblin does not read `bytes` as an ELF, PE or Mach-O file (the file is then
    /// Plain, without a warning: M4 plan, D9), also when it panics on them: goblin 0.10.7 does
    /// arithmetic on the file's values that overflows in a debug build (Codex M4 code r4 #1),
    /// and the Processor runs in s4's process. A release build wraps it instead, so the same
    /// file may have a Trail there (Fable M4 code r5 #1). A fat Mach-O whose slices are not a
    /// valid fat file's is None as well.
    pub fn parse(bytes: &'a [u8]) -> Option<Map<'a>> {
        quietly(|| Map::read(bytes))
    }

    /// `parse` without catching a panic, for the fuzz: a panic in our own code must fail it
    /// (Fable M4 code r5 #3).
    #[doc(hidden)]
    pub fn parse_loudly(bytes: &'a [u8]) -> Option<Map<'a>> {
        Map::read(bytes)
    }

    fn read(bytes: &'a [u8]) -> Option<Map<'a>> {
        use goblin::mach::Mach;
        let hint = goblin::peek_bytes(bytes.get(..16)?.try_into().ok()?).ok()?;
        let images = match hint {
            goblin::Hint::Elf(_) => vec![elf(&goblin::elf::Elf::parse(bytes).ok()?, bytes)],
            goblin::Hint::PE => {
                if !pe_section_names_fit(bytes) {
                    return None;
                }
                let mut opts = goblin::pe::options::ParseOptions::default();
                opts.parse_imports = false;
                opts.parse_resources = false;
                opts.parse_tls_data = false;
                opts.parse_attribute_certificates = false;
                vec![pe(
                    &goblin::pe::PE::parse_with_opts(bytes, &opts).ok()?,
                    bytes,
                )]
            }
            goblin::Hint::Mach(_) | goblin::Hint::MachFat(_) => match Mach::parse(bytes).ok()? {
                Mach::Binary(m) => vec![macho(&m, bytes, 0..bytes.len())],
                Mach::Fat(f) => fat(&f, bytes)?,
            },
            _ => return None,
        };
        let mut images: Vec<Image> = images
            .into_iter()
            .map(|mut img| {
                img.sections.retain(|s| {
                    s.file.start < s.file.end
                        && s.file.end <= img.range.end.max(s.file.end).min(bytes.len())
                });
                img.sections
                    .sort_by_key(|s| (s.file.start, std::cmp::Reverse(s.file.end)));
                img
            })
            .collect();
        images.sort_by_key(|i| i.range.start);
        Some(Map { images })
    }

    /// The Trail of the byte at file offset `at`: the image, the section and the symbol, each
    /// present when `at` is inside one.
    pub fn trail(&self, at: usize, lines: &LineIndex) -> Trail {
        let mut t = Vec::new();
        let Some(img) = self.images.iter().find(|i| i.range.contains(&at)) else {
            return t;
        };
        t.push(segment("object", &img.label, &img.range, lines));
        // The last section that starts at or before `at` and holds it.
        let Some(sect) = img
            .sections
            .iter()
            .rev()
            .find(|s| s.file.start <= at && at < s.file.end)
        else {
            return t;
        };
        t.push(segment("section", &sect.name, &sect.file, lines));
        let i = sect.symbols.partition_point(|s| s.at <= at);
        // The closest symbol at or before `at`, unless its size ends before `at` (D7).
        if let Some(sym) = i.checked_sub(1).map(|i| &sect.symbols[i])
            && sym.end.is_none_or(|e| at < e)
        {
            let next = sect.symbols.get(i).map_or(sect.file.end, |s| s.at);
            let end = sym.end.map_or(next, |e| e.min(next).max(sym.at + 1));
            let name = demangle(sym.name);
            let shown = if at == sym.at {
                name
            } else {
                format!("{name}+0x{:x}", at - sym.at)
            };
            t.push(segment("symbol", &shown, &(sym.at..end), lines));
        }
        t
    }
}

/// The images of a fat Mach-O, each over its slice of the file, or None when its slices are
/// not a valid fat file's (Codex M4 code r5 #1, as xnu's `mach_fat.c` checks them): more than
/// `MAX_ARCHES`, one over the arch table, or one over another. Otherwise every arch is parsed
/// once, and they hold at most the file's bytes together. A slice that ends past the file is
/// skipped, as before.
fn fat<'a>(f: &goblin::mach::MultiArch<'a>, bytes: &'a [u8]) -> Option<Vec<Image<'a>>> {
    if f.narches > MAX_ARCHES {
        return None;
    }
    let table = 8 + 20 * f.narches;
    let mut ranges = Vec::new();
    for arch in f.iter_arches() {
        let arch = arch.ok()?;
        let start = arch.offset as usize;
        let end = start.checked_add(arch.size as usize)?;
        if start < table && arch.size > 0 {
            return None;
        }
        ranges.push(start..end);
    }
    let mut sorted: Vec<&Range<usize>> = ranges.iter().filter(|r| !r.is_empty()).collect();
    sorted.sort_by_key(|r| r.start);
    if sorted.windows(2).any(|w| w[0].end > w[1].start) {
        return None;
    }
    let mut v = Vec::new();
    for (i, r) in ranges.iter().enumerate() {
        let (start, end) = (r.start, r.end.min(bytes.len()));
        if let (Ok(goblin::mach::SingleArch::MachO(m)), Some(slice)) =
            (f.get(i), bytes.get(start..end))
        {
            let mut img = macho(&m, slice, start..end);
            for s in &mut img.sections {
                s.file = s.file.start + start..s.file.end + start;
                for sym in &mut s.symbols {
                    sym.at += start;
                    sym.end = sym.end.map(|e| e + start);
                }
            }
            v.push(img);
        }
    }
    Some(v)
}

/// Whether the long names of a PE's sections (`/4`: at offset 4 of the COFF string table),
/// which goblin copies for each section that names them, fit in the file together (Codex M4
/// code r5 #1). The headers are read as goblin reads them; what it cannot read it rejects
/// itself.
fn pe_section_names_fit(bytes: &[u8]) -> bool {
    let le = |at: usize, n: usize| {
        bytes
            .get(at..at.checked_add(n)?)
            .map(|b| b.iter().rev().fold(0usize, |a, &x| a << 8 | x as usize))
    };
    let Some(coff) = le(0x3c, 4).map(|pe| pe + 4) else {
        return true;
    };
    let (Some(n), Some(symtab), Some(nsyms), Some(optional)) = (
        le(coff + 2, 2),
        le(coff + 8, 4),
        le(coff + 12, 4),
        le(coff + 16, 2),
    ) else {
        return true;
    };
    let strings = symtab.saturating_add(nsyms.saturating_mul(18));
    let mut total = 0usize;
    for i in 0..n {
        let Some(name) = bytes
            .get(coff + 20 + optional + 40 * i..)
            .and_then(|b| b.get(..8))
        else {
            break;
        };
        let Some(at) = section_name_offset(name).and_then(|o| strings.checked_add(o)) else {
            continue;
        };
        let Some(rest) = bytes.get(at..) else {
            continue;
        };
        total += memchr::memchr(0, rest).unwrap_or(rest.len());
        if total > bytes.len() {
            return false;
        }
    }
    true
}

/// The offset in the string table that a section's 8-byte name points to, as goblin reads it:
/// `/` and its decimal digits, or `//` and up to six base-64 digits.
fn section_name_offset(name: &[u8]) -> Option<usize> {
    let rest = name.strip_prefix(b"/")?;
    let rest = &rest[..memchr::memchr(0, rest).unwrap_or(rest.len())];
    if let Some(b64) = rest.strip_prefix(b"/") {
        return b64.iter().try_fold(0usize, |a, &c| {
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            };
            Some(a * 64 + v as usize)
        });
    }
    std::str::from_utf8(rest).ok()?.parse().ok()
}

fn segment(kind: &str, name: &str, range: &Range<usize>, lines: &LineIndex) -> Segment {
    Segment {
        kind: kind.into(),
        name: name.into(),
        line: lines.line_of(range.start),
        end_line: lines.line_of(range.end.saturating_sub(1).max(range.start)),
        level: None,
        signature: None,
    }
}

/// A Rust symbol demangled without its hash (`{:#}`); any other name as the table has it.
fn demangle(name: &str) -> String {
    match rustc_demangle::try_demangle(name) {
        Ok(d) => format!("{d:#}"),
        Err(_) => name.to_string(),
    }
}

/// A symbol as a table gives it: the index of its section in `sections`, its address minus the
/// section's address, its size when known (and not zero), and its name.
type RawSym<'a> = (usize, u64, Option<u64>, &'a str);

/// Adds the symbols to the sections they fall in.
fn place<'a>(sections: &mut [Sect<'a>], syms: Vec<RawSym<'a>>) {
    for (i, delta, size, name) in syms {
        let Some(s) = sections.get_mut(i) else {
            continue;
        };
        let Ok(delta) = usize::try_from(delta) else {
            continue;
        };
        let Some(at) = s.file.start.checked_add(delta) else {
            continue;
        };
        if name.is_empty() || at >= s.file.end {
            continue;
        }
        let end = size
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| at.checked_add(n));
        s.symbols.push(Sym { at, end, name });
    }
    for s in sections {
        // Stable: of the symbols at one offset, the first in the symbol tables is kept.
        s.symbols.sort_by_key(|s| s.at);
        s.symbols.dedup_by_key(|s| s.at);
    }
}

fn elf_machine(m: u16, is_64: bool) -> String {
    use goblin::elf::header::*;
    match m {
        EM_X86_64 => "x86-64".into(),
        EM_386 => "i386".into(),
        EM_AARCH64 => "aarch64".into(),
        EM_ARM => "arm".into(),
        EM_RISCV if is_64 => "riscv64".into(),
        EM_RISCV => "riscv32".into(),
        EM_PPC64 => "ppc64".into(),
        EM_PPC => "ppc".into(),
        EM_MIPS => "mips".into(),
        EM_S390 => "s390x".into(),
        258 => "loongarch64".into(),
        m => format!("machine 0x{m:x}"),
    }
}

fn elf<'a>(e: &goblin::elf::Elf<'a>, bytes: &'a [u8]) -> Image<'a> {
    use goblin::elf::section_header::{SHT_NOBITS, SHT_NULL};
    let len = bytes.len();
    let mut sections = Vec::new();
    // ELF section index → index in `sections`.
    let mut index = vec![usize::MAX; e.section_headers.len()];
    let mut addr = vec![0u64; e.section_headers.len()];
    for (i, sh) in e.section_headers.iter().enumerate() {
        if sh.sh_type == SHT_NULL || sh.sh_type == SHT_NOBITS || sh.sh_size == 0 {
            continue;
        }
        let start = sh.sh_offset as usize;
        let end = start.saturating_add(sh.sh_size as usize).min(len);
        let name = e.shdr_strtab.get_at(sh.sh_name).unwrap_or("");
        index[i] = sections.len();
        addr[i] = sh.sh_addr;
        sections.push(Sect {
            name: Cow::Borrowed(name),
            file: start..end,
            symbols: Vec::new(),
        });
    }
    let mut syms = Vec::new();
    for (tab, strs) in [(&e.syms, &e.strtab), (&e.dynsyms, &e.dynstrtab)] {
        for s in tab.iter() {
            use goblin::elf::sym::{STT_FILE, STT_SECTION};
            let sh = s.st_shndx;
            if s.st_type() == STT_SECTION || s.st_type() == STT_FILE || sh >= index.len() {
                continue;
            }
            if index[sh] == usize::MAX || s.st_value < addr[sh] {
                continue;
            }
            let name = strs.get_at(s.st_name).unwrap_or("");
            let size = (s.st_size > 0).then_some(s.st_size);
            syms.push((index[sh], s.st_value - addr[sh], size, name));
        }
    }
    place(&mut sections, syms);
    Image {
        label: format!("ELF {}", elf_machine(e.header.e_machine, e.is_64)),
        range: 0..len,
        sections,
    }
}

fn pe<'a>(p: &goblin::pe::PE<'a>, bytes: &'a [u8]) -> Image<'a> {
    let machine = match p.header.coff_header.machine {
        0x8664 => "x86-64".to_string(),
        0x14c => "i386".into(),
        0xaa64 => "aarch64".into(),
        0x1c0 | 0x1c4 => "arm".into(),
        m => format!("machine 0x{m:x}"),
    };
    let mut sections = Vec::new();
    let mut rvas = Vec::new();
    for s in &p.sections {
        let start = s.pointer_to_raw_data as usize;
        // The raw data is padded to the file alignment; the virtual size says what is used.
        let used = if s.virtual_size > 0 {
            s.virtual_size.min(s.size_of_raw_data)
        } else {
            s.size_of_raw_data
        };
        let end = start.saturating_add(used as usize).min(bytes.len());
        sections.push(Sect {
            // At most the file's size together (`pe_section_names_fit`).
            name: Cow::Owned(s.name().unwrap_or("").to_string()),
            file: start..end,
            symbols: Vec::new(),
        });
        rvas.push((
            s.virtual_address as u64,
            s.virtual_size.max(s.size_of_raw_data) as u64,
        ));
    }
    let mut syms = Vec::new();
    for x in &p.exports {
        let (Some(name), rva) = (x.name, x.rva as u64) else {
            continue;
        };
        if let Some(i) = rvas.iter().position(|(a, n)| *a <= rva && rva < a + n) {
            syms.push((i, rva - rvas[i].0, None, name));
        }
    }
    // A COFF symbol table, which MinGW leaves in images that were not stripped.
    let coff = &p.header.coff_header;
    if let (Ok(Some(table)), Ok(Some(strings))) = (coff.symbols(bytes), coff.strings(bytes)) {
        for (_, inline, s) in table.iter() {
            // External and static symbols, except the static ones that define a section
            // (`.rdata`, with an auxiliary record) and `.`-names.
            if s.section_number <= 0 || s.storage_class != 2 && s.storage_class != 3 {
                continue;
            }
            if s.is_section_definition() {
                continue;
            }
            // A long name is at its offset in the string table, which counts from the table's
            // 4-byte length (one under 4 is malformed: goblin's `name` panics on it, found
            // fuzzing for code review r4).
            let name = match inline {
                Some(n) => Some(n),
                None => u32::from_le_bytes(s.name[4..8].try_into().unwrap_or_default())
                    .checked_sub(4)
                    .and_then(|o| strings.get_at(o as usize)),
            };
            if let Some(name) = name
                && !name.starts_with('.')
            {
                syms.push((s.section_number as usize - 1, s.value as u64, None, name));
            }
        }
    }
    place(&mut sections, syms);
    Image {
        label: format!("PE {machine}"),
        range: 0..bytes.len(),
        sections,
    }
}

fn macho_cpu(cpu: u32) -> String {
    match cpu {
        0x0100_0007 => "x86-64".into(),
        7 => "i386".into(),
        0x0100_000c => "arm64".into(),
        0x0200_000c => "arm64_32".into(),
        12 => "arm".into(),
        0x0100_0012 => "ppc64".into(),
        18 => "ppc".into(),
        c => format!("cpu 0x{c:x}"),
    }
}

/// The symbols of a Mach-O's `LC_SYMTAB` that the file has room for, each `(name, n_type,
/// n_sect, n_value)`. The count may say more, each past the file's end (found fuzzing for code
/// review r4, a count of 318 million); a symbol whose name is out of the string table or not
/// UTF-8 is left out, and the ones after it are read (Fable M4 code r5 #2). The string table is
/// read once, where goblin's iterator reads a name's bytes once per symbol that names it, which
/// costs a name's length times the symbols (Codex M4 code r5 #1).
fn macho_symbols<'a>(
    m: &goblin::mach::MachO<'a>,
    bytes: &'a [u8],
) -> impl Iterator<Item = (&'a str, u8, u8, u64)> + 'a {
    // The last `LC_SYMTAB`, as goblin keeps it.
    let symtab = m.load_commands.iter().rev().find_map(|c| match c.command {
        goblin::mach::load_command::CommandVariant::Symtab(t) => Some(t),
        _ => None,
    });
    let (is_64, le) = (m.is_64, m.little_endian);
    let size = if is_64 { 16 } else { 12 };
    let (symoff, nsyms, stroff, strsize) = symtab.map_or((0, 0, 0, 0), |t| {
        (
            t.symoff as usize,
            t.nsyms as usize,
            t.stroff as usize,
            t.strsize as usize,
        )
    });
    let count = nsyms.min(bytes.len().saturating_sub(symoff) / size);
    let table = bytes
        .get(stroff..)
        .map_or(&[][..], |b| &b[..strsize.min(b.len())]);
    let names = Names::new(table);
    let word = move |b: &[u8]| {
        let mut a = [0u8; 8];
        let a = &mut a[..b.len()];
        a.copy_from_slice(b);
        if le {
            a.reverse();
        }
        a.iter().fold(0u64, |v, &x| v << 8 | x as u64)
    };
    (0..count).filter_map(move |i| {
        let e = bytes.get(symoff + size * i..symoff + size * (i + 1))?;
        let value = if is_64 {
            word(&e[8..16])
        } else {
            word(&e[8..12])
        };
        let name = names.get(word(&e[..4]) as usize)?;
        Some((name, e[4], e[5], value))
    })
}

/// A string table of NUL-ended names, read once: each name a symbol points to, at any offset,
/// is found in it by a binary search, and checked for UTF-8 in constant time.
struct Names<'a> {
    /// Each name's start, and its bytes from the first offset after which they are UTF-8 up to
    /// its NUL (or the table's end); names of no bytes are left out.
    names: Vec<(u32, u32, &'a str)>,
}

impl<'a> Names<'a> {
    fn new(table: &'a [u8]) -> Self {
        let table = &table[..table.len().min(u32::MAX as usize)];
        let mut names = Vec::new();
        let mut start = 0;
        while start < table.len() {
            let end = memchr::memchr(0, &table[start..]).map_or(table.len(), |n| start + n);
            // After the last bytes that are not UTF-8, every char boundary starts a name.
            let mut valid = start;
            let mut at = start;
            for chunk in table[start..end].utf8_chunks() {
                at += chunk.valid().len() + chunk.invalid().len();
                if !chunk.invalid().is_empty() {
                    valid = at;
                }
            }
            if let Ok(text) = std::str::from_utf8(&table[valid..end])
                && !text.is_empty()
            {
                names.push((start as u32, valid as u32, text));
            }
            start = end + 1;
        }
        Names { names }
    }

    fn get(&self, at: usize) -> Option<&'a str> {
        let i = self
            .names
            .partition_point(|n| n.0 as usize <= at)
            .checked_sub(1)?;
        let (_, valid, text) = self.names[i];
        text.get(at.checked_sub(valid as usize)?..)
            .filter(|t| !t.is_empty())
    }
}

/// `bytes` is the slice of the file this Mach-O is, at `range`; its offsets are relative to it.
fn macho<'a>(m: &goblin::mach::MachO<'a>, bytes: &'a [u8], range: Range<usize>) -> Image<'a> {
    let len = bytes.len();
    let (head, each) = if m.is_64 { (72, 80) } else { (56, 68) };
    let mut sections = Vec::new();
    let mut addrs = Vec::new();
    for seg in m.segments.iter() {
        // The sections a segment's command has room for: goblin reads as many as it says, from
        // whatever follows the command (Codex M4 code r5 #1).
        let room = (seg.cmdsize as usize).saturating_sub(head) / each;
        let Ok(list) = seg.into_iter().take(room).collect::<Result<Vec<_>, _>>() else {
            continue;
        };
        for (s, _) in list {
            const S_ZEROFILL: u32 = 0x1;
            const S_GB_ZEROFILL: u32 = 0xc;
            const S_THREAD_LOCAL_ZEROFILL: u32 = 0x12;
            let zero = matches!(
                s.flags & 0xff,
                S_ZEROFILL | S_GB_ZEROFILL | S_THREAD_LOCAL_ZEROFILL
            );
            let start = s.offset as usize;
            let end = if zero || start == 0 {
                start
            } else {
                start.saturating_add(s.size as usize).min(len)
            };
            let name = format!("{},{}", s.segname().unwrap_or(""), s.name().unwrap_or(""));
            // n_sect counts every section, also the ones without bytes in the file.
            addrs.push(s.addr);
            sections.push(Sect {
                name: Cow::Owned(name),
                file: start..end,
                symbols: Vec::new(),
            });
        }
    }
    let mut syms = Vec::new();
    for (name, n_type, n_sect, n_value) in macho_symbols(m, bytes) {
        const N_STAB: u8 = 0xe0;
        const N_TYPE: u8 = 0x0e;
        const N_SECT: u8 = 0x0e;
        if n_type & N_STAB != 0 || n_type & N_TYPE != N_SECT || n_sect == 0 {
            continue;
        }
        // A local `l…` or `L…` is an assembler's label (`ltmp1`, `L_.str`), not a symbol; an
        // external one is a symbol (Codex r1 #8).
        const N_EXT: u8 = 0x01;
        if n_type & N_EXT == 0 && (name.starts_with('l') || name.starts_with('L')) {
            continue;
        }
        let i = n_sect as usize - 1;
        let Some(&a) = addrs.get(i) else { continue };
        if n_value < a {
            continue;
        }
        syms.push((i, n_value - a, None, name));
    }
    place(&mut sections, syms);
    Image {
        label: format!("Mach-O {}", macho_cpu(m.header.cputype)),
        range,
        sections,
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{self, Spec};
    use super::*;

    fn names(t: &Trail) -> Vec<&str> {
        t.iter().map(|s| s.name.as_str()).collect()
    }

    fn spec() -> Spec {
        Spec::new()
            .section(".text", b"\x55\x48\x89\xe5\xc3\0\0\0")
            .section(".rodata", b"\0\0\0\0hello\0version=1.2.3-needle\0tail\0")
            .symbol("main.greeting", 1, 4)
            .symbol("main.version", 1, 10)
    }

    fn trail_at(file: &[u8], at: usize) -> Vec<String> {
        let m = Map::parse(file).expect("parses");
        names(&m.trail(at, &LineIndex::new(file)))
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn elf_trail_has_image_section_and_symbol_offset() {
        let l = fixture::elf(&spec());
        let at = l.offset(1, 10 + 14); // "needle" in "version=1.2.3-needle"
        assert_eq!(&l.file[at..at + 6], b"needle");
        assert_eq!(
            trail_at(&l.file, at),
            ["ELF x86-64", ".rodata", "main.version+0xe"]
        );
        // On the symbol's first byte the offset is left out.
        assert_eq!(
            trail_at(&l.file, l.offset(1, 10)),
            ["ELF x86-64", ".rodata", "main.version"]
        );
        // Before the first symbol of the section: no symbol Segment.
        assert_eq!(trail_at(&l.file, l.offset(1, 1)), ["ELF x86-64", ".rodata"]);
        // A symbol of another section never names a Match in this one.
        assert_eq!(trail_at(&l.file, l.offset(0, 2)), ["ELF x86-64", ".text"]);
        // In the ELF header: the image only.
        assert_eq!(trail_at(&l.file, 3), ["ELF x86-64"]);
    }

    /// D7: an ELF symbol whose size ends before the Match does not name it.
    #[test]
    fn an_elf_symbol_names_only_its_own_bytes() {
        let s = Spec::new()
            .section(".rodata", b"version=1.2.3\0another needle\0")
            .sized("version", 0, 0, 14);
        let l = fixture::elf(&s);
        assert_eq!(
            trail_at(&l.file, l.offset(0, 2)),
            ["ELF x86-64", ".rodata", "version+0x2"]
        );
        assert_eq!(
            trail_at(&l.file, l.offset(0, 22)),
            ["ELF x86-64", ".rodata"]
        );
    }

    #[test]
    fn a_stripped_elf_has_no_symbol_segment() {
        let mut s = spec();
        s.symbols.clear();
        let l = fixture::elf(&s);
        assert_eq!(
            trail_at(&l.file, l.offset(1, 24)),
            ["ELF x86-64", ".rodata"]
        );
    }

    #[test]
    fn elf_machines_are_named() {
        let l = fixture::elf(&spec().machine(183));
        assert_eq!(trail_at(&l.file, 0)[0], "ELF aarch64");
        let l = fixture::elf(&spec().machine(0x9999));
        assert_eq!(trail_at(&l.file, 0)[0], "ELF machine 0x9999");
    }

    #[test]
    fn pe_trail_uses_the_coff_symbols() {
        let s = Spec::new()
            .section(".text", b"\xc3\0\0\0")
            .section(".rdata", b"\0\0abc\0version=1.2.3-needle\0")
            .symbol("_version_string", 1, 6);
        let l = fixture::pe(&s);
        let at = l.offset(1, 6 + 14);
        assert_eq!(&l.file[at..at + 6], b"needle");
        assert_eq!(
            trail_at(&l.file, at),
            ["PE x86-64", ".rdata", "_version_string+0xe"]
        );
        // The padding after a section's used bytes is in no section.
        assert_eq!(trail_at(&l.file, l.offset(1, 100)), ["PE x86-64"]);
    }

    #[test]
    fn macho_sections_are_segment_comma_section() {
        let s = Spec::new()
            .section("__TEXT,__text", b"\xc3\0\0\0")
            .section("__TEXT,__cstring", b"hi\0version=1.2.3-needle\0")
            .symbol("_version", 1, 3);
        let l = fixture::macho(&s);
        let at = l.offset(1, 3 + 14);
        assert_eq!(&l.file[at..at + 6], b"needle");
        assert_eq!(
            trail_at(&l.file, at),
            ["Mach-O x86-64", "__TEXT,__cstring", "_version+0xe"]
        );
    }

    /// MinGW's section symbols (`.rdata`) and the assembler's `ltmp`/`L` labels are not
    /// symbols a person looks for.
    #[test]
    fn section_symbols_and_local_labels_are_left_out() {
        let s = Spec::new()
            .section(".rdata", b"version=1.2.3-needle\0")
            .symbol(".rdata", 0, 0)
            .symbol("version", 0, 0);
        let l = fixture::pe(&s);
        assert_eq!(trail_at(&l.file, l.offset(0, 14))[2], "version+0xe");
        let s = Spec::new()
            .section("__TEXT,__const", b"version=1.2.3-needle\0")
            .local("ltmp1", 0, 0)
            .symbol("_version", 0, 0)
            .local("L_.str", 0, 2);
        let l = fixture::macho(&s);
        assert_eq!(trail_at(&l.file, l.offset(0, 14))[2], "_version+0xe");
    }

    /// Codex r1 #8: an external Mach-O symbol whose name starts with `l` or `L` is a symbol;
    /// only a local one is an assembler's label.
    #[test]
    fn an_external_macho_symbol_starting_with_l_names_its_bytes() {
        let s = Spec::new()
            .section("__TEXT,__const", b"abcneedle\0")
            .symbol("long_function", 0, 0)
            .symbol("Log", 0, 1)
            .local("lstatic", 0, 2);
        let l = fixture::macho(&s);
        assert_eq!(
            trail_at(&l.file, l.offset(0, 3)),
            ["Mach-O x86-64", "__TEXT,__const", "Log+0x2"]
        );
        let s = Spec::new()
            .section("__TEXT,__const", b"abcneedle\0")
            .symbol("long_function", 0, 0);
        let l = fixture::macho(&s);
        assert_eq!(trail_at(&l.file, l.offset(0, 3))[2], "long_function+0x3");
    }

    #[test]
    fn a_fat_macho_names_each_architecture() {
        let s = Spec::new()
            .section("__TEXT,__cstring", b"hi\0version=1.2.3-needle\0")
            .symbol("_version", 0, 3);
        let x86 = fixture::macho(&s);
        let arm = fixture::macho(&s.clone().machine(0x0100_000c));
        let (fat, starts) = fixture::fat(&[
            (0x0100_0007, x86.file.clone()),
            (0x0100_000c, arm.file.clone()),
        ]);
        let in_arm = starts[1] + arm.offset(0, 17);
        assert_eq!(&fat[in_arm..in_arm + 6], b"needle");
        assert_eq!(
            trail_at(&fat, in_arm),
            ["Mach-O arm64", "__TEXT,__cstring", "_version+0xe"]
        );
        let in_x86 = starts[0] + x86.offset(0, 17);
        assert_eq!(trail_at(&fat, in_x86)[0], "Mach-O x86-64");
        // The fat header belongs to no architecture.
        assert!(trail_at(&fat, 2).is_empty());
    }

    /// Codex M4 code r5 #1: a fat file's slices neither overlap nor repeat, nor cover its arch
    /// table, and Apple's loader reads that table from the first 4 KiB (xnu's `mach_fat.c`):
    /// a fat file that breaks one of these is no object file (D9), and is not parsed once per
    /// arch that names the same bytes.
    #[test]
    fn a_fat_macho_whose_slices_overlap_is_no_object_file() {
        let s = Spec::new()
            .section("__TEXT,__cstring", b"hi\0version=1.2.3-needle\0")
            .symbol("_version", 0, 3);
        let img = fixture::macho(&s).file;
        let n = img.len() as u32;
        let (x86, arm) = (0x0100_0007, 0x0100_000c);
        let one = |entries: &[(u32, u32, u32)]| fixture::fat_raw(entries, &[(0x1000, &img)]);
        let both = |entries: &[(u32, u32, u32)]| {
            fixture::fat_raw(entries, &[(0x1000, &img), (0x2000, &img)])
        };
        // Valid: two slices one after the other, or one of no bytes beside another.
        assert!(Map::parse(&both(&[(x86, 0x1000, n), (arm, 0x2000, n)])).is_some());
        assert!(Map::parse(&one(&[(x86, 0x1000, n), (arm, 0, 0)])).is_some());
        for (why, f) in [
            ("repeated", one(&[(x86, 0x1000, n), (arm, 0x1000, n)])),
            (
                "overlapping",
                both(&[(x86, 0x1000, 0x1001), (arm, 0x2000, n)]),
            ),
            ("over the table", one(&[(x86, 0x1000, n), (arm, 20, 100)])),
            ("4096 repeated", one(&vec![(x86, 0x1000, n); 4096])),
        ] {
            assert_eq!(Map::parse(&f), None, "{why}");
        }
        // 204 arches fit in the first 4 KiB; 205 do not.
        let many = |k: u32| {
            let mut e = vec![(x86, 0x1000, n)];
            e.extend((1..k).map(|i| (arm + i, 0, 0)));
            one(&e)
        };
        assert!(Map::parse(&many(204)).is_some());
        assert_eq!(Map::parse(&many(205)), None);
    }

    /// Fable M4 code r5 #2: a Mach-O symbol whose name is out of the string table or is not
    /// UTF-8 is left out alone: the symbols after it still name their bytes.
    #[test]
    fn a_macho_symbol_with_a_broken_name_leaves_out_only_itself() {
        let s = Spec::new()
            .section("__TEXT,__const", b"aaaaaaaabbbbbbbbccccneedle\0")
            .symbol("_a", 0, 0)
            .symbol("_b", 0, 8)
            .symbol("_c", 0, 16);
        let l = fixture::macho(&s);
        let at = l.offset(0, 20);
        assert_eq!(trail_at(&l.file, at)[2], "_c+0x4");
        // LC_SYMTAB follows the one LC_SEGMENT_64 (72 bytes and an 80-byte section).
        let symtab = 32 + 72 + 80;
        let symoff = u32::from_le_bytes(l.file[symtab + 8..symtab + 12].try_into().unwrap());
        let stroff = u32::from_le_bytes(l.file[symtab + 16..symtab + 20].try_into().unwrap());
        let b = symoff as usize + 16;
        let mut out = l.file.clone();
        out[b..b + 4].copy_from_slice(&0x7fff_0000u32.to_le_bytes());
        assert_eq!(trail_at(&out, at)[2], "_c+0x4");
        assert_eq!(trail_at(&out, l.offset(0, 10))[2], "_a+0xa");
        let mut bad = l.file.clone();
        let name = u32::from_le_bytes(bad[b..b + 4].try_into().unwrap()) as usize;
        bad[stroff as usize + name + 1] = 0xff;
        assert_eq!(trail_at(&bad, at)[2], "_c+0x4");
        assert_eq!(trail_at(&bad, l.offset(0, 10))[2], "_a+0xa");
    }

    /// A name at any offset of a string table, as goblin's `Strtab::get_at` reads one: the
    /// rest of a name from that offset, when it is UTF-8 and not empty.
    #[test]
    fn names_are_found_at_any_offset() {
        let n = Names::new(b"\0_main\0\xff_b\0\0a\xc3\xa9z\0tail");
        assert_eq!(n.get(1), Some("_main"));
        assert_eq!(n.get(3), Some("ain"));
        assert_eq!(n.get(6), None); // its NUL
        assert_eq!(n.get(0), None);
        assert_eq!(n.get(7), None); // `\xff_b`: not UTF-8
        assert_eq!(n.get(8), Some("_b"));
        assert_eq!(n.get(11), None);
        assert_eq!(n.get(12), Some("a\u{e9}z"));
        assert_eq!(n.get(14), None); // inside `\u{e9}`
        assert_eq!(n.get(15), Some("z"));
        assert_eq!(n.get(17), Some("tail"));
        assert_eq!(n.get(21), None);
        assert_eq!(n.get(1000), None);
    }

    #[test]
    fn rust_symbols_are_demangled_without_their_hash() {
        assert_eq!(
            demangle("_ZN4core3fmt5write17h0123456789abcdefE"),
            "core::fmt::write"
        );
        assert_eq!(demangle("_ZN3foo3barEv"), "_ZN3foo3barEv"); // C++: kept
        assert_eq!(demangle("main.version"), "main.version");
    }

    /// Codex M4 code r4 #1 and the implementers' r4 note: goblin 0.10.7 panics on a 64-bit
    /// Mach-O with an `LC_MAIN` whose `__TEXT` is loaded below its file offset (a subtraction
    /// that overflows, goblin/src/mach/mod.rs:280, in a debug build; a release build wraps, and
    /// the file has a Trail). The Processor runs in s4's process: such a file is no object
    /// file. A PE had the same risk, a COFF symbol whose name's offset in the string table is
    /// under 4 (goblin's pe/symbol.rs:240, found fuzzing): the Processor reads that offset
    /// itself since Codex M4 code r5 #1, and leaves the symbol out in both builds.
    #[test]
    fn a_file_goblin_panics_on_is_no_object_file() {
        let s = Spec::new()
            .section("__TEXT,__text", b"\xc3\0\0\0needle\0")
            .entry(0)
            .vmaddr(0);
        let f = fixture::macho(&s).file;
        let got = std::panic::catch_unwind(|| Map::parse(&f));
        let got = got.expect("Map::parse panicked");
        if cfg!(debug_assertions) {
            assert_eq!(got, None);
        }
        let pe = Spec::new()
            .section(".rdata", b"\0needle\0")
            .symbol("version_string", 0, 1);
        let f = fixture::pe_with_name_offset_0(fixture::pe(&pe).file);
        assert_eq!(trail_at(&f, 512 + 1), ["PE x86-64", ".rdata"]);
    }

    /// Found fuzzing for code review r4: a 387-byte Mach-O whose `LC_SYMTAB` says it has 318
    /// million symbols took 4 s in a release build (185 s in debug), every symbol past the
    /// file's end an error that was skipped. The symbols end at the first one that cannot be
    /// read.
    #[test]
    fn a_macho_symbol_count_past_the_file_costs_nothing() {
        let s = Spec::new()
            .section("__TEXT,__const", b"abcneedle\0")
            .symbol("_start", 0, 0);
        let l = fixture::macho(&s);
        let mut f = l.file.clone();
        // LC_SYMTAB follows the one LC_SEGMENT_64 (72 bytes and an 80-byte section).
        let symtab = 32 + 72 + 80;
        assert_eq!(f[symtab], 0x2);
        f[symtab + 12..symtab + 16].copy_from_slice(&0x1300_0001u32.to_le_bytes());
        let started = std::time::Instant::now();
        assert_eq!(
            trail_at(&f, l.offset(0, 3)),
            ["Mach-O x86-64", "__TEXT,__const", "_start+0x3"]
        );
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_millis(500), "{took:?}");
    }

    #[test]
    fn what_goblin_cannot_read_is_none() {
        assert!(Map::parse(b"").is_none());
        assert!(Map::parse(b"\x7fELF garbage").is_none());
        assert!(Map::parse(b"MZ but not a PE file at all, just text\n").is_none());
        assert!(Map::parse(b"plain text\n").is_none());
    }

    #[test]
    fn segments_carry_the_lines_rg_a_counts() {
        let s = Spec::new()
            .section(".rodata", b"a\nb\nneedle\n")
            .symbol("s", 0, 4);
        let l = fixture::elf(&s);
        let m = Map::parse(&l.file).unwrap();
        let idx = LineIndex::new(&l.file);
        let at = l.offset(0, 4);
        let t = m.trail(at, &idx);
        let before = memchr::memchr_iter(b'\n', &l.file[..at]).count() as u64;
        assert_eq!(t[2].line, before + 1);
        assert_eq!(t[2].kind, "symbol");
        assert_eq!(t[1].kind, "section");
        assert_eq!(t[0].kind, "object");
    }

    /// Linux's `/bin/ls` is an ELF that links glibc; macOS's is a Mach-O (M4 plan r1, Fable #7).
    #[cfg(target_os = "linux")]
    #[test]
    fn the_real_ls_binary_parses() {
        // The machine's own ELF, when there is one: a smoke test, not a fixture.
        let Ok(bytes) = std::fs::read("/bin/ls") else {
            return;
        };
        let m = Map::parse(&bytes).expect("/bin/ls parses");
        let at = memchr::memmem::find(&bytes, b"GLIBC_2.").unwrap();
        let t = m.trail(at, &LineIndex::new(&bytes));
        assert!(t[0].name.starts_with("ELF "), "{t:?}");
        assert!(t.len() >= 2, "{t:?}");
    }
}
