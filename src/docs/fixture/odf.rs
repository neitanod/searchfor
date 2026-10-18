//! The OpenDocument writers (M4 plan, D19).
use super::{Block, esc, zip};

const ODF_NS: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0""#;

fn odf(mime: &str, body: &str) -> Vec<u8> {
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ODF_NS} office:version="1.3"><office:body>{body}</office:body></office:document-content>"#
    );
    let manifest = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="{mime}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#
    );
    zip(
        &[
            ("mimetype", mime.as_bytes().to_vec()),
            ("content.xml", content.into_bytes()),
            ("META-INF/manifest.xml", manifest.into_bytes()),
        ],
        &["mimetype"],
    )
}

/// An OpenDocument text: headings as `text:h`, tables as `table:table`.
pub fn odt(blocks: &[Block]) -> Vec<u8> {
    let mut body = String::from("<office:text>");
    for b in blocks {
        match b {
            Block::Heading(l, t) => body.push_str(&format!(
                r#"<text:h text:outline-level="{l}">{}</text:h>"#,
                esc(t)
            )),
            Block::Text(t) => body.push_str(&format!("<text:p>{}</text:p>", esc(t))),
            Block::Table(rows) => {
                body.push_str(r#"<table:table table:name="Table1">"#);
                for r in rows {
                    body.push_str("<table:table-row>");
                    for c in r {
                        body.push_str(&format!(
                            "<table:table-cell><text:p>{}</text:p></table:table-cell>",
                            esc(c)
                        ));
                    }
                    body.push_str("</table:table-row>");
                }
                body.push_str("</table:table>");
            }
        }
    }
    body.push_str("</office:text>");
    odf("application/vnd.oasis.opendocument.text", &body)
}

/// An OpenDocument spreadsheet; numbers carry `office:value` as LibreOffice writes them, and
/// each sheet ends with the repeated empty rows LibreOffice adds.
pub fn ods(sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    let mut body = String::from("<office:spreadsheet>");
    for (name, rows) in sheets {
        body.push_str(&format!(r#"<table:table table:name="{}">"#, esc(name)));
        for row in *rows {
            body.push_str("<table:table-row>");
            for v in *row {
                if v.is_empty() {
                    body.push_str("<table:table-cell/>");
                } else if v.parse::<f64>().is_ok() {
                    body.push_str(&format!(r#"<table:table-cell office:value-type="float" office:value="{v}"><text:p>{v}</text:p></table:table-cell>"#));
                } else {
                    body.push_str(&format!(r#"<table:table-cell office:value-type="string"><text:p>{}</text:p></table:table-cell>"#, esc(v)));
                }
            }
            body.push_str(
                r#"<table:table-cell table:number-columns-repeated="16380"/></table:table-row>"#,
            );
        }
        body.push_str(r#"<table:table-row table:number-rows-repeated="1048570"><table:table-cell table:number-columns-repeated="16384"/></table:table-row></table:table>"#);
    }
    body.push_str("</office:spreadsheet>");
    odf("application/vnd.oasis.opendocument.spreadsheet", &body)
}

/// An OpenDocument presentation: pages of paragraphs, each with optional notes.
pub fn odp(slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    let mut body = String::from("<office:presentation>");
    for (i, (paras, notes)) in slides.iter().enumerate() {
        body.push_str(&format!(
            r#"<draw:page draw:name="page{}"><draw:frame><draw:text-box>"#,
            i + 1
        ));
        for t in *paras {
            body.push_str(&format!("<text:p>{}</text:p>", esc(t)));
        }
        body.push_str("</draw:text-box></draw:frame>");
        if let Some(n) = notes {
            body.push_str(&format!(
                "<presentation:notes><draw:frame><draw:text-box><text:p>{}</text:p></draw:text-box></draw:frame></presentation:notes>",
                esc(n)
            ));
        }
        body.push_str("</draw:page>");
    }
    body.push_str("</office:presentation>");
    odf("application/vnd.oasis.opendocument.presentation", &body)
}
