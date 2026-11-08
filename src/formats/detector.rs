//! The Detector (spec §4.1, M6 spec §4.1): override, exact name, compound extension, path glob
//! that writes the extension, extension, path glob that leaves it open, magic bytes, shebang.
use super::{
    Registry,
    definition::{FormatDefinition, path_glob},
};
use globset::{Glob, GlobMatcher};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::path::Path;

/// FNV-1a, for the Detector's maps of short keys (names, extensions, path components): the
/// default SipHash cost about 25 ns a lookup, and a path asks a few of them plus one for every
/// directory component (criterion 51). The keys come from the Definitions, so a path cannot
/// make them collide.
struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type Map<K, V> = HashMap<K, V, BuildHasherDefault<Fnv>>;

enum Override {
    All(String),
    Glob(GlobMatcher, String),
}

pub struct Detector<'r> {
    registry: &'r Registry,
    overrides: Vec<Override>,
    /// `--docs`, on by default: with `--no-docs` the document Formats (`processor = "docs"`)
    /// are never detected, so their files are Plain, as ripgrep reads them (spec §5.8).
    docs: bool,
    /// With `--docs`, the exact names and the extensions (lowercase) the document Formats
    /// claim (`may_be_document`).
    doc_names: HashSet<String>,
    doc_extensions: HashSet<String>,
    /// The first active Definition (in the Registry's order) of each exact name and of each
    /// extension (lowercase): `detect` asks them of every file.
    by_name: Map<String, usize>,
    by_extension: Map<String, usize>,
    /// Compound extensions (`blade.php`, M6 spec §4.1 signal 3), lowercase, keyed by their last
    /// extension: only a name whose last extension is a key compares its last two, so every
    /// other name costs the extension lookup it costs without them. Each compound appears
    /// once, with the first active Definition (in the Registry's order) that claims it.
    compound: Map<String, Vec<(String, usize)>>,
    /// The `paths` globs of the active Definitions (M6 spec §4.1 signals 4 and 6).
    paths: PathIndex,
    /// With `--docs`, the `paths` globs of the document Formats (`may_be_document`).
    doc_paths: PathIndex,
    /// What `detect` gives a name without an extension and an empty head, past its exact
    /// names: the first active Definition whose magic bytes are empty.
    empty_head: Option<usize>,
    /// An active Definition looks at a file's first bytes (magic bytes or a shebang).
    by_content: bool,
    /// With `--docs`, the document Formats that have magic bytes (`document_magic`).
    doc_magic: Vec<usize>,
    /// With `--docs`, a document Format has a shebang.
    doc_shebang: bool,
}

impl<'r> Detector<'r> {
    /// `--format` values: `NAME` or `GLOB=NAME`. Unknown names are an error.
    pub fn new(registry: &'r Registry, formats: &[String]) -> anyhow::Result<Self> {
        let mut overrides = Vec::new();
        for f in formats {
            let (glob, name) = match f.rsplit_once('=') {
                Some((g, n)) => (Some(g), n),
                None => (None, f.as_str()),
            };
            if registry.get(name).is_none() {
                anyhow::bail!("unknown format `{name}` (see --list-formats)");
            }
            overrides.push(match glob {
                Some(g) => Override::Glob(Glob::new(g)?.compile_matcher(), name.to_string()),
                None => Override::All(name.to_string()),
            });
        }
        let mut d = Self {
            registry,
            overrides,
            docs: false,
            doc_names: HashSet::new(),
            doc_extensions: HashSet::new(),
            by_name: Map::default(),
            by_extension: Map::default(),
            compound: Map::default(),
            paths: PathIndex::default(),
            doc_paths: PathIndex::default(),
            empty_head: None,
            by_content: false,
            doc_magic: Vec::new(),
            doc_shebang: false,
        };
        d.index();
        Ok(d)
    }

    fn index(&mut self) {
        self.by_name.clear();
        self.by_extension.clear();
        self.compound.clear();
        self.paths = PathIndex::default();
        self.doc_paths = PathIndex::default();
        self.empty_head = None;
        self.by_content = false;
        self.doc_magic.clear();
        self.doc_shebang = false;
        for (i, d) in self.registry.all().iter().enumerate() {
            if !self.active(d) {
                continue;
            }
            if self.empty_head.is_none() && d.magic_at(&[]) {
                self.empty_head = Some(i);
            }
            self.by_content |= !d.magic.is_empty() || !d.shebangs.is_empty();
            if d.processor == "docs" && !d.magic.is_empty() {
                self.doc_magic.push(i);
            }
            self.doc_shebang |= d.processor == "docs" && !d.shebangs.is_empty();
            for n in &d.names {
                self.by_name.entry(n.clone()).or_insert(i);
            }
            for p in &d.paths {
                self.paths.add(i, p);
                if d.processor == "docs" {
                    self.doc_paths.add(i, p);
                }
            }
            for e in &d.extensions {
                let e = e.to_ascii_lowercase();
                match e.rsplit_once('.') {
                    Some((_, last)) => {
                        let same = self.compound.entry(last.to_string()).or_default();
                        if !same.iter().any(|(c, _)| *c == e) {
                            same.push((e, i));
                        }
                    }
                    None => {
                        self.by_extension.entry(e).or_insert(i);
                    }
                }
            }
        }
    }

    /// The path globs of a name without an extension: a dotfile's (`.env`) glob that writes
    /// the rest of its name as an extension (signal 4), then every glob that leaves it open.
    fn by_paths_without_extension(&self, path: &Path) -> Option<usize> {
        if self.paths.is_empty() {
            return None;
        }
        let at = At::new(path);
        self.paths
            .before_extension(&at)
            .or_else(|| self.paths.after_extension(&at))
    }

    /// The first Definition, in the Registry's order, whose compound extension ends
    /// `file_name`, whose last extension is `ext` (lowercase).
    fn compound_of(&self, file_name: &str, ext: &str) -> Option<usize> {
        self.compound
            .get(ext)?
            .iter()
            .filter(|(c, _)| ends_with_extension(file_name, c))
            .map(|&(_, i)| i)
            .min()
    }

