//! The HTML rules the markup walk applies (spec §5.4): void elements, raw text elements, and
//! the end tags HTML lets an author leave out. A subset of the HTML parsing algorithm: it closes
//! elements, and never inserts one (no implied `tbody`, `head` or `body`). Each tag name is
//! classified once, when its start tag is read (`Info`), so the rules cost a few comparisons.

/// The tags the rules name, as their own value; every other tag is `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Known {
    Li,
    Dt,
    Dd,
    Tr,
    Td,
    Th,
    Thead,
    Tbody,
    Tfoot,
    Option,
    Optgroup,
    P,
    Ul,
    Ol,
    Menu,
    Dl,
    Table,
    Template,
    Select,
    Datalist,
    Heading,
    Other,
}

/// What the rules need to know about a tag name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub known: Known,
    /// Its start tag closes an open `p` (HTML's «close a p element» list).
    pub closes_p: bool,
    /// The search for a `p` to close stops at it.
    pub scope: bool,
    /// It never has content.
    pub void: bool,
    /// Its content is text up to its own end tag.
    pub raw_text: bool,
    /// A hidden Segment, not searchable by name: it holds the whole page (M3 plan, D7).
    pub hidden: bool,
    /// Bit `k`: it is what rule `k` ends, or what stops rule `k`'s search (`RULES`, `P_RULE`).
    targets: u8,
    stops: u8,
}

/// Classifies a tag name, without case.
pub fn info(name: &[u8]) -> Info {
    let mut buf = [0u8; 12];
    let mut i = Info {
        known: Known::Other,
        closes_p: false,
        scope: false,
        void: false,
        raw_text: false,
        hidden: false,
        targets: 0,
        stops: 0,
    };
    if name.len() > buf.len() {
        return i;
    }
    let lower = &mut buf[..name.len()];
    lower.copy_from_slice(name);
    lower.make_ascii_lowercase();
    i.known = match &*lower {
        b"li" => Known::Li,
        b"dt" => Known::Dt,
        b"dd" => Known::Dd,
        b"tr" => Known::Tr,
        b"td" => Known::Td,
        b"th" => Known::Th,
        b"thead" => Known::Thead,
        b"tbody" => Known::Tbody,
        b"tfoot" => Known::Tfoot,
        b"option" => Known::Option,
        b"optgroup" => Known::Optgroup,
        b"p" => Known::P,
        b"ul" => Known::Ul,
        b"ol" => Known::Ol,
        b"menu" => Known::Menu,
        b"dl" => Known::Dl,
        b"table" => Known::Table,
        b"template" => Known::Template,
        b"select" => Known::Select,
        b"datalist" => Known::Datalist,
        b"h1" | b"h2" | b"h3" | b"h4" | b"h5" | b"h6" => Known::Heading,
        _ => Known::Other,
    };
    i.closes_p = matches!(
        &*lower,
        b"address"
            | b"article"
            | b"aside"
            | b"blockquote"
            | b"details"
            | b"dialog"
            | b"dd"
            | b"div"
            | b"dl"
            | b"dt"
            | b"fieldset"
            | b"figcaption"
            | b"figure"
            | b"footer"
            | b"form"
            | b"h1"
            | b"h2"
            | b"h3"
            | b"h4"
            | b"h5"
            | b"h6"
            | b"header"
            | b"hgroup"
            | b"hr"
            | b"li"
            | b"main"
            | b"menu"
            | b"nav"
            | b"ol"
            | b"p"
            | b"pre"
            | b"section"
            | b"table"
            | b"ul"
    );
    i.scope = matches!(
        &*lower,
        b"table" | b"td" | b"th" | b"caption" | b"template" | b"button" | b"html" | b"object"
    );
    i.void = matches!(
        &*lower,
        b"area"
            | b"base"
            | b"br"
            | b"col"
            | b"embed"
            | b"hr"
            | b"img"
            | b"input"
            | b"link"
            | b"meta"
            | b"param"
            | b"source"
            | b"track"
            | b"wbr"
    );
    i.raw_text = matches!(
        &*lower,
        b"script"
            | b"style"
            | b"textarea"
            | b"title"
            | b"xmp"
            | b"iframe"
            | b"noembed"
            | b"noframes"
            | b"plaintext"
    );
    i.hidden = matches!(&*lower, b"html" | b"body");
    let (targets, stops) = masks()[i.known as usize];
    i.targets = targets;
    i.stops = stops;
    i.stops |= (i.scope as u8) << P_RULE;
    i
}

/// The implied-end rules that search the open elements: what a start tag ends, and what stops
/// the search (spec §5.4). `P` is the «close a p element» rule, which `scope` elements stop.
const RULES: [(&[Known], &[Known]); 7] = {
    use Known::*;
    [
        (&[Li], &[Ul, Ol, Menu, Table, Td, Th, Template]),
        (&[Dt, Dd], &[Dl, Table, Td, Th, Template]),
        (&[Tr], &[Table, Template]),
        (&[Td, Th], &[Tr, Table, Template]),
        (&[Thead, Tbody, Tfoot], &[Table, Template]),
        (&[Option], &[Select, Datalist, Optgroup]),
        (&[Optgroup], &[Select]),
    ]
};
const P_RULE: usize = RULES.len();

