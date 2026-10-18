//! A pull reader of the XML inside Office, OpenDocument and EPUB files (M4 plan, D12): start
//! and end tags with their attributes, and text with its entities decoded. Those files are
//! written by programs and are well formed; at the first construct this reader cannot read,
//! the part ends there, as M3's XML scanner ends its structure (M3 plan, D12).
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev<'a> {
    /// A start tag: its name (with its prefix), its attributes as written, and whether it is
    /// empty (`<br/>`).
    Start {
        name: &'a str,
        attrs: &'a str,
        empty: bool,
    },
    End {
        name: &'a str,
    },
    Text(Cow<'a, str>),
}

pub struct Pull<'a> {
    s: &'a str,
    at: usize,
}

impl<'a> Pull<'a> {
    pub fn new(s: &'a str) -> Self {
        // A BOM before the declaration.
        Pull {
            s: s.strip_prefix('\u{feff}').unwrap_or(s),
            at: 0,
        }
    }
}

impl<'a> Iterator for Pull<'a> {
    type Item = Ev<'a>;

    fn next(&mut self) -> Option<Ev<'a>> {
        loop {
            let rest = self.s.get(self.at..)?;
            if rest.is_empty() {
                return None;
            }
            if !rest.starts_with('<') {
                let end = rest.find('<').unwrap_or(rest.len());
                self.at += end;
                return Some(Ev::Text(unescape(&rest[..end])));
            }
            if let Some(r) = rest.strip_prefix("<!--") {
                self.at += 4 + r.find("-->")? + 3;
                continue;
            }
            if let Some(r) = rest.strip_prefix("<![CDATA[") {
                let end = r.find("]]>")?;
                self.at += 9 + end + 3;
                return Some(Ev::Text(Cow::Borrowed(&r[..end])));
            }
            if rest.starts_with("<?") {
                self.at += rest.find("?>")? + 2;
                continue;
            }
            if rest.starts_with("<!") {
                // A doctype; an internal subset ends at `]>`.
                let end = match (rest.find('['), rest.find('>')) {
                    (Some(b), Some(g)) if b < g => rest.find("]>")? + 2,
                    (_, g) => g? + 1,
                };
                self.at += end;
                continue;
            }
            let end = tag_end(rest)?;
            let tag = &rest[1..end];
            self.at += end + 1;
            if let Some(name) = tag.strip_prefix('/') {
                return Some(Ev::End { name: name.trim() });
            }
            let (tag, empty) = match tag.strip_suffix('/') {
                Some(t) => (t, true),
                None => (tag, false),
            };
            let n = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
            if n == 0 {
                return None;
            }
            return Some(Ev::Start {
                name: &tag[..n],
                attrs: &tag[n..],
                empty,
            });
        }
    }
}

/// The `>` that ends the tag at the start of `s`, outside quoted attribute values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices().skip(1) {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            (None, '<') => return None,
            _ => {}
        }
    }
    None
}

/// The name without its prefix: `w:p` → `p`.
pub fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

/// The value of the attribute whose local name is `want` (`w:val` and `val` both answer
/// `val`), entities decoded.
pub fn attr<'a>(attrs: &'a str, want: &str) -> Option<Cow<'a, str>> {
    find_attr(attrs, |name| local(name) == want)
}

/// The value of the attribute with a prefix whose local name is `want`: `r:id` for `id`, never
/// an unprefixed `id`.
pub fn prefixed<'a>(attrs: &'a str, want: &str) -> Option<Cow<'a, str>> {
    find_attr(attrs, |name| name.contains(':') && local(name) == want)
}

fn find_attr<'a>(attrs: &'a str, wanted: impl Fn(&str) -> bool) -> Option<Cow<'a, str>> {
    let mut s = attrs;
    loop {
        s = s.trim_start();
        let eq = s.find('=')?;
        let name = s[..eq].trim();
        let after = s[eq + 1..].trim_start();
        let q = after.chars().next()?;
        if q != '"' && q != '\'' {
            return None;
        }
        let end = after[1..].find(q)?;
        let value = &after[1..1 + end];
        if wanted(name) {
            return Some(unescape(value));
        }
        s = &after[end + 2..];
    }
}

/// XML's five entities and character references, and the HTML entities EPUB books use most;
/// an unknown entity is kept as written.
pub fn unescape(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        // An entity is short: its `;` comes within 12 bytes (searched as bytes, so a
        // multi-byte character after a lone `&` is never cut).
        let Some(semi) = rest.as_bytes()[..rest.len().min(12)]
            .iter()
            .position(|&b| b == b';')
        else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let c = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "lsquo" => Some('‘'),
            "rsquo" => Some('’'),
            "ldquo" => Some('“'),
            "rdquo" => Some('”'),
            "laquo" => Some('«'),
            "raquo" => Some('»'),
            "copy" => Some('©'),
            _ => ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_text_and_entities() {
        let x = r#"<?xml version="1.0"?><!-- c --><w:p a="1>2" b='x'><w:t>a &amp; b &#233;&#x41;</w:t><w:br/></w:p><![CDATA[<raw>]]>"#;
        let ev: Vec<Ev> = Pull::new(x).collect();
        assert_eq!(
            ev,
            [
                Ev::Start {
                    name: "w:p",
                    attrs: r#" a="1>2" b='x'"#,
                    empty: false
                },
                Ev::Start {
                    name: "w:t",
                    attrs: "",
                    empty: false
                },
                Ev::Text(Cow::Owned("a & b éA".into())),
                Ev::End { name: "w:t" },
                Ev::Start {
                    name: "w:br",
                    attrs: "",
                    empty: true
                },
                Ev::End { name: "w:p" },
                Ev::Text(Cow::Borrowed("<raw>")),
            ]
        );
        assert_eq!(
            attr(r#" w:val="Heading1" x='&lt;'"#, "val").as_deref(),
            Some("Heading1")
        );
        assert_eq!(
            attr(r#" w:val="Heading1" x='&lt;'"#, "x").as_deref(),
            Some("<")
        );
        assert_eq!(attr(r#" w:val="H""#, "nope"), None);
        assert_eq!(
            prefixed(r#" id="256" r:id="rId2""#, "id").as_deref(),
            Some("rId2")
        );
        assert_eq!(
            attr(r#" id="256" r:id="rId2""#, "id").as_deref(),
            Some("256")
        );
        assert_eq!(local("text:p"), "p");
    }

    #[test]
    fn unknown_entities_stay_and_a_broken_tag_ends_the_part() {
        assert_eq!(unescape("a &bogus; &amp"), "a &bogus; &amp");
        // Found by the fuzz: a lone `&` before multi-byte text.
        assert_eq!(unescape("&eedle é"), "&eedle é");
        let ev: Vec<Ev> = Pull::new("<a>x</a><b attr='unclosed>").collect();
        assert_eq!(ev.len(), 3);
    }
}
