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
                    ..
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
                Ev::End { name, .. } if local(name) == "nav" && in_toc => {
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
                    ..
                } if in_toc && local(name) == "a" => {
                    link = attr(attrs, "href").map(|h| (resolve(nav, &h), String::new()));
                }
                Ev::Text(t) => {
                    if let Some(l) = link.as_mut() {
                        l.1.push_str(&t);
                    }
                }
                Ev::End { name, .. } if local(name) == "a" => {
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
            Ev::End { name, .. } if local(name) == "text" => in_text = false,
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
            Ev::End { name, .. } if local(name) == "navPoint" => {
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
    // Depth inside `pre`, whose white space is kept.
    let mut pre = 0usize;
    let mut in_body = false;
    // `line` holds the text of a `pre`, which is written with its spaces.
    let mut kept = false;
    // Only `collapse` writes a collapsible space (a ' '), and only between words; a no-break
    // space is text, never trimmed (Codex M4 code r4 #3).
    let flush = |line: &mut String, kept: &mut bool, b: &mut Builder| {
        if *kept {
            b.verbatim(line);
        } else {
            b.verbatim(line.trim_matches(' '));
        }
        line.clear();
        *kept = false;
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
                if l == "pre" && !empty {
                    pre += 1;
                }
                if let Some(level) = l
                    .strip_prefix('h')
                    .and_then(|d| d.parse::<u8>().ok())
                    .filter(|d| (1..=6).contains(d))
                    && !empty
                {
                    flush(&mut line, &mut kept, b);
                    heading = Some((level, String::new()));
                } else if l == "br" {
                    // A space that collapses with the white space around it: a block is one
                    // line (spec §5.8; Codex M4 code r3 #3, Fable r4 #2). In a `pre` it ends
                    // the line, as its line breaks do.
                    match heading.as_mut() {
                        Some(h) => collapse(&mut h.1, " "),
                        None if pre > 0 => {
                            kept = true;
                            flush(&mut line, &mut kept, b);
                        }
                        None => collapse(&mut line, " "),
                    }
                } else if block(l) {
                    flush(&mut line, &mut kept, b);
                }
            }
            Ev::End { name, .. } => {
                let l = local(name);
                if matches!(l, "script" | "style" | "head") {
                    skip = skip.saturating_sub(1);
                    continue;
                }
                if l == "pre" {
                    pre = pre.saturating_sub(1);
                }
                if l.len() == 2 && l.starts_with('h') && heading.is_some() {
                    let (level, text) = heading.take().unwrap();
                    b.heading(level, &text);
                } else if block(l) {
                    flush(&mut line, &mut kept, b);
                }
            }
            Ev::Text(t) if skip == 0 && in_body => match heading.as_mut() {
                Some(h) if pre > 0 => h.1.push_str(&t),
                Some(h) => collapse(&mut h.1, &t),
                // Each line of a `pre` is a line, with its spaces (Fable M4 code r4 #3).
                None if pre > 0 => {
                    let mut parts = t.split('\n');
                    line.push_str(parts.next().unwrap_or(""));
                    kept = true;
                    for part in parts {
                        if line.ends_with('\r') {
                            line.pop();
                        }
                        flush(&mut line, &mut kept, b);
                        line.push_str(part);
                        kept = true;
                    }
                }
                None => collapse(&mut line, &t),
            },
            _ => {}
        }
        if b.full() {
            return;
        }
    }
    flush(&mut line, &mut kept, b);
}

