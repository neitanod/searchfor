//! The tags of HTML and XML markup, in file order (spec §5.4): just what Trails need. Text,
//! comments, CDATA sections, processing instructions and doctypes are skipped.
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    /// `<name …>` or `<name …/>`: `tag` covers it from `<` to `>`.
    Start {
        name: Range<usize>,
        tag: Range<usize>,
        self_closing: bool,
    },
    /// `</name …>`.
    End {
        name: Range<usize>,
        tag: Range<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Tag(Tag),
    /// No more tags.
    Eof,
    /// XML only: the markup is not well formed from this byte on (spec §5.4).
    Error(usize),
}

pub struct Scanner<'a> {
    src: &'a [u8],
    pos: usize,
    xml: bool,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// The end of the XML Name that starts at `i`: a NameStartChar, then NameChars, as XML 1.0
/// (fifth edition) defines them over Unicode; Err when there is none or the bytes are not
/// UTF-8 (M3 review r2, Codex #3: `<©>` is no tag).
fn xml_name_end(s: &[u8], mut i: usize) -> Result<usize, ()> {
    let start = i;
    while i < s.len() {
        // ASCII without decoding: the common case.
        if s[i] < 0x80 {
            let b = s[i];
            let ok = b.is_ascii_alphabetic()
                || b == b'_'
                || b == b':'
                || (i > start && (b.is_ascii_digit() || b == b'-' || b == b'.'));
            if !ok {
                break;
            }
            i += 1;
            continue;
        }
        let len = match s[i] {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => return Err(()),
        };
        let c = std::str::from_utf8(s.get(i..i + len).ok_or(())?)
            .map_err(|_| ())?
            .chars()
            .next()
            .unwrap();
        let ok = if i == start {
            xml_name_start(c)
        } else {
            xml_name_char(c)
        };
        if !ok {
            break;
        }
        i += len;
    }
    if i == start { Err(()) } else { Ok(i) }
}

