//! Java class files (M6 spec §9): `Java 17 › com.example.Cart › addItem` (a method), `› NEEDLE`
//! (a field), `› constant pool`. The image is named from the class version, the class in dotted
//! form. A Match in a constant-pool string belongs to the first method, in declaration order,
//! whose code loads it (`ldc`, `ldc_w`, found by walking the bytecode with the JVM's
//! instruction lengths), else to the field whose `ConstantValue` it is or the member whose
//! descriptor it is, else to the constant pool; a Match in a member's own name has the class's
//! Trail (M6 spec §3 rule 5). Labels are decoded from modified UTF-8; the bytes rg searches are
//! never transformed.
use super::layouts::{Builder, sub, u16_at, u32_at};
use std::borrow::Cow;
use std::ops::Range;

/// One constant-pool entry: its tag, its bytes, and for a Utf8 the bytes of its text, for a
/// Class or a String the index it names.
#[derive(Clone)]
struct Entry {
    tag: u8,
    bytes: Range<usize>,
    text: Range<usize>,
    index: u16,
}

/// A field or a method: its bytes, the indexes of its name and descriptor, and the bytes of its
/// attributes (name index, contents).
struct Member {
    bytes: Range<usize>,
    name: u16,
    descriptor: u16,
    attributes: Vec<(u16, Range<usize>)>,
}

/// Who a constant-pool entry belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Pool,
    Class,
    Field(usize),
    Method(usize),
}

/// The fields or the methods at `at`: (them, the byte after them).
fn members(bytes: &[u8], mut at: usize, all: &Range<usize>) -> Option<(Vec<Member>, usize)> {
    let count = u16_at(bytes, at, false)? as usize;
    at += 2;
    let mut out = Vec::with_capacity(count.min((all.end - at) / 8));
    for _ in 0..count {
        let start = at;
        let name = u16_at(bytes, at + 2, false)?;
        let descriptor = u16_at(bytes, at + 4, false)?;
        let (attributes, end) = attributes(bytes, at + 6, all)?;
        at = end;
        out.push(Member {
            bytes: start..end,
            name,
            descriptor,
            attributes,
        });
    }
    Some((out, at))
}

/// The attributes at `at`: (name index and contents of each, the byte after them).
fn attributes(
    bytes: &[u8],
    mut at: usize,
    all: &Range<usize>,
) -> Option<(Vec<(u16, Range<usize>)>, usize)> {
    let count = u16_at(bytes, at, false)? as usize;
    at += 2;
    let mut out = Vec::with_capacity(count.min((all.end - at) / 6));
    for _ in 0..count {
        let name = u16_at(bytes, at, false)?;
        let len = u32_at(bytes, at + 2, false)? as usize;
        let body = sub(all, at + 6, len)?;
        at = body.end;
        out.push((name, body));
    }
    Some((out, at))
}

/// The image's name from the class version: `Java 17` from 49 on, `Java 1.x` below, else
/// `Java class M.m`.
fn version(major: u16, minor: u16) -> String {
    match major {
        45..=48 => format!("Java 1.{}", major - 44),
        49..=99 => format!("Java {}", major - 44),
        _ => format!("Java class {major}.{minor}"),
    }
}

/// Modified UTF-8 as a label: a two-byte NUL and surrogate pairs written as two three-byte
/// sequences, which standard UTF-8 refuses, decoded; anything else invalid as U+FFFD.
fn modified_utf8(b: &[u8]) -> Cow<'_, str> {
    if let Ok(s) = std::str::from_utf8(b) {
        return Cow::Borrowed(s);
    }
    let mut units: Vec<u16> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let (unit, w) = if c < 0x80 {
            (c as u16, 1)
        } else if c & 0xe0 == 0xc0 && b.get(i + 1).is_some_and(|x| x & 0xc0 == 0x80) {
            ((((c & 0x1f) as u16) << 6) | (b[i + 1] & 0x3f) as u16, 2)
        } else if c & 0xf0 == 0xe0
            && b.get(i + 1).is_some_and(|x| x & 0xc0 == 0x80)
            && b.get(i + 2).is_some_and(|x| x & 0xc0 == 0x80)
        {
            (
                (((c & 0x0f) as u16) << 12)
                    | (((b[i + 1] & 0x3f) as u16) << 6)
                    | (b[i + 2] & 0x3f) as u16,
                3,
            )
        } else {
            (0xfffd, 1)
        };
        units.push(unit);
        i += w;
    }
    Cow::Owned(
        char::decode_utf16(units)
            .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect(),
    )
}

