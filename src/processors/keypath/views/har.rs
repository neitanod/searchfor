//! HAR captures (M6 spec §8.8): `log` hides; each element of `entries` is an `entry` named by
//! its request's method and URL without the scheme (`GET api.example.com/v1/users`), its
//! signature with the full URL and the response's status; `pages` are named by their `title`,
//! and the elements of `headers`, `cookies`, `queryString` and `postData.params` by their
//! `name` (`header Authorization`). Every other key is keypath's.
use super::{Built, Ctx, NameBuf, Node, Probe, Role, find, text_of};
use crate::model::Segment;

pub(super) fn project(ctx: Ctx, p: &Probe) -> (Role, Ctx) {
    let named = |word| (Role::Plain, Ctx::Named(word));
    match ctx {
        Ctx::Har if p.is("log") && p.object() => (Role::Hidden, Ctx::Log),
        Ctx::Log if p.is("entries") && p.array() => (Role::Plain, Ctx::Entries),
        Ctx::Log if p.is("pages") && p.array() => (Role::Plain, Ctx::Pages),
        Ctx::Entries if p.element() && p.object() => (Role::Node(Node::Entry), Ctx::Entry),
        Ctx::Pages if p.element() && p.object() => (Role::Node(Node::Page), Ctx::Plain),
        Ctx::Entry if (p.is("request") || p.is("response")) && p.object() => {
            (Role::Plain, Ctx::Exchange)
        }
        Ctx::Exchange if p.is("headers") && p.array() => named("header"),
        Ctx::Exchange if p.is("cookies") && p.array() => named("cookie"),
        Ctx::Exchange if p.is("queryString") && p.array() => named("queryString"),
        Ctx::Exchange if p.is("postData") && p.object() => (Role::Plain, Ctx::PostData),
        Ctx::PostData if p.is("params") && p.array() => named("param"),
        Ctx::Named(word) if p.element() && p.object() => {
            (Role::Node(Node::Named(word)), Ctx::Plain)
        }
        _ => (Role::Plain, Ctx::Plain),
    }
}

/// The Segment of an entry, a page or a named element whose object starts at `value`; None
/// when the values its name needs are absent, and it keeps keypath's element label.
pub(super) fn node(
    src: &[u8],
    node: Node,
    value: usize,
    base: Segment,
    name_chars: usize,
) -> Option<Built> {
    match node {
        Node::Entry => entry(src, value, base, name_chars),
        Node::Page => named(src, value, base, name_chars, "title", None),
        Node::Named(word) => named(src, value, base, name_chars, "name", Some(word)),
        _ => None,
    }
}

fn entry(src: &[u8], value: usize, base: Segment, name_chars: usize) -> Option<Built> {
    let found = find(
        src,
        value,
        &[
            &["request", "method"],
            &["request", "url"],
            &["response", "status"],
        ],
    );
    let [method, url, status] = [0, 1, 2].map(|i| found[i].as_ref().and_then(|v| text_of(src, v)));
    if method.is_none() && url.is_none() {
        return None;
    }
    let mut name = NameBuf::default();
    let mut signature = Vec::new();
    if let Some(m) = &method {
        name.source(m, 0);
        signature.push(m.0.as_str());
    }
    if let Some(u) = &url {
        if method.is_some() {
            name.word(" ");
        }
        name.source(u, scheme(&u.0));
        signature.push(u.0.as_str());
    }
    if let Some(s) = &status {
        signature.push(s.0.as_str());
    }
    let (text, pieces) = name.finish(name_chars);
    Some(Built {
        seg: Segment {
            kind: "entry".into(),
            name: text,
            signature: Some(signature.join(" ")),
            ..base
        },
        label: value..value,
        pieces: Some(pieces),
    })
}

/// The bytes of a URL's scheme and its `://`, 0 when it has none.
fn scheme(url: &str) -> usize {
    match url.find("://") {
        Some(i)
            if i > 0
                && url[..i]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) =>
        {
            i + 3
        }
        _ => 0,
    }
}

/// An element named by its `key` (`title`, `name`), after `word` if any. Its label is empty,
/// as an entry's: a Match in that value stays inside it, under the key.
fn named(
    src: &[u8],
    value: usize,
    base: Segment,
    name_chars: usize,
    key: &str,
    word: Option<&str>,
) -> Option<Built> {
    let v = find(src, value, &[&[key]])[0].clone()?;
    let text = text_of(src, &v)?;
    let mut name = NameBuf::default();
    if let Some(w) = word {
        name.word(w);
        name.word(" ");
    }
    name.source(&text, 0);
    let (text, pieces) = name.finish(name_chars);
    Some(Built {
        seg: Segment {
            kind: "element".into(),
            name: text,
            ..base
        },
        label: value..value,
        pieces: Some(pieces),
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::{every_byte, needles};

    #[test]
    fn late_names_missing_fields_and_other_shapes() {
        // The request after the Match, a missing method, and an element without a name.
        let src = br#"{"log":{"version":"1.2","entries":[
{"response":{"status":404,"content":{"text":"needle"}},"request":{"url":"ftp+x://h/p?needle"}},
{"request":{"method":"POST","url":"/rel","headers":[{"value":"needle"},{"name":"A\u00e9","value":"needle"}]}},
{"x":"needle"}
],"pages":[{"id":"needle"}]}}"#;
        assert_eq!(
            needles(src, "har"),
            [
                "h/p?needle › response › content › text",
                "h/p?needle › request › url",
                "POST /rel › request › headers[0] › value",
                "POST /rel › request › header Aé › value",
                "entries[2] › x",
                "pages[0] › id",
            ]
        );
        every_byte(src, "har");
        // A Match in a value that names a Segment stays inside it, under its own key
        // (orchestrator's decision 1 on code round 1): an entry's URL, a header's name, a
        // page's title.
        let names = br#"{"log":{"entries":[{"request":{"method":"GET","url":"http://h/needle",
"headers":[{"name":"needle","value":"v"}]}}],"pages":[{"title":"needle"}]}}"#;
        assert_eq!(
            needles(names, "har"),
            [
                "GET h/needle › request › url",
                "GET h/needle › request › header needle › name",
                "needle › title",
            ]
        );
        every_byte(names, "har");
        // Not HAR: keypath's Trails.
        assert_eq!(needles(br#"{"log":["needle"]}"#, "har"), ["log[0]"]);
    }
}
