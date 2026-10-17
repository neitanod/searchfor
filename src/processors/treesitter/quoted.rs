//! The name of a quoted `@name`: a JavaScript-family string literal (`"abc"`, `''`) names
//! its Segment by its contents, escapes decoded, as one name (review m2-code-r2, Codex #1).
use std::ops::Range;
use tree_sitter::Node;

/// `Name::pieces`: (range in the name's text, range in the Source).
pub(super) type Pieces = Vec<(Range<usize>, Range<usize>)>;

/// The decoded contents of `n` and their pieces, when `n` is a JavaScript, TypeScript or TSX
/// string literal: a `string` between two equal quote tokens whose children are only
/// `string_fragment` and `escape_sequence`. A fragment is verbatim (each invalid UTF-8 run a
/// U+FFFD piece of its own), an escape is one decoded piece over its Source, and an escape that
/// decodes to nothing (a line continuation) has no piece. None for any other node.
pub(super) fn string_name(src: &[u8], n: Node) -> Option<(String, Pieces)> {
    if n.kind() != "string" || n.child_count() < 2 || n.end_byte() > src.len() {
        return None;
    }
    let mut cursor = n.walk();
    let kids: Vec<Node> = n.children(&mut cursor).collect();
    let (open, close) = (kids[0], kids[kids.len() - 1]);
    let quote = |k: Node| !k.is_named() && matches!(k.kind(), "\"" | "'");
    if !quote(open) || close.kind() != open.kind() || close.start_byte() < open.end_byte() {
        return None;
    }
    let middle = &kids[1..kids.len() - 1];
    if !middle
        .iter()
        .all(|k| matches!(k.kind(), "string_fragment" | "escape_sequence"))
    {
        return None;
    }
    let inner = open.end_byte()..close.start_byte();
    let mut name = Name::default();
    let mut at = inner.start;
    let mut i = 0;
    while i < middle.len() {
        let k = middle[i];
        // Bytes no child covers stay verbatim: the name still covers the whole contents.
        name.raw(src, at..k.start_byte());
        at = k.end_byte();
        if k.kind() == "string_fragment" {
            name.raw(src, k.byte_range());
            i += 1;
            continue;
        }
        let escape = decode_escape(&src[k.byte_range()]);
        // A surrogate pair is two escapes and one character.
        if let Escape::Unit(hi @ 0xD800..0xDC00) = escape
            && let Some(&next) = middle.get(i + 1)
            && next.kind() == "escape_sequence"
            && next.start_byte() == k.end_byte()
            && let Escape::Unit(lo @ 0xDC00..0xE000) = decode_escape(&src[next.byte_range()])
        {
            let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
            let c = char::from_u32(cp).unwrap_or('\u{fffd}');
            name.piece(c.encode_utf8(&mut [0; 4]), k.start_byte()..next.end_byte());
            at = next.end_byte();
            i += 2;
            continue;
        }
        let text = match escape {
            Escape::Text(t) => t,
            Escape::Unit(_) => "\u{fffd}".to_string(),
        };
        name.piece(&text, k.byte_range());
        i += 1;
    }
    name.raw(src, at..inner.end);
    if name.pieces.is_empty() {
        // An empty key (`""`) is still a name, at its contents.
        name.pieces.push((0..0, inner));
    }
    Some((name.text, name.pieces))
}

#[derive(Default)]
struct Name {
    text: String,
    pieces: Pieces,
}

impl Name {
    /// `text` decodes the Source bytes `from`; an empty text has no piece.
    fn piece(&mut self, text: &str, from: Range<usize>) {
        if text.is_empty() {
            return;
        }
        let t = self.text.len();
        self.pieces.push((t..t + text.len(), from));
        self.text.push_str(text);
    }