/// The length of the instruction at `pc` of `code` (whose first byte is offset 0 of the
/// method), or None for an opcode the JVM does not define or one that runs past the code.
fn instruction(code: &[u8], pc: usize) -> Option<usize> {
    let op = *code.get(pc)?;
    let n = match op {
        0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xa9 | 0xbc => 2,
        0x11 | 0x13 | 0x14 | 0x84 | 0x99..=0xa8 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1
        | 0xc6 | 0xc7 => 3,
        0xc5 => 4,
        0xb9 | 0xba | 0xc8 | 0xc9 => 5,
        0xc4 => {
            if *code.get(pc + 1)? == 0x84 {
                6
            } else {
                4
            }
        }
        0xaa | 0xab => {
            // Padding to a multiple of four from the start of the code, then the default and
            // the table.
            let at = (pc + 4) & !3;
            let int = |k: usize| -> Option<i64> {
                Some(i64::from(i32::from_be_bytes(
                    code.get(at + 4 * k..at + 4 * k + 4)?.try_into().ok()?,
                )))
            };
            let entries = if op == 0xaa {
                let (low, high) = (int(1)?, int(2)?);
                if high < low {
                    return None;
                }
                3 + usize::try_from(high - low + 1).ok()?
            } else {
                let n = usize::try_from(int(1)?).ok()?;
                2 + n.checked_mul(2)?
            };
            at.checked_add(entries.checked_mul(4)?)? - pc
        }
        0x00..=0x0f | 0x1a..=0x35 | 0x3b..=0x83 | 0x85..=0x98 | 0xac..=0xb1 | 0xbe | 0xbf
        | 0xc2 | 0xc3 | 0xca | 0xfe | 0xff => 1,
        _ => return None,
    };
    (pc.checked_add(n)? <= code.len()).then_some(n)
}

