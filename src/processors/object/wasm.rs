//! WebAssembly modules (M6 spec §9): `WebAssembly › export › needle_fn`, `› import › env.log`,
//! `› data › data[0]`, `› code › total` (a function named by the `name` section, else
//! `func[N]`), `› custom .debug_str`. Sections are named by their id, custom ones `custom NAME`;
//! every size and count is an unsigned LEB128 checked against the bytes left before it is used.
//! An export or import is the Segment of its own name, as §9's `› export › needle_fn` says.
use super::layouts::{Builder, sub};
use std::borrow::Cow;
use std::ops::Range;

const SECTIONS: [&str; 14] = [
    "custom",
    "type",
    "import",
    "function",
    "table",
    "memory",
    "global",
    "export",
    "start",
    "element",
    "code",
    "data",
    "datacount",
    "tag",
];

/// An unsigned LEB128 of at most 32 bits at `at`, before `end`: (value, the byte after it).
/// None when it runs past `end`, is longer than five bytes or overflows.
fn leb(b: &[u8], at: usize, end: usize) -> Option<(usize, usize)> {
    let mut v: u64 = 0;
    for i in 0..5 {
        let x = *b.get(at + i).filter(|_| at + i < end)?;
        v |= u64::from(x & 0x7f) << (7 * i);
        if x & 0x80 == 0 {
            return u32::try_from(v).ok().map(|v| (v as usize, at + i + 1));
        }
    }
    None
}

/// A signed LEB128 of at most `max` bytes: the byte after it.
fn skip_sleb(b: &[u8], at: usize, end: usize, max: usize) -> Option<usize> {
    (0..max).find_map(|i| {
        let x = *b.get(at + i).filter(|_| at + i < end)?;
        (x & 0x80 == 0).then_some(at + i + 1)
    })
}

/// A name: its LEB128 length, then its bytes. (its bytes, the byte after it)
fn name(b: &[u8], at: usize, end: usize) -> Option<(Range<usize>, usize)> {
    let (n, at) = leb(b, at, end)?;
    let r = sub(&(at..end), at, n)?;
    Some((r.clone(), r.end))
}

/// The byte after a constant expression, up to its `end` instruction.
fn skip_expr(b: &[u8], mut at: usize, end: usize) -> Option<usize> {
    loop {
        let op = *b.get(at).filter(|_| at < end)?;
        at += 1;
        at = match op {
            0x0b => return Some(at),
            0x41 => skip_sleb(b, at, end, 5)?,
            0x42 => skip_sleb(b, at, end, 10)?,
            0x43 => sub(&(at..end), at, 4)?.end,
            0x44 => sub(&(at..end), at, 8)?.end,
            0x23 | 0xd2 => leb(b, at, end)?.1,
            0xd0 => sub(&(at..end), at, 1)?.end,
            // The extended constant instructions.
            0x6a..=0x6c | 0x7c..=0x7e => at,
            _ => return None,
        };
    }
}

/// The limits of a table or a memory: their flags, then one or two LEB128s.
fn skip_limits(b: &[u8], at: usize, end: usize) -> Option<usize> {
    let flags = *b.get(at).filter(|_| at < end)?;
    let (_, at) = leb64(b, at + 1, end)?;
    if flags & 1 != 0 {
        return Some(leb64(b, at, end)?.1);
    }
    Some(at)
}

/// An unsigned LEB128 of at most 64 bits (memory64 limits): (nothing, the byte after it).
fn leb64(b: &[u8], at: usize, end: usize) -> Option<((), usize)> {
    Some(((), skip_sleb(b, at, end, 10)?))
}

