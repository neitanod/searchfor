//! The Word, Excel and PowerPoint writers (M4 plan, D19).
use super::{Block, ct, esc, rels, zip};

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

/// A Word document. Headings use the styles `Heading1`…`Heading3`, named `heading N` in
/// `styles.xml` as Word names them.
pub fn docx(blocks: &[Block]) -> Vec<u8> {
    let mut body = String::new();
    for b in blocks {
        match b {
            Block::Heading(l, t) => body.push_str(&format!(
                r#"<w:p><w:pPr><w:pStyle w:val="Heading{l}"/></w:pPr><w:r><w:t>{}</w:t></w:r></w:p>"#,
                esc(t)
            )),
            Block::Text(t) => body.push_str(&format!(
                r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                esc(t)
            )),
            Block::Table(rows) => {
                body.push_str("<w:tbl>");
                for r in rows {
                    body.push_str("<w:tr>");
                    for c in r {
                        body.push_str(&format!("<w:tc><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:tc>", esc(c)));
                    }
                    body.push_str("</w:tr>");
                }
                body.push_str("</w:tbl>");
            }
        }
    }
    let doc = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document {W}><w:body>{body}<w:sectPr/></w:body></w:document>"#
    );
    let mut styles = format!(r#"<?xml version="1.0" encoding="UTF-8"?><w:styles {W}>"#);
    for l in 1..=3 {
        styles.push_str(&format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading{l}"><w:name w:val="heading {l}"/></w:style>"#
        ));
    }
    styles.push_str("</w:styles>");
    zip(
        &[
            (
                "[Content_Types].xml",
                ct(&[(
                    "word/document.xml",
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
                )]),
            ),
            (
                "_rels/.rels",
                rels(&[("rId1", "officeDocument", "word/document.xml")]),
            ),
            ("word/document.xml", doc.into_bytes()),
            ("word/styles.xml", styles.into_bytes()),
        ],
        &[],
    )
}

/// An Excel workbook: sheets of rows, text cells in the shared strings, numbers inline.
pub fn xlsx(sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    let mut shared: Vec<String> = Vec::new();
    let mut parts = Vec::new();
    let mut book = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>"#,
    );
    let mut brels = Vec::new();
    for (i, (name, rows)) in sheets.iter().enumerate() {
        book.push_str(&format!(
            r#"<sheet name="{}" sheetId="{}" r:id="rId{}"/>"#,
            esc(name),
            i + 1,
            i + 1
        ));
        brels.push((
            format!("rId{}", i + 1),
            format!("worksheets/sheet{}.xml", i + 1),
        ));
        let mut x = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        );
        for (r, row) in rows.iter().enumerate() {
            x.push_str(&format!(r#"<row r="{}">"#, r + 1));
            for (c, v) in row.iter().enumerate() {
                if v.is_empty() {
                    continue;
                }
                let at = format!("{}{}", crate::docs::column_letters(c), r + 1);
                if v.parse::<f64>().is_ok() {
                    x.push_str(&format!(r#"<c r="{at}"><v>{v}</v></c>"#));
                } else {
                    shared.push(v.to_string());
                    x.push_str(&format!(
                        r#"<c r="{at}" t="s"><v>{}</v></c>"#,
                        shared.len() - 1
                    ));
                }
            }
            x.push_str("</row>");
        }
        x.push_str("</sheetData></worksheet>");
        parts.push((format!("xl/worksheets/sheet{}.xml", i + 1), x.into_bytes()));
    }
    book.push_str("</sheets></workbook>");
    let mut sst = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    );
    for s in &shared {
        sst.push_str(&format!("<si><t>{}</t></si>", esc(s)));
    }
    sst.push_str("</sst>");
    let mut r: Vec<(&str, &str, &str)> = brels
        .iter()
        .map(|(i, t)| (i.as_str(), "worksheet", t.as_str()))
        .collect();
    let sst_id = format!("rId{}", sheets.len() + 1);
    r.push((&sst_id, "sharedStrings", "sharedStrings.xml"));
    let mut all = vec![
        ("[Content_Types].xml".to_string(), ct(&[])),
        (
            "_rels/.rels".to_string(),
            rels(&[("rId1", "officeDocument", "xl/workbook.xml")]),
        ),
        ("xl/workbook.xml".to_string(), book.into_bytes()),
        ("xl/_rels/workbook.xml.rels".to_string(), rels(&r)),
        ("xl/sharedStrings.xml".to_string(), sst.into_bytes()),
    ];
    all.extend(parts);
    let refs: Vec<(&str, Vec<u8>)> = all.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &[])
}

/// A PowerPoint presentation: slides of paragraphs, each with optional notes.
pub fn pptx(slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    const NS: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    let mut pres =
        format!(r#"<?xml version="1.0" encoding="UTF-8"?><p:presentation {NS}><p:sldIdLst>"#);
    let mut prels = Vec::new();
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let shape = |paras: &[&str], ph: Option<&str>| {
        let mut s =
            String::from("<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"\"/><p:cNvSpPr/><p:nvPr>");
        if let Some(t) = ph {
            s.push_str(&format!(r#"<p:ph type="{t}"/>"#));
        }
        s.push_str("</p:nvPr></p:nvSpPr><p:txBody>");
        for t in paras {
            s.push_str(&format!("<a:p><a:r><a:t>{}</a:t></a:r></a:p>", esc(t)));
        }
        s.push_str("</p:txBody></p:sp>");
        s
    };
    for (i, (paras, notes)) in slides.iter().enumerate() {
        let n = i + 1;
        // The slide's id comes before its relationship id, as PowerPoint writes it.
        pres.push_str(&format!(r#"<p:sldId id="{}" r:id="rId{n}"/>"#, 255 + n));
        prels.push((format!("rId{n}"), format!("slides/slide{n}.xml")));
        let sl = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><p:sld {NS}><p:cSld><p:spTree>{}</p:spTree></p:cSld></p:sld>"#,
            shape(paras, None)
        );
        parts.push((format!("ppt/slides/slide{n}.xml"), sl.into_bytes()));
        if let Some(text) = notes {
            let nx = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><p:notes {NS}><p:cSld><p:spTree>{}{}</p:spTree></p:cSld></p:notes>"#,
                shape(&["slide image"], Some("sldImg")),
                shape(&[text], Some("body"))
            );
            parts.push((
                format!("ppt/notesSlides/notesSlide{n}.xml"),
                nx.into_bytes(),
            ));
            parts.push((
                format!("ppt/slides/_rels/slide{n}.xml.rels"),
                rels(&[(
                    "rId1",
                    "notesSlide",
                    &format!("../notesSlides/notesSlide{n}.xml"),
                )]),
            ));
        }
    }
    pres.push_str("</p:sldIdLst></p:presentation>");
    let r: Vec<(&str, &str, &str)> = prels
        .iter()
        .map(|(i, t)| (i.as_str(), "slide", t.as_str()))
        .collect();
    let mut all = vec![
        ("[Content_Types].xml".to_string(), ct(&[])),
        (
            "_rels/.rels".to_string(),
            rels(&[("rId1", "officeDocument", "ppt/presentation.xml")]),
        ),
        ("ppt/presentation.xml".to_string(), pres.into_bytes()),
        ("ppt/_rels/presentation.xml.rels".to_string(), rels(&r)),
    ];
    all.extend(parts);
    let refs: Vec<(&str, Vec<u8>)> = all.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &[])
}
