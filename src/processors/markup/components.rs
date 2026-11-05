//! The content of a `<script>`, read by the compiled-in JavaScript, TypeScript or TSX Grammar
//! with the query of that Format's Definition, or by keypath's JSON for JSON-LD (M6 spec §8.3).
//! Its Segments go under the `script` element in the coordinates of the whole Source: ranges,
//! labels, name pieces, anchors and lines are moved by where the content starts (M6 plan, D10).
//! Only compiled-in Grammars read a script, so a component never starts a Grammar Helper (D11).
use super::{Processor, scan};
use crate::model::{Input, Name, Trail};
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

/// The language a script's `lang` names: none or `js` JavaScript, `ts` TypeScript, `tsx` TSX
/// (M6 spec §8.3); None for a language with no compiled-in Grammar.
pub(crate) fn lang_of(lang: Option<&[u8]>) -> Option<Lang> {
    let lang = lang
        .map_or(&b""[..], <[u8]>::trim_ascii)
        .to_ascii_lowercase();
    match &lang[..] {
        b"" | b"js" | b"javascript" | b"jsx" => Some(Lang::JavaScript),
        b"ts" | b"typescript" => Some(Lang::TypeScript),
        b"tsx" => Some(Lang::Tsx),
        _ => None,
    }
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
    /// before the content. None when the script keeps its element's Segment alone: a `lang`
    /// without a compiled-in Grammar, or past the Grammar Definition's `max_bytes`.
    pub(crate) fn outline(
        &self,
        src: &[u8],
        tag: &Range<usize>,
        name_end: usize,
        content: Range<usize>,
        lines: u64,
    ) -> Option<Outline> {
        let lang = scan::attribute(src, tag, name_end, b"lang", false).map(|r| &src[r]);
        let lang = lang_of(lang)?;
        let slice = &src[content.clone()];
        let c = self.compiled(lang)?;
        if slice.len() as i64 > self.max_bytes(lang) {
            return None;
        }
        let tree = treesitter::parse(c, slice)?;
        let mut o = treesitter::outline(c, &tree, slice, None);
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
    fn a_scripts_lang_picks_its_grammar() {
        let k = |l: Option<&str>| lang_of(l.map(str::as_bytes));
        assert_eq!(k(None), Some(Lang::JavaScript));
        assert_eq!(k(Some(" JS ")), Some(Lang::JavaScript));
        assert_eq!(k(Some("TS")), Some(Lang::TypeScript));
        assert_eq!(k(Some("typescript")), Some(Lang::TypeScript));
        assert_eq!(k(Some("tsx")), Some(Lang::Tsx));
        assert_eq!(k(Some("coffee")), None);
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