/// None when the file has no WebAssembly header of version 1.
pub(super) fn layout(b: &mut Builder) -> Option<()> {
    let bytes = b.bytes;
    if bytes.get(..8)? != b"\0asm\x01\0\0\0" {
        return None;
    }
    // (id, the section's first byte, its contents)
    let mut sections = Vec::new();
    let mut at = 8;
    while at < bytes.len() {
        let id = bytes[at];
        let Some((size, start)) = leb(bytes, at + 1, bytes.len()) else {
            break;
        };
        let Some(content) = sub(&(0..bytes.len()), start, size) else {
            break;
        };
        sections.push((id, at, content.clone()));
        at = content.end;
    }
    let names = function_names(bytes, &sections);
    let image = b.node(None, "object", "WebAssembly".into(), 0..bytes.len());
    let mut imported = 0usize;
    for (id, start, content) in sections {
        let end = content.end;
        let custom = (id == 0).then(|| name(bytes, content.start, end)).flatten();
        let label: Cow<str> = match (id, &custom) {
            (0, Some((n, _))) => format!("custom {}", b.utf8(n.clone())).into(),
            (1..=13, _) | (0, None) => SECTIONS[id as usize].into(),
            _ => format!("section {id}").into(),
        };
        let node = b.node(Some(image), "section", label, start..end);
        // A Match in a custom section's own name is outside it (M6 spec §3 rule 5).
        if let Some((n, _)) = &custom {
            b.own(image, n.clone());
        }
        if !matches!(id, 2 | 7 | 10 | 11) {
            continue;
        }
        let Some((count, mut at)) = leb(bytes, content.start, end) else {
            continue;
        };
        for i in 0..count.min(end - content.start) {
            let entry = at;
            let next = match id {
                2 => (|| {
                    let (module, at) = name(bytes, at, end)?;
                    let (field, at) = name(bytes, at, end)?;
                    let kind = *bytes.get(at).filter(|_| at < end)?;
                    let at = match kind {
                        0 => leb(bytes, at + 1, end)?.1,
                        1 => skip_limits(bytes, at + 2, end)?,
                        2 => skip_limits(bytes, at + 1, end)?,
                        3 => sub(&(at..end), at + 1, 2)?.end,
                        4 => leb(bytes, at + 2, end)?.1,
                        _ => return None,
                    };
                    imported += usize::from(kind == 0);
                    let label = format!("{}.{}", b.utf8(module), b.utf8(field));
                    b.node(Some(node), "symbol", label.into(), entry..at);
                    Some(at)
                })(),
                7 => (|| {
                    let (n, at) = name(bytes, at, end)?;
                    let at = leb(bytes, at + 1, end)?.1;
                    b.node(Some(node), "symbol", b.utf8(n), entry..at);
                    Some(at)
                })(),
                10 => (|| {
                    let (size, body) = leb(bytes, at, end)?;
                    let body = sub(&(body..end), body, size)?;
                    let index = imported + i;
                    let label: Cow<str> = match names.binary_search_by_key(&index, |&(k, _)| k) {
                        Ok(k) => b.utf8(names[k].1.clone()),
                        Err(_) => format!("func[{index}]").into(),
                    };
                    b.node(Some(node), "symbol", label, entry..body.end);
                    Some(body.end)
                })(),
                _ => (|| {
                    let (flags, at) = leb(bytes, at, end)?;
                    let at = match flags {
                        0 => skip_expr(bytes, at, end)?,
                        1 => at,
                        2 => skip_expr(bytes, leb(bytes, at, end)?.1, end)?,
                        _ => return None,
                    };
                    let (len, data) = leb(bytes, at, end)?;
                    let data = sub(&(data..end), data, len)?;
                    b.node(
                        Some(node),
                        "symbol",
                        format!("data[{i}]").into(),
                        entry..data.end,
                    );
                    Some(data.end)
                })(),
            };
            match next {
                Some(n) => at = n,
                None => break,
            }
        }
    }
    Some(())
}

/// The function names of the `name` custom section, wherever it is: (index, name), sorted by
/// index, the first of each index.
fn function_names(b: &[u8], sections: &[(u8, usize, Range<usize>)]) -> Vec<(usize, Range<usize>)> {
    let mut out = Vec::new();
    for (_, _, content) in sections.iter().filter(|s| s.0 == 0) {
        let end = content.end;
        let Some((n, mut at)) = name(b, content.start, end) else {
            continue;
        };
        if &b[n] != b"name" {
            continue;
        }
        while let Some(&sub_id) = b.get(at).filter(|_| at < end) {
            let Some((size, start)) = leb(b, at + 1, end) else {
                break;
            };
            let Some(data) = sub(&(start..end), start, size) else {
                break;
            };
            if sub_id == 1
                && let Some((count, mut k)) = leb(b, data.start, data.end)
            {
                for _ in 0..count.min(data.len()) {
                    let Some((index, after)) = leb(b, k, data.end) else {
                        break;
                    };
                    let Some((n, after)) = name(b, after, data.end) else {
                        break;
                    };
                    out.push((index, n));
                    k = after;
                }
            }
            at = data.end;
        }
    }
    out.sort_by_key(|&(i, _)| i);
    out.dedup_by_key(|&mut (i, _)| i);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leb128_lengths_are_bounded() {
        assert_eq!(leb(&[0x80, 0x01], 0, 2), Some((128, 2)));
        assert_eq!(leb(&[0x80, 0x01], 0, 1), None);
        assert_eq!(
            leb(&[0xff, 0xff, 0xff, 0xff, 0x0f], 0, 5),
            Some((u32::MAX as usize, 5))
        );
        assert_eq!(leb(&[0xff, 0xff, 0xff, 0xff, 0x1f], 0, 5), None);
        assert_eq!(leb(&[0x80; 6], 0, 6), None);
    }
}
