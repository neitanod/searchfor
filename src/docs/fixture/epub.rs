//! The EPUB writer (M4 plan, D19).
use super::{esc, zip};

/// An EPUB: chapters (title in the table of contents, XHTML body). `nav` writes an EPUB 3
/// navigation document; without it, an EPUB 2 NCX.
pub fn epub(chapters: &[(&str, &str)], nav: bool) -> Vec<u8> {
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut toc = String::new();
    for (i, (title, body)) in chapters.iter().enumerate() {
        let n = i + 1;
        manifest.push_str(&format!(
            r#"<item id="c{n}" href="text/c{n}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{n}"/>"#));
        let x = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{}</title><style>p {{ x: y }}</style></head><body>{body}</body></html>"#,
            esc(title)
        );
        parts.push((format!("OEBPS/text/c{n}.xhtml"), x.into_bytes()));
        if nav {
            toc.push_str(&format!(
                r#"<li><a href="text/c{n}.xhtml">{}</a></li>"#,
                esc(title)
            ));
        } else {
            toc.push_str(&format!(
                r#"<navPoint id="n{n}"><navLabel><text>{}</text></navLabel><content src="text/c{n}.xhtml"/></navPoint>"#,
                esc(title)
            ));
        }
    }
    let (toc_item, toc_attr, toc_part) = if nav {
        (
            r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
            "",
            (
                "OEBPS/nav.xhtml".to_string(),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="toc"><ol>{toc}</ol></nav></body></html>"#
                ),
            ),
        )
    } else {
        (
            r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
            r#" toc="ncx""#,
            (
                "OEBPS/toc.ncx".to_string(),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap>{toc}</navMap></ncx>"#
                ),
            ),
        )
    };
    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><manifest>{toc_item}{manifest}</manifest><spine{toc_attr}>{spine}</spine></package>"#
    );
    let container = r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let mut all = vec![
        ("mimetype".to_string(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".to_string(),
            container.as_bytes().to_vec(),
        ),
        ("OEBPS/content.opf".to_string(), opf.into_bytes()),
        (toc_part.0, toc_part.1.into_bytes()),
    ];
    all.extend(parts);
    let refs: Vec<(&str, Vec<u8>)> = all.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs, &["mimetype"])
}
