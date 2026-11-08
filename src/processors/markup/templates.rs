//! The template blocks of Svelte and Blade (M6 spec §8.4, §8.5): what opens and closes a
//! `block` Segment in the text between tags. The markup walk keeps them on its stack of
//! elements, so a closer closes its innermost open block of that keyword and every element
//! inside it, as an HTML end tag closes (spec §5.4). Text inside raw text elements (`<script>`,
//! `<style>`) and inside Blade comments is never read for blocks.
use std::ops::Range;

/// The `templates` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateSyntax {
    Svelte,
    Blade,
}

impl TemplateSyntax {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "svelte" => Some(TemplateSyntax::Svelte),
            "blade" => Some(TemplateSyntax::Blade),
            _ => None,
        }
    }

    /// The keywords of its blocks, by the index a block keeps (`Event::Open::kw`).
    #[cfg(test)]
    fn keywords(self) -> &'static [&'static [u8]] {
        match self {
            TemplateSyntax::Svelte => SVELTE,
            TemplateSyntax::Blade => BLADE,
        }
    }
}

/// Svelte's blocks (spec §8.4).
const SVELTE: &[&[u8]] = &[b"if", b"each", b"await", b"key", b"snippet"];

/// Blade's paired directives (spec §8.5).
const BLADE: &[&[u8]] = &[
    b"section",
    b"if",
    b"foreach",
    b"forelse",
    b"for",
    b"while",
    b"switch",
    b"push",
    b"component",
    b"slot",
    b"php",
    b"once",
    b"auth",
    b"guest",
    b"isset",
    b"empty",
    b"unless",
    b"verbatim",
];

/// The most keywords a syntax has: the size of a walk's count of open blocks per keyword.
pub(crate) const KEYWORDS: usize = BLADE.len();

const SECTION: u8 = 0;
const PHP: u8 = 10;
const EMPTY: u8 = 15;
const VERBATIM: u8 = 17;

/// What a template construct at a byte is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    /// None starts there: the byte is text.
    Text,
    /// One that opens and closes nothing, through `end`: a Blade comment or escaped `@@`, an
    /// inline directive (`@section('a', 'b')`, `@php($x)`).
    Skip(usize),
    /// A block opens: `name` is how it is named, as written (`#each items as item`,
    /// `@foreach($items as $item)`), and `tag` the whole opener.
    Open {
        kw: u8,
        name: Range<usize>,
        tag: Range<usize>,
    },
    /// A closer of the blocks of keyword `kw`.
    Close { kw: u8, tag: Range<usize> },
}

/// Blade: whether the text inside a block of `kw` is read for nothing but its closer
/// (`@verbatim` holds text; `@php`'s body is PHP), and whether its tags are text too (PHP's).
pub(crate) fn quiet(syntax: TemplateSyntax, kw: u8) -> Option<bool> {
    match (syntax, kw) {
        (TemplateSyntax::Blade, VERBATIM) => Some(false),
        (TemplateSyntax::Blade, PHP) => Some(true),
        _ => None,
    }
}

/// The bytes after which a template construct may start, besides `<`: `{` for Svelte; `@`
/// and `{` (comments) for Blade, only `@` inside a quiet block.
pub(crate) fn triggers(syntax: TemplateSyntax, quiet: bool) -> (u8, Option<u8>) {
    match (syntax, quiet) {
        (TemplateSyntax::Svelte, _) => (b'{', None),
        (TemplateSyntax::Blade, false) => (b'@', Some(b'{')),
        (TemplateSyntax::Blade, true) => (b'@', None),
    }
}

fn word_end(src: &[u8], mut i: usize) -> usize {
    while i < src.len() && (src[i].is_ascii_alphanumeric() || src[i] == b'_') {
        i += 1;
    }
    i
}

fn find(words: &[&[u8]], w: &[u8]) -> Option<u8> {
    words.iter().position(|k| *k == w).map(|k| k as u8)
}

/// Bytes an opener's expression may span, as LaTeX's `GROUP_MAX`: past them it does not
/// close and the opener is text. A block's expression is a line or a few; the bound keeps an
/// unclosed brace, parenthesis or quote from making every later opener read to the end of the
/// file (code review M6 r1, Fable #4 = Codex #4).
const EXPR_MAX: usize = 4096;

