//! Processors turn Match anchors into Trails (design note 6).
pub mod external;
pub mod frontmatter;
pub mod keypath;
pub mod markdown;
pub mod treesitter;

use crate::model::{Input, Name, Trail};
use crate::trail::{Outline, TrailCursor, names_of, trail_for};

/// The public contract. M5's External Processors implement it over the protocol of spec §4.4,
/// which C10 extends with an `"op"` field: `"trails"` (the request of §4.4) and `"names"`
/// (no `matches`; the answer is
/// `{"names":[{"text","pieces":[[text_start,text_end,src_start,src_end],…],"anchor","trail":[…]}]}`).
/// An External Processor that answers `names` with an error makes `--in-headings` skip its
/// files with one warning.
pub trait Processor: Send + Sync {
    /// One Trail per anchor (absolute byte offset in `input.bytes`), in the same order.
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>>;
    /// Every searchable Segment name with the Source pieces of its text (`--in-headings`).
    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>>;
    /// The file's Outline, for a Processor that builds one: then the Trails can be computed
    /// while the rows are printed. None: only `trails`.
    fn outline_of(&self, _input: &Input, _params: &toml::Table) -> Option<anyhow::Result<Outline>> {
        None
    }
}

/// Built-in Processors that can describe a whole file as an Outline.
pub trait OutlineProcessor: Send + Sync {
    fn outline(&self, src: &[u8], params: &toml::Table) -> anyhow::Result<Outline>;
}

impl<T: OutlineProcessor> Processor for T {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let o = self.outline(input.bytes, params)?;
        // Anchors come in file order: one pass over the Outline.
        if anchors.is_sorted() {
            let mut c = TrailCursor::new(&o);
            return Ok(anchors.iter().map(|&a| c.trail(a)).collect());
        }
        Ok(anchors.iter().map(|&a| trail_for(&o, a)).collect())
    }

    fn outline_of(&self, input: &Input, params: &toml::Table) -> Option<anyhow::Result<Outline>> {
        Some(self.outline(input.bytes, params))
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(names_of(&self.outline(input.bytes, params)?))
    }
}

/// The stateless built-in Processors, by the name in a Format Definition's `processor`. The
/// pipeline uses it before the search, for the streaming path of Outline Processors; it has no
/// side effects.
pub fn by_name(name: &str) -> Option<&'static dyn Processor> {
    match name {
        "markdown" => Some(&markdown::Markdown),
        "keypath" => Some(&keypath::Keypath),
        _ => None,
    }
}

/// A Processor for one file: a built-in, the run's treesitter, or the Helper Processor of its
/// Definition.
pub enum ProcessorRef<'a> {
    BuiltIn(&'a dyn Processor),
    Helper(std::sync::Arc<external::HelperProcessor>),
}

impl<'a> std::ops::Deref for ProcessorRef<'a> {
    type Target = dyn Processor + 'a;
    fn deref(&self) -> &Self::Target {
        match self {
            ProcessorRef::BuiltIn(p) => *p,
            ProcessorRef::Helper(h) => h.as_ref(),
        }
    }
}

/// The Processors of one run (design note 3): the built-ins, and one Helper Processor per
/// Definition that needs one, started on first use and stopped when this is dropped.
pub struct Processors {
    threads: usize,
    warn: crate::helper::Warn,
    helpers: std::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<external::HelperProcessor>>,
    >,
    /// Definitions disabled for this run, already warned about.
    disabled: std::sync::Mutex<std::collections::HashSet<String>>,
    tmp: std::sync::Arc<external::RunTemp>,
    /// The run's treesitter (D39).
    treesitter: treesitter::TreeSitter,
}

impl Processors {
    pub fn new(threads: usize, warn: crate::helper::Warn) -> Self {
        Self {
            threads: threads.max(1),
            warn,
            helpers: Default::default(),
            disabled: Default::default(),
            tmp: std::sync::Arc::new(external::RunTemp::new()),
            treesitter: treesitter::TreeSitter::default(),
        }
    }

