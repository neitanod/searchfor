//! Processors turn Match anchors into Trails (design note 6).
pub mod csv;
pub mod cursor;
pub mod external;
pub mod frontmatter;
pub mod keypath;
pub mod markdown;
pub mod markup;
pub mod object;
pub mod subtitles;
pub mod treesitter;

use crate::model::{Input, Name, SharedTrail, Trail};
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
    /// Prepares the file once for an ordered cursor (M5b spec §6.1). The default is the model
    /// adapter (`cursor::ModelPrepared`, plan D11): its `streams` is false, and the pipeline
    /// then takes the one-shot whole-file model. Native cursors override it.
    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        Ok(Box::new(cursor::ModelPrepared::new(self, input, params)))
    }
    /// Whether `prepare` gives a native cursor for files of a Definition with `params`, told
    /// before the search without preparing anything (plan Task 9.1).
    fn capability(&self, _params: &toml::Table) -> Capability {
        Capability::Model
    }
}

/// What `Processor::prepare` gives for a Definition (plan Task 9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// An ordered cursor over a preparation made once: the rows stream.
    Native,
    /// The model fallback: the pipeline builds the whole file's model (plan D11).
    Model,
}

/// The Trails of a prepared file in file order (M5b spec §6.1).
pub trait Cursor {
    /// The Trail of a Match whose first byte is `byte`. Offsets come in non-decreasing order,
    /// a repeated offset allowed; an offset before the last one is answered too, never with a
    /// stale Trail. Consecutive equal Trails are the same `Rc`.
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail>;

    /// Where the outermost Segment that holds the last offset asked starts, a hidden one or
    /// one whose label holds the offset included; the offset itself when none holds it. No
    /// later Trail has a Segment that starts before it, so a stream may let go of the Matches
    /// before it (--expand; code review r1, Codex #1). None: the cursor cannot tell.
    fn open_from(&mut self) -> Option<usize> {
        None
    }
}

/// A file prepared once by its Processor (plan D9, D10): it owns or borrows what its cursors
/// read. Worker-local: it never crosses threads.
pub trait Prepared {
    fn cursor(&self) -> Box<dyn Cursor + '_>;
    /// Every searchable Segment name (`--in-headings`), as `Processor::names`.
    fn names(&self) -> anyhow::Result<Vec<Name>>;
    /// A native cursor; false for the model fallback (`cursor::ModelPrepared`).
    fn streams(&self) -> bool {
        false
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

    /// The Outline built once, and the cursor over it (plan D10).
    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        let outline = self.outline(input.bytes, params)?;
        Ok(Box::new(cursor::OutlinePrepared::new(outline)))
    }

    fn capability(&self, _params: &toml::Table) -> Capability {
        Capability::Native
    }
}

