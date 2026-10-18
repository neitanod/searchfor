//! A pull reader of the XML inside Office, OpenDocument and EPUB files (M4 plan, D12): start
//! and end tags with their attributes, and text with its entities decoded. Those files are
//! written by programs and are well formed; at the first construct this reader cannot read,
//! the part ends there, as M3's XML scanner ends its structure (M3 plan, D12).
//!
//! Each tag also says the namespace its prefix is bound to (Codex M4 code r2 #1): a document
//! may bind the OpenDocument names to any prefix, so a reader that must tell `office:text` from
//! another `text` asks for the namespace and the local name, never the prefix as written.
use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev<'a> {
    /// A start tag: its name (with its prefix), the namespace of that prefix, its attributes
    /// as written, and whether it is empty (`<br/>`).
    Start {
        name: &'a str,
        ns: Ns,
        attrs: &'a str,
        empty: bool,
    },
    End {
        name: &'a str,
        ns: Ns,
    },
    Text(Cow<'a, str>),
}

/// The namespaces the readers tell apart by their URI; any other is `Other`, and a name
/// without a prefix outside a default namespace is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ns {
    None,
    Other,
    /// OpenDocument's `office`, `style`, `text`, `table`, `drawing` and `presentation`.
    Office,
    Style,
    Text,
    Table,
    Draw,
    Presentation,
    /// WordprocessingML, transitional or strict.
    Word,
    /// Office Open XML's markup compatibility (`mc:AlternateContent`).
    Mc,
}

impl Ns {
    fn of_uri(uri: &str) -> Ns {
        match uri.trim() {
            "urn:oasis:names:tc:opendocument:xmlns:office:1.0" => Ns::Office,
            "urn:oasis:names:tc:opendocument:xmlns:style:1.0" => Ns::Style,
            "urn:oasis:names:tc:opendocument:xmlns:text:1.0" => Ns::Text,
            "urn:oasis:names:tc:opendocument:xmlns:table:1.0" => Ns::Table,
            "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" => Ns::Draw,
            "urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" => Ns::Presentation,
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            | "http://purl.oclc.org/ooxml/wordprocessingml/main" => Ns::Word,
            "http://schemas.openxmlformats.org/markup-compatibility/2006" => Ns::Mc,
            "" => Ns::None,
            _ => Ns::Other,
        }
    }

    /// The namespace of a prefix no element declares: the one its usual prefix names, as the
    /// programs that write these files spell it. A document without its declarations is not
    /// well formed, and is read as its writer meant it.
    fn of_usual_prefix(prefix: &str) -> Ns {
        match prefix {
            "" => Ns::None,
            "office" => Ns::Office,
            "style" => Ns::Style,
            "text" => Ns::Text,
            "table" => Ns::Table,
            "draw" => Ns::Draw,
            "presentation" => Ns::Presentation,
            "w" => Ns::Word,
            "mc" => Ns::Mc,
            _ => Ns::Other,
        }
    }
}

pub struct Pull<'a> {
    s: &'a str,
    at: usize,
    /// The depth of the element being read: 1 inside the root.
    depth: usize,
    /// The namespaces each prefix is bound to (`""` for the default one), innermost last: a
    /// lookup costs the same however many prefixes a document declares.
    bound: HashMap<&'a str, Vec<Ns>>,
    /// The bindings in the order they were made, with the depth of the element that made each.
    made: Vec<(usize, &'a str)>,
}

impl<'a> Pull<'a> {
    pub fn new(s: &'a str) -> Self {
        // A BOM before the declaration.
        Pull {
            s: s.strip_prefix('\u{feff}').unwrap_or(s),
            at: 0,
            depth: 0,
            bound: HashMap::new(),
            made: Vec::new(),
        }
    }

    /// The namespace of the element named `name`, with the bindings in scope.
    fn ns(&self, name: &str) -> Ns {
        let prefix = name.rfind(':').map_or("", |i| &name[..i]);
        self.bound
            .get(prefix)
            .and_then(|b| b.last())
            .map_or_else(|| Ns::of_usual_prefix(prefix), |ns| *ns)
    }

    /// Binds the prefixes that the attributes of an element at `depth` declare.
    fn bind(&mut self, attrs: &'a str, depth: usize) {
        if !attrs.contains("xmlns") {
            return;
        }
        each_attr(attrs, |name, value| {
            let prefix = match name.strip_prefix("xmlns") {
                Some("") => "",
                Some(p) => match p.strip_prefix(':') {
                    Some(p) => p,
                    None => return false,
                },
                None => return false,
            };
            let ns = Ns::of_uri(&unescape(value));
            self.bound.entry(prefix).or_default().push(ns);
            self.made.push((depth, prefix));
            false
        });
    }