    /// The Processor names a Format Definition may use (`Registry::load`).
    pub fn known(name: &str) -> bool {
        by_name(name).is_some() || matches!(name, "treesitter" | "external")
    }

    /// Whether files of `def` may get Trails, without building anything or warning: for
    /// `complete_input`, which runs before the search (design note 2). C9 adds installed
    /// Grammars.
    pub fn describes(&self, def: &crate::formats::definition::FormatDefinition) -> bool {
        match def.processor.as_str() {
            "external" => !self.disabled.lock().unwrap().contains(&def.name),
            "treesitter" => {
                let grammar = def
                    .params
                    .get("grammar")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                treesitter::grammars::language(grammar).is_some()
            }
            p => by_name(p).is_some(),
        }
    }

    /// The Processor for files of `def`, or None (the file is Plain). Warns once per run for a
    /// Definition that cannot work: a Grammar not available, an `external` without a valid
    /// `command`.
    pub fn for_definition(
        &self,
        def: &crate::formats::definition::FormatDefinition,
    ) -> Option<ProcessorRef<'_>> {
        if self.disabled.lock().unwrap().contains(&def.name) {
            return None;
        }
        match def.processor.as_str() {
            "external" => self.helper(def, || {
                crate::helper::HostConfig::external(&def.name, &def.params, self.threads)
            }),
            "treesitter" => {
                let grammar = def
                    .params
                    .get("grammar")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if treesitter::grammars::language(grammar).is_some() {
                    return Some(ProcessorRef::BuiltIn(&self.treesitter));
                }
                // C9: an installed Grammar Helper serves it here.
                self.disable(
                    def,
                    format!(
                        "sfor: {}: grammar `{grammar}` is neither compiled in nor installed (see --list-grammars)",
                        def.name
                    ),
                );
                None
            }
            p => by_name(p).map(ProcessorRef::BuiltIn),
        }
    }

    fn helper(
        &self,
        def: &crate::formats::definition::FormatDefinition,
        cfg: impl FnOnce() -> Result<crate::helper::HostConfig, String>,
    ) -> Option<ProcessorRef<'_>> {
        let mut map = self.helpers.lock().unwrap();
        if let Some(h) = map.get(&def.name) {
            return Some(ProcessorRef::Helper(h.clone()));
        }
        match cfg() {
            Err(e) => {
                drop(map);
                self.disable(def, format!("sfor: {}: {e}", def.name));
                None
            }
            Ok(cfg) => {
                let launcher = Box::new(crate::helper::host::CommandLauncher {
                    argv: cfg.command.clone(),
                });
                let h = std::sync::Arc::new(external::HelperProcessor::new(
                    cfg,
                    launcher,
                    self.tmp.clone(),
                    self.warn.clone(),
                ));
                map.insert(def.name.clone(), h.clone());
                Some(ProcessorRef::Helper(h))
            }
        }
    }

    fn disable(&self, def: &crate::formats::definition::FormatDefinition, msg: String) {
        if self.disabled.lock().unwrap().insert(def.name.clone()) {
            (self.warn)(msg);
        }
    }

    /// Closes every helper's stdin and waits for it (spec §4.4, Lifetime). Idempotent.
    pub fn shutdown(&self) {
        let helpers: Vec<_> = self
            .helpers
            .lock()
            .unwrap()
            .drain()
            .map(|(_, h)| h)
            .collect();
        for h in helpers {
            h.shutdown();
        }
    }
}

impl Drop for Processors {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trail::OutlineSegment;
    use std::path::Path;

    /// One heading named "A" over the first 10 bytes, its label the first 4.
    struct Fixed;

    impl OutlineProcessor for Fixed {
        fn outline(&self, src: &[u8], _: &toml::Table) -> anyhow::Result<Outline> {
            let mut s = OutlineSegment::new("heading", "A", 0..10, 0..4);
            s.name = Some(vec![(0..1, 2..3)]);
            Ok(Outline::build(vec![s], src))
        }
    }

