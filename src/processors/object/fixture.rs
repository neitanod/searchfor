//! Small object files written byte by byte, for the tests of the `object` Processor, the
//! end-to-end tests, the fuzz and the perf ceilings (M4 plan, D19). Not used by `s4` itself.
//! Each writer lays out its sections in order after the headers, so a test knows where every
//! byte lands: `Spec::offset` gives the file offset of a byte of a section.

/// What to write: sections (name, bytes) and symbols (name, section index, offset in it, size;
/// only ELF writes the size, 0 meaning unknown).
#[derive(Debug, Clone, Default)]
pub struct Spec {
    pub sections: Vec<(String, Vec<u8>)>,
    pub symbols: Vec<(String, usize, u64, u64)>,
    /// The machine: an ELF `e_machine`, a PE `Machine` or a Mach-O `cputype`; 0 picks x86-64.
    pub machine: u32,
    /// The symbols Mach-O writes without `N_EXT`, by name: the others are external.
    pub locals: Vec<String>,
    /// Mach-O only: an `LC_MAIN` with this entry offset.
    pub entry: Option<u64>,
    /// Mach-O only: the address every segment is loaded at, instead of 0x1_0000_0000 plus its
    /// file offset (one lower than its file offset is malformed).
    pub vmaddr: Option<u64>,
}

impl Spec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn section(mut self, name: &str, bytes: &[u8]) -> Self {
        self.sections.push((name.into(), bytes.to_vec()));
        self
    }

    pub fn symbol(self, name: &str, section: usize, offset: u64) -> Self {
        self.sized(name, section, offset, 0)
    }

    /// A symbol that is not external (Mach-O only; the other writers write it as any symbol).
    pub fn local(mut self, name: &str, section: usize, offset: u64) -> Self {
        self.locals.push(name.into());
        self.sized(name, section, offset, 0)
    }

    pub fn sized(mut self, name: &str, section: usize, offset: u64, size: u64) -> Self {
        self.symbols.push((name.into(), section, offset, size));
        self
    }

    pub fn machine(mut self, m: u32) -> Self {
        self.machine = m;
        self
    }

    pub fn entry(mut self, offset: u64) -> Self {
        self.entry = Some(offset);
        self
    }

    pub fn vmaddr(mut self, addr: u64) -> Self {
        self.vmaddr = Some(addr);
        self
    }
}

fn pad(out: &mut Vec<u8>, align: usize) {
    while !out.len().is_multiple_of(align) {
        out.push(0);
    }
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}
fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}
fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend(v.to_le_bytes());
}
fn name16(out: &mut Vec<u8>, name: &str) {
    let mut n = [0u8; 16];
    n[..name.len().min(16)].copy_from_slice(&name.as_bytes()[..name.len().min(16)]);
    out.extend(n);
}

/// Where each section's bytes start in the file, as the writers place them.
pub struct Layout {
    pub file: Vec<u8>,
    pub starts: Vec<usize>,
}

impl Layout {
    /// The file offset of byte `at` of section `section`.
    pub fn offset(&self, section: usize, at: usize) -> usize {
        self.starts[section] + at
    }
}