/// For each rule, the 0-based position among the open elements of the innermost one that rule
/// would end, when no stop lies between it and the current element. Kept per open element and
/// computed when it opens (`reach`), so a start tag finds what it ends in constant time, however
/// deep the elements are (M3 review r2, Codex #1: 40 000 nested `div`s searched the whole stack
/// for a `p` each).
/// `NONE` where nothing is reachable.
pub type Reach = [u32; RULES.len() + 1];
pub const NONE: u32 = u32::MAX;
pub const NO_REACH: Reach = [NONE; RULES.len() + 1];

/// The `Reach` of an element at position `at` with tag `info`, opened inside `parent`'s.
pub fn reach(parent: &Reach, info: &Info, at: usize) -> Reach {
    let mut r = *parent;
    if info.targets | info.stops == 0 {
        return r;
    }
    for (k, slot) in r.iter_mut().enumerate() {
        if info.targets >> k & 1 == 1 {
            *slot = at as u32;
        } else if info.stops >> k & 1 == 1 {
            *slot = NONE;
        }
    }
    r
}

/// For each `Known`, the bits of the rules it is a target or a stop of (`RULES`), computed once.
fn masks() -> &'static [(u8, u8); Known::Other as usize + 1] {
    static MASKS: std::sync::OnceLock<[(u8, u8); Known::Other as usize + 1]> =
        std::sync::OnceLock::new();
    MASKS.get_or_init(|| {
        let mut m = [(0u8, 0u8); Known::Other as usize + 1];
        for (k, (targets, stops)) in RULES.iter().enumerate() {
            for t in *targets {
                m[*t as usize].0 |= 1 << k;
            }
            for t in *stops {
                m[*t as usize].1 |= 1 << k;
            }
        }
        m[Known::P as usize].0 |= 1 << P_RULE;
        m
    })
}

/// How many of the `open` open elements the start tag `start` closes, given the current
/// element `top` and its `Reach`: the innermost ones, up to and including the element it ends
/// implicitly. 0 when it ends none.
pub fn closed_by_start(open: usize, top: Option<&Info>, reach: &Reach, start: &Info) -> usize {
    use Known::*;
    let rule = match start.known {
        Li => Some(0),
        Dt | Dd => Some(1),
        Tr => Some(2),
        Td | Th => Some(3),
        Thead | Tbody | Tfoot => Some(4),
        Option => Some(5),
        Optgroup => Some(6),
        _ => None,
    };
    let ends = |k: usize| (reach[k] != NONE).then(|| open - reach[k] as usize);
    if let Some(n) = rule.and_then(ends) {
        return n;
    }
    if start.closes_p
        && let Some(n) = ends(P_RULE)
    {
        return n;
    }
    // An `optgroup` that closes no group still ends an option that is the current element
    // (M3 review r1, Codex #9).
    if start.known == Optgroup && top.is_some_and(|el| el.known == Option) {
        return 1;
    }
    // A heading start tag closes a heading that is the current element.
    if start.known == Heading && top.is_some_and(|el| el.known == Heading) {
        return 1;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(open: &[&str], name: &str) -> usize {
        let open: Vec<Info> = open.iter().map(|s| info(s.as_bytes())).collect();
        let mut r = NO_REACH;
        for (at, i) in open.iter().enumerate() {
            r = reach(&r, i, at);
        }
        closed_by_start(open.len(), open.last(), &r, &info(name.as_bytes()))
    }

    #[test]
    fn list_items_rows_and_cells_end_their_siblings() {
        assert_eq!(closed(&["ul", "li"], "li"), 1);
        assert_eq!(closed(&["ul", "li", "b"], "LI"), 2);
        // A nested list stops the search: its first item ends nothing outside it.
        assert_eq!(closed(&["ul", "li", "ol"], "li"), 0);
        assert_eq!(closed(&["table", "tr", "td"], "td"), 1);
        assert_eq!(closed(&["table", "tr", "td"], "tr"), 2);
        assert_eq!(closed(&["table", "tbody", "tr", "td"], "tbody"), 3);
        assert_eq!(closed(&["dl", "dt"], "dd"), 1);
        assert_eq!(closed(&["select", "option"], "option"), 1);
        assert_eq!(closed(&["select", "option"], "optgroup"), 1);
        assert_eq!(closed(&["select", "optgroup", "option"], "optgroup"), 2);
    }

    #[test]
    fn block_starts_end_a_paragraph_within_scope() {
        assert_eq!(closed(&["body", "p"], "div"), 1);
        assert_eq!(closed(&["body", "p", "b"], "p"), 2);
        assert_eq!(closed(&["p", "table", "tr", "td"], "div"), 0);
        assert_eq!(closed(&["body", "p"], "span"), 0);
        assert_eq!(closed(&["body", "h2"], "h3"), 1);
    }

    #[test]
    fn classification_ignores_case_and_long_names() {
        assert!(info(b"BR").void && info(b"Script").raw_text && info(b"BODY").hidden);
        assert_eq!(info(b"a-very-long-custom-element").known, Known::Other);
    }
}
