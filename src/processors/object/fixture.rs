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
/// `__cstring`), and an `LC_SYMTAB` when `spec.symbols` is not empty. Sections are loaded at
/// 0x1_0000_0000 plus their file offset.
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
    let cmds: usize =
        segs.iter().map(|s| 72 + 80 * s.1.len()).sum::<usize>() + if with_syms { 24 } else { 0 };
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
    put32(&mut out, (segs.len() + usize::from(with_syms)) as u32);
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
        put64(&mut out, BASE + lo as u64);
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
