//! EPUB (spec §5.8): `3. The Storm › The harbour`. The spine's documents in order, each a
//! chapter named by its entry in the table of contents (the EPUB 3 `nav`, else the EPUB 2
//! NCX), or `chapter N` without one; inside, the XHTML's `h1`–`h6` are headings.
use super::xml::{Ev, Pull, attr, local};
use super::zipped::{Zip, resolve};
use super::{Builder, DocError};
use std::collections::HashMap;

pub fn extract(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let mut z = Zip::open(bytes)?;
    let container = z
        .text("META-INF/container.xml")?
        .ok_or_else(|| DocError::broken("no META-INF/container.xml"))?;
    let opf_path = Pull::new(&container)
        .find_map(|ev| match ev {
            Ev::Start { name, attrs, .. } if local(name) == "rootfile" => {
                attr(attrs, "full-path").map(|s| s.into_owned())
            }
            _ => None,
        })
        .ok_or_else(|| DocError::broken("no rootfile in META-INF/container.xml"))?;
    let opf = z
        .text(&opf_path)?
        .ok_or_else(|| DocError::broken(format!("no package document {opf_path}")))?;
    // Manifest: id → (part, media type, properties); spine: ids in reading order.
    let mut manifest: HashMap<String, (String, String, String)> = HashMap::new();
    let mut spine = Vec::new();
    let mut ncx = None;
    for ev in Pull::new(&opf) {
        let Ev::Start { name, attrs, .. } = ev else {
            continue;
        };
        match local(name) {
            "item" => {
                if let (Some(id), Some(href)) = (attr(attrs, "id"), attr(attrs, "href")) {
                    let media = attr(attrs, "media-type").unwrap_or_default().into_owned();
                    let props = attr(attrs, "properties").unwrap_or_default().into_owned();
                    manifest.insert(id.into_owned(), (resolve(&opf_path, &href), media, props));
                }
            }
            "spine" => ncx = attr(attrs, "toc").map(|s| s.into_owned()),
            "itemref" => {
                if attr(attrs, "linear").as_deref() != Some("no")
                    && let Some(id) = attr(attrs, "idref")
                {
                    spine.push(id.into_owned());
                }
            }
            _ => {}
        }
    }
    let titles = titles(&mut z, &manifest, ncx.as_deref())?;
    let mut n = 0;
    for id in spine {
        let Some((part, media, _)) = manifest.get(&id) else {
            continue;
        };
        if !media.contains("html") {
            continue;
        }
        let Some(xhtml) = z.text(part)? else { continue };
        n += 1;
        let title = titles
            .get(part)
            .cloned()
            .unwrap_or_else(|| format!("chapter {n}"));
        b.chapter(1, &title);
        body(&xhtml, b);
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The first table-of-contents title of each part.
fn titles(
    z: &mut Zip,
    manifest: &HashMap<String, (String, String, String)>,
    ncx: Option<&str>,
) -> Result<HashMap<String, String>, DocError> {
    let mut out = HashMap::new();
    if let Some((nav, _, _)) = manifest
        .values()
        .find(|m| m.2.split_whitespace().any(|p| p == "nav"))
        && let Some(xml) = z.text(nav)?
    {
        // <nav epub:type="toc"> … <a href="c1.xhtml#s">Title</a>
        let (mut in_toc, mut depth, mut link): (bool, usize, Option<(String, String)>) =
            (false, 0, None);
        for ev in Pull::new(&xml) {
            match ev {
                Ev::Start {
                    name,
                    attrs,
                    empty: false,
                } if local(name) == "nav" => {
                    if in_toc {
                        depth += 1;
                    } else if attr(attrs, "type")
                        .is_some_and(|t| t.split_whitespace().any(|t| t == "toc"))
                    {
                        in_toc = true;
                        depth = 0;
                    }
                }
                Ev::End { name } if local(name) == "nav" && in_toc => {
                    if depth == 0 {
                        in_toc = false;
                    } else {
                        depth -= 1;
                    }
                }
                Ev::Start {
                    name,
                    attrs,
                    empty: false,
                } if in_toc && local(name) == "a" => {
                    link = attr(attrs, "href").map(|h| (resolve(nav, &h), String::new()));
                }
                Ev::Text(t) => {
                    if let Some(l) = link.as_mut() {
                        l.1.push_str(&t);
                    }
                }
                Ev::End { name } if local(name) == "a" => {
                    if let Some((part, title)) = link.take() {
                        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
                        if !title.is_empty() {
                            out.entry(part).or_insert(title);
                        }
                    }
                }
                _ => {}
            }
        }
        if !out.is_empty() {
            return Ok(out);
        }
    }
    let Some((ncx, _, _)) = ncx.and_then(|id| manifest.get(id)) else {
        return Ok(out);
    };
    let Some(xml) = z.text(ncx)? else {
        return Ok(out);
    };
    // <navPoint><navLabel><text>Title</text></navLabel><content src="c1.xhtml"/>
    let (mut label, mut in_text) = (Vec::<String>::new(), false);
    for ev in Pull::new(&xml) {
        match ev {
            Ev::Start {
                name, empty: false, ..
            } if local(name) == "navPoint" => label.push(String::new()),
            Ev::Start {
                name, empty: false, ..
            } if local(name) == "text" => in_text = true,
            Ev::End { name } if local(name) == "text" => in_text = false,
            Ev::Text(t) if in_text => {
                if let Some(l) = label.last_mut() {
                    l.push_str(&t);
                }
            }
            Ev::Start { name, attrs, .. } if local(name) == "content" => {
                if let (Some(src), Some(l)) = (attr(attrs, "src"), label.last()) {
                    let title = l.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !title.is_empty() {
                        out.entry(resolve(ncx, &src)).or_insert(title);
                    }
                }
            }
            Ev::End { name } if local(name) == "navPoint" => {
                label.pop();
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The text of an XHTML document's body: a line per block, headings by their level.
fn body(xhtml: &str, b: &mut Builder) {
    let mut line = String::new();
    let mut heading: Option<(u8, String)> = None;
    let mut skip = 0usize;
    let mut in_body = false;
    let flush = |line: &mut String, b: &mut Builder| {
        b.line(line);
        line.clear();
    };
    for ev in Pull::new(xhtml) {
        match ev {
            Ev::Start { name, empty, .. } => {
                let l = local(name);
                if l == "body" {
                    in_body = true;
                }
                if matches!(l, "script" | "style" | "head") && !empty {
                    skip += 1;
                    continue;
                }
                if let Some(level) = l
                    .strip_prefix('h')
                    .and_then(|d| d.parse::<u8>().ok())
                    .filter(|d| (1..=6).contains(d))
                    && !empty
                {
                    flush(&mut line, b);
                    heading = Some((level, String::new()));
                } else if l == "br" {
                    match heading.as_mut() {
                        Some(h) => h.1.push(' '),
                        None => flush(&mut line, b),
                    }
                } else if block(l) {
                    flush(&mut line, b);
                }
            }
            Ev::End { name } => {
                let l = local(name);
                if matches!(l, "script" | "style" | "head") {
                    skip = skip.saturating_sub(1);
                    continue;
                }
                if l.len() == 2 && l.starts_with('h') && heading.is_some() {
                    let (level, text) = heading.take().unwrap();
                    b.heading(level, &text);
                } else if block(l) {
                    flush(&mut line, b);
                }
            }
            Ev::Text(t) if skip == 0 && in_body => match heading.as_mut() {
                Some(h) => h.1.push_str(&t),
                None => line.push_str(&t),
            },
            _ => {}
        }
        if b.full() {
            return;
        }
    }
    flush(&mut line, b);
}

/// Elements whose start and end break the line.
fn block(l: &str) -> bool {
    matches!(
        l,
        "p" | "div"
            | "li"
            | "dt"
            | "dd"
            | "tr"
            | "td"
            | "th"
            | "blockquote"
            | "pre"
            | "section"
            | "article"
            | "aside"
            | "header"
            | "footer"
            | "figcaption"
            | "caption"
            | "table"
            | "ul"
            | "ol"
            | "dl"
            | "hr"
            | "figure"
            | "nav"
            | "body"
    )
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::super::{Kind, extract};
    use crate::trail::trail_for;

    fn trail(doc: &[u8], needle: &str) -> Vec<String> {
        let e = extract(Kind::Epub, doc).unwrap();
        let at = memchr::memmem::find(&e.text, needle.as_bytes())
            .unwrap_or_else(|| panic!("{needle} not in {:?}", String::from_utf8_lossy(&e.text)));
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    const CH1: &str = "<h1>The Harbour</h1><p>A needle in the water.</p><h2>At night</h2><p>Second needle &amp; more.</p>";
    const CH2: &str = "<p>No heading, a needle.</p><script>needle()</script>";

    #[test]
    fn chapters_from_the_nav_and_headings_inside() {
        let d = fixture::epub(&[("1. Arrival", CH1), ("2. The Storm", CH2)], true);
        assert_eq!(trail(&d, "in the water"), ["1. Arrival", "The Harbour"]);
        assert_eq!(
            trail(&d, "Second"),
            ["1. Arrival", "The Harbour", "At night"]
        );
        assert_eq!(trail(&d, "No heading"), ["2. The Storm"]);
        let e = extract(Kind::Epub, &d).unwrap();
        // Scripts and the head are not text.
        assert!(memchr::memmem::find(&e.text, b"needle()").is_none());
        assert!(memchr::memmem::find(&e.text, b"x: y").is_none());
        assert!(memchr::memmem::find(&e.text, b"needle & more").is_some());
    }

    /// Pending C8: a chapter whose first heading repeats its title (pandoc writes the nav
    /// entry and the `h1` alike) is in the Trail once; the heading is still a heading, which
    /// `--in-headings` finds, and its subheadings nest under the chapter.
    #[test]
    fn a_heading_that_repeats_its_chapter_is_in_the_trail_once() {
        let ch = "<h1>Arrival</h1><p>A needle in the water.</p><h2>The Harbour</h2><p>Second needle.</p><h1>Later</h1><p>Third needle.</p>";
        let d = fixture::epub(&[("Arrival", ch)], true);
        assert_eq!(trail(&d, "A needle"), ["Arrival"]);
        assert_eq!(trail(&d, "Second"), ["Arrival", "The Harbour"]);
        assert_eq!(trail(&d, "Third"), ["Arrival", "Later"]);
        let e = extract(Kind::Epub, &d).unwrap();
        let names = crate::trail::names_of(&e.outline);
        assert_eq!(
            names.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
            ["Arrival", "The Harbour", "Later"]
        );
    }

    #[test]
    fn an_epub2_ncx_names_the_chapters_too() {
        let d = fixture::epub(&[("1. Arrival", CH1), ("2. The Storm", CH2)], false);
        assert_eq!(trail(&d, "No heading"), ["2. The Storm"]);
    }

    #[test]
    fn without_a_container_it_is_an_error() {
        let z = fixture::zip(
            &[("mimetype", b"application/epub+zip".to_vec())],
            &["mimetype"],
        );
        assert!(extract(Kind::Epub, &z).is_err());
    }
}