    /// Detects the document Formats too (`--docs`). `new` indexed the Definitions without
    /// them, so `with_docs(false)` indexes nothing again (criterion 51).
    pub fn with_docs(mut self, on: bool) -> Self {
        if on == self.docs {
            return self;
        }
        self.docs = on;
        if on {
            for d in self.registry.all().iter().filter(|d| d.processor == "docs") {
                self.doc_names.extend(d.names.iter().cloned());
                // A compound extension (`tar.docx`) by its last one, which a name with it has.
                self.doc_extensions.extend(d.extensions.iter().map(|e| {
                    let e = e.to_ascii_lowercase();
                    match e.rsplit_once('.') {
                        Some((_, last)) => last.to_string(),
                        None => e,
                    }
                }));
            }
        }
        self.index();
        self
    }

    /// False when `path` cannot be a document, which a set lookup tells for most files, before
    /// `detect` walks every Definition: --docs asks it of every file (M4 plan r1, Fable #5).
    /// True with an override, for a name without an extension (its first bytes decide), and
    /// for a name or extension a document Format claims.
    pub fn may_be_document(&self, path: &Path) -> bool {
        if !self.docs {
            return false;
        }
        let (Some(name), Some(ext)) = (
            path.file_name().and_then(|n| n.to_str()),
            path.extension().and_then(|e| e.to_str()),
        ) else {
            return true;
        };
        !self.overrides.is_empty()
            || self.doc_names.contains(name)
            || self
                .doc_extensions
                .contains(normalized_extension(ext).as_ref())
            || (!self.doc_paths.is_empty() && {
                let at = At::new(path);
                self.doc_paths.before_extension(&at).is_some()
                    || self.doc_paths.after_extension(&at).is_some()
            })
    }

    /// Whether `detect` may find a document Format in `head`, the first bytes of a name
    /// without an extension whose exact name decides nothing: its magic bytes start it, or an
    /// override or a document Format's shebang may decide. False tells that the file is no
    /// document without `detect`'s walk of every Definition (code review r6, Fable #1).
    pub fn document_magic(&self, head: &[u8]) -> bool {
        let defs = self.registry.all();
        !self.overrides.is_empty()
            || self.doc_shebang
            || self.doc_magic.iter().any(|&i| defs[i].magic_at(head))
    }

    /// Whether `d` takes part in detection: a document Format unless `--no-docs`.
    fn active(&self, d: &FormatDefinition) -> bool {
        self.docs || d.processor != "docs"
    }

    /// The object Format whose magic bytes start `head`, whatever the file's name: binaries are
    /// often named without a telling extension (`libc.so.6`, `ls`), so for a binary file with a
    /// Match the magic decides (spec §4.1, M4 plan D5).
    pub fn object(&self, head: &[u8]) -> Option<&'r FormatDefinition> {
        self.registry
            .all()
            .iter()
            .find(|d| d.processor == "object" && d.magic_at(head))
    }

    /// The object Formats whose magic bytes start `head`, in the Registry's order: the
    /// candidates a binary file with a Match is read as, until one's layout reads it (M6 spec
    /// §4.2, `cafebabe` starting a fat Mach-O and a Java class). `object` is the first.
    pub fn object_candidates<'a>(
        &'a self,
        head: &'a [u8],
    ) -> impl Iterator<Item = &'r FormatDefinition> + 'a {
        self.registry
            .all()
            .iter()
            .filter(move |d| d.processor == "object" && d.magic_at(head))
    }

    /// `detect` before reading the file: Some when the path alone decides (an override, an
    /// exact name, an extension, a name with an extension no Format claims, or no Format that
    /// looks at the content), None when the Format may depend on the first bytes (magic bytes
    /// or a shebang), which happens only for a name without extension (M2 plan, D15).
    pub fn detect_by_path(&self, path: &Path) -> Option<Option<&'r FormatDefinition>> {
        // A name without an extension and no override, asked of every such file: `detect` with
        // an empty head would look at its exact names and at the magic bytes that are empty,
        // without walking every Definition (code review r6, Fable #1).
        if self.overrides.is_empty() && path.extension().is_none() {
            let defs = self.registry.all();
            let named = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| self.by_name.get(n))
                .copied();
            if let Some(i) = named
                .or_else(|| self.by_paths_without_extension(path))
                .or(self.empty_head)
            {
                return Some(Some(&defs[i]));
            }
            return (!self.by_content).then_some(None);
        }
        if let Some(d) = self.detect(path, &[]) {
            return Some(Some(d));
        }
        if path.extension().is_some() {
            return Some(None);
        }
        let by_content = self
            .registry
            .all()
            .iter()
            .any(|d| self.active(d) && (!d.magic.is_empty() || !d.shebangs.is_empty()));
        (!by_content).then_some(None)
    }

    /// `head` is the start of the file; 512 bytes are enough for every signal. Magic bytes and
    /// a shebang are read only when the file name has no extension (M2 plan, D15); a dotfile
    /// such as `.bashrc` has none.
    pub fn detect(&self, path: &Path, head: &[u8]) -> Option<&'r FormatDefinition> {
        let defs = self.registry.all();
        for o in &self.overrides {
            match o {
                Override::All(n) => return self.registry.get(n),
                Override::Glob(g, n) if g.is_match(path) => return self.registry.get(n),
                Override::Glob(..) => {}
            }
        }
        // A file name that is not UTF-8 skips the name and extension signals only.
        let name = path.file_name();
        let file_name = name.and_then(|n| n.to_str());
        if let Some(file_name) = file_name
            && let Some(&i) = self.by_name.get(file_name)
        {
            return Some(&defs[i]);
        }
        if let Some(ext) = path.extension() {
            // The file name, read once for every signal below (criterion 51).
            let name = name.map(|n| n.as_encoded_bytes()).unwrap_or_default();
            if let Some(ext) = ext.to_str() {
                let ext = normalized_extension(ext);
                // A compound extension beats the last extension alone (M6 spec §4.1 signal 3).
                if !self.compound.is_empty()
                    && let Some(file_name) = file_name
                    && let Some(i) = self.compound_of(file_name, &ext)
                {
                    return Some(&defs[i]);
                }
                // A path glob that writes the extension beats it (signal 4), looked up by the
                // extension: a name whose extension no such glob writes pays one lookup.
                if !self.paths.is_empty()
                    && let Some(i) =
                        self.paths
                            .before_extension(&At::with_name(path, name, Some(ext.as_ref())))
                {
                    return Some(&defs[i]);
                }
                if let Some(&i) = self.by_extension.get(ext.as_ref()) {
                    return Some(&defs[i]);
                }
            }
            // A path glob that leaves the extension open (signal 6).
            if !self.paths.is_empty()
                && let Some(i) = self.paths.after_extension(&At::with_name(path, name, None))
            {
                return Some(&defs[i]);
            }
            // Magic bytes and shebangs decide only for a name without extension (M2 plan, D15).
            return None;
        }
        if let Some(i) = self.by_paths_without_extension(path) {
            return Some(&defs[i]);
        }
        if let Some(d) = defs.iter().find(|d| self.active(d) && d.magic_at(head)) {
            return Some(d);
        }
        let interp = shebang_interpreter(head)?;
        defs.iter().find(|d| d.shebangs.iter().any(|s| s == interp))
    }
}

