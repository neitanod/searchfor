//! Telegram's JSON exports (M6 spec §10.5): a chat, the root of a chat export or an element of
//! an account export's `chats.list`, is a `chat` named by its `name`; an element of its
//! `messages` is a `message` named by its `date` as written and its `from`
//! (`2024-01-31T10:04:12 Ana`). The arrays `text` and `text_entities` keep their keys and hide
//! their indexes; the account's wrappers hide.
use super::{Built, Ctx, NameBuf, Node, Probe, Role, find, text_of};
use crate::model::Segment;

pub(super) fn project(ctx: Ctx, p: &Probe) -> (Role, Ctx) {
    match ctx {
        Ctx::Account if p.is("chats") && p.object() => (Role::Hidden, Ctx::Chats),
        Ctx::Chats if p.is("list") && p.array() => (Role::Hidden, Ctx::ChatList),
        Ctx::ChatList if p.element() && p.object() => (Role::Node(Node::Chat), Ctx::Chat),
        Ctx::Chat if p.is("messages") && p.array() => (Role::Plain, Ctx::Messages),
        Ctx::Messages if p.element() && p.object() => (Role::Node(Node::Message), Ctx::Message),
        Ctx::Message if (p.is("text") || p.is("text_entities")) && p.array() => {
            (Role::Shown, Ctx::Lines)
        }
        _ => (Role::Plain, Ctx::Plain),
    }
}

/// The Segment of a chat or a message whose object starts at `value`; None when it has
/// neither of the values its name needs, and it keeps keypath's element label.
pub(super) fn node(
    src: &[u8],
    node: Node,
    value: usize,
    base: Segment,
    name_chars: usize,
) -> Option<Built> {
    match node {
        Node::Chat => chat(src, value, base, name_chars),
        Node::Message => message(src, value, base, name_chars),
        _ => None,
    }
}

/// A chat, named by its `name`. Its label is empty, as a message's: a Match in the name stays
/// inside the chat, under `name`.
pub(super) fn chat(src: &[u8], value: usize, base: Segment, name_chars: usize) -> Option<Built> {
    let v = find(src, value, &[&["name"]])[0].clone()?;
    let text = text_of(src, &v)?;
    let mut name = NameBuf::default();
    name.source(&text, 0);
    let (text, pieces) = name.finish(name_chars);
    Some(Built {
        seg: Segment {
            kind: "chat".into(),
            name: text,
            ..base
        },
        label: value..value,
        pieces: Some(pieces),
    })
}

/// A message: its date as written and its sender, either left out when absent. Its signature
/// repeats its name, which the tree prints alone (a Protobuf `message` prints its kind first).
fn message(src: &[u8], value: usize, base: Segment, name_chars: usize) -> Option<Built> {
    let found = find(src, value, &[&["date"], &["from"]]);
    let [date, from] = [0, 1].map(|i| found[i].as_ref().and_then(|v| text_of(src, v)));
    if date.is_none() && from.is_none() {
        return None;
    }
    let mut name = NameBuf::default();
    if let Some(d) = &date {
        name.source(d, 0);
    }
    if let Some(f) = &from {
        if date.is_some() {
            name.word(" ");
        }
        name.source(f, 0);
    }
    let (text, pieces) = name.finish(name_chars);
    Some(Built {
        seg: Segment {
            kind: "message".into(),
            signature: Some(text.clone()),
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
    fn chats_messages_and_absent_ingredients() {
        // A sender without a date, a message with neither, the chat's name after its
        // messages, and two Matches on one line, each with its own Trail.
        let src = br#"{"messages":[
{"from":"Ana","text":["needle",{"type":"link","text":"needle"}]},
{"id":7,"text":"needle"}
],"name":"Late","x":"needle"}
{"messages":["needle"]}"#;
        assert_eq!(
            needles(src, "telegram"),
            [
                "Late › Ana › text",
                "Late › Ana › text › text",
                "Late › messages[1] › text",
                "Late › x",
                "messages[0]",
            ]
        );
        every_byte(src, "telegram");
        let account = br#"{"about":"needle","chats":{"about":"x","list":[{"name":"W","messages":[{"date":"d","text":"needle"}]},{"messages":[]}]}}"#;
        assert_eq!(needles(account, "telegram"), ["about", "W › d › text"]);
        every_byte(account, "telegram");
        // A Match in the values that name a chat or a message stays inside it, under its own
        // key (orchestrator's decision 1 on code round 1).
        let names = br#"{"name":"needle","messages":[{"date":"d","from":"needle","text":"x"}]}"#;
        assert_eq!(
            needles(names, "telegram"),
            ["needle › name", "needle › d needle › from"]
        );
        every_byte(names, "telegram");
        let account = br#"{"chats":{"list":[{"name":"needle","messages":[]}]}}"#;
        assert_eq!(needles(account, "telegram"), ["needle › name"]);
        // Neither shape: keypath's Trails.
        assert_eq!(needles(br#"{"name":"needle"}"#, "telegram"), ["name"]);
    }
}
