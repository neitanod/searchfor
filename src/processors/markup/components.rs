//! The content of a `<script>`, read by the compiled-in JavaScript, TypeScript or TSX Grammar
//! with the query of that Format's Definition, or by keypath's JSON for JSON-LD (M6 spec §8.3).
//! Its Segments go under the `script` element in the coordinates of the whole Source: ranges,
//! labels, name pieces, anchors and lines are moved by where the content starts (M6 plan, D10).
//! Only compiled-in Grammars read a script, so a component never starts a Grammar Helper (D11).
use super::{Processor, scan};
use crate::model::{Input, Name, Trail};
use crate::processors::keypath::{collect::Collector, json};
use crate::processors::treesitter::{self, Compiled};
use crate::processors::{Capability, Prepared};
use crate::trail::Outline;
use std::ops::Range;
use std::sync::OnceLock;

/// The `[params]` of the Definitions whose queries read a component's scripts: the Registry's
/// active `javascript`, `typescript` and `tsx`, a user's copy included.
pub struct ScriptDefinitions {
    pub javascript: toml::Table,
    pub typescript: toml::Table,
    pub tsx: toml::Table,
}

impl ScriptDefinitions {
    /// The built-in Definitions' (`formats/javascript.toml`, `typescript.toml`, `tsx.toml`).
    pub fn builtin() -> Self {
        let params = |file: &str| {
            crate::formats::builtin::BUILTIN
                .iter()
                .find(|(f, _)| *f == file)
                .and_then(|(_, src)| toml::from_str::<toml::Table>(src).ok())
                .and_then(|t| t.get("params").and_then(|p| p.as_table()).cloned())
                .unwrap_or_default()
        };
        Self {
            javascript: params("javascript.toml"),
            typescript: params("typescript.toml"),
            tsx: params("tsx.toml"),
        }
    }

    /// The Definitions of `registry`, each one it lacks the built-in's.
    pub fn from_registry(registry: &crate::formats::Registry) -> Self {
        let builtin = Self::builtin();
        let get = |name: &str, fallback: toml::Table| {
            registry.get(name).map_or(fallback, |d| d.params.clone())
        };
        Self {
            javascript: get("javascript", builtin.javascript),
            typescript: get("typescript", builtin.typescript),
            tsx: get("tsx", builtin.tsx),
        }
    }

    fn of(&self, lang: Lang) -> &toml::Table {
        match lang {
            Lang::JavaScript => &self.javascript,
            Lang::TypeScript => &self.typescript,
            Lang::Tsx => &self.tsx,
        }
    }
}

/// The language of a script's code, by its `lang` (M6 spec §8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    JavaScript,
    TypeScript,
    Tsx,
}

/// What a `<script>`'s attributes make of its content (M6 spec §8.3, «Which scripts are
/// read»).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptKind {
    Code(Lang),
    JsonLd,
    Text,
}

/// WHATWG HTML's «JavaScript MIME type» essence matches, as spec §8.3 lists them.
const JAVASCRIPT_TYPES: &[&[u8]] = &[
    b"application/ecmascript",
    b"application/javascript",
    b"application/x-ecmascript",
    b"application/x-javascript",
    b"text/ecmascript",
    b"text/javascript",
    b"text/javascript1.0",
    b"text/javascript1.1",
    b"text/javascript1.2",
    b"text/javascript1.3",
    b"text/javascript1.4",
    b"text/javascript1.5",
    b"text/jscript",
    b"text/livescript",
    b"text/x-ecmascript",
    b"text/x-javascript",
];

fn trim(b: &[u8]) -> &[u8] {
    b.trim_ascii()
}