/// A file's path, as the path globs look at it.
struct At<'p> {
    path: &'p Path,
    /// The file name, or empty.
    name: &'p [u8],
    /// The lowercase extension signal 4 looks up: the path's, or for a dotfile without one
    /// (`.env`) the rest of its name, which a glob such as `*.env` writes as an extension.
    key: Option<std::borrow::Cow<'p, str>>,
}

impl<'p> At<'p> {
    fn new(path: &'p Path) -> Self {
        let key = path
            .extension()
            .and_then(|e| e.to_str())
            .map(normalized_extension);
        Self::from_key(path, key)
    }

    /// `new` of a path with an extension, with the file name and the extension (normalized)
    /// `detect` already read.
    fn with_name(path: &'p Path, name: &'p [u8], key: Option<&'p str>) -> Self {
        Self {
            path,
            name,
            key: key.map(std::borrow::Cow::Borrowed),
        }
    }

    fn from_key(path: &'p Path, key: Option<std::borrow::Cow<'p, str>>) -> Self {
        let name = path
            .file_name()
            .map(|n| n.as_encoded_bytes())
            .unwrap_or_default();
        let key = key.or_else(|| {
            let rest = path
                .extension()
                .is_none()
                .then(|| name.strip_prefix(b"."))??;
            let rest = std::str::from_utf8(rest).ok()?;
            (!rest.is_empty() && !rest.contains('.')).then(|| normalized_extension(rest))
        });
        Self { path, name, key }
    }

    /// Whether a directory above the file is `token`, or starts with it. `bytes` finds
    /// `token` in the path: a path without it has no such directory, which costs one search
    /// instead of a walk of its components (criterion 51).
    fn has_directory(&self, filter: &DirFilter, bytes: &memchr::memmem::Finder) -> bool {
        if bytes
            .find(self.path.as_os_str().as_encoded_bytes())
            .is_none()
        {
            return false;
        }
        let Some(parent) = self.path.parent() else {
            return false;
        };
        parent.components().any(|c| {
            let c = c.as_os_str().as_encoded_bytes();
            match filter {
                DirFilter::Component(t) => c == t.as_slice(),
                DirFilter::Prefix(t) => c.starts_with(t),
            }
        })
    }
}

/// A literal run of a glob's last component, which every file name it matches holds.
enum NameFilter {
    Any,
    Prefix(Vec<u8>),
    Infix(Box<memchr::memmem::Finder<'static>>),
}

impl NameFilter {
    fn passes(&self, name: &[u8]) -> bool {
        match self {
            Self::Any => true,
            Self::Prefix(p) => name.starts_with(p),
            Self::Infix(f) => f.find(name).is_some(),
        }
    }
}

/// A directory of the path every match has: a literal component of the glob (`.ssh`,
/// `nginx`), or a component's literal start (`ChatExport_`).
enum DirFilter {
    Component(Vec<u8>),
    Prefix(Vec<u8>),
}

struct PathRule {
    definition: usize,
    /// The glob, compiled into `matcher` the first time a path passes the literal filters:
    /// compiling the M6 Definitions' globs took about 3 ms at every start (criterion 51,
    /// measured 06/11/2026), which a search whose paths no filter admits never needs.
    glob: String,
    /// None for an invalid glob, which matches nothing (`FormatDefinition::parse` refuses a
    /// Definition with one).
    matcher: std::sync::OnceLock<Option<GlobMatcher>>,
    /// A glob without a `/`, matched against the file name only.
    basename: bool,
    name: NameFilter,
    /// The directory filter, and a search for its bytes.
    dir: Option<(DirFilter, memchr::memmem::Finder<'static>)>,
}

/// The `paths` globs, sorted once into M6 spec §4.1's signal 4 or 6 and indexed by literals a
/// match needs, so a file no glob can match runs no matcher: the filters only skip rules, the
/// rule's GlobMatcher decides, and the first Definition in the Registry's order wins.
#[derive(Default)]
struct PathIndex {
    rules: Vec<PathRule>,
    /// Signal 4: globs that write the extension, under each lowercase extension they write.
    before_extension: Map<String, Vec<usize>>,
    /// Signal 6, by a literal component a match has (a directory or the whole file name).
    by_component: Map<Vec<u8>, Vec<usize>>,
    /// The `component_bit`s of the keys of `by_component`: a path's component whose bit is
    /// clear is not looked up.
    component_bits: [u64; 4],
    /// Signal 6, by the file name through its first dot after the first byte (`Makefile.`,
    /// `.env.`): the key of every name a literal prefix that holds that dot admits.
    by_name_prefix: Map<Vec<u8>, Vec<usize>>,
    /// Signal 6 globs with no literal to index (`**/*`): asked of every name past the
    /// extension, through their filters (D5).
    fallback: Vec<usize>,
}

impl PathIndex {
    fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// How many rules have compiled their glob.
    #[cfg(test)]
    fn compiled(&self) -> usize {
        self.rules
            .iter()
            .filter(|r| r.matcher.get().is_some())
            .count()
    }

    fn add(&mut self, definition: usize, pattern: &str) {
        let shape = GlobShape::of(pattern);
        let id = self.rules.len();
        self.rules.push(PathRule {
            definition,
            glob: pattern.to_string(),
            matcher: std::sync::OnceLock::new(),
            basename: !pattern.contains('/'),
            name: shape.name_filter(),
            dir: shape.dir_filter().map(|d| {
                let bytes = match &d {
                    DirFilter::Component(t) | DirFilter::Prefix(t) => t.clone(),
                };
                (d, memchr::memmem::Finder::new(&bytes).into_owned())
            }),
        });
        if let Some(extensions) = shape.extensions() {
            for e in extensions {
                let at = self.before_extension.entry(e).or_default();
                if !at.contains(&id) {
                    at.push(id);
                }
            }
            return;
        }
        match shape.token() {
            Some(Token::Component(c)) => {
                let (word, bit) = component_bit(&c);
                self.component_bits[word] |= bit;
                self.by_component.entry(c).or_default().push(id)
            }
            Some(Token::NamePrefix(p)) => self.by_name_prefix.entry(p).or_default().push(id),
            None => self.fallback.push(id),
        }
    }