/// The byte after the `close` that ends an expression opened before `from`, skipping quoted
/// strings and nested `open`…`close` pairs; None when the file ends first, or `EXPR_MAX`
/// bytes do.
fn balanced(src: &[u8], from: usize, open: u8, close: u8) -> Option<usize> {
    let limit = src.len().min(from.saturating_add(EXPR_MAX));
    let mut depth = 0usize;
    let mut i = from;
    while i < limit {
        match src[i] {
            q @ (b'"' | b'\'' | b'`') => {
                i += 1;
                while i < limit && src[i] != q {
                    i += if src[i] == b'\\' { 2 } else { 1 };
                }
            }
            b if b == open => depth += 1,
            b if b == close => {
                if depth == 0 {
                    return Some(i + 1);
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether a balanced argument list `(…)` over `args` holds a comma outside its strings and
/// inner brackets: `@section('a', 'b')` is inline (spec §8.5).
fn two_arguments(src: &[u8], args: Range<usize>) -> bool {
    let mut depth = 0usize;
    let mut i = args.start + 1;
    while i + 1 < args.end {
        match src[i] {
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < args.end && src[i] != q {
                    i += if src[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// The template construct that starts at `at`, a trigger byte (`triggers`). `quiet`: the
/// keyword of the open quiet block, whose closer alone is read.
pub(crate) fn scan(syntax: TemplateSyntax, src: &[u8], at: usize, quiet: Option<u8>) -> Event {
    match syntax {
        TemplateSyntax::Svelte => svelte(src, at),
        TemplateSyntax::Blade => blade(src, at, quiet),
    }
}

/// `{#kw …}` opens, `{/kw}` closes; `{:else}`, `{@html …}` and the expressions are text.
fn svelte(src: &[u8], at: usize) -> Event {
    let open = match src.get(at + 1) {
        Some(b'#') => true,
        Some(b'/') => false,
        _ => return Event::Text,
    };
    let w = at + 2;
    let we = word_end(src, w);
    let Some(kw) = find(SVELTE, &src[w..we]) else {
        return Event::Text;
    };
    if open {
        if !src
            .get(we)
            .is_some_and(|&b| b == b'}' || b.is_ascii_whitespace())
        {
            return Event::Text;
        }
        let Some(end) = balanced(src, we, b'{', b'}') else {
            return Event::Text;
        };
        let mut name_end = end - 1;
        while name_end > at + 1 && src[name_end - 1].is_ascii_whitespace() {
            name_end -= 1;
        }
        return Event::Open {
            kw,
            name: at + 1..name_end,
            tag: at..end,
        };
    }
    let mut i = we;
    while i < src.len() && src[i].is_ascii_whitespace() {
        i += 1;
    }
    if src.get(i) == Some(&b'}') {
        Event::Close { kw, tag: at..i + 1 }
    } else {
        Event::Text
    }
}

/// `@kw(…)` or `@kw` opens, `@endkw` closes (`@show`, `@stop` and `@overwrite` close a
/// `@section`); `{{-- … --}}` is a comment; `@@` escapes. A `@` after a word character is text
/// (an e-mail address), as Blade reads it.
fn blade(src: &[u8], at: usize, quiet: Option<u8>) -> Event {
    if src[at] == b'{' {
        if quiet.is_none() && src[at..].starts_with(b"{{--") {
            let end =
                memchr::memmem::find(&src[at + 4..], b"--}}").map_or(src.len(), |k| at + 4 + k + 4);
            return Event::Skip(end);
        }
        return Event::Text;
    }
    if at > 0 && (src[at - 1].is_ascii_alphanumeric() || src[at - 1] == b'_') {
        return Event::Text;
    }
    if src.get(at + 1) == Some(&b'@') {
        return Event::Skip(at + 2);
    }
    let w = at + 1;
    let we = word_end(src, w);
    let word = &src[w..we];
    let closer = word
        .strip_prefix(b"end")
        .and_then(|k| find(BLADE, k))
        .or_else(|| matches!(word, b"show" | b"stop" | b"overwrite").then_some(SECTION));
    if let Some(kw) = closer {
        if quiet.is_some_and(|q| q != kw) {
            return Event::Text;
        }
        return Event::Close { kw, tag: at..we };
    }
    if quiet.is_some() {
        return Event::Text;
    }
    let Some(kw) = find(BLADE, word) else {
        return Event::Text;
    };
    let mut i = we;
    while i < src.len() && matches!(src[i], b' ' | b'\t') {
        i += 1;
    }
    let args = if src.get(i) == Some(&b'(') {
        balanced(src, i + 1, b'(', b')').map(|e| i..e)
    } else {
        None
    };
    match (&args, kw) {
        (Some(a), SECTION) if two_arguments(src, a.clone()) => return Event::Skip(a.end),
        (Some(a), PHP) => return Event::Skip(a.end),
        // `@empty` with no argument is a branch of `@forelse`.
        (None, EMPTY) => return Event::Text,
        _ => {}
    }
    // Arguments that do not close within 4 KiB leave the block named by the directive alone,
    // closed by its `@end…` (code review M6 r2, decision 1).
    let end = args.map_or(we, |a| a.end);
    Event::Open {
        kw,
        name: at..end,
        tag: at..end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(syntax: TemplateSyntax, src: &str) -> Vec<String> {
        let s = src.as_bytes();
        let (a, b) = triggers(syntax, false);
        let mut out = Vec::new();
        let mut i = 0;
        let mut quiet = None;
        while i < s.len() {
            if s[i] != a && Some(s[i]) != b {
                i += 1;
                continue;
            }
            match scan(syntax, s, i, quiet) {
                Event::Text => i += 1,
                Event::Skip(e) => {
                    out.push(format!("skip {}", &src[i..e]));
                    i = e;
                }
                Event::Open { kw, name, tag } => {
                    if super::quiet(syntax, kw).is_some() {
                        quiet = Some(kw);
                    }
                    out.push(format!("open {}", &src[name]));
                    i = tag.end;
                }
                Event::Close { kw, tag } => {
                    if quiet == Some(kw) {
                        quiet = None;
                    }
                    let k = std::str::from_utf8(syntax.keywords()[kw as usize]).unwrap();
                    out.push(format!("close {k}"));
                    i = tag.end;
                }
            }
        }
        out
    }

    #[test]
    fn svelte_blocks_open_and_close_by_keyword() {
        assert_eq!(
            events(
                TemplateSyntax::Svelte,
                "{#each items as { id, n }}x{:else}y{/each }{#if a}{/missing}{@html h}{#iff}"
            ),
            ["open #each items as { id, n }", "close each", "open #if a"]
        );
        assert_eq!(
            events(TemplateSyntax::Svelte, "{#if s === '}'  }"),
            ["open #if s === '}'"]
        );
    }

    /// Code review M6 r1, Fable #4 = Codex #4: a Svelte opener whose expression does not
    /// close within 4 KiB is text, so each of a run of unclosed openers (`{#if (`, `{#if '`)
    /// reads at most that far instead of to the end of the file.
    #[test]
    fn a_svelte_expression_that_does_not_close_within_its_bound_is_text() {
        let pad = |n: usize| " ".repeat(n);
        let svelte = |n: usize| format!("{{#if a{}}}x{{/if}}", pad(n));
        assert_eq!(
            events(TemplateSyntax::Svelte, &svelte(4000)).len(),
            2,
            "closes within the bound"
        );
        assert_eq!(
            events(TemplateSyntax::Svelte, &svelte(5000)),
            ["close if"],
            "closes past the bound"
        );
        let quote = format!("{{#if '{}'}}", pad(5000));
        assert_eq!(events(TemplateSyntax::Svelte, &quote), Vec::<String>::new());
    }

    /// Code review M6 r2, decision 1 (Codex #5, Fable #2): a Blade opener whose argument list
    /// does not close within 4 KiB opens a block named by the directive alone (`@if`); its
    /// arguments are text and its `@end…` closes it, since in Blade the directive is one word
    /// (in Svelte, without `}`, the opener has no end). Each unclosed `@if(` still reads at
    /// most 4 KiB.
    #[test]
    fn a_blade_opener_whose_arguments_do_not_close_within_its_bound_is_the_directive_alone() {
        let pad = |n: usize| " ".repeat(n);
        let blade = |n: usize| format!("@if(a{}) x @endif", pad(n));
        assert_eq!(events(TemplateSyntax::Blade, &blade(4000)).len(), 2);
        assert_eq!(
            events(TemplateSyntax::Blade, &blade(5000)),
            ["open @if", "close if"]
        );
    }

    #[test]
    fn blade_directives_comments_and_escapes() {
        assert_eq!(
            events(
                TemplateSyntax::Blade,
                "@section('a') x @endsection @section('b', 'c') @foreach ($i as $x)a@ endforeach @endforeach"
            ),
            [
                "open @section('a')",
                "close section",
                "skip @section('b', 'c')",
                "open @foreach ($i as $x)",
                "close foreach"
            ]
        );
        assert_eq!(
            events(
                TemplateSyntax::Blade,
                "a@if(x) {{-- @if(y) --}} @@if @php($x) @empty @empty($y) @show"
            ),
            [
                "skip {{-- @if(y) --}}",
                "skip @@",
                "skip @php($x)",
                "open @empty($y)",
                "close section"
            ]
        );
        assert_eq!(
            events(
                TemplateSyntax::Blade,
                "@verbatim @if(x) {{-- --}} @endif @endverbatim @php if (a) @endif @endphp"
            ),
            ["open @verbatim", "close verbatim", "open @php", "close php"]
        );
    }
}