    /// Drops the bindings of the elements at `depth` and deeper.
    fn unbind(&mut self, depth: usize) {
        while let Some(&(d, prefix)) = self.made.last()
            && d >= depth
        {
            self.made.pop();
            if let Some(b) = self.bound.get_mut(prefix) {
                b.pop();
            }
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
                let name = name.trim();
                let ns = self.ns(name);
                self.unbind(self.depth);
                self.depth = self.depth.saturating_sub(1);
                return Some(Ev::End { name, ns });
            }
            let (tag, empty) = match tag.strip_suffix('/') {
                Some(t) => (t, true),
                None => (tag, false),
            };
            let n = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
            if n == 0 {
                return None;
            }
            let (name, attrs) = (&tag[..n], &tag[n..]);
            let depth = self.depth + 1;
            self.bind(attrs, depth);
            let ns = self.ns(name);
            if empty {
                self.unbind(depth);
            } else {
                self.depth = depth;
            }
            return Some(Ev::Start {
                name,
                ns,
                attrs,
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

/// A namespace declaration (`xmlns`, `xmlns:p`) is never an attribute a reader asks for.
fn find_attr<'a>(attrs: &'a str, wanted: impl Fn(&str) -> bool) -> Option<Cow<'a, str>> {
    let mut found = None;
    each_attr(attrs, |name, value| {
        if name != "xmlns" && !name.starts_with("xmlns:") && wanted(name) {
            found = Some(unescape(value));
            return true;
        }
        false
    });
    found
}

/// Calls `f` with the name and the value as written of each attribute, until it answers true.
fn each_attr<'a>(attrs: &'a str, mut f: impl FnMut(&'a str, &'a str) -> bool) {
    let mut s = attrs;
    loop {
        s = s.trim_start();
        let Some(eq) = s.find('=') else { return };
        let name = s[..eq].trim();
        let after = s[eq + 1..].trim_start();
        let Some(q) = after.chars().next() else {
            return;
        };
        if q != '"' && q != '\'' {
            return;
        }
        let Some(end) = after[1..].find(q) else {
            return;
        };
        if f(name, &after[1..1 + end]) {
            return;
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
                    ns: Ns::Word,
                    attrs: r#" a="1>2" b='x'"#,
                    empty: false
                },
                Ev::Start {
                    name: "w:t",
                    ns: Ns::Word,
                    attrs: "",
                    empty: false
                },
                Ev::Text(Cow::Owned("a & b éA".into())),
                Ev::End {
                    name: "w:t",
                    ns: Ns::Word
                },
                Ev::Start {
                    name: "w:br",
                    ns: Ns::Word,
                    attrs: "",
                    empty: true
                },
                Ev::End {
                    name: "w:p",
                    ns: Ns::Word
                },
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

    /// Codex M4 code r2 #1: a tag's namespace comes from the declarations in scope, innermost
    /// first; a prefix nothing declares is read by its usual name; a declaration is never an
    /// attribute.
    #[test]
    fn namespaces_come_from_the_declarations_in_scope() {
        const T: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
        let x = format!(
            r#"<r xmlns="{T}" xmlns:q="urn:other"><p/><q:p xmlns:q="{T}"><q:s/></q:p><q:s/><text:p/><x:p/><a xmlns=""><p/></a><p/></r>"#
        );
        let ns: Vec<(&str, Ns)> = Pull::new(&x)
            .filter_map(|e| match e {
                Ev::Start { name, ns, .. } => Some((name, ns)),
                _ => None,
            })
            .collect();
        assert_eq!(
            ns,
            [
                ("r", Ns::Text),
                ("p", Ns::Text),
                ("q:p", Ns::Text),
                ("q:s", Ns::Text),
                ("q:s", Ns::Other),
                ("text:p", Ns::Text),
                ("x:p", Ns::Other),
                ("a", Ns::None),
                ("p", Ns::None),
                ("p", Ns::Text),
            ]
        );
        let end: Vec<Ns> = Pull::new(&x)
            .filter_map(|e| match e {
                Ev::End { ns, .. } => Some(ns),
                _ => None,
            })
            .collect();
        assert_eq!(end, [Ns::Text, Ns::None, Ns::Text]);
        assert_eq!(
            attr(r#" xmlns:name="u" name="n""#, "name").as_deref(),
            Some("n")
        );
        assert_eq!(attr(r#" xmlns="u""#, "xmlns"), None);
    }

    /// A part that declares many prefixes costs a lookup per tag, not a scan of them all.
    #[test]
    fn many_declarations_do_not_slow_each_tag() {
        let n = 50_000;
        let mut x = String::from("<r");
        for i in 0..n {
            x.push_str(&format!(r#" xmlns:p{i}="urn:{i}""#));
        }
        x.push('>');
        for _ in 0..n {
            x.push_str("<y:a/>");
        }
        x.push_str("</r>");
        let started = std::time::Instant::now();
        assert_eq!(Pull::new(&x).count(), n + 2);
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_secs(2), "{took:?}");
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