fn xml_name_start(c: char) -> bool {
    matches!(c, ':' | 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

fn xml_name_char(c: char) -> bool {
    xml_name_start(c)
        || matches!(
            c,
            '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}'
        )
}

/// Where a tag name may start: an ASCII letter in HTML; in XML also `_`, `:` and any non-ASCII
/// byte (names are UTF-8).
fn name_start(b: u8, xml: bool) -> bool {
    b.is_ascii_alphabetic() || (xml && (b == b'_' || b == b':' || b >= 0x80))
}

impl<'a> Scanner<'a> {
    pub fn new(src: &'a [u8], xml: bool) -> Self {
        Self { src, pos: 0, xml }
    }

    /// The byte after `pat`, searched from `from`; None when the file ends first.
    fn after(&self, from: usize, pat: &[u8]) -> Option<usize> {
        memchr::memmem::find(&self.src[from.min(self.src.len())..], pat)
            .map(|i| from + i + pat.len())
    }

    /// HTML: the end of a tag whose name ends at `from` — the byte after its `>` — and whether
    /// a `/` right before that `>` makes it self-closing. Attribute values are skipped: a
    /// quote opens a value only right after `=` and optional spaces, and an unquoted value runs
    /// to the next space or `>`, so the `/`s of `href=/docs/` are part of it (M3 review r1,
    /// Fable #5).
    fn tag_end(&self, from: usize) -> Option<(usize, bool)> {
        let s = self.src;
        let mut i = from;
        let mut after_eq = false;
        let mut slash = false;
        while i < s.len() {
            let b = s[i];
            if after_eq && (b == b'"' || b == b'\'') {
                i = memchr::memchr(b, &s[i + 1..]).map(|k| i + 1 + k)?;
                after_eq = false;
                slash = false;
            } else if after_eq && !is_space(b) && b != b'>' {
                while i < s.len() && !is_space(s[i]) && s[i] != b'>' {
                    i += 1;
                }
                after_eq = false;
                slash = false;
                continue;
            } else if b == b'>' {
                return Some((i + 1, slash));
            } else if b == b'=' {
                after_eq = true;
            } else if !is_space(b) {
                slash = b == b'/';
            }
            i += 1;
        }
        None
    }

    /// The byte after the `>` that ends a doctype whose keyword ends at `from`: outside quoted
    /// literals and, in the internal subset, outside comments and processing instructions, so a
    /// `]>` inside an entity value ends nothing (M3 review r1, Codex #4).
    fn doctype_end(&self, from: usize) -> Option<usize> {
        let s = self.src;
        let mut i = from;
        let mut subset = false;
        while i < s.len() {
            match s[i] {
                q @ (b'"' | b'\'') => {
                    i = memchr::memchr(q, &s[i + 1..]).map(|k| i + 1 + k)?;
                }
                b'<' if subset && s[i..].starts_with(b"<!--") => {
                    i = self.after(i + 4, b"-->")? - 1;
                }
                b'<' if subset && s[i..].starts_with(b"<?") => {
                    i = self.after(i + 2, b"?>")? - 1;
                }
                b'[' if !subset => subset = true,
                b']' if subset => subset = false,
                b'>' if !subset => return Some(i + 1),
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// XML: the byte after a well-formed tag whose name starts at `ns` (`<name (S attr S? =
    /// S? "value")* S? /?>`, or `</name S?>`); Err when it is not one.
    fn xml_tag_end(&self, ns: usize, close: bool) -> Result<usize, ()> {
        let s = self.src;
        // XML's white space: no form feed (M3 review r2, Codex #3).
        let spaces = |mut i: usize| {
            while i < s.len() && matches!(s[i], b' ' | b'\t' | b'\n' | b'\r') {
                i += 1;
            }
            i
        };
        let mut i = xml_name_end(s, ns)?;
        if close {
            i = spaces(i);
            return if s.get(i) == Some(&b'>') {
                Ok(i + 1)
            } else {
                Err(())
            };
        }
        // The attribute names seen, for duplicates; past 16 of them, also a set.
        let mut seen: Vec<std::ops::Range<usize>> = Vec::new();
        let mut set: std::collections::HashSet<&[u8]> = std::collections::HashSet::new();
        loop {
            let j = spaces(i);
            match s.get(j) {
                Some(b'>') => return Ok(j + 1),
                Some(b'/') if s.get(j + 1) == Some(&b'>') => return Ok(j + 2),
                Some(_) if j > i => {}
                _ => return Err(()),
            }
            let name_end = xml_name_end(s, j)?;
            let attr = &s[j..name_end];
            let dup = if seen.len() < 16 {
                seen.iter().any(|r| &s[r.clone()] == attr)
            } else {
                if set.is_empty() {
                    set.extend(seen.iter().map(|r| &s[r.clone()]));
                }
                !set.insert(attr)
            };
            if dup {
                return Err(());
            }
            seen.push(j..name_end);
            i = spaces(name_end);
            if s.get(i) != Some(&b'=') {
                return Err(());
            }
            i = spaces(i + 1);
            let q = *s.get(i).filter(|&&b| b == b'"' || b == b'\'').ok_or(())?;
            let v = memchr::memchr(q, &s[i + 1..]).ok_or(())? + i + 1;
            if s[i + 1..v].contains(&b'<') {
                return Err(());
            }
            i = v + 1;
        }
    }

    /// The end of the name that starts at `from`.
    fn name_end(&self, from: usize) -> usize {
        let s = self.src;
        let mut i = from;
        while i < s.len() && !is_space(s[i]) && s[i] != b'>' && s[i] != b'/' {
            i += 1;
        }
        i
    }

    /// Jumps to the `</name` that ends a raw text element (HTML's `script`, `style`, …),
    /// compared without case, or to the end of the file.
    pub fn skip_raw_text(&mut self, name: &[u8]) {
        let s = self.src;
        let mut i = self.pos;
        while let Some(k) = memchr::memchr(b'<', &s[i.min(s.len())..]) {
            let at = i + k;
            let rest = &s[at..];
            if rest.len() >= 2 + name.len()
                && rest[1] == b'/'
                && rest[2..2 + name.len()].eq_ignore_ascii_case(name)
                && rest
                    .get(2 + name.len())
                    .is_none_or(|&b| is_space(b) || b == b'>' || b == b'/')
            {
                self.pos = at;
                return;
            }
            i = at + 1;
        }
        self.pos = s.len();
    }

    /// The next tag, or the end, or (XML) the first byte that is not well formed.
    pub fn step(&mut self) -> Step {
        let s = self.src;
        loop {
            let Some(k) = memchr::memchr(b'<', &s[self.pos.min(s.len())..]) else {
                self.pos = s.len();
                return Step::Eof;
            };
            let lt = self.pos + k;
            let rest = &s[lt..];
            // In HTML a construct the file ends inside is dropped, as the HTML parser drops an
            // unfinished tag; in XML it is an error.
            let unfinished = |me: &mut Self| {
                me.pos = s.len();
                if me.xml { Step::Error(lt) } else { Step::Eof }
            };
            if rest.starts_with(b"<!--") {
                match self.after(lt + 4, b"-->") {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<![CDATA[") && self.xml {
                match self.after(lt + 9, b"]]>") {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<?") && self.xml {
                match self.after(lt + 2, b"?>") {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<!") && self.xml {
                // A doctype, with its internal subset in brackets.
                if !rest[2..].starts_with(b"DOCTYPE") {
                    self.pos = s.len();
                    return Step::Error(lt);
                }
                match self.doctype_end(lt + 9) {
                    Some(e) => self.pos = e,
                    None => return unfinished(self),
                }
                continue;
            }
            if rest.starts_with(b"<!") || rest.starts_with(b"<?") {
                // HTML: a doctype or a bogus comment, to the next `>`.
                match memchr::memchr(b'>', rest) {
                    Some(g) => self.pos = lt + g + 1,
                    None => return unfinished(self),
                }
                continue;
            }
            let close = rest.get(1) == Some(&b'/');
            let ns = lt + 1 + close as usize;
            if !s.get(ns).is_some_and(|&b| name_start(b, self.xml)) {
                if self.xml {
                    self.pos = s.len();
                    return Step::Error(lt);
                }
                if close {
                    // `</` and no name: a bogus comment in HTML.
                    match memchr::memchr(b'>', rest) {
                        Some(g) => self.pos = lt + g + 1,
                        None => return unfinished(self),
                    }
                } else {
                    // `<` as text.
                    self.pos = lt + 1;
                }
                continue;
            }
            let ne = self.name_end(ns);
            let (end, self_closing) = if self.xml {
                // XML: a tag must be well formed, attributes included (M3 review r1, Codex #5).
                match self.xml_tag_end(ns, close) {
                    // Values are quoted, so a `/` before the `>` can only be `/>`.
                    Ok(e) => (e, s[e - 2] == b'/'),
                    Err(()) => {
                        self.pos = s.len();
                        return Step::Error(lt);
                    }
                }
            } else {
                match self.tag_end(ne) {
                    Some(t) => t,
                    None => return unfinished(self),
                }
            };
            self.pos = end;
            let name = ns..ne;
            let tag = lt..end;
            if close {
                return Step::Tag(Tag::End { name, tag });
            }
            return Step::Tag(Tag::Start {
                name,
                tag,
                self_closing,
            });
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }
}

/// The value of attribute `attr` (compared without case in HTML) in the start tag `tag`,
/// whose name ends at `name_end`: its byte range, quotes excluded. None when absent or empty.
pub fn attribute(
    src: &[u8],
    tag: &Range<usize>,
    name_end: usize,
    attr: &[u8],
    xml: bool,
) -> Option<Range<usize>> {
    let s = &src[..tag.end];
    let mut i = name_end;
    while i < s.len() {
        while i < s.len() && (is_space(s[i]) || s[i] == b'/') {
            i += 1;
        }
        if i >= s.len() || s[i] == b'>' {
            return None;
        }
        let a = i;
        while i < s.len() && !is_space(s[i]) && !matches!(s[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let key = &s[a..i];
        while i < s.len() && is_space(s[i]) {
            i += 1;
        }
        let mut value = None;
        if i < s.len() && s[i] == b'=' {
            i += 1;
            while i < s.len() && is_space(s[i]) {
                i += 1;
            }
            if i < s.len() && (s[i] == b'"' || s[i] == b'\'') {
                let q = s[i];
                let v0 = i + 1;
                let v1 = memchr::memchr(q, &s[v0..]).map_or(s.len(), |k| v0 + k);
                value = Some(v0..v1);
                i = v1 + 1;
            } else {
                let v0 = i;
                while i < s.len() && !is_space(s[i]) && s[i] != b'>' {
                    i += 1;
                }
                value = Some(v0..i);
            }
        }
        let same = if xml {
            key == attr
        } else {
            key.eq_ignore_ascii_case(attr)
        };
        if same {
            return value.filter(|v| !v.is_empty());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(src: &str, xml: bool) -> Vec<String> {
        let mut sc = Scanner::new(src.as_bytes(), xml);
        let mut out = Vec::new();
        loop {
            match sc.step() {
                Step::Tag(Tag::Start {
                    name, self_closing, ..
                }) => out.push(format!(
                    "<{}{}",
                    &src[name],
                    if self_closing { "/" } else { "" }
                )),
                Step::Tag(Tag::End { name, .. }) => out.push(format!("</{}", &src[name])),
                Step::Eof => return out,
                Step::Error(at) => {
                    out.push(format!("error at {at}"));
                    return out;
                }
            }
        }
    }

    #[test]
    fn html_skips_comments_doctypes_and_text_angles() {
        assert_eq!(
            tags("<!DOCTYPE html><!-- <b> --><p>a < b<br/></p>", false),
            ["<p", "<br/", "</p"]
        );
        // A quote opens a value only after `=`: `it's` does not hide the `>`.
        assert_eq!(tags("<a title=it's>x</a><i>", false), ["<a", "</a", "<i"]);
        assert_eq!(tags(r#"<a href="x>y">z</a>"#, false), ["<a", "</a"]);
        // A `/` inside an unquoted value is not a self-closing slash (M3 review r1, Fable #5).
        assert_eq!(
            tags("<a href=/x/>z</a><a href=/>z</a>", false),
            ["<a", "</a", "<a", "</a"]
        );
        assert_eq!(tags("<br/><x a=1 />", false), ["<br/", "<x/"]);
        // The file ends inside a comment or a tag: what came before stays.
        assert_eq!(tags("<p>x<!-- open", false), ["<p"]);
        assert_eq!(tags("<p>x<a href='", false), ["<p"]);
    }

    #[test]
    fn xml_skips_cdata_pis_and_doctype_subsets() {
        assert_eq!(
            tags(
                "<?xml version='1.0'?><!DOCTYPE r [<!ENTITY e '>'>]><r><![CDATA[<x>]]><x:y/></r>",
                true
            ),
            ["<r", "<x:y/", "</r"]
        );
    }

    #[test]
    fn xml_reports_where_it_stops_being_well_formed() {
        assert_eq!(tags("<r>a < b</r>", true), ["<r", "error at 5"]);
        assert_eq!(tags("<r><!-- open", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r><!ELEMENT x></r>", true), ["<r", "error at 3"]);
    }

    #[test]
    fn a_doctype_ends_outside_its_literals_and_comments() {
        assert_eq!(
            tags(
                r#"<!DOCTYPE r [<!ENTITY e "]> <fake>"><!-- ]> --><?p ]>?>]><r>x</r>"#,
                true
            ),
            ["<r", "</r"]
        );
        assert_eq!(tags("<!DOCTYPE r SYSTEM 'a>b.dtd'><r/>", true), ["<r/"]);
    }

    #[test]
    fn xml_tags_must_be_well_formed() {
        assert_eq!(tags("<r><a=b>x</a=b></r>", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r><a x=bare>x</a></r>", true), ["<r", "error at 3"]);
        assert_eq!(
            tags("<r><a x='1'y='2'>x</a></r>", true),
            ["<r", "error at 3"]
        );
        assert_eq!(tags("<r><a x=\"<\">x</a></r>", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r></r junk>", true), ["<r", "error at 3"]);
        assert_eq!(
            tags("<r a = 'x' b=\"y\" ><s.t-1 /></r >", true),
            ["<r", "<s.t-1/", "</r"]
        );
        // XML's names and spaces, and no attribute twice (M3 review r2, Codex #3).
        assert_eq!(tags("<r><©>x</©></r>", true), ["<r", "error at 3"]);
        assert_eq!(tags("<r><a\x0cb='x'>x</a></r>", true), ["<r", "error at 3"]);
        assert_eq!(
            tags("<r><a x='1' x='2'>x</a></r>", true),
            ["<r", "error at 3"]
        );
        assert_eq!(
            tags("<r><ñandú·x ü='1'>x</ñandú·x></r>", true),
            ["<r", "<ñandú·x", "</ñandú·x", "</r"]
        );
        assert_eq!(tags("<r><·a/></r>", true), ["<r", "error at 3"]);
        // HTML stays tolerant.
        assert_eq!(tags("<p a=bare b>x</p junk>", false), ["<p", "</p"]);
    }

    #[test]
    fn raw_text_ends_at_its_own_end_tag_only() {
        let src = "<script>if (a<b) x = '</div>';</SCRIPT ><p>";
        let mut sc = Scanner::new(src.as_bytes(), false);
        assert!(matches!(sc.step(), Step::Tag(Tag::Start { .. })));
        sc.skip_raw_text(b"script");
        assert_eq!(sc.pos(), src.find("</SCRIPT").unwrap());
        assert!(matches!(sc.step(), Step::Tag(Tag::End { .. })));
        assert!(matches!(sc.step(), Step::Tag(Tag::Start { .. })));
    }

    #[test]
    fn attributes_by_name() {
        let src = br#"<div data-x="id=1" ID='main' class = "a  b" hidden>"#;
        let tag = 0..src.len();
        let get = |a: &[u8], xml| {
            attribute(src, &tag, 4, a, xml)
                .map(|r| std::str::from_utf8(&src[r]).unwrap().to_string())
        };
        assert_eq!(get(b"id", false).as_deref(), Some("main"));
        assert_eq!(get(b"id", true), None); // XML compares names with case
        assert_eq!(get(b"class", false).as_deref(), Some("a  b"));
        assert_eq!(get(b"hidden", false), None);
        assert_eq!(attribute(b"<p id=x>", &(0..8), 2, b"id", false), Some(6..7));
    }
}