/// The stateless built-in Processors, by the name in a Format Definition's `processor`. The
/// pipeline uses it before the search, for the streaming path of Outline Processors; it has no
/// side effects.
pub fn by_name(name: &str) -> Option<&'static dyn Processor> {
    match name {
        "markdown" => Some(&markdown::Markdown),
        "keypath" => Some(&keypath::Keypath),
        "markup" => Some(&markup::Markup),
        "csv" => Some(&csv::Csv),
        "subtitles" => Some(&subtitles::Subtitles),
        "object" => Some(&object::Object),
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
    /// The Grammar directory (spec §7); None: no Grammar is installed.
    grammars: Option<std::path::PathBuf>,
    /// The Grammar Helpers installed there, read once per run (review r1, Codex #5).
    installed: std::sync::OnceLock<Vec<crate::grammars::Installed>>,
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
            grammars: None,
            installed: std::sync::OnceLock::new(),
        }
    }

    /// The Grammar directory whose Grammar Helpers serve the Grammars not compiled in.
    pub fn with_grammars(mut self, dir: Option<std::path::PathBuf>) -> Self {
        self.grammars = dir;
        self.installed = std::sync::OnceLock::new();
        self
    }

    /// The binary of the installed Grammar Helper of `grammar`, from the run's one reading of
    /// the Grammar directory.
    fn installed_binary(&self, grammar: &str) -> Option<&std::path::Path> {
        self.installed
            .get_or_init(|| {
                self.grammars
                    .as_deref()
                    .map(crate::grammars::installed)
                    .unwrap_or_default()
            })
            .iter()
            .find(|i| i.name == grammar)
            .map(|i| i.binary.as_path())
    }

    fn grammar_of(def: &crate::formats::definition::FormatDefinition) -> &str {
        def.params
            .get("grammar")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    /// Whether `grammar` is compiled in or installed.
    fn has_grammar(&self, grammar: &str) -> bool {
        treesitter::grammars::language(grammar).is_some()
            || self.installed_binary(grammar).is_some()
    }

    /// False only for a `treesitter` Definition whose Grammar is neither compiled in nor
    /// installed (`--list-formats` marks it unavailable, spec §7). Never warns.
    pub fn available(&self, def: &crate::formats::definition::FormatDefinition) -> bool {
        def.processor != "treesitter" || self.has_grammar(Self::grammar_of(def))
    }

    /// The Host of the installed Grammar Helper of `grammar` (design note 8): `concurrent`,
    /// with the Definition's `params` in `hello` and its `timeout_ms` if positive. None when
    /// the Grammar is not installed.
    fn grammar_helper_config(
        &self,
        def: &crate::formats::definition::FormatDefinition,
        grammar: &str,
    ) -> Option<crate::helper::HostConfig> {
        let bin = self.installed_binary(grammar)?;
        let timeout = match def.params.get("timeout_ms").and_then(|v| v.as_integer()) {
            Some(ms) if ms > 0 => std::time::Duration::from_millis(ms as u64),
            _ => crate::helper::DEFAULT_TIMEOUT,
        };
        Some(crate::helper::HostConfig {
            format: def.name.clone(),
            command: vec![bin.to_string_lossy().into_owned()],
            mode: crate::helper::Mode::Concurrent,
            timeout,
            params: def.params.clone(),
            threads: self.threads,
        })
    }

    /// The Processor names a Format Definition may use (`Registry::load`).
    pub fn known(name: &str) -> bool {
        by_name(name).is_some() || matches!(name, "treesitter" | "external" | "docs")
    }

    /// Whether files of `def` may get Trails, without building anything or warning: for
    /// `complete_input`, which runs before the search (design note 2). A Grammar describes
    /// when it is compiled in or installed.
    pub fn describes(&self, def: &crate::formats::definition::FormatDefinition) -> bool {
        match def.processor.as_str() {
            "external" => !self.disabled.lock().unwrap().contains(&def.name),
            "treesitter" => self.has_grammar(Self::grammar_of(def)),
            // A document's Trails come from its Extractor, before the search (M4 plan,
            // design note 4); as a Processor, `docs` describes nothing.
            "docs" => false,
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
                let grammar = Self::grammar_of(def);
                if treesitter::grammars::language(grammar).is_some() {
                    return Some(ProcessorRef::BuiltIn(&self.treesitter));
                }
                // Not compiled in: its installed Grammar Helper serves it (spec §7).
                if let Some(cfg) = self.grammar_helper_config(def, grammar) {
                    return self.helper(def, || Ok(cfg));
                }
                self.disable(
                    def,
                    format!(
                        "s4: {}: grammar `{grammar}` is neither compiled in nor installed (see --list-grammars)",
                        def.name
                    ),
                );
                None
            }
            "docs" => None,
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
                self.disable(def, format!("s4: {}: {e}", def.name));
                None
            }
            Ok(cfg) => {
                let launcher = Box::new(crate::helper::host::CommandLauncher {
                    argv: cfg.command.clone(),
                });
                let h = external::HelperProcessor::new(
                    cfg,
                    launcher,
                    self.tmp.clone(),
                    self.warn.clone(),
                );
                // A Grammar Helper keeps the treesitter's `max_bytes` (spec §5.3).
                let h = std::sync::Arc::new(if def.processor == "treesitter" {
                    h.with_size_limit()
                } else {
                    h
                });
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

    /// Closes every helper's stdin and waits for it (spec §4.4, Lifetime), every Definition's
    /// at once (review m2-code-r1, Fable #1). Idempotent.
    pub fn shutdown(&self) {
        let helpers: Vec<_> = self
            .helpers
            .lock()
            .unwrap()
            .drain()
            .map(|(_, h)| h)
            .collect();
        std::thread::scope(|s| {
            for h in &helpers {
                s.spawn(move || h.shutdown());
            }
        });
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

    /// M4: `object` is a stateless built-in; `docs` is known to Definitions but serves no file
    /// as a Processor (its Extractors run before the search: M4 plan, design note 4).
    #[test]
    fn object_is_built_in_and_docs_is_known_but_describes_nothing() {
        assert!(by_name("object").is_some());
        assert!(Processors::known("object") && Processors::known("docs"));
        assert!(by_name("docs").is_none());
        let (w, seen) = collecting();
        let ps = Processors::new(4, w);
        let pdf = def("name='pdf'\nprocessor='docs'\n[params]\nextractor='pdf'");
        assert!(ps.for_definition(&pdf).is_none());
        assert!(!ps.describes(&pdf));
        assert!(ps.available(&pdf));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn by_name_knows_the_m3_built_ins() {
        for p in ["markup", "csv", "subtitles"] {
            assert!(by_name(p).is_some(), "{p}");
            assert!(Processors::known(p), "{p}");
        }
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

    /// A complete install of the Lua Grammar Helper under a new Grammar directory.
    fn lua_installed() -> tempfile::TempDir {
        let grammars = tempfile::tempdir().unwrap();
        let lua = grammars.path().join("lua");
        std::fs::create_dir(&lua).unwrap();
        std::fs::write(lua.join(crate::grammars::binary_name("lua")), "").unwrap();
        std::fs::write(lua.join("lua.toml"), "name='lua'\nprocessor='treesitter'").unwrap();
        std::fs::write(lua.join("installed.json"), r#"{"version":"0.2.0"}"#).unwrap();
        grammars
    }

    #[test]
    fn an_installed_grammar_is_served_by_its_helper() {
        let grammars = lua_installed();
        let (w, seen) = collecting();
        let ps = Processors::new(4, w).with_grammars(Some(grammars.path().to_path_buf()));
        let d = def("name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''");
        assert!(matches!(
            ps.for_definition(&d),
            Some(ProcessorRef::Helper(_))
        ));
        assert!(ps.available(&d));
        assert!(ps.describes(&d));
        assert!(seen.lock().unwrap().is_empty());
        let missing =
            def("name='cobol'\nprocessor='treesitter'\n[params]\ngrammar='cobol'\nquery=''");
        assert!(!ps.available(&missing));
        assert!(ps.available(&def(
            "name='php2'\nprocessor='treesitter'\n[params]\ngrammar='php'\nquery=''"
        )));
    }

    #[test]
    fn installed_grammars_are_read_once_per_run() {
        let grammars = lua_installed();
        let (w, seen) = collecting();
        let ps = Processors::new(4, w).with_grammars(Some(grammars.path().to_path_buf()));
        let d = def("name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''");
        assert!(ps.available(&d));
        std::fs::remove_dir_all(grammars.path().join("lua")).unwrap();
        assert!(ps.describes(&d));
        assert!(matches!(
            ps.for_definition(&d),
            Some(ProcessorRef::Helper(_))
        ));
        assert!(seen.lock().unwrap().is_empty());
        // Without a Grammar directory nothing is installed.
        let (w, _) = collecting();
        assert!(!Processors::new(4, w).with_grammars(None).available(&d));
    }

    #[test]
    fn a_grammar_helper_runs_concurrent_with_the_definitions_params() {
        let grammars = lua_installed();
        let (w, _) = collecting();
        let ps = Processors::new(3, w).with_grammars(Some(grammars.path().to_path_buf()));
        let d = def(
            "name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''\ntimeout_ms=700",
        );
        let cfg = ps.grammar_helper_config(&d, "lua").unwrap();
        assert_eq!(
            cfg.command,
            [grammars
                .path()
                .join("lua")
                .join(crate::grammars::binary_name("lua"))
                .to_string_lossy()
                .into_owned()]
        );
        assert_eq!(cfg.mode, crate::helper::Mode::Concurrent);
        assert_eq!(cfg.timeout, std::time::Duration::from_millis(700));
        assert_eq!(cfg.params, d.params);
        assert_eq!((cfg.format.as_str(), cfg.threads), ("lua", 3));
        let plain = def("name='lua'\nprocessor='treesitter'\n[params]\ngrammar='lua'\nquery=''");
        assert_eq!(
            ps.grammar_helper_config(&plain, "lua").unwrap().timeout,
            crate::helper::DEFAULT_TIMEOUT
        );
        assert!(ps.grammar_helper_config(&plain, "cobol").is_none());
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

    #[test]
    fn a_legacy_processor_gets_a_working_default_cursor() {
        struct Legacy;
        impl Processor for Legacy {
            fn trails(
                &self,
                _: &Input,
                _: &toml::Table,
                a: &[usize],
            ) -> anyhow::Result<Vec<Trail>> {
                Ok(a.iter()
                    .map(|_| {
                        vec![crate::model::Segment {
                            kind: "test".into(),
                            name: "same".into(),
                            line: 1,
                            end_line: 2,
                            level: None,
                            signature: None,
                        }]
                    })
                    .collect())
            }
            fn names(&self, _: &Input, _: &toml::Table) -> anyhow::Result<Vec<Name>> {
                Ok(Vec::new())
            }
        }
        let params = toml::Table::new();
        let input = Input {
            bytes: b"x\ny\n",
            path: std::path::Path::new("x"),
            format: "test",
            disk: None,
        };
        let legacy = Legacy;
        let prepared = legacy.prepare(input, &params).unwrap();
        let mut cursor = prepared.cursor();
        let a = cursor.trail_at(0).unwrap();
        let b = cursor.trail_at(2).unwrap();
        assert_eq!(a.as_ref(), b.as_ref());
        assert!(std::rc::Rc::ptr_eq(&a, &b));
        assert!(!prepared.streams());
    }
}