/// None when the file is no class file that reads to its end: a constant pool of known tags,
/// then the class, its interfaces, fields, methods and attributes, all inside the file.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let bytes = b.bytes;
    let all = 0..bytes.len();
    if !bytes.starts_with(b"\xca\xfe\xba\xbe") {
        return None;
    }
    let minor = u16_at(bytes, 4, false)?;
    let major = u16_at(bytes, 6, false)?;
    let count = u16_at(bytes, 8, false)? as usize;
    let none = Entry {
        tag: 0,
        bytes: 0..0,
        text: 0..0,
        index: 0,
    };
    let mut pool = vec![none.clone(); count.min(bytes.len() / 3 + 1)];
    let mut at = 10;
    let mut i = 1;
    while i < count {
        let tag = *bytes.get(at)?;
        let (len, text, index) = match tag {
            1 => {
                let n = u16_at(bytes, at + 1, false)? as usize;
                (3 + n, sub(&all, at + 3, n)?, 0)
            }
            7 | 8 | 16 | 19 | 20 => (3, 0..0, u16_at(bytes, at + 1, false)?),
            3 | 4 | 9..=12 | 17 | 18 => (5, 0..0, 0),
            5 | 6 => (9, 0..0, 0),
            15 => (4, 0..0, 0),
            _ => return None,
        };
        let entry = sub(&all, at, len)?;
        *pool.get_mut(i)? = Entry {
            tag,
            bytes: entry.clone(),
            text,
            index,
        };
        // Longs and doubles take two slots.
        i += if matches!(tag, 5 | 6) { 2 } else { 1 };
        at = entry.end;
    }
    let pool_bytes = 10..at;
    let body = at;
    let this = u16_at(bytes, at + 2, false)?;
    let interfaces = u16_at(bytes, at + 6, false)? as usize;
    at = sub(&all, at + 8, 2 * interfaces)?.end;
    let (fields, at) = members(bytes, at, &all)?;
    let (methods, at) = members(bytes, at, &all)?;
    let (_, _end) = attributes(bytes, at, &all)?;

    let utf8 = |k: u16| {
        pool.get(k as usize)
            .filter(|e| e.tag == 1)
            .map(|e| e.text.clone())
    };
    let class_name = pool
        .get(this as usize)
        .filter(|e| e.tag == 7)
        .and_then(|e| utf8(e.index))
        .map_or(Cow::Borrowed("class"), |r| {
            Cow::Owned(modified_utf8(&bytes[r]).replace('/', "."))
        });
    // Who each constant belongs to, the first that claims it in the order of the spec.
    let mut owner = vec![Owner::Pool; pool.len()];
    let claim = |k: usize, o: Owner, owner: &mut Vec<Owner>| {
        if let Some(slot) = owner.get_mut(k)
            && *slot == Owner::Pool
        {
            *slot = o;
        }
    };
    let code_name = |k: u16| utf8(k).is_some_and(|r| &bytes[r] == b"Code");
    for (m, method) in methods.iter().enumerate() {
        for (_, attr) in method.attributes.iter().filter(|(n, _)| code_name(*n)) {
            let Some(len) = u32_at(bytes, attr.start + 4, false) else {
                continue;
            };
            let Some(code) = sub(attr, attr.start + 8, len as usize) else {
                continue;
            };
            let code = &bytes[code];
            let mut pc = 0;
            while let Some(n) = instruction(code, pc) {
                let index = match code[pc] {
                    0x12 => Some(code[pc + 1] as usize),
                    0x13 => Some(u16::from_be_bytes([code[pc + 1], code[pc + 2]]) as usize),
                    _ => None,
                };
                if let Some(k) = index
                    && let Some(e) = pool.get(k).filter(|e| e.tag == 8)
                {
                    claim(k, Owner::Method(m), &mut owner);
                    claim(e.index as usize, Owner::Method(m), &mut owner);
                }
                pc += n;
            }
        }
    }
    let constant_value = |k: u16| utf8(k).is_some_and(|r| &bytes[r] == b"ConstantValue");
    for (f, field) in fields.iter().enumerate() {
        for (_, attr) in field.attributes.iter().filter(|(n, _)| constant_value(*n)) {
            if let Some(k) = u16_at(bytes, attr.start, false).map(usize::from)
                && attr.len() >= 2
                && let Some(e) = pool.get(k).filter(|e| e.tag == 8)
            {
                claim(k, Owner::Field(f), &mut owner);
                claim(e.index as usize, Owner::Field(f), &mut owner);
            }
        }
    }
    for member in fields.iter().chain(&methods) {
        claim(member.name as usize, Owner::Class, &mut owner);
    }
    if let Some(e) = pool.get(this as usize).filter(|e| e.tag == 7) {
        claim(e.index as usize, Owner::Class, &mut owner);
    }
    for (f, field) in fields.iter().enumerate() {
        claim(field.descriptor as usize, Owner::Field(f), &mut owner);
    }
    for (m, method) in methods.iter().enumerate() {
        claim(method.descriptor as usize, Owner::Method(m), &mut owner);
    }

    let image = b.node(None, "object", version(major, minor).into(), all.clone());
    let class = b.node(Some(image), "class", class_name, body..bytes.len());
    b.node(Some(class), "section", "constant pool".into(), pool_bytes);
    let label = |k: u16| {
        utf8(k).map_or(Cow::Borrowed("?"), |r| modified_utf8(&bytes[r]))
    };
    let field_nodes: Vec<usize> = fields
        .iter()
        .map(|f| b.node(Some(class), "field", label(f.name), f.bytes.clone()))
        .collect();
    let method_nodes: Vec<usize> = methods
        .iter()
        .map(|m| b.node(Some(class), "method", label(m.name), m.bytes.clone()))
        .collect();
    for (k, o) in owner.iter().enumerate() {
        let node = match *o {
            Owner::Pool => continue,
            Owner::Class => class,
            Owner::Field(f) => field_nodes[f],
            Owner::Method(m) => method_nodes[m],
        };
        // The constant's whole entry: a Match in its tag or its length is in it too.
        b.own(node, pool[k].bytes.clone());
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_modified_utf8() {
        assert_eq!(version(61, 0), "Java 17");
        assert_eq!(version(46, 0), "Java 1.2");
        assert_eq!(version(44, 3), "Java class 44.3");
        assert_eq!(modified_utf8(b"a\xc0\x80b"), "a\0b");
        assert_eq!(modified_utf8(b"\xed\xa0\xbd\xed\xb8\x80"), "😀");
    }

    #[test]
    fn switches_are_padded_from_the_code_start() {
        // tableswitch at 1: padding to 4, default, low 0, high 1, two offsets.
        let mut code = vec![0x00, 0xaa, 0, 0];
        code.extend(0i32.to_be_bytes());
        code.extend(0i32.to_be_bytes());
        code.extend(1i32.to_be_bytes());
        code.extend([0; 8]);
        assert_eq!(instruction(&code, 1), Some(code.len() - 1));
        // lookupswitch with a pair count past the code.
        let mut code = vec![0xab, 0, 0, 0];
        code.extend(0i32.to_be_bytes());
        code.extend(i32::MAX.to_be_bytes());
        assert_eq!(instruction(&code, 0), None);
        assert_eq!(instruction(&[0xc4, 0x84, 0, 1, 0, 1], 0), Some(6));
        assert_eq!(instruction(&[0xcb], 0), None);
    }
}