/// ELF64 little-endian shared object (`ET_DYN`), its sections loaded at 0x400000 plus their
/// file offset, a `.symtab` when `spec.symbols` is not empty, and `.shstrtab` last.
pub fn elf(spec: &Spec) -> Layout {
    const BASE: u64 = 0x40_0000;
    let machine = if spec.machine == 0 {
        62
    } else {
        spec.machine as u16
    };
    let mut out = vec![0u8; 64];
    let mut starts = Vec::new();
    for (_, data) in &spec.sections {
        pad(&mut out, 16);
        starts.push(out.len());
        out.extend(data);
    }
    // .strtab and .symtab
    let mut strtab = vec![0u8];
    let mut symtab = vec![0u8; 24];
    for (name, sec, off, size) in &spec.symbols {
        let st_name = strtab.len() as u32;
        strtab.extend(name.as_bytes());
        strtab.push(0);
        put32(&mut symtab, st_name);
        symtab.push(0x11); // STB_GLOBAL, STT_OBJECT
        symtab.push(0);
        put16(&mut symtab, *sec as u16 + 1);
        put64(&mut symtab, BASE + (starts[*sec] as u64) + off);
        put64(&mut symtab, *size);
    }
    let with_syms = !spec.symbols.is_empty();
    let mut shstrtab = vec![0u8];
    let mut names = Vec::new();
    let mut all: Vec<&str> = spec.sections.iter().map(|(n, _)| n.as_str()).collect();
    if with_syms {
        all.extend([".symtab", ".strtab"]);
    }
    all.push(".shstrtab");
    for n in &all {
        names.push(shstrtab.len() as u32);
        shstrtab.extend(n.as_bytes());
        shstrtab.push(0);
    }
    let mut extra = Vec::new(); // (offset, size) of symtab, strtab, shstrtab
    if with_syms {
        pad(&mut out, 8);
        extra.push((out.len(), symtab.len()));
        out.extend(&symtab);
        extra.push((out.len(), strtab.len()));
        out.extend(&strtab);
    }
    extra.push((out.len(), shstrtab.len()));
    out.extend(&shstrtab);
    pad(&mut out, 8);
    let shoff = out.len();
    let n = spec.sections.len();
    let shnum = 1 + all.len();
    out.extend([0u8; 64]); // SHT_NULL
    let header = |out: &mut Vec<u8>,
                  name: u32,
                  ty: u32,
                  flags: u64,
                  addr: u64,
                  off: usize,
                  size: usize,
                  link: u32,
                  info: u32,
                  ent: u64| {
        put32(out, name);
        put32(out, ty);
        put64(out, flags);
        put64(out, addr);
        put64(out, off as u64);
        put64(out, size as u64);
        put32(out, link);
        put32(out, info);
        put64(out, 1);
        put64(out, ent);
    };
    for (i, (_, data)) in spec.sections.iter().enumerate() {
        let off = starts[i];
        header(
            &mut out,
            names[i],
            1,
            2,
            BASE + off as u64,
            off,
            data.len(),
            0,
            0,
            0,
        );
    }
    let mut k = n;
    if with_syms {
        let (o, s) = extra[0];
        header(&mut out, names[k], 2, 0, 0, o, s, (n + 2) as u32, 1, 24);
        k += 1;
        let (o, s) = extra[1];
        header(&mut out, names[k], 3, 0, 0, o, s, 0, 0, 0);
        k += 1;
    }
    let (o, s) = *extra.last().unwrap();
    header(&mut out, names[k], 3, 0, 0, o, s, 0, 0, 0);
    let mut h = Vec::new();
    h.extend(b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0");
    put16(&mut h, 3); // ET_DYN
    put16(&mut h, machine);
    put32(&mut h, 1);
    put64(&mut h, 0); // entry
    put64(&mut h, 0); // phoff
    put64(&mut h, shoff as u64);
    put32(&mut h, 0);
    put16(&mut h, 64);
    put16(&mut h, 56);
    put16(&mut h, 0);
    put16(&mut h, 64);
    put16(&mut h, shnum as u16);
    put16(&mut h, (shnum - 1) as u16);
    out[..64].copy_from_slice(&h);
    Layout { file: out, starts }
}

/// PE32+ image: DOS header, PE header, sections aligned to 0x200 in the file and 0x1000 in
/// memory, and a COFF symbol table (as MinGW leaves in its images) when `spec.symbols` is not
/// empty.
pub fn pe(spec: &Spec) -> Layout {
    let machine = if spec.machine == 0 {
        0x8664
    } else {
        spec.machine as u16
    };
    let n = spec.sections.len();
    let headers = 0x40 + 4 + 20 + 240 + 40 * n;
    let mut starts = Vec::new();
    let mut at = headers.div_ceil(0x200) * 0x200;
    for (_, d) in &spec.sections {
        starts.push(at);
        at += d.len().div_ceil(0x200).max(1) * 0x200;
    }
    let symtab_at = at;
    let mut out = Vec::new();
    out.extend(b"MZ");
    out.resize(0x3c, 0);
    put32(&mut out, 0x40);
    out.extend(b"PE\0\0");
    put16(&mut out, machine);
    put16(&mut out, n as u16);
    put32(&mut out, 0);
    put32(
        &mut out,
        if spec.symbols.is_empty() {
            0
        } else {
            symtab_at as u32
        },
    );
    put32(&mut out, spec.symbols.len() as u32);
    put16(&mut out, 240);
    put16(&mut out, 0x22);
    // Optional header (PE32+).
    put16(&mut out, 0x20b);
    out.extend([14, 0]);
    put32(&mut out, 0); // SizeOfCode
    put32(&mut out, 0);
    put32(&mut out, 0);
    put32(&mut out, 0x1000); // AddressOfEntryPoint
    put32(&mut out, 0x1000); // BaseOfCode
    put64(&mut out, 0x1_4000_0000);
    put32(&mut out, 0x1000);
    put32(&mut out, 0x200);
    put16(&mut out, 6);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, 6);
    put16(&mut out, 0);
    put32(&mut out, 0);
    put32(&mut out, (0x1000 * (n + 1)) as u32); // SizeOfImage
    put32(&mut out, starts.first().copied().unwrap_or(0x200) as u32); // SizeOfHeaders
    put32(&mut out, 0);
    put16(&mut out, 3);
    put16(&mut out, 0);
    for _ in 0..4 {
        put64(&mut out, 0x1000);
    }
    put32(&mut out, 0);
    put32(&mut out, 16);
    out.extend([0u8; 128]);
    for (i, (name, d)) in spec.sections.iter().enumerate() {
        let mut nm = [0u8; 8];
        nm[..name.len().min(8)].copy_from_slice(&name.as_bytes()[..name.len().min(8)]);
        out.extend(nm);
        put32(&mut out, d.len() as u32);
        put32(&mut out, (0x1000 * (i + 1)) as u32);
        put32(&mut out, (d.len().div_ceil(0x200).max(1) * 0x200) as u32);
        put32(&mut out, starts[i] as u32);
        put32(&mut out, 0);
        put32(&mut out, 0);
        put16(&mut out, 0);
        put16(&mut out, 0);
        put32(&mut out, 0x4000_0040); // initialized data, readable
    }
    for (i, (_, d)) in spec.sections.iter().enumerate() {
        out.resize(starts[i], 0);
        out.extend(d);
    }
    out.resize(symtab_at, 0);
    if !spec.symbols.is_empty() {
        let mut strings = Vec::new();
        for (name, sec, off, _) in &spec.symbols {
            if name.len() <= 8 {
                let mut nm = [0u8; 8];
                nm[..name.len()].copy_from_slice(name.as_bytes());
                out.extend(nm);
            } else {
                put32(&mut out, 0);
                put32(&mut out, 4 + strings.len() as u32);
                strings.extend(name.as_bytes());
                strings.push(0);
            }
            put32(&mut out, *off as u32);
            out.extend((*sec as i16 + 1).to_le_bytes());
            put16(&mut out, 0);
            out.push(2); // IMAGE_SYM_CLASS_EXTERNAL
            out.push(0);
        }
        put32(&mut out, 4 + strings.len() as u32);
        out.extend(&strings);
    }
    Layout { file: out, starts }
}

