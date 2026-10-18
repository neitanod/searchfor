//! Small documents written by code, for the tests of the Extractors, the end-to-end tests, the
//! fuzz and the perf ceilings (M4 plan, D19). Each is the least a reader of its format needs,
//! written here so no fixture comes from a third party. Not used by `s4` itself.
use std::io::{Cursor, Write};

mod epub;
mod odf;
mod ooxml;
mod pdf;
pub use epub::*;
pub use odf::*;
pub use ooxml::*;
pub use pdf::*;

/// One block of a text document.
#[derive(Debug, Clone)]
pub enum Block {
    Heading(u8, String),
    Text(String),
    /// Rows of cells.
    Table(Vec<Vec<String>>),
}

pub fn h(level: u8, s: &str) -> Block {
    Block::Heading(level, s.into())
}
pub fn p(s: &str) -> Block {
    Block::Text(s.into())
}
pub fn table(rows: &[&[&str]]) -> Block {
    Block::Table(
        rows.iter()
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect(),
    )
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The parts of a zip, in order, each passed through `f` (name, bytes) and written again as
/// `zip` writes them: for the fuzz, which changes the XML inside documents.
pub fn rezip(bytes: &[u8], mut f: impl FnMut(&str, Vec<u8>) -> Vec<u8>) -> Vec<u8> {
    use std::io::Read;
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut parts = Vec::new();
    for i in 0..z.len() {
        let mut file = z.by_index(i).unwrap();
        let name = file.name().unwrap().to_string();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        let data = f(&name, data);
        parts.push((name, data));
    }
    let refs: Vec<(&str, Vec<u8>)> = parts.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &["mimetype"])
}

/// A zip of `parts` (name, bytes), deflated, except those named in `stored`.
pub fn zip(parts: &[(&str, Vec<u8>)], stored: &[&str]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in parts {
        let method = if stored.contains(name) {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let opts = zip::write::SimpleFileOptions::default().compression_method(method);
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn ct(overrides: &[(&str, &str)]) -> Vec<u8> {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
    );
    for (part, ty) in overrides {
        s.push_str(&format!(
            r#"<Override PartName="/{part}" ContentType="{ty}"/>"#
        ));
    }
    s.push_str("</Types>");
    s.into_bytes()
}

pub fn rels(list: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut s = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for (id, ty, target) in list {
        s.push_str(&format!(
            r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{ty}" Target="{target}"/>"#
        ));
    }
    s.push_str("</Relationships>");
    s.into_bytes()
}
