//! The `object` Processor (spec §5.7): the Trail of a Match in an ELF, PE or Mach-O file is
//! `ELF x86-64 › .rodata › main.version+0x10`: the image, the section the Match's bytes are in,
//! and the closest symbol of that section at or before them. Parsed with goblin (M4 plan, D3).
#[doc(hidden)]
pub mod fixture;
pub mod rows;

use crate::model::{Input, Name, Segment, Trail};
use crate::trail::LineIndex;
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
struct Sect {
    name: String,
    file: Range<usize>,
    symbols: Vec<Sym>,
}

/// A symbol at file offset `at`; `end` is where it ends when the table says (ELF's `st_size`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sym {
    at: usize,
    end: Option<usize>,
    name: String,
}

/// One image: a whole ELF or PE file, or one architecture of a fat Mach-O.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Image {
    label: String,
    range: Range<usize>,
    /// Sorted by start; sections without bytes in the file are left out.
    sections: Vec<Sect>,
}

/// What the `object` Processor knows of a file: its images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    images: Vec<Image>,
}

impl Map {
    /// None when goblin does not read `bytes` as an ELF, PE or Mach-O file (the file is then
    /// Plain, without a warning: M4 plan, D9), also when it panics on them: goblin 0.10.7 does
    /// arithmetic on the file's values that overflows in a debug build (Codex M4 code r4 #1),
    /// and the Processor runs in s4's process.
    pub fn parse(bytes: &[u8]) -> Option<Map> {
        quietly(|| Map::read(bytes))
    }

    fn read(bytes: &[u8]) -> Option<Map> {
        let images = match goblin::Object::parse(bytes).ok()? {
            goblin::Object::Elf(e) => vec![elf(&e, bytes.len())],
            goblin::Object::PE(p) => vec![pe(&p, bytes)],
            goblin::Object::Mach(goblin::mach::Mach::Binary(m)) => vec![macho(&m, 0..bytes.len())],
            goblin::Object::Mach(goblin::mach::Mach::Fat(f)) => {
                let mut v = Vec::new();
                for (i, arch) in f.iter_arches().enumerate() {
                    let arch = arch.ok()?;
                    let start = arch.offset as usize;
                    let end = start.checked_add(arch.size as usize)?.min(bytes.len());
                    if let Ok(goblin::mach::SingleArch::MachO(m)) = f.get(i) {
                        let mut img = macho(&m, start..end);
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
                v
            }
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
            let shown = if at == sym.at {
                sym.name.clone()
            } else {
                format!("{}+0x{:x}", sym.name, at - sym.at)
            };
            t.push(segment("symbol", &shown, &(sym.at..end), lines));
        }
        t
    }
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
type RawSym = (usize, u64, Option<u64>, String);

/// Adds the symbols to the sections they fall in.
fn place(sections: &mut [Sect], syms: Vec<RawSym>) {
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
        s.symbols.push(Sym {
            at,
            end,
            name: demangle(&name),
        });
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

fn elf(e: &goblin::elf::Elf, len: usize) -> Image {
    use goblin::elf::section_header::{SHT_NOBITS, SHT_NULL};
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
        let name = e.shdr_strtab.get_at(sh.sh_name).unwrap_or("").to_string();
        index[i] = sections.len();
        addr[i] = sh.sh_addr;
        sections.push(Sect {
            name,
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
            let name = strs.get_at(s.st_name).unwrap_or("").to_string();
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

fn pe(p: &goblin::pe::PE, bytes: &[u8]) -> Image {
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
            name: s.name().unwrap_or("").to_string(),
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
            syms.push((i, rva - rvas[i].0, None, name.to_string()));
        }
    }
    // A COFF symbol table, which MinGW leaves in images that were not stripped.
    let coff = &p.header.coff_header;
    if let (Ok(Some(table)), Ok(Some(strings))) = (coff.symbols(bytes), coff.strings(bytes)) {
        for (_, _, s) in table.iter() {
            // External and static symbols, except the static ones that define a section
            // (`.rdata`, with an auxiliary record) and `.`-names.
            if s.section_number <= 0 || s.storage_class != 2 && s.storage_class != 3 {
                continue;
            }
            if s.is_section_definition() {
                continue;
            }
            if let Ok(name) = s.name(&strings)
                && !name.starts_with('.')
            {
                syms.push((
                    s.section_number as usize - 1,
                    s.value as u64,
                    None,
                    name.to_string(),
                ));
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

/// `range` is the slice of the file this Mach-O is; its offsets are relative to it.
fn macho(m: &goblin::mach::MachO, range: Range<usize>) -> Image {
    let len = range.len();
    let mut sections = Vec::new();
    let mut addrs = Vec::new();
    for seg in m.segments.iter() {
        let Ok(list) = seg.sections() else { continue };
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
                name,
                file: start..end,
                symbols: Vec::new(),
            });
        }
    }
    let mut syms = Vec::new();
    for (name, nl) in m.symbols().flatten() {
        const N_STAB: u8 = 0xe0;
        const N_TYPE: u8 = 0x0e;
        const N_SECT: u8 = 0x0e;
        if nl.n_type & N_STAB != 0 || nl.n_type & N_TYPE != N_SECT || nl.n_sect == 0 {
            continue;
        }
        // A local `l…` or `L…` is an assembler's label (`ltmp1`, `L_.str`), not a symbol; an
        // external one is a symbol (Codex r1 #8).
        const N_EXT: u8 = 0x01;
        if nl.n_type & N_EXT == 0 && (name.starts_with('l') || name.starts_with('L')) {
            continue;
        }
        let i = nl.n_sect - 1;
        let Some(&a) = addrs.get(i) else { continue };
        if nl.n_value < a {
            continue;
        }
        syms.push((i, nl.n_value - a, None, name.to_string()));
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
    /// that overflows, goblin/src/mach/mod.rs:280, in a debug build; a release build wraps).
    /// The Processor runs in s4's process: such a file is no object file. A PE has the same
    /// risk: a COFF symbol whose name's offset in the string table is under 4 (pe/symbol.rs:240,
    /// found fuzzing); none was found in ELF files.
    #[test]
    fn a_file_goblin_panics_on_is_no_object_file() {
        let s = Spec::new()
            .section("__TEXT,__text", b"\xc3\0\0\0needle\0")
            .entry(0)
            .vmaddr(0);
        let pe = Spec::new()
            .section(".rdata", b"\0needle\0")
            .symbol("version_string", 0, 1);
        for f in [
            fixture::macho(&s).file,
            fixture::pe_with_name_offset_0(fixture::pe(&pe).file),
        ] {
            let got = std::panic::catch_unwind(|| Map::parse(&f));
            let got = got.expect("Map::parse panicked");
            if cfg!(debug_assertions) {
                assert_eq!(got, None);
            }
        }
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