    /// False when no component of `path` is a key of `by_component`: its bytes split at the
    /// separators hold every component (and `.`, `..` and the empty runs, which are no key), so
    /// a path no key admits costs a pass over its bytes, not a walk of its components
    /// (criterion 51). True asks the walk, which decides.
    fn may_hold_component(&self, path: &Path) -> bool {
        let bytes = path.as_os_str().as_encoded_bytes();
        let mut from = 0;
        for at in separators(bytes).chain(std::iter::once(bytes.len())) {
            if self.is_component_key(&bytes[from..at]) {
                return true;
            }
            from = at + 1;
        }
        false
    }

    fn is_component_key(&self, c: &[u8]) -> bool {
        let (word, bit) = component_bit(c);
        self.component_bits[word] & bit != 0 && self.by_component.contains_key(c)
    }

    /// Signal 4: the first Definition whose glob that writes the file's extension matches.
    fn before_extension(&self, at: &At) -> Option<usize> {
        let key = at.key.as_deref()?;
        let ids = self.before_extension.get(key)?;
        // The glob writes its extension in lower case, and an extension is compared in any
        // case, as signal 5's table compares it: a path whose extension is not written so
        // is matched with it lowered (review r1, Fable #50).
        if let Some(ext) = at.path.extension()
            && ext.as_encoded_bytes() != key.as_bytes()
            && ext.as_encoded_bytes().eq_ignore_ascii_case(key.as_bytes())
        {
            let lowered = at.path.with_extension(key);
            let name = lowered
                .file_name()
                .map(|n| n.as_encoded_bytes())
                .unwrap_or_default();
            let at = At::with_name(&lowered, name, Some(key));
            return self.first(&at, ids.iter().copied());
        }
        self.first(at, ids.iter().copied())
    }

    /// Signal 6: the first Definition whose glob that leaves the extension open matches.
    fn after_extension(&self, at: &At) -> Option<usize> {
        let mut found = None;
        let mut take = |ids: Option<&Vec<usize>>| {
            if let Some(ids) = ids
                && let Some(d) = self.first(at, ids.iter().copied())
            {
                found = Some(found.map_or(d, |f: usize| f.min(d)));
            }
        };
        if !self.by_name_prefix.is_empty()
            && let Some(key) = name_prefix_key(at.name)
        {
            take(self.by_name_prefix.get(key));
        }
        if !self.by_component.is_empty() && self.may_hold_component(at.path) {
            for c in at.path.components() {
                if let std::path::Component::Normal(c) = c {
                    take(self.by_component.get(c.as_encoded_bytes()));
                }
            }
        }
        if !self.fallback.is_empty() {
            take(Some(&self.fallback));
        }
        found
    }

    /// The smallest Definition among `ids` whose rule matches `at`: a rule of a later
    /// Definition than one already matched runs no matcher.
    fn first(&self, at: &At, ids: impl Iterator<Item = usize>) -> Option<usize> {
        let mut best: Option<usize> = None;
        for id in ids {
            let r = &self.rules[id];
            if best.is_some_and(|b| b <= r.definition)
                || !r.name.passes(at.name)
                || r.dir.as_ref().is_some_and(|(d, f)| !at.has_directory(d, f))
            {
                continue;
            }
            let Some(matcher) = r
                .matcher
                .get_or_init(|| path_glob(&r.glob).ok().map(|g| g.compile_matcher()))
            else {
                continue;
            };
            let hit = if r.basename {
                at.path.file_name().is_some_and(|n| matcher.is_match(n))
            } else {
                matcher.is_match(at.path)
            };
            if hit {
                best = Some(r.definition);
            }
        }
        best
    }
}

/// The offsets of the path separators in `bytes`: `/`, and `\\` on Windows.
fn separators(bytes: &[u8]) -> impl Iterator<Item = usize> + '_ {
    #[cfg(windows)]
    return memchr::memchr2_iter(b'/', b'\\', bytes);
    #[cfg(not(windows))]
    return memchr::memchr_iter(b'/', bytes);
}

/// The bit of `PathIndex::component_bits` of component `c`, from its length and its first and
/// last bytes: (word, bit). Most directories of a path share none with the few keys.
fn component_bit(c: &[u8]) -> (usize, u64) {
    let (first, last) = (c.first().copied(), c.last().copied());
    let k =
        (c.len() * 31 + usize::from(first.unwrap_or(0)) * 7 + usize::from(last.unwrap_or(0))) % 256;
    (k / 64, 1 << (k % 64))
}

/// The file name through its first dot after the first byte: `Makefile.` of
/// `Makefile.debug`, `.env.` of `.env.production`.
fn name_prefix_key(name: &[u8]) -> Option<&[u8]> {
    let dot = 1 + memchr::memchr(b'.', name.get(1..)?)?;
    Some(&name[..=dot])
}

/// How a signal-6 glob is found.
enum Token {
    Component(Vec<u8>),
    NamePrefix(Vec<u8>),
}

/// The literal parts of a glob that every path it matches holds.
struct GlobShape<'g> {
    /// The components before the last, empty for a glob without a `/`.
    dirs: Vec<&'g str>,
    last: &'g str,
    /// False when a literal of the glob may not be a literal of the path: an escape, or a
    /// `{…}` or `[…]` group that holds a `/`. Such a glob has no filter.
    plain: bool,
}

impl<'g> GlobShape<'g> {
    fn of(pattern: &'g str) -> Self {
        let plain = !pattern.contains('\\') && !group_holds_slash(pattern);
        let (dirs, last) = match pattern.rsplit_once('/') {
            Some((dirs, last)) => (dirs.split('/').collect(), last),
            None => (Vec::new(), pattern),
        };
        Self { dirs, last, plain }
    }

    /// Signal 4's lowercase extensions when the text after the last `.` of the last component
    /// is a literal extension or a `{…}` list of literal ones, with no `*`, `?` or `[`.
    fn extensions(&self) -> Option<Vec<String>> {
        let (_, after) = self.last.rsplit_once('.')?;
        let literal =
            |e: &str| !e.is_empty() && !e.contains(['*', '?', '[', ']', '{', '}', '\\', '/']);
        if let Some(list) = after.strip_prefix('{').and_then(|a| a.strip_suffix('}')) {
            let all: Vec<&str> = list.split(',').collect();
            return all
                .iter()
                .all(|e| literal(e))
                .then(|| all.iter().map(|e| e.to_ascii_lowercase()).collect());
        }
        literal(after).then(|| vec![after.to_ascii_lowercase()])
    }