    /// The Source bytes `r` verbatim: each valid run as is, each invalid one a U+FFFD.
    fn raw(&mut self, src: &[u8], r: Range<usize>) {
        if r.start >= r.end {
            return;
        }
        let mut at = r.start;
        for chunk in src[r].utf8_chunks() {
            let v = chunk.valid();
            self.piece(v, at..at + v.len());
            at += v.len();
            let bad = chunk.invalid();
            if !bad.is_empty() {
                self.piece("\u{fffd}", at..at + bad.len());
                at += bad.len();
            }
        }
    }
}

/// What one escape sequence decodes to.
enum Escape {
    Text(String),
    /// A UTF-16 surrogate (`\uD83D`): half of a pair, or a U+FFFD alone.
    Unit(u32),
}

/// An ECMAScript escape sequence (`\n`, `\x41`, `b`, `\u{1F600}`, a legacy octal `\101`,
/// a line continuation, or `\` before any other character, which stands for that character).
fn decode_escape(raw: &[u8]) -> Escape {
    let s = String::from_utf8_lossy(raw);
    let Some(body) = s.strip_prefix('\\') else {
        return Escape::Text(s.into_owned());
    };
    let mut chars = body.chars();
    let Some(c) = chars.next() else {
        return Escape::Text(String::new());
    };
    let rest = chars.as_str();
    let hex = |h: &str| u32::from_str_radix(h, 16).ok();
    let code = |cp: Option<u32>| match cp {
        Some(u @ 0xD800..0xE000) => Escape::Unit(u),
        Some(cp) => Escape::Text(char::from_u32(cp).unwrap_or('\u{fffd}').to_string()),
        None => Escape::Text("\u{fffd}".to_string()),
    };
    let one = |c: char| Escape::Text(c.to_string());
    match c {
        'n' => one('\n'),
        't' => one('\t'),
        'r' => one('\r'),
        'b' => one('\u{8}'),
        'f' => one('\u{c}'),
        'v' => one('\u{b}'),
        // A line continuation: `\` and a line terminator stand for nothing.
        '\r' | '\n' | '\u{2028}' | '\u{2029}' => Escape::Text(String::new()),
        'x' => code(rest.get(..2).and_then(hex)),
        'u' => match rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
            Some(h) => code(hex(h)),
            None => code(rest.get(..4).and_then(hex)),
        },
        '0'..='7' => {
            // Legacy octal: up to three digits from 0–3, up to two from 4–7; the rest of the
            // token is literal (`\777` is `?7`).
            let max = if c <= '3' { 3 } else { 2 };
            let digits: String = std::iter::once(c)
                .chain(rest.chars().take_while(|d| ('0'..='7').contains(d)))
                .take(max)
                .collect();
            let cp = u32::from_str_radix(&digits, 8).unwrap_or(0);
            let mut out = char::from_u32(cp).unwrap_or('\u{fffd}').to_string();
            out.push_str(&body[digits.len()..]);
            Escape::Text(out)
        }
        other => one(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(raw: &str) -> String {
        match decode_escape(raw.as_bytes()) {
            Escape::Text(t) => t,
            Escape::Unit(u) => format!("unit {u:x}"),
        }
    }

    #[test]
    fn ecmascript_escapes() {
        for (raw, want) in [
            ("\\n", "\n"),
            ("\\t", "\t"),
            ("\\v", "\u{b}"),
            ("\\0", "\0"),
            ("\\x41", "A"),
            ("\\u00e9", "é"),
            ("\\u{1F600}", "😀"),
            ("\\u{110000}", "\u{fffd}"),
            ("\\101", "A"),
            ("\\777", "?7"),
            ("\\8", "8"),
            ("\\q", "q"),
            ("\\\"", "\""),
            ("\\é", "é"),
            ("\\\n", ""),
            ("\\\r\n", ""),
            ("\\\u{2028}", ""),
            ("\\uD83D", "unit d83d"),
        ] {
            assert_eq!(text(raw), want, "{raw:?}");
        }
    }
}