    fn input(bytes: &[u8]) -> Input<'_> {
        Input {
            bytes,
            path: Path::new("x.md"),
            format: "markdown",
            disk: None,
        }
    }

    #[test]
    fn outline_processors_give_one_trail_per_anchor_in_order() {
        let src = b"# A\nbody\n\nrest\n";
        let t = Fixed
            .trails(&input(src), &toml::Table::new(), &[5, 12, 2])
            .unwrap();
        let names: Vec<Vec<&str>> = t
            .iter()
            .map(|t| t.iter().map(|s| s.name.as_str()).collect())
            .collect();
        assert_eq!(names, vec![vec!["A"], vec![], vec![]]);
    }

    #[test]
    fn outline_processors_report_names() {
        let n = Fixed
            .names(&input(b"# A\nbody\n\nrest\n"), &toml::Table::new())
            .unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].text, "A");
        assert_eq!(n[0].pieces, vec![(0..1, 2..3)]);
    }

    #[test]
    fn by_name_knows_the_stateless_built_ins_only() {
        assert!(by_name("markdown").is_some());
        assert!(by_name("keypath").is_some());
        // `treesitter` and `external` keep per-run state: `Processors` serves them.
        assert!(by_name("treesitter").is_none());
        assert!(by_name("external").is_none());
        assert!(by_name("plain").is_none());
        assert!(by_name("").is_none());
    }

    use crate::formats::definition::FormatDefinition;
    use std::sync::{Arc, Mutex};

    fn def(src: &str) -> FormatDefinition {
        FormatDefinition::parse(src, Processors::known).unwrap()
    }

    fn collecting() -> (crate::helper::Warn, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        (Arc::new(move |m| s.lock().unwrap().push(m)), seen)
    }

    #[test]
    fn known_processors_include_the_m2_ones() {
        for p in ["markdown", "keypath", "treesitter", "external"] {
            assert!(Processors::known(p), "{p}");
        }
        assert!(!Processors::known("plain"));
    }

    #[test]
    fn built_ins_come_from_by_name() {
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        assert!(matches!(
            ps.for_definition(&def("name='m'\nprocessor='markdown'")),
            Some(ProcessorRef::BuiltIn(_))
        ));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn a_missing_grammar_warns_once_and_gives_none() {
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        let d = def("name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''");
        assert!(ps.for_definition(&d).is_none());
        assert!(ps.for_definition(&d).is_none());
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(
            seen[0].contains("lua") && seen[0].contains("--list-grammars"),
            "{seen:?}"
        );
    }

    #[test]
    fn an_external_definition_without_command_warns_once() {
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        let d = def("name='ini'\nprocessor='external'");
        assert!(ps.for_definition(&d).is_none());
        assert!(ps.for_definition(&d).is_none());
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert!(seen.lock().unwrap()[0].contains("`command`"));
    }

    #[test]
    fn describes_answers_without_building_or_warning() {
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        assert!(ps.describes(&def("name='m'\nprocessor='markdown'")));
        // Until C6 compiles Grammars in, `external` stands for a Processor that may describe.
        assert!(ps.describes(&def(
            "name='ini'\nprocessor='external'\n[params]\ncommand=['x']"
        )));
        assert!(!ps.describes(&def(
            "name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''"
        )));
        assert!(seen.lock().unwrap().is_empty());
        assert!(ps.helpers.lock().unwrap().is_empty());
    }

    #[test]
    fn run_temp_writes_and_removes_files() {
        let t = external::RunTemp::new();
        let f = t
            .write(std::path::Path::new("dir/a.ini"), b"[s]\nk=v\n")
            .unwrap();
        assert!(
            f.path().to_string_lossy().ends_with("-a.ini"),
            "{}",
            f.path().display()
        );
        assert_eq!(std::fs::read(f.path()).unwrap(), b"[s]\nk=v\n");
        let p = f.path().to_path_buf();
        drop(f);
        assert!(!p.exists());
        let s = t.write(std::path::Path::new("<stdin>"), b"x").unwrap();
        assert!(
            s.path().to_string_lossy().ends_with("-_stdin_"),
            "{}",
            s.path().display()
        );
    }
}