    /// The longest literal run of the last component (before a written extension), as a
    /// prefix when it starts the component; the whole component when it is all literal.
    fn name_filter(&self) -> NameFilter {
        if !self.plain {
            return NameFilter::Any;
        }
        if is_literal(self.last) {
            return NameFilter::Prefix(self.last.as_bytes().to_vec());
        }
        let stem = match self.extensions() {
            Some(_) => self.last.rsplit_once('.').map_or(self.last, |(s, _)| s),
            None => self.last,
        };
        match longest_run(stem) {
            Some((0, run)) => NameFilter::Prefix(run.as_bytes().to_vec()),
            Some((_, run)) => NameFilter::Infix(Box::new(
                memchr::memmem::Finder::new(run.as_bytes()).into_owned(),
            )),
            None => NameFilter::Any,
        }
    }

    /// The longest literal directory component, else the longest literal start of one.
    fn dir_filter(&self) -> Option<DirFilter> {
        if !self.plain {
            return None;
        }
        let usable = |c: &&str| !c.is_empty() && *c != "." && *c != ".." && !c.contains(':');
        if let Some(c) = self
            .dirs
            .iter()
            .copied()
            .filter(usable)
            .filter(|c| is_literal(c))
            .max_by_key(|c| c.len())
        {
            return Some(DirFilter::Component(c.as_bytes().to_vec()));
        }
        self.dirs
            .iter()
            .copied()
            .filter(usable)
            .filter_map(|c| longest_run(c).filter(|(at, _)| *at == 0))
            .map(|(_, run)| run)
            .max_by_key(|run| run.len())
            .map(|run| DirFilter::Prefix(run.as_bytes().to_vec()))
    }

    /// The index key of a signal-6 glob: its literal directory, its whole literal file name,
    /// or its file name's literal start through a dot after the first byte.
    fn token(&self) -> Option<Token> {
        if !self.plain {
            return None;
        }
        if let Some(DirFilter::Component(c)) = self.dir_filter() {
            return Some(Token::Component(c));
        }
        if is_literal(self.last) && self.last != "." && self.last != ".." {
            return Some(Token::Component(self.last.as_bytes().to_vec()));
        }
        if self.dirs.is_empty()
            && let NameFilter::Prefix(p) = self.name_filter()
            && let Some(key) = name_prefix_key(&p)
        {
            return Some(Token::NamePrefix(key.to_vec()));
        }
        None
    }
}

/// Whether `s` has no glob syntax.
fn is_literal(s: &str) -> bool {
    !s.is_empty() && !s.contains(['*', '?', '[', ']', '{', '}', '\\'])
}

/// The longest run of literal characters of `component` before its first `[`, `{` or `\`, and
/// where it starts.
fn longest_run(component: &str) -> Option<(usize, &str)> {
    let end = component.find(['[', '{', '\\']).unwrap_or(component.len());
    let mut best: Option<(usize, &str)> = None;
    let mut start = 0;
    for (i, c) in component[..end]
        .char_indices()
        .chain(std::iter::once((end, '*')))
    {
        if c == '*' || c == '?' {
            if i > start && best.is_none_or(|(_, b)| b.len() < i - start) {
                best = Some((start, &component[start..i]));
            }
            start = i + c.len_utf8();
        }
    }
    best
}

/// Whether a `{…}` or `[…]` group of `pattern` holds a `/`.
fn group_holds_slash(pattern: &str) -> bool {
    let mut open: Option<char> = None;
    for c in pattern.chars() {
        match (open, c) {
            (None, '{') => open = Some('}'),
            (None, '[') => open = Some(']'),
            (Some(close), c) if c == close => open = None,
            (Some(_), '/') => return true,
            _ => {}
        }
    }
    false
}

/// The exact names and the extensions (lowercase) the built-in document Formats claim
/// (formats/docx.toml and the other eight; a unit test keeps them equal).
pub const BUILTIN_DOC_NAMES: &[&str] = &[];
pub const BUILTIN_DOC_EXTENSIONS: &[&str] = &[
    "docx", "docm", "dotx", "dotm", "epub", "xlsx", "xlsm", "xltx", "xltm", "pdf", "pptx", "pptm",
    "potx", "potm", "ppsx", "ppsm", "ods", "ots", "odt", "ott", "sqlite", "sqlite3", "db", "db3",
    "s3db", "sl3", "odp", "otp",
];

/// `Detector::may_be_document` with `--docs`, no override and only the built-in Definitions,
/// before the Registry is loaded: a search with no candidate never loads it (code review r7,
/// Codex #3).
pub fn builtin_may_be_document(path: &Path) -> bool {
    let (Some(name), Some(ext)) = (
        path.file_name().and_then(|n| n.to_str()),
        path.extension().and_then(|e| e.to_str()),
    ) else {
        return true;
    };
    BUILTIN_DOC_NAMES.contains(&name)
        || BUILTIN_DOC_EXTENSIONS.contains(&normalized_extension(ext).as_ref())
}

/// `ext` in lowercase, as the Detector keys extensions: borrowed unless it holds an ASCII
/// uppercase letter, so the common lowercase extension costs no copy per file (M5b plan,
/// Task 3.2).
fn normalized_extension(ext: &str) -> std::borrow::Cow<'_, str> {
    if ext.bytes().any(|b| b.is_ascii_uppercase()) {
        std::borrow::Cow::Owned(ext.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Borrowed(ext)
    }
}

/// Whether `file_name` ends with `.` and `compound` (lowercase), in any case, after a
/// nonempty stem: `cart.BLADE.PHP` ends with `blade.php`, `blade.php` and `.blade.php` do not,
/// as their last two extensions are not two.
fn ends_with_extension(file_name: &str, compound: &str) -> bool {
    let (name, c) = (file_name.as_bytes(), compound.as_bytes());
    name.len() > c.len() + 1
        && name[name.len() - c.len() - 1] == b'.'
        && name[name.len() - c.len()..].eq_ignore_ascii_case(c)
}

