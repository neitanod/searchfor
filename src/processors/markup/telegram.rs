//! markup's `view = "telegram"` (M6 spec §10.5): Telegram Desktop's HTML export, whose page
//! is a list of `div.message`. Each message is a `message` Segment named by the `title` of its
//! date element and its sender's name; a `joined` message without a sender takes the sender of
//! the one before. Every other element is hidden: the page's wrappers and the layout inside a
//! message, so a message keeps one Section. A file without Telegram's class names keeps the
//! HTML Trails of spec §5.4, so a change of Telegram's template costs the decoration, never the
//! Match.
use super::html;
use super::scan::{self, Scanner, Step, Tag};
use crate::text::{CutName, Pieces};
use std::ops::Range;

/// Whether the class attribute of the start tag `tag`, whose name ends at `name_end`, holds
/// `token` among its words.
pub(crate) fn has_class(src: &[u8], tag: &Range<usize>, name_end: usize, token: &[u8]) -> bool {
    scan::attribute(src, tag, name_end, b"class", false).is_some_and(|r| {
        src[r]
            .split(|b| b.is_ascii_whitespace())
            .any(|w| w == token)
    })
}

/// Whether `src` is laid out as Telegram's export: an element of class `message` (attributes
/// in any order, any quotes). One walk over the tags, until the first one.
pub(crate) fn detect(src: &[u8]) -> bool {
    let mut sc = Scanner::new(src, false);
    loop {
        match sc.step() {
            Step::Tag(Tag::Start { name, tag, .. }) => {
                if has_class(src, &tag, name.end, b"message") {
                    return true;
                }
                if html::info(&src[name.clone()]).raw_text {
                    sc.skip_raw_text(&src[name]);
                }
            }
            Step::Eof | Step::Error(_) => return false,
            Step::Tag(_) => {}
        }
    }
}

/// What a message's name is made of, found while its elements are read: the `title` of its
/// date element, its sender's name, or, for a `joined` message without one, the name of the
/// sender before it, which is the file's text but not this message's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MsgName {
    pub date: Option<Range<usize>>,
    pub sender: Option<Range<usize>>,
    pub inherited: Option<Range<usize>>,
}

/// The text after `from` up to the next tag, without the white space around it.
fn text_after(src: &[u8], from: usize) -> Option<Range<usize>> {
    let to = memchr::memchr(b'<', &src[from..]).map_or(src.len(), |k| from + k);
    let mut a = from;
    let mut b = to;
    while a < b && src[a].is_ascii_whitespace() {
        a += 1;
    }
    while b > a && src[b - 1].is_ascii_whitespace() {
        b -= 1;
    }
    (a < b).then_some(a..b)
}

/// Reads what the start tag `tag` (its name ending at `name_end`) of an element inside a
/// message gives the message's name: the first date element's `title`, the first sender.
pub(crate) fn ingredient(src: &[u8], tag: &Range<usize>, name_end: usize, msg: &mut MsgName) {
    if msg.date.is_none() && has_class(src, tag, name_end, b"date") {
        msg.date = scan::attribute(src, tag, name_end, b"title", false);
    }
    if msg.sender.is_none() && has_class(src, tag, name_end, b"from_name") {
        msg.sender = text_after(src, tag.end);
    }
}

/// The character an HTML character reference at the start of `s` stands for, and its length;
/// None when `s` starts with none this view decodes.
fn entity(s: &[u8]) -> Option<(char, usize)> {
    let semi = memchr::memchr(b';', &s[..s.len().min(12)])?;
    let body = &s[1..semi];
    let c = match body {
        b"amp" => '&',
        b"lt" => '<',
        b"gt" => '>',
        b"quot" => '"',
        b"apos" => '\'',
        b"nbsp" => '\u{a0}',
        _ => {
            let num = body.strip_prefix(b"#")?;
            let (digits, radix) = match num.first() {
                Some(b'x' | b'X') => (&num[1..], 16),
                _ => (num, 10),
            };
            let n = u32::from_str_radix(std::str::from_utf8(digits).ok()?, radix).ok()?;
            char::from_u32(n)?
        }
    };
    Some((c, semi + 1))
}

/// The text of `src[r]` pushed to `n`, its character references decoded; with `pieces`, each
/// verbatim run and each reference maps to its Source bytes, else nothing maps.
fn push_text(n: &mut CutName, src: &[u8], r: Range<usize>, pieces: bool) {
    let mut at = r.start;
    let mut run = r.start;
    let verbatim = |n: &mut CutName, run: Range<usize>| {
        if pieces {
            n.push_lossy(src, run);
        } else {
            n.push_str(&String::from_utf8_lossy(&src[run]));
        }
    };
    while let Some(k) = memchr::memchr(b'&', &src[at..r.end]) {
        let amp = at + k;
        match entity(&src[amp..r.end]) {
            Some((c, len)) => {
                verbatim(n, run..amp);
                if pieces {
                    n.push_decoded(c, amp..amp + len);
                } else {
                    n.push_str(c.encode_utf8(&mut [0; 4]));
                }
                at = amp + len;
                run = at;
            }
            None => at = amp + 1,
        }
    }
    verbatim(n, run..r.end);
}

/// A message's name, `31.01.2024 10:04:12 Ana`, cut at `name_chars`, with the pieces of the
/// date and of its own sender (an inherited sender maps to nothing: spec §3, point 4).
pub(crate) fn project(src: &[u8], name_chars: usize, m: &MsgName) -> (String, Pieces) {
    let mut n = CutName::new(name_chars);
    if let Some(d) = &m.date {
        push_text(&mut n, src, d.clone(), true);
    }
    let sender = m
        .sender
        .as_ref()
        .map(|r| (r, true))
        .or(m.inherited.as_ref().map(|r| (r, false)));
    if let Some((r, own)) = sender {
        if m.date.is_some() {
            n.push_str(" ");
        }
        push_text(&mut n, src, r.clone(), own);
    }
    n.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telegram_is_told_by_the_message_class() {
        assert!(detect(
            b"<div class=\"page\"><div id=x class='message default'>"
        ));
        assert!(!detect(
            b"<main><p class=messages>x</p><script>'<b class=message>'</script>"
        ));
    }

    #[test]
    fn a_message_is_named_by_its_date_and_sender() {
        let src = b"<div class='date details' title='31.01.2024 10:04'><div class=from_name>\n Ana &amp; Bo&#x21; \n</div>";
        let mut m = MsgName::default();
        let tag = |s: &str| {
            let a = memchr::memmem::find(src, s.as_bytes()).unwrap();
            a..a + memchr::memchr(b'>', &src[a..]).unwrap() + 1
        };
        let date = tag("<div class='date");
        ingredient(src, &date, date.start + 4, &mut m);
        let from = tag("<div class=from");
        ingredient(src, &from, from.start + 4, &mut m);
        let (text, pieces) = project(src, 40, &m);
        assert_eq!(text, "31.01.2024 10:04 Ana & Bo!");
        let amp = memchr::memmem::find(src, b"&amp;").unwrap();
        assert!(pieces.contains(&(21..22, amp..amp + 5)), "{pieces:?}");
        let joined = MsgName {
            inherited: m.sender.clone(),
            ..MsgName::default()
        };
        let (text, pieces) = project(src, 40, &joined);
        assert_eq!((text.as_str(), pieces.len()), ("Ana & Bo!", 0));
    }
}