/// Appends `t` to `out` with its white space collapsed, as a browser shows it: a run of it is
/// one space, none at the start of a line (a `br` outside a `pre` is one too).
fn collapse(out: &mut String, t: &str) {
    let mut last_space = out.is_empty() || out.ends_with(' ');
    for c in t.chars() {
        if c.is_ascii_whitespace() {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
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

    /// Every element of `xml` with the prefix `x`, bound where the default namespace was.
    fn prefixed(xml: &str) -> String {
        let mut out = String::new();
        let mut rest = xml;
        while let Some(i) = rest.find('<') {
            out.push_str(&rest[..=i]);
            rest = &rest[i + 1..];
            let slash = rest.starts_with('/');
            if slash {
                out.push('/');
                rest = &rest[1..];
            }
            if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                out.push_str("x:");
            }
        }
        out.push_str(rest);
        out.replace("xmlns=", "xmlns:x=")
    }

    /// Codex M4 code r2 #1, in EPUB: the reader knows its elements by their local names, so
    /// a book whose XHTML, package and table of contents use a prefix reads as one without.
    #[test]
    fn an_epub_with_prefixed_elements_reads_the_same() {
        assert_eq!(
            prefixed(r#"<?xml version="1.0"?><html xmlns="u"><p>a<br/></p></html>"#),
            r#"<?xml version="1.0"?><x:html xmlns:x="u"><x:p>a<x:br/></x:p></x:html>"#
        );
        for nav in [true, false] {
            let d = fixture::epub(&[("1. Arrival", CH1), ("2. The Storm", CH2)], nav);
            let p = fixture::rezip(&d, |name, data| {
                if name == "mimetype" {
                    return data;
                }
                prefixed(&String::from_utf8(data).unwrap()).into_bytes()
            });
            assert_eq!(
                extract(Kind::Epub, &p).unwrap().text,
                extract(Kind::Epub, &d).unwrap().text
            );
            assert_eq!(
                trail(&p, "Second"),
                ["1. Arrival", "The Harbour", "At night"]
            );
            assert_eq!(trail(&p, "No heading"), ["2. The Storm"]);
        }
    }

    /// Codex M4 code r3 #3: a `br` inside a block is a space, as a line break inside a Word or
    /// OpenDocument paragraph: the block stays one line, and the lines after it keep their
    /// numbers. White space is collapsed as a browser shows it, except in a `pre`.
    #[test]
    fn a_line_break_inside_a_block_is_a_space() {
        let ch = concat!(
            "<p>one<br/>two</p>",
            "<p>three<br />\n    four</p>",
            "<p>\n  A needle\n  in the water.\n</p>",
            "<pre>a  b</pre>",
            "<h2>Big<br/>head</h2>",
            "<p>after needle</p>",
            // Code review r4, Fable #2: a `br` collapses with the white space around it.
            "<p><br/>brfirst needle six</p>",
            "<p>a<br/><br/>needle nineteen</p>",
            "<p>c <br/> d<br/></p>",
        );
        let d = fixture::epub(&[("Arrival", ch)], true);
        let e = extract(Kind::Epub, &d).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&e.text),
            concat!(
                "one two\nthree four\nA needle in the water.\na  b\nBig head\nafter needle\n",
                "brfirst needle six\na needle nineteen\nc d\n"
            )
        );
        assert_eq!(trail(&d, "after needle"), ["Arrival", "Big head"]);
    }

    /// Code review r4, Fable #3: each line of a `pre` is a line, with its spaces, as a browser
    /// shows it and as a code block of Markdown is; Codex #3: with the spaces at its end.
    #[test]
    fn each_line_of_a_pre_is_a_line() {
        let ch = concat!(
            "<h2>Code</h2>",
            "<pre><code>fn main() {\n    let x = 1;\n\n    needle();\n}\n</code></pre>",
            "<pre>one<br/>  two</pre>",
            // Codex M4 code r4 #3: a `pre` keeps its spaces at the end of a line too, and a
            // no-break space is never collapsed nor trimmed.
            "<pre>  a  b  \n c </pre>",
            "<p>after\n needle</p>",
            "<p>a&nbsp;</p>",
            "<p>&#160; b &nbsp; </p>",
        );
        let d = fixture::epub(&[("Arrival", ch)], true);
        let e = extract(Kind::Epub, &d).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&e.text),
            concat!(
                "Code\nfn main() {\n    let x = 1;\n    needle();\n}\none\n  two\n",
                "  a  b  \n c \nafter needle\na\u{a0}\n\u{a0} b \u{a0}\n"
            )
        );
        assert_eq!(trail(&d, "    needle();"), ["Arrival", "Code"]);
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