/// Mach-O 64-bit little-endian executable: one `LC_SEGMENT_64` per segment name, in the order
/// the sections first name them (`__TEXT,__cstring` names segment `__TEXT`, section
/// `__cstring`), an `LC_SYMTAB` when `spec.symbols` is not empty, and an `LC_MAIN` with
/// `spec.entry`. Sections are loaded at 0x1_0000_0000 plus their file offset.
pub fn macho(spec: &Spec) -> Layout {
    const BASE: u64 = 0x1_0000_0000;
    let cputype = if spec.machine == 0 {
        0x0100_0007
    } else {
        spec.machine
    };
    let mut segs: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, (name, _)) in spec.sections.iter().enumerate() {
        let seg = name.split(',').next().unwrap_or("").to_string();
        match segs.iter_mut().find(|s| s.0 == seg) {
            Some(s) => s.1.push(i),
            None => segs.push((seg, vec![i])),
        }
    }
    let with_syms = !spec.symbols.is_empty();
    let with_main = spec.entry.is_some();
    let cmds: usize = segs.iter().map(|s| 72 + 80 * s.1.len()).sum::<usize>()
        + if with_syms { 24 } else { 0 }
        + if with_main { 24 } else { 0 };
    let mut at = 32 + cmds;
    let mut starts = vec![0; spec.sections.len()];
    for (_, idx) in &segs {
        for &i in idx {
            at = at.div_ceil(16) * 16;
            starts[i] = at;
            at += spec.sections[i].1.len();
        }
    }
    let symoff = at.div_ceil(8) * 8;
    let mut strtab = vec![b' ', 0];
    let mut syms = Vec::new();
    // n_sect is 1-based over every section in load-command order.
    let order: Vec<usize> = segs.iter().flat_map(|s| s.1.iter().copied()).collect();
    for (name, sec, off, _) in &spec.symbols {
        put32(&mut syms, strtab.len() as u32);
        strtab.extend(name.as_bytes());
        strtab.push(0);
        // N_SECT, and N_EXT unless the symbol is local.
        syms.push(if spec.locals.contains(name) {
            0x0e
        } else {
            0x0f
        });
        syms.push(order.iter().position(|&s| s == *sec).unwrap() as u8 + 1);
        put16(&mut syms, 0);
        put64(&mut syms, BASE + starts[*sec] as u64 + off);
    }
    let stroff = symoff + syms.len();
    let mut out = Vec::new();
    put32(&mut out, 0xfeed_facf);
    put32(&mut out, cputype);
    put32(&mut out, 3);
    put32(&mut out, 2); // MH_EXECUTE
    put32(
        &mut out,
        (segs.len() + usize::from(with_syms) + usize::from(with_main)) as u32,
    );
    put32(&mut out, cmds as u32);
    put32(&mut out, 0);
    put32(&mut out, 0);
    for (seg, idx) in &segs {
        let lo = idx.iter().map(|&i| starts[i]).min().unwrap_or(0);
        let hi = idx
            .iter()
            .map(|&i| starts[i] + spec.sections[i].1.len())
            .max()
            .unwrap_or(0);
        put32(&mut out, 0x19);
        put32(&mut out, (72 + 80 * idx.len()) as u32);
        name16(&mut out, seg);
        put64(&mut out, spec.vmaddr.unwrap_or(BASE + lo as u64));
        put64(&mut out, (hi - lo) as u64);
        put64(&mut out, lo as u64);
        put64(&mut out, (hi - lo) as u64);
        put32(&mut out, 5);
        put32(&mut out, 5);
        put32(&mut out, idx.len() as u32);
        put32(&mut out, 0);
        for &i in idx {
            let (name, d) = &spec.sections[i];
            let sect = name.split(',').nth(1).unwrap_or(name);
            name16(&mut out, sect);
            name16(&mut out, seg);
            put64(&mut out, BASE + starts[i] as u64);
            put64(&mut out, d.len() as u64);
            put32(&mut out, starts[i] as u32);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
            put32(&mut out, 0);
        }
    }
    if with_syms {
        put32(&mut out, 0x2);
        put32(&mut out, 24);
        put32(&mut out, symoff as u32);
        put32(&mut out, spec.symbols.len() as u32);
        put32(&mut out, stroff as u32);
        put32(&mut out, strtab.len() as u32);
    }
    if let Some(entry) = spec.entry {
        put32(&mut out, 0x8000_0028); // LC_MAIN
        put32(&mut out, 24);
        put64(&mut out, entry);
        put64(&mut out, 0);
    }
    for &i in &order {
        out.resize(starts[i], 0);
        out.extend(&spec.sections[i].1);
    }
    if with_syms {
        out.resize(symoff, 0);
        out.extend(&syms);
        out.extend(&strtab);
    }
    Layout { file: out, starts }
}