/// `#!/usr/bin/env bash -e` → "bash"; `#!/bin/sh` → "sh".
fn shebang_interpreter(head: &[u8]) -> Option<&str> {
    let line = head.strip_prefix(b"#!")?;
    let line = &line[..memchr::memchr(b'\n', line).unwrap_or(line.len())];
    let line = std::str::from_utf8(line).ok()?.trim();
    let mut words = line.split_whitespace();
    let first = words.next()?;
    let prog = first.rsplit('/').next()?;
    if prog == "env" {
        words.find(|w| !w.starts_with('-'))
    } else {
        Some(prog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_extensions_borrow_the_existing_bytes() {
        use std::borrow::Cow;
        assert!(matches!(normalized_extension("txt"), Cow::Borrowed("txt")));
        assert!(matches!(normalized_extension(""), Cow::Borrowed("")));
        assert_eq!(normalized_extension("TXT"), "txt");
        assert_eq!(normalized_extension("DocX"), "docx");
    }

    /// Code review r7, Codex #3: before the Registry is loaded, the built-in document Formats'
    /// exact names and extensions tell which names may be documents, as the Detector of the
    /// built-in Definitions does.
    #[test]
    fn the_built_in_document_names_are_the_document_formats() {
        let r = registry();
        let docs = r.all().iter().filter(|d| d.processor == "docs");
        let names: HashSet<String> = docs.clone().flat_map(|d| d.names.clone()).collect();
        let extensions: HashSet<String> = docs
            .flat_map(|d| d.extensions.iter().map(|e| e.to_ascii_lowercase()))
            .collect();
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        assert_eq!(names, set(BUILTIN_DOC_NAMES));
        assert_eq!(extensions, set(BUILTIN_DOC_EXTENSIONS));
        let d = Detector::new(&r, &[]).unwrap().with_docs(true);
        for path in [
            "a.txt",
            "r.pdf",
            "R.PDF",
            "app.db",
            "x.Sqlite3",
            "Makefile",
            "bin/ls",
            ".bashrc",
            "a.b.docx",
            "dir.pdf/a.md",
            "noext/",
        ] {
            let p = Path::new(path);
            assert_eq!(builtin_may_be_document(p), d.may_be_document(p), "{path}");
        }
    }

    #[test]
    fn only_some_names_may_be_documents() {
        let r = registry();
        let d = Detector::new(&r, &[]).unwrap().with_docs(true);
        let p = Path::new;
        for (path, may) in [
            ("a.txt", false),
            ("src/main.rs", false),
            ("r.pdf", true),
            ("R.PDF", true),
            ("app.db", true),
            ("Makefile", true),
            ("bin/ls", true),
        ] {
            assert_eq!(d.may_be_document(p(path)), may, "{path}");
        }
        assert!(!Detector::new(&r, &[]).unwrap().may_be_document(p("r.pdf")));
        let forced = Detector::new(&r, &["*.txt=pdf".into()])
            .unwrap()
            .with_docs(true);
        assert!(forced.may_be_document(p("a.txt")));
    }

    /// `detect`'s name and extension signals give the first active Definition, in the
    /// Registry's order, whose names or extensions (in any case) match, with `--docs` off and on.
    #[test]
    fn names_and_extensions_pick_the_first_active_definition() {
        let r = registry();
        let scan = |d: &Detector, path: &str| {
            let p = Path::new(path);
            let file = p.file_name().and_then(|n| n.to_str()).unwrap();
            let ext = p.extension().and_then(|e| e.to_str());
            r.all()
                .iter()
                .find(|f| d.active(f) && f.names.iter().any(|n| n == file))
                .or_else(|| {
                    ext.and_then(|ext| {
                        r.all().iter().find(|f| {
                            d.active(f) && f.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
                        })
                    })
                })
                .map(|f| f.name.clone())
        };
        let paths = [
            "Makefile",
            "a.md",
            "README.MD",
            "x.Rs",
            "r.pdf",
            "R.PDF",
            "t.docx",
            "s.XLSX",
            "app.db",
            "b.epub",
            "c.odt",
            "d.json",
            "e.unknown",
            "f.HtMl",
        ];
        for docs in [false, true] {
            let d = Detector::new(&r, &[]).unwrap().with_docs(docs);
            for path in paths {
                let got = d.detect(Path::new(path), b"").map(|f| f.name.clone());
                if path.contains('.') || scan(&d, path).is_some() {
                    assert_eq!(got, scan(&d, path), "{path} with --docs {docs}");
                }
            }
        }
    }

    /// Code review r6, Fable #1: `detect_by_path` of a name without an extension answers
    /// without walking every Definition, and as `detect` with an empty head would; and
    /// `document_magic` is false only for first bytes no document Format's `detect` finds.
    #[test]
    fn names_without_an_extension_are_decided_without_walking_every_definition() {
        let r = registry();
        let p = Path::new;
        for formats in [
            &[][..],
            &["*.txt=pdf".to_string()],
            &["markdown".to_string()],
        ] {
            for docs in [true, false] {
                let d = Detector::new(&r, formats).unwrap().with_docs(docs);
                for path in [
                    "Makefile", "LICENSE", ".bashrc", "bin/ls", "a.txt", "r.pdf", "x.",
                ] {
                    let slow = match d.detect(p(path), &[]) {
                        Some(def) => Some(Some(def.name.clone())),
                        None if p(path).extension().is_some() => Some(None),
                        None => None,
                    };
                    let fast = d
                        .detect_by_path(p(path))
                        .map(|o| o.map(|def| def.name.clone()));
                    assert_eq!(fast, slow, "{formats:?} {docs} {path}");
                }
            }
        }
        let d = Detector::new(&r, &[]).unwrap().with_docs(true);
        for (head, doc) in [
            (&b"%PDF-1.7\n"[..], true),
            (b"SQLite format 3\0", true),
            (b"\x7fELF\x02\x01", false),
            (b"#!/bin/sh\n", false),
            (b"plain text\n", false),
            (b"", false),
        ] {
            assert_eq!(d.document_magic(head), doc, "{head:?}");
            let found = d.detect(p("noext"), head);
            assert_eq!(
                found.is_some_and(|f| f.processor == "docs"),
                doc,
                "{head:?}"
            );
        }
        let forced = Detector::new(&r, &["pdf".into()]).unwrap().with_docs(true);
        assert!(forced.document_magic(b"plain text\n"));
    }

    /// M6 plan, Task 1.2: the literals that index a path glob only skip rules that cannot
    /// match, so the index answers what the glob alone answers, for every glob shape.
    #[test]
    fn the_path_index_answers_what_the_glob_answers() {
        let patterns = [
            "**/ChatExport_*/result.json",
            "**/ChatExport_*/messages*.html",
            "*WhatsApp*.txt",
            "**/nginx/**/*.conf",
            "**/nginx/sites-enabled/*",
            "**/.ssh/config",
            "**/.ssh/config.d/*",
            "**/systemd/**/*.{path,target,conf}",
            "**/.git/config",
            ".env.*",
            "Makefile.*",
            "Dockerfile.*",
            "*.conf",
            "*.env",
            "**/*",
            "*",
            "config",
            "foo*bar.txt",
            "?akefile.*",
            "[ab]*.txt",
            "{a,b}/c.txt",
            "{x/y,z}/*.md",
            "a[/]b/*.md",
            "\\*.txt",
            "**/x.d/*.{a,B}",
            "src/**/mod.rs",
            "**/ChatExport_*/*",
        ];
        let paths = [
            "x/ChatExport_2024/result.json",
            "ChatExport_1/result.json",
            "x/ChatExport_1/messages/y.html",
            "x/ChatExport_1/messages2.html",
            "x/ChatExport_1/a/result.json",
            "x/result.json",
            "x/WhatsApp Chat with Ana.txt",
            "x/WhatsApp/notes.txt",
            "/etc/nginx/a/b.conf",
            "/etc/nginx.conf",
            "/etc/xnginx/b.conf",
            "/etc/nginx/sites-enabled/default",
            "/etc/nginx/sites-enabled/a/b",
            "/home/x/.ssh/config",
            "/home/x/.ssh/config.d/local",
            "/home/x/.ssh/configure",
            "/etc/systemd/system/demo.path",
            "/etc/systemd/system/demo.PATH",
            "/etc/systemd/a.conf",
            "repo/.git/config",
            ".env",
            ".env.production",
            "x/.env.local",
            ".conf",
            "..env",
            "Makefile.debug",
            "Makefile",
            "makefile.x",
            "Dockerfile.dev",
            "config",
            "a/config",
            "foo-and-bar.txt",
            "a.txt",
            "b.txt",
            "a/c.txt",
            "z/q.md",
            "x/y/q.md",
            "*.txt",
            "a/x.d/f.a",
            "a/x.d/f.B",
            "a/x.d/f.b",
            "src/a/b/mod.rs",
            "./src/mod.rs",
            "naïve/ChatExport_é/result.json",
            "x.tar.gz",
            "noext",
        ];
        for pattern in patterns {
            let glob = path_glob(pattern).unwrap();
            let matcher = glob.compile_matcher();
            let mut index = PathIndex::default();
            index.add(0, pattern);
            let written = GlobShape::of(pattern).extensions().unwrap_or_default();
            for path in paths {
                // A glob that writes the extension matches it in any case (review r1,
                // Fable #50): the glob is asked about the path with that extension lowered.
                let lowered = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .and_then(|e| {
                        let low = e.to_ascii_lowercase();
                        (low != e && written.contains(&low))
                            .then(|| Path::new(path).with_extension(low))
                    });
                let p = lowered.as_deref().unwrap_or(Path::new(path));
                let want = if pattern.contains('/') {
                    matcher.is_match(p)
                } else {
                    p.file_name().is_some_and(|n| matcher.is_match(n))
                };
                let at = At::new(Path::new(path));
                let got = index.before_extension(&at).or(index.after_extension(&at));
                assert_eq!(got.is_some(), want, "{pattern} on {path}");
            }
        }
    }

    #[test]
    fn globs_sort_into_signal_4_or_6() {
        let ext = |p: &str| GlobShape::of(p).extensions();
        let v = |e: &[&str]| Some(e.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(ext("**/ChatExport_*/result.json"), v(&["json"]));
        assert_eq!(ext("**/ChatExport_*/messages*.html"), v(&["html"]));
        assert_eq!(ext("*WhatsApp*.TXT"), v(&["txt"]));
        assert_eq!(ext("**/systemd/**/*.{path,Target}"), v(&["path", "target"]));
        for six in [
            "**/.ssh/config",
            "**/nginx/sites-enabled/*",
            "Dockerfile.*",
            ".env.*",
            "Makefile.*",
            "x.[ch]",
            "x.{a,*}",
            "x.",
        ] {
            assert_eq!(ext(six), None, "{six}");
        }
    }

    fn known(p: &str) -> bool {
        crate::processors::Processors::known(p)
    }

    /// The built-ins plus `bashy` (shebang), `myfy` (magic) and `makey` (exact name). `myfy`'s
    /// magic is not ELF's, which the built-in `elf` claims since M4, and `makey`'s name is no
    /// built-in's: `Makefile` is M6's `make` (M6 plan, Task 0).
    fn registry() -> Registry {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("bashy.toml"),
            "name='bashy'\nshebangs=['fish']\nprocessor='markdown'",
        )
        .unwrap();
        std::fs::write(
            d.path().join("myfy.toml"),
            "name='myfy'\nmagic=[{hex='7f4d5946'}]\nprocessor='markdown'",
        )
        .unwrap();
        std::fs::write(
            d.path().join("makey.toml"),
            "name='makey'\nnames=['Makey']\nprocessor='markdown'",
        )
        .unwrap();
        let (reg, warnings) = Registry::load(Some(d.path()), &known);
        assert!(warnings.is_empty(), "{warnings:?}");
        reg
    }

    #[test]
    fn signals_in_order() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        assert_eq!(
            det.detect(Path::new("a/README.MD"), b"").unwrap().name,
            "markdown"
        );
        assert_eq!(det.detect(Path::new("Makey"), b"").unwrap().name, "makey");
        assert_eq!(
            det.detect(Path::new("prog"), b"\x7fMYF\x02").unwrap().name,
            "myfy"
        );
        assert_eq!(
            det.detect(Path::new("run"), b"#!/usr/bin/env -S fish -e\n")
                .unwrap()
                .name,
            "bashy"
        );
        assert!(det.detect(Path::new("data.bin"), b"\x00\x01").is_none());
    }

    #[test]
    fn the_path_alone_decides_unless_a_format_reads_the_content() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        let name = |p: &str| det.detect_by_path(Path::new(p)).map(|d| d.map(|d| &d.name));
        assert_eq!(name("a.md"), Some(Some(&"markdown".to_string())));
        assert_eq!(name("Makey"), Some(Some(&"makey".to_string())));
        // myfy (magic) and bashy (shebang) need the first bytes of a name without extension.
        assert_eq!(name("data"), None);
        let (builtin, _) = Registry::load(None, &known);
        let det = Detector::new(&builtin, &[]).unwrap();
        assert_eq!(det.detect_by_path(Path::new("data.txt")), Some(None));
        let det = Detector::new(&builtin, &["markdown".into()]).unwrap();
        assert!(det.detect_by_path(Path::new("data.txt")).unwrap().is_some());
    }

    #[test]
    fn content_signals_need_a_file_without_extension() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        // A shebang or magic bytes decide only when the name has no extension (M2 plan, D15).
        assert_eq!(
            det.detect(Path::new("run"), b"#!/usr/bin/env fish\n")
                .unwrap()
                .name,
            "bashy"
        );
        assert!(
            det.detect(Path::new("run.txt"), b"#!/usr/bin/env fish\n")
                .is_none()
        );
        assert!(det.detect(Path::new("prog.bin"), b"\x7fMYF\x02").is_none());
        assert_eq!(
            det.detect(Path::new(".bashrc_like"), b"\x7fMYF")
                .unwrap()
                .name,
            "myfy"
        );
        // So a file with an unclaimed extension is Plain from its path alone.
        assert_eq!(det.detect_by_path(Path::new("data.txt")), Some(None));
        assert_eq!(det.detect_by_path(Path::new("data")), None);
    }

    #[test]
    fn earlier_signals_beat_later_ones() {
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        // Exact name beats extension, extension beats magic, magic beats shebang.
        assert_eq!(
            det.detect(Path::new("Makey"), b"\x7fMYF").unwrap().name,
            "makey"
        );
        assert_eq!(
            det.detect(Path::new("x.md"), b"\x7fMYF").unwrap().name,
            "markdown"
        );
        assert_eq!(
            det.detect(Path::new("x.md"), b"#!/bin/bash\n")
                .unwrap()
                .name,
            "markdown"
        );
    }

    #[test]
    fn overrides_win_and_unknown_is_an_error() {
        let reg = registry();
        let det = Detector::new(&reg, &["*.txt=markdown".into()]).unwrap();
        assert_eq!(
            det.detect(Path::new("notes.txt"), b"").unwrap().name,
            "markdown"
        );
        assert!(det.detect(Path::new("notes.log"), b"").is_none());
        assert!(Detector::new(&reg, &["nope".into()]).is_err());
        assert!(Detector::new(&reg, &["*.txt=nope".into()]).is_err());
    }

    #[test]
    fn a_bare_name_applies_to_every_file() {
        let reg = registry();
        let det = Detector::new(&reg, &["makey".into()]).unwrap();
        assert_eq!(det.detect(Path::new("x.md"), b"").unwrap().name, "makey");
        assert_eq!(
            det.detect(Path::new("data.bin"), b"\x7fMYF").unwrap().name,
            "makey"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_non_utf8_file_name_still_reaches_magic_and_shebang() {
        use std::os::unix::ffi::OsStrExt;
        let reg = registry();
        let det = Detector::new(&reg, &[]).unwrap();
        let name = std::ffi::OsStr::from_bytes(b"\xff\xfe");
        assert_eq!(
            det.detect(Path::new(name), b"\x7fMYF").unwrap().name,
            "myfy"
        );
        assert_eq!(
            det.detect(Path::new(name), b"#!/usr/bin/fish\n")
                .unwrap()
                .name,
            "bashy"
        );
    }

    #[test]
    fn shebang_interpreters() {
        assert_eq!(shebang_interpreter(b"#!/bin/sh\necho"), Some("sh"));
        assert_eq!(
            shebang_interpreter(b"#!/usr/bin/env bash -e\n"),
            Some("bash")
        );
        assert_eq!(shebang_interpreter(b"#! /usr/bin/python3"), Some("python3"));
        assert_eq!(shebang_interpreter(b"#!/usr/bin/env\n"), None);
        assert_eq!(shebang_interpreter(b"# not a shebang"), None);
        assert_eq!(shebang_interpreter(b""), None);
    }

    /// M6 spec §4.2: `cafebabe` starts a fat Mach-O and a Java class, whose Definitions are
    /// both candidates, in the Registry's order; `object` stays the first of them. Each binary
    /// magic of group E names its Format.
    #[test]
    fn object_candidates_follow_the_registry_order() {
        let (builtin, _) = Registry::load(None, &known);
        let det = Detector::new(&builtin, &[]).unwrap();
        let names = |head: &[u8]| {
            det.object_candidates(head)
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(b"\xca\xfe\xba\xbe\0\0\0\x3d"),
            ["macho", "java-class"]
        );
        assert_eq!(det.object(b"\xca\xfe\xba\xbe").unwrap().name, "macho");
        for (head, name) in [
            (&b"\xff\xd8\xff\xe1"[..], "jpeg"),
            (b"\x89PNG\r\n\x1a\n", "png"),
            (b"RIFF\0\0\0\0WEBPVP8 ", "webp"),
            (b"II*\0", "tiff"),
            (b"MM\0*", "tiff"),
            (b"ID3\x04", "id3"),
            (b"\0\0\0\x20ftypM4A ", "mp4"),
            (b"\0asm\x01\0\0\0", "wasm"),
        ] {
            assert_eq!(names(head), [name], "{head:?}");
        }
        assert!(names(b"plain text").is_empty());
    }

    /// Criterion 51: building the Detector compiles no glob of `paths` (about 3 ms a time for
    /// the M6 Definitions, twice per run, measured 06/11/2026); a glob compiles the first time a
    /// path passes its rule's literal filters, so a run whose paths no filter admits compiles
    /// none, and the same glob answers as before.
    #[test]
    fn path_globs_compile_only_when_a_path_reaches_them() {
        let (builtin, _) = Registry::load(None, &known);
        let det = Detector::new(&builtin, &[]).unwrap().with_docs(false);
        assert!(!det.paths.is_empty());
        assert_eq!(det.paths.compiled(), 0);
        for p in [
            "a/file-1.json",
            "a/notes.txt",
            "a/b/c.unclaimed",
            "a/x.conf",
        ] {
            assert!(det.detect_by_path(Path::new(p)).is_some(), "{p}");
        }
        assert_eq!(det.paths.compiled(), 0);
        let nginx = det.detect(Path::new("/etc/nginx/sites-enabled/default"), b"");
        assert_eq!(nginx.unwrap().name, "nginx");
        assert_eq!(det.paths.compiled(), 1);
    }
}
