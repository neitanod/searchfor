//! XMP packets (M6 spec §9): XML, inside its packet, with the element path of `markup`'s XML
//! rules below `rdf:Description`, whose ancestors every packet repeats:
//! `JPEG › APP1 XMP › dc:title › rdf:Alt › rdf:li`. Read with `markup`'s scanner over the
//! packet's bytes alone; from the first byte that is not well formed on, the bytes belong to the
//! packet's container.
use super::layouts::Builder;
use crate::processors::markup::scan::{Scanner, Step, Tag};
use std::ops::Range;

/// The wrappers of a packet, left out of the Trail while every element around them is one too.
const WRAPPERS: &[&[u8]] = &[b"x:xmpmeta", b"x:xapmeta", b"rdf:RDF", b"rdf:Description"];

/// The elements of the XMP packet `packet` under its container `parent`.
pub(super) fn read(b: &mut Builder, packet: Range<usize>, parent: usize) {
    let src = &b.bytes[packet.clone()];
    let base = packet.start;
    // (element name, the node it opened or None for a wrapper)
    let mut open: Vec<(Range<usize>, Option<usize>)> = Vec::new();
    // (node, the bytes it owns), in the order the nodes opened: a parent before its children.
    let mut owned: Vec<(usize, Range<usize>)> = Vec::new();
    let mut scan = Scanner::new(src, true);
    let end = loop {
        match scan.step() {
            Step::Eof => break src.len(),
            Step::Error(at) => break at,
            Step::Tag(Tag::Start {
                name,
                tag,
                self_closing,
            }) => {
                let wrapper = open.iter().all(|(_, n)| n.is_none())
                    && WRAPPERS.contains(&&src[name.clone()]);
                let node = (!wrapper).then(|| {
                    let under = open.iter().rev().find_map(|(_, n)| *n).unwrap_or(parent);
                    let n = b.open(
                        Some(under),
                        "tag",
                        b.utf8(base + name.start..base + name.end),
                        base + tag.start..base + tag.end,
                    );
                    owned.push((n, base + tag.start..base + tag.end));
                    n
                });
                if !self_closing {
                    open.push((name, node));
                }
            }
            Step::Tag(Tag::End { name, tag }) => {
                match open.last() {
                    Some((n, _)) if src[n.clone()] == src[name.clone()] => {}
                    _ => break tag.start,
                }
                if let Some((_, Some(node))) = open.pop()
                    && let Some(o) = owned.iter_mut().rev().find(|(n, _)| *n == node)
                {
                    o.1.end = base + tag.end;
                }
            }
            Step::Tag(Tag::Block { .. } | Tag::Unblock { .. }) => {}
        }
    };
    // What is still open at the end, or at the first error, owns the bytes up to it.
    for (_, node) in &open {
        if let Some(node) = node
            && let Some(o) = owned.iter_mut().rev().find(|(n, _)| n == node)
        {
            o.1.end = base + end;
        }
    }
    for (node, range) in owned {
        if node != parent {
            b.set_span(node, range.clone());
            b.own(node, range);
        }
    }
}