/// `file`, a PE written by `pe` whose first symbol's name is in the string table, with that
/// name's offset made 0, inside the table's length field: malformed (goblin 0.10.7 subtracts 4
/// from it).
pub fn pe_with_name_offset_0(mut file: Vec<u8>) -> Vec<u8> {
    let symtab = u32::from_le_bytes(file[0x4c..0x50].try_into().unwrap()) as usize;
    assert_eq!(
        &file[symtab..symtab + 4],
        &[0; 4],
        "the name is in the string table"
    );
    file[symtab + 4..symtab + 8].fill(0);
    file
}

/// A fat (universal) Mach-O holding `arches`, each `(cputype, file)`, at 4 KiB boundaries.
/// Returns the file and where each arch starts.
pub fn fat(arches: &[(u32, Vec<u8>)]) -> (Vec<u8>, Vec<usize>) {
    let mut out = Vec::new();
    out.extend(0xcafe_babeu32.to_be_bytes());
    out.extend((arches.len() as u32).to_be_bytes());
    let mut at = 4096;
    let mut starts = Vec::new();
    for (cpu, f) in arches {
        starts.push(at);
        for v in [*cpu, 3, at as u32, f.len() as u32, 12] {
            out.extend(v.to_be_bytes());
        }
        at = (at + f.len()).div_ceil(4096) * 4096;
    }
    for ((_, f), s) in arches.iter().zip(&starts) {
        out.resize(*s, 0);
        out.extend(f);
    }
    (out, starts)
}