/// The kind of a script whose `type` and `lang` attributes are these (None: absent or empty).
/// No `type`, an empty one, `module` or a JavaScript MIME type, compared trimmed and without
/// case as browsers compare it, is code, whose `lang` picks TypeScript or TSX; an unknown
/// `lang` is text. `application/ld+json` is JSON-LD; any other type is text.
pub(crate) fn script_kind(type_attr: Option<&[u8]>, lang: Option<&[u8]>) -> ScriptKind {
    let t = type_attr.map_or(&b""[..], trim);
    let code = t.is_empty()
        || t.eq_ignore_ascii_case(b"module")
        || JAVASCRIPT_TYPES.iter().any(|j| t.eq_ignore_ascii_case(j));
    if !code {
        return if t.eq_ignore_ascii_case(b"application/ld+json") {
            ScriptKind::JsonLd
        } else {
            ScriptKind::Text
        };
    }
    let lang = lang.map_or(&b""[..], trim).to_ascii_lowercase();
    match &lang[..] {
        b"" | b"js" | b"javascript" | b"jsx" => ScriptKind::Code(Lang::JavaScript),
        b"ts" | b"typescript" => ScriptKind::Code(Lang::TypeScript),
        b"tsx" => ScriptKind::Code(Lang::Tsx),
        _ => ScriptKind::Text,
    }
}

/// The caps of a Format's scripts (`script_max_bytes`, `script_max_line`; None: no cap). Past
/// either, a script keeps the `script` Segment alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Caps {
    pub max_bytes: Option<usize>,
    pub max_line: Option<usize>,
}

/// Whether a line of `s` is longer than `max` bytes, its `\n` and a CR before it excluded: one
/// `memchr` pass.
fn has_line_over(s: &[u8], max: usize) -> bool {
    let mut start = 0;
    for nl in memchr::memchr_iter(b'\n', s) {
        let end = if nl > start && s[nl - 1] == b'\r' {
            nl - 1
        } else {
            nl
        };
        if end - start > max {
            return true;
        }
        start = nl + 1;
    }
    s.len() - start > max
}

/// The script Definitions of a run and their queries, compiled once, the first time a script
/// of their language is read.
pub(crate) struct Scripts {
    defs: ScriptDefinitions,
    compiled: [OnceLock<Option<Compiled>>; 3],
}

impl Scripts {
    pub(crate) fn new(defs: ScriptDefinitions) -> Self {
        Self {
            defs,
            compiled: Default::default(),
        }
    }

    /// The query of `lang`'s Definition, compiled for its Grammar; None when the Grammar is
    /// not compiled in or the query does not compile.
    fn compiled(&self, lang: Lang) -> Option<&Compiled> {
        self.compiled[lang as usize]
            .get_or_init(|| {
                let params = self.defs.of(lang);
                let grammar = params.get("grammar").and_then(|v| v.as_str())?;
                let language = treesitter::grammars::language(grammar)?;
                treesitter::compile(grammar, &language, params).ok()
            })
            .as_ref()
    }

    /// `lang`'s Definition's `max_bytes` (spec §5.3).
    fn max_bytes(&self, lang: Lang) -> i64 {
        self.defs
            .of(lang)
            .get("max_bytes")
            .and_then(|v| v.as_integer())
            .unwrap_or(treesitter::DEFAULT_MAX_BYTES)
    }

    /// The Outline of the script whose start tag is `tag` (its name ending at `name_end`) and
    /// whose content is `content`, in `src`'s coordinates, `lines` being the line breaks
    /// before the content. None when the script keeps its element's Segment alone: text, a
    /// `lang` without a compiled-in Grammar, or past a cap or the Grammar's `max_bytes`.
    pub(crate) fn outline(
        &self,
        src: &[u8],
        tag: &Range<usize>,
        name_end: usize,
        content: Range<usize>,
        lines: u64,
        caps: Caps,
    ) -> Option<Outline> {
        let attr = |a: &[u8]| scan::attribute(src, tag, name_end, a, false).map(|r| &src[r]);
        let kind = script_kind(attr(b"type"), attr(b"lang"));
        let slice = &src[content.clone()];
        if kind == ScriptKind::Text
            || caps.max_bytes.is_some_and(|m| slice.len() > m)
            || caps.max_line.is_some_and(|m| has_line_over(slice, m))
        {
            return None;
        }
        let mut o = match kind {
            ScriptKind::Code(lang) => {
                let c = self.compiled(lang)?;
                if slice.len() as i64 > self.max_bytes(lang) {
                    return None;
                }
                let tree = treesitter::parse(c, slice)?;
                treesitter::outline(c, &tree, slice, None)
            }
            ScriptKind::JsonLd => {
                let mut c = Collector::for_names();
                let end = json::walk(slice, &mut c);
                c.finish(end, slice)
            }
            ScriptKind::Text => unreachable!("text was left above"),
        };
        treesitter::shift(&mut o, content.start, lines);
        Some(o)
    }
}