/// A fat Mach-O whose arch table is `entries`, each `(cputype, offset, size)`, with each of
/// `images` written at its offset: the ranges may overlap or repeat, which a fat file must not.
pub fn fat_raw(entries: &[(u32, u32, u32)], images: &[(usize, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(0xcafe_babeu32.to_be_bytes());
    out.extend((entries.len() as u32).to_be_bytes());
    for &(cpu, at, size) in entries {
        for v in [cpu, 3, at, size, 12] {
            out.extend(v.to_be_bytes());
        }
    }
    for &(at, f) in images {
        if out.len() < at + f.len() {
            out.resize(at + f.len(), 0);
        }
        out[at..at + f.len()].copy_from_slice(f);
    }
    out
}

/// `file`, an ELF, PE or Mach-O written by these writers, with every symbol's name made the
/// longest one: the name's bytes are in the file once, and the symbol tables name them once per
/// symbol.
pub fn shared_names(mut file: Vec<u8>) -> Vec<u8> {
    let u32_at = |f: &[u8], at: usize| u32::from_le_bytes(f[at..at + 4].try_into().unwrap());
    match goblin::Object::parse(&file).expect("a file of these writers") {
        goblin::Object::Elf(e) => {
            let sh = e
                .section_headers
                .iter()
                .find(|s| s.sh_type == goblin::elf::section_header::SHT_SYMTAB)
                .expect("a .symtab");
            let (at, n) = (sh.sh_offset as usize, sh.sh_size as usize / 24);
            let longest = (1..n)
                .map(|i| u32_at(&file, at + 24 * i))
                .max_by_key(|&o| e.strtab.get_at(o as usize).map_or(0, str::len))
                .expect("a symbol");
            drop(e);
            for i in 1..n {
                file[at + 24 * i..at + 24 * i + 4].copy_from_slice(&longest.to_le_bytes());
            }
        }
        goblin::Object::Mach(goblin::mach::Mach::Binary(m)) => {
            let (at, n) = m
                .load_commands
                .iter()
                .find_map(|c| match c.command {
                    goblin::mach::load_command::CommandVariant::Symtab(s) => {
                        Some((s.symoff as usize, s.nsyms as usize))
                    }
                    _ => None,
                })
                .expect("an LC_SYMTAB");
            let longest = m
                .symbols()
                .flatten()
                .enumerate()
                .max_by_key(|(_, (name, _))| name.len())
                .map(|(i, _)| u32_at(&file, at + 16 * i))
                .expect("a symbol");
            drop(m);
            for i in 0..n {
                file[at + 16 * i..at + 16 * i + 4].copy_from_slice(&longest.to_le_bytes());
            }
        }
        goblin::Object::PE(p) => {
            let coff = p.header.coff_header;
            let (at, n) = (
                coff.pointer_to_symbol_table as usize,
                coff.number_of_symbol_table as usize,
            );
            drop(p);
            let strings = at + 18 * n;
            // The names in the string table, by their offset in it.
            let longest = (0..n)
                .filter(|i| u32_at(&file, at + 18 * i) == 0)
                .map(|i| u32_at(&file, at + 18 * i + 4))
                .max_by_key(|&o| {
                    let s = &file[strings + o as usize..];
                    memchr::memchr(0, s).unwrap_or(s.len())
                })
                .expect("a name in the string table");
            for i in 0..n {
                file[at + 18 * i..at + 18 * i + 4].fill(0);
                file[at + 18 * i + 4..at + 18 * i + 8].copy_from_slice(&longest.to_le_bytes());
            }
        }
        _ => panic!("not a file of these writers"),
    }
    file
}

/// `file`, a Mach-O written by `macho`, whose first load command, a segment, is read `n` times:
/// the header says `n` commands and the first one's size is 0, so each starts where it does.
/// The segment says it holds `nsects` sections, more than its command has room for; the file
/// is grown with zeros so that they all fit in it.
pub fn macho_repeated_segment(mut file: Vec<u8>, n: u32, nsects: u32) -> Vec<u8> {
    let len = (32 + 72 + 80 * nsects as usize).max(8 * n as usize);
    if file.len() < len {
        file.resize(len, 0);
    }
    file[16..20].copy_from_slice(&n.to_le_bytes());
    file[20..24].copy_from_slice(&(8 * n).to_le_bytes());
    assert_eq!(file[32], 0x19, "the first command is an LC_SEGMENT_64");
    file[36..40].copy_from_slice(&0u32.to_le_bytes());
    file[32 + 64..32 + 68].copy_from_slice(&nsects.to_le_bytes());
    file
}

/// `file`, a PE32+ written by `pe` whose first section has room for them, with an import table
/// at that section's start: `dlls` import descriptors that all name one lookup table of
/// `entries` imports by ordinal.
pub fn pe_shared_imports(mut file: Vec<u8>, start: usize, dlls: usize, entries: usize) -> Vec<u8> {
    const RVA: usize = 0x1000;
    let table = 20 * (dlls + 1);
    let name = table + 8 * (entries + 1);
    assert!(file.len() >= start + name + 6, "the section has room");
    for i in 0..dlls {
        let d = start + 20 * i;
        for (k, v) in [(0, RVA + table), (12, RVA + name), (16, RVA + table)] {
            file[d + k..d + k + 4].copy_from_slice(&(v as u32).to_le_bytes());
        }
    }
    for i in 0..entries {
        let e = start + table + 8 * i;
        file[e..e + 8].copy_from_slice(&(0x8000_0000_0000_0001u64 + i as u64).to_le_bytes());
    }
    file[start + name..start + name + 6].copy_from_slice(b"a.dll\0");
    // The import table's entry of the data directories (PE32+: at 0x58 + 112, the second).
    file[0xd0..0xd4].copy_from_slice(&(RVA as u32).to_le_bytes());
    file[0xd4..0xd8].copy_from_slice(&(table as u32).to_le_bytes());
    file
}

/// The bytes an export table of `exports` takes in `pe_exports`.
pub fn pe_exports_size(exports: &[(&str, u32)]) -> usize {
    let mut names: Vec<&str> = exports.iter().map(|e| e.0).collect();
    names.sort_unstable();
    names.dedup();
    40 + 10 * exports.len() + names.iter().map(|n| n.len() + 1).sum::<usize>()
}

/// `file`, a PE32+ written by `pe` with a section at file offset `start` and RVA `rva` that has
/// room for it (`pe_exports_size`; `pe` loads section `i` at `0x1000 * (i + 1)`), with an
/// export table at that section's start: one export per `(name, rva)` of `exports`, ordinal `i`
/// the `i`-th, each distinct name written once and named by every export that has it.
pub fn pe_exports(mut file: Vec<u8>, start: usize, rva: u32, exports: &[(&str, u32)]) -> Vec<u8> {
    let base = rva as usize;
    let n = exports.len();
    let (funcs, names, ords) = (40, 40 + 4 * n, 40 + 8 * n);
    let mut strings = 40 + 10 * n;
    assert!(
        file.len() >= start + pe_exports_size(exports),
        "the section has room"
    );
    let mut at = |o: usize, v: &[u8]| file[start + o..start + o + v.len()].copy_from_slice(v);
    at(20, &(n as u32).to_le_bytes()); // NumberOfFunctions
    at(24, &(n as u32).to_le_bytes()); // NumberOfNames
    at(28, &((base + funcs) as u32).to_le_bytes());
    at(32, &((base + names) as u32).to_le_bytes());
    at(36, &((base + ords) as u32).to_le_bytes());
    let mut written = std::collections::HashMap::new();
    for (i, (name, rva)) in exports.iter().enumerate() {
        let o = match written.get(name) {
            Some(&o) => o,
            None => {
                let o = strings;
                strings += name.len() + 1;
                at(o, name.as_bytes());
                written.insert(*name, o);
                o
            }
        };
        at(funcs + 4 * i, &rva.to_le_bytes());
        at(names + 4 * i, &((base + o) as u32).to_le_bytes());
        at(ords + 2 * i, &(i as u16).to_le_bytes());
    }
    // The export table's entry of the data directories (PE32+: at 0x58 + 112, the first).
    file[0xc8..0xcc].copy_from_slice(&(base as u32).to_le_bytes());
    file[0xcc..0xd0].copy_from_slice(&(strings as u32).to_le_bytes());
    file
}