/// The scripts of the stateless `Markup`: the built-in Definitions.
pub(crate) fn builtin() -> &'static Scripts {
    static BUILTIN: OnceLock<Scripts> = OnceLock::new();
    BUILTIN.get_or_init(|| Scripts::new(ScriptDefinitions::builtin()))
}

/// A markup Processor whose scripts are read with the Definitions of one run's Registry
/// (`Processors::for_definition_in`): built once per Definition, its queries compiled once.
pub struct ComponentMarkup {
    scripts: Scripts,
}

impl ComponentMarkup {
    pub fn new(definitions: ScriptDefinitions) -> Self {
        Self {
            scripts: Scripts::new(definitions),
        }
    }
}

impl Processor for ComponentMarkup {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        super::trails_in(input, params, anchors, Some(&self.scripts))
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        super::names_in(input, params, Some(&self.scripts))
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        super::prepare_in(input, params, Some(&self.scripts))
    }

    fn capability(&self, _: &toml::Table) -> Capability {
        Capability::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_kinds_follow_type_and_lang() {
        let k = |t: Option<&str>, l: Option<&str>| {
            script_kind(t.map(str::as_bytes), l.map(str::as_bytes))
        };
        assert_eq!(k(None, None), ScriptKind::Code(Lang::JavaScript));
        assert_eq!(k(Some(" "), None), ScriptKind::Code(Lang::JavaScript));
        assert_eq!(k(Some("Module"), None), ScriptKind::Code(Lang::JavaScript));
        assert_eq!(
            k(Some(" Text/JavaScript "), Some("TS")),
            ScriptKind::Code(Lang::TypeScript)
        );
        assert_eq!(k(None, Some("tsx")), ScriptKind::Code(Lang::Tsx));
        assert_eq!(k(None, Some("coffee")), ScriptKind::Text);
        assert_eq!(k(Some("application/ld+json"), None), ScriptKind::JsonLd);
        for t in [
            "text/plain",
            "text/x-template",
            "importmap",
            "speculationrules",
        ] {
            assert_eq!(k(Some(t), None), ScriptKind::Text, "{t}");
        }
    }

    #[test]
    fn the_longest_line_excludes_its_line_break() {
        assert!(!has_line_over(b"abc\r\nabc\n", 3));
        assert!(has_line_over(b"abc\nabcd", 3));
        assert!(!has_line_over(b"", 0));
        assert!(has_line_over(b"ab\rc\n", 3));
    }

    #[test]
    fn a_script_outline_is_in_the_sources_coordinates() {
        let src = b"<p>x</p>\n<script lang=ts>\nfunction f() {\n  needle;\n}\n</script>\n";
        let tag_start = memchr::memmem::find(src, b"<script").unwrap();
        let tag_end = tag_start + memchr::memchr(b'>', &src[tag_start..]).unwrap() + 1;
        let content_end = memchr::memmem::find(src, b"</script").unwrap();
        let o = builtin()
            .outline(
                src,
                &(tag_start..tag_end),
                tag_start + 7,
                tag_end..content_end,
                1,
                Caps::default(),
            )
            .unwrap();
        assert_eq!(o.segments.len(), 1);
        let f = &o.segments[0];
        assert_eq!(f.segment.name, "f");
        assert_eq!((f.segment.line, f.segment.end_line), (3, 5));
        let at = memchr::memmem::find(src, b"f()").unwrap();
        assert_eq!(f.label, at..at + 1);
        assert_eq!(f.name.as_ref().unwrap()[0].1, at..at + 1);
        assert_eq!(f.name_anchor, at);
    }
}
