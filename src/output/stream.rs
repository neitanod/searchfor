//! Files printed row by row as the searcher reports them (code review r1, Fable #1): Plain
//! files, and files whose Processor builds an Outline, without structural flags. No model of the
//! file is kept: each Trail is computed as its row is printed, and only the context lines that
//! open a group wait for the Match after them. Prints exactly what the printers print from a
//! Described file.
use super::json::{JsonFile, JsonOptions, RunStats};
use super::{Layout, LineText, RenderOptions, Row, flat, tree};
use crate::matcher::PatternMatcher;
use crate::model::{FileStats, Line, Segment, Submatch};
use crate::search::{MaxCount, context_submatches, match_submatches};
use crate::trail::{Outline, TrailCursor};
use grep_searcher::{Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;
use termcolor::WriteColor;

/// What a file is printed from, besides the searcher's lines.
pub struct StreamFile<'a> {
    pub path: &'a Path,
    pub explicit: bool,
    /// The Format's name (JSON's `format`), None for Plain.
    pub format: Option<&'a str>,
    /// The bytes the search runs over: needed with an Outline (the context lines that wait for
    /// a Match are kept as ranges of it).
    pub src: Option<&'a [u8]>,
    pub outline: Option<&'a Outline>,
    /// `-m`, counted here (see `MaxCount`).
    pub max_count: Option<u64>,
    /// The flat layout's separator from the file before, written before the first row (see
    /// `flat::render`).
    pub lead: Option<&'a [u8]>,
}

/// A row of a Match line: its Trail and the submatches under it.
type RowGroup = (Rc<[Segment]>, Vec<(usize, usize)>);

/// A line waiting for the Trail of the next Match of its group.
struct Pending {
    line: Line,
    gap_before: bool,
}

pub struct StreamSink<'a, W: WriteColor> {
    w: &'a mut W,
    matcher: &'a PatternMatcher,
    invert: bool,
    o: &'a RenderOptions,
    file: StreamFile<'a>,
    trails: Option<TrailCursor<'a>>,
    head: flat::FileHead,
    json: Option<JsonFile<'a>>,
    tree: tree::TreeState,
    max_count: MaxCount,
    /// Line number of the last row, for the `--` between groups.
    prev: Option<u64>,
    /// The tree's path line is written.
    header: bool,
    /// Binary data was found: rows after it are not printed (design note 3).
    binary: bool,
    /// No colour, no -M and no Outline: a short valid line can be written without the
    /// printers' machinery.
    fast: bool,
    /// The path and its separator for the flat layout (empty without file names), and the
    /// width of that path in characters.
    path_prefix: (Vec<u8>, usize),
    scratch: Vec<u8>,
    /// Trail of the last row of the current group, once it has a Match.
    group: Option<Rc<[Segment]>>,
    pending: Vec<Pending>,
    /// The last Trail computed, by its Segments' indices, to share it with the next rows.
    last: (Vec<usize>, Rc<[Segment]>),
    indices: Vec<usize>,
    started: Instant,
    pub stats: FileStats,
    pub has_match: bool,
}

impl<'a, W: WriteColor> StreamSink<'a, W> {
    pub fn new(
        w: &'a mut W,
        matcher: &'a PatternMatcher,
        invert: bool,
        o: &'a RenderOptions,
        jo: &'a JsonOptions,
        file: StreamFile<'a>,
    ) -> Self {
        let fast = !w.supports_color()
            && o.max_columns.is_none()
            && file.outline.is_none()
            && o.look.plain();
        Self {
            w,
            matcher,
            invert,
            o,
            trails: file.outline.map(TrailCursor::new),
            head: flat::FileHead::new(file.path, o),
            json: (o.layout == Layout::Json).then(|| JsonFile::new(file.path, file.format, jo)),
            tree: tree::TreeState::default(),
            max_count: MaxCount::new(file.max_count),
            prev: None,
            header: false,
            binary: false,
            fast,
            path_prefix: if o.with_filename && o.layout == Layout::Flat {
                let p = super::path_bytes(file.path);
                let width = String::from_utf8_lossy(&p).chars().count();
                (p, width)
            } else {
                (Vec::new(), 0)
            },
            scratch: Vec::new(),
            group: None,
            pending: Vec::new(),
            last: (Vec::new(), Rc::from(Vec::new())),
            indices: Vec::new(),
            started: Instant::now(),
            stats: FileStats::default(),
            has_match: false,
            file,
        }
    }

    /// The Trail of a Match at byte `at`, shared with the rows before it when it is the same.
    fn trail_at(&mut self, at: usize) -> Rc<[Segment]> {
        let Some(c) = self.trails.as_mut() else {
            return self.last.1.clone();
        };
        c.indices(at, &mut self.indices);
        if self.indices != self.last.0 {
            self.last = (self.indices.clone(), Rc::from(c.segments(&self.indices)));
        }
        self.last.1.clone()
    }

    /// Notes the line's place: true when it opens a new group after a gap, which ends the
    /// group before (its waiting context lines keep an empty Trail).
    fn step(&mut self, line_number: u64) -> std::io::Result<bool> {
        let gap = self.prev.is_some_and(|p| p + 1 != line_number);
        self.prev = Some(line_number);
        if gap {
            self.flush(None)?;
            self.group = None;
        }
        Ok(gap)
    }

    /// Prints the waiting context lines under `trail` (empty when None).
    fn flush(&mut self, trail: Option<&Rc<[Segment]>>) -> std::io::Result<()> {
        let empty: Rc<[Segment]> = Rc::from(Vec::new());
        let trail = trail.unwrap_or(&empty).clone();
        for p in std::mem::take(&mut self.pending) {
            let src = self.file.src.unwrap_or_default();
            let subs: Vec<(usize, usize)> =
                p.line.submatches.iter().map(|s| (s.start, s.end)).collect();
            let row = Row {
                offset: None,
                trail: &trail,
                line_number: p.line.line_number,
                line: Rc::new(LineText::new(p.line.bytes(src))),
                subs,
                is_match: false,
                gap_before: p.gap_before,
                column: None,
                byte_offset: p.line.absolute_offset,
            };
            self.row(&row)?;
        }
        Ok(())
    }

    /// The file's `lead`, before its first row.
    fn lead(&mut self) -> std::io::Result<()> {
        if let Some(l) = self.file.lead.take() {
            self.w.write_all(l)?;
            self.w.write_all(b"\n")?;
        }
        Ok(())
    }

    /// One row in the tree or flat layout.
    fn row(&mut self, r: &Row) -> std::io::Result<()> {
        if self.o.layout == Layout::Flat {
            self.lead()?;
            return flat::write_row(self.w, &self.head, r, self.o);
        }
        if !self.header {
            self.header = true;
            self.tree.header(self.w, self.file.path, self.o)?;
        }
        self.tree.row(self.w, r, self.o)
    }

    fn matched_line(
        &mut self,
        line: Line,
        bytes: &[u8],
        subs: &dyn Fn() -> Vec<Submatch>,
    ) -> std::io::Result<()> {
        let gap = if self.json.is_none() && !self.binary {
            self.step(line.line_number)?
        } else {
            false
        };
        let outlined = self.trails.is_some();
        if self.json.is_none()
            && !self.binary
            && self.fast
            && self.write_fast(line.line_number, bytes, true, gap)?
        {
            return Ok(());
        }
        let line = Line {
            submatches: subs(),
            ..line
        };
        // One Trail per submatch, or for the line start of an inverted Match.
        let base = line.absolute_offset as usize;
        let trails: Vec<Rc<[Segment]>> = if outlined {
            let mut trails = Vec::with_capacity(line.submatches.len().max(1));
            if line.submatches.is_empty() {
                trails.push(self.trail_at(base));
            }
            for s in &line.submatches {
                trails.push(self.trail_at(base + s.start));
            }
            trails
        } else {
            vec![self.last.1.clone(); line.submatches.len().max(1)]
        };
        if let Some(j) = self.json.as_mut() {
            self.stats.matches += line.submatches.len() as u64;
            let refs: Vec<&[Segment]> = trails.iter().map(|t| &t[..]).collect();
            return j.matched(self.w, &line, bytes, &refs, &[]);
        }
        if self.binary {
            return Ok(());
        }
        // The rows of the line, as `output::match_rows`: one per distinct Trail, or one under
        // the first Trail when -M omits the line.
        let groups: Vec<RowGroup> = if line.submatches.is_empty() || super::omitted(bytes, self.o) {
            let subs = line.submatches.iter().map(|s| (s.start, s.end)).collect();
            vec![(trails[0].clone(), subs)]
        } else {
            super::group_by_trail(
                trails
                    .into_iter()
                    .zip(line.submatches.iter().map(|s| (s.start, s.end))),
            )
        };
        self.flush(Some(&groups[0].0))?;
        let gap_before = gap && self.o.context;
        // One text for every row of the line, which outlives none of the Trails it is under.
        let text = Rc::new(LineText::new(bytes));
        for (n, (trail, subs)) in groups.iter().enumerate() {
            let row = Row {
                offset: None,
                trail,
                line_number: line.line_number,
                line: text.clone(),
                column: subs.first().map(|s| s.0 as u64 + 1),
                subs: subs.clone(),
                is_match: true,
                gap_before: gap_before && n == 0,
                byte_offset: line.absolute_offset,
            };
            self.row(&row)?;
            self.group = Some(trail.clone());
        }
        Ok(())
    }

    fn context_line(
        &mut self,
        line: Line,
        bytes: &[u8],
        subs: &dyn Fn() -> Vec<Submatch>,
    ) -> std::io::Result<()> {
        if let Some(j) = self.json.as_mut() {
            let line = Line {
                submatches: subs(),
                ..line
            };
            return j.context(self.w, &line, bytes);
        }
        if self.binary {
            return Ok(());
        }
        let gap = self.step(line.line_number)?;
        if self.fast && self.write_fast(line.line_number, bytes, false, gap)? {
            return Ok(());
        }
        let line = Line {
            submatches: subs(),
            ..line
        };
        let gap_before = gap && self.o.context;
        match self.group.clone() {
            // The first lines of a group wait for the Trail of its first Match.
            None if self.trails.is_some() => {
                self.pending.push(Pending { line, gap_before });
                Ok(())
            }
            group => {
                let trail = group.unwrap_or_else(|| self.last.1.clone());
                let subs = line.submatches.iter().map(|s| (s.start, s.end)).collect();
                let row = Row {
                    offset: None,
                    trail: &trail,
                    line_number: line.line_number,
                    line: Rc::new(LineText::new(bytes)),
                    subs,
                    is_match: false,
                    gap_before,
                    column: None,
                    byte_offset: line.absolute_offset,
                };
                self.row(&row)
            }
        }
    }

    /// What `flat::write_row` and `tree::write_row` print for a row without a Trail, when there
    /// is no colour and no -M and the line is valid UTF-8 that fits: the prefix and the line.
    /// Returns false, writing nothing, when the line needs the printers (Fragments, U+FFFD).
    fn write_fast(
        &mut self,
        line_number: u64,
        bytes: &[u8],
        is_match: bool,
        gap: bool,
    ) -> std::io::Result<bool> {
        let raw = crate::text::strip_terminator(bytes);
        let sep = if is_match { b':' } else { b'-' };
        let p = &mut self.scratch;
        p.clear();
        let tree = self.o.layout == Layout::Tree;
        if tree {
            p.extend_from_slice(b"  ");
        } else if !self.path_prefix.0.is_empty() {
            p.extend_from_slice(&self.path_prefix.0);
            p.push(sep);
        }
        if self.o.line_numbers {
            let mut digits = [0u8; 20];
            let mut n = line_number;
            let mut at = digits.len();
            loop {
                at -= 1;
                digits[at] = b'0' + (n % 10) as u8;
                n /= 10;
                if n == 0 {
                    break;
                }
            }
            p.extend_from_slice(&digits[at..]);
            p.push(sep);
        }
        // Flat: a Match row puts a space between its head and its text.
        if !tree && is_match && !p.is_empty() {
            p.push(b' ');
        }
        // Characters before the text: the path counts in characters, the rest is ASCII.
        let used = p.len() - self.path_prefix.0.len() + self.path_prefix.1;
        let avail = self.o.width.saturating_sub(used);
        if raw.len() > avail || (!raw.is_ascii() && std::str::from_utf8(raw).is_err()) {
            return Ok(false);
        }
        if !self.header && tree {
            self.header = true;
            self.tree.header(self.w, self.file.path, self.o)?;
        }
        self.lead()?;
        if gap && self.o.context {
            self.w.write_all(if tree { b"  --\n" } else { b"--\n" })?;
        }
        self.w.write_all(&self.scratch)?;
        self.w.write_all(raw)?;
        self.w.write_all(b"\n")?;
        Ok(true)
    }

    /// After the search: the waiting context lines, the tree's path line if no row wrote it,
    /// ripgrep's binary message, or JSON's `end`. Returns the file's JSON stats, for the
    /// summary. Writes nothing for a file without a Match.
    pub fn close(mut self) -> std::io::Result<Option<RunStats>> {
        if !self.has_match {
            return Ok(None);
        }
        if let Some(j) = self.json.as_mut() {
            return j.end(self.w, &self.stats).map(Some);
        }
        self.flush(None)?;
        if self.o.layout == Layout::Tree && !self.header {
            self.tree.header(self.w, self.file.path, self.o)?;
        }
        let msg = super::binary_message(self.stats.binary_offset, true, self.file.explicit, self.o);
        if let Some(msg) = msg {
            match self.o.layout {
                Layout::Tree => tree::write_binary_message(self.w, &msg)?,
                _ => flat::write_binary_message(self.w, self.file.path, &msg, self.o)?,
            }
        }
        Ok(None)
    }
}

impl<W: WriteColor> Sink for StreamSink<'_, W> {
    type Error = std::io::Error;

    fn begin(&mut self, s: &Searcher) -> Result<bool, Self::Error> {
        self.max_count.begin(s);
        self.started = Instant::now();
        Ok(true)
    }

    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.max_count.matched();
        self.stats.matched_lines += m.lines().count() as u64;
        self.has_match = true;
        let (matcher, invert) = (self.matcher, self.invert);
        let subs = || match_submatches(matcher, invert, m);
        let line = Line {
            line_number: m.line_number().unwrap_or(0),
            absolute_offset: m.absolute_byte_offset(),
            len: m.bytes().len(),
            owned: None,
            submatches: Vec::new(),
        };
        self.matched_line(line, m.bytes(), &subs)?;
        Ok(self.max_count.go_on())
    }

    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
        self.max_count.context(c);
        let (matcher, invert) = (self.matcher, self.invert);
        let subs = || context_submatches(matcher, invert, c.bytes());
        let line = Line {
            line_number: c.line_number().unwrap_or(0),
            absolute_offset: c.absolute_byte_offset(),
            len: c.bytes().len(),
            owned: None,
            submatches: Vec::new(),
        };
        self.context_line(line, c.bytes(), &subs)?;
        Ok(self.max_count.go_on())
    }

    fn binary_data(&mut self, _s: &Searcher, _offset: u64) -> Result<bool, Self::Error> {
        self.binary = true;
        Ok(true)
    }

    fn finish(&mut self, _s: &Searcher, f: &SinkFinish) -> Result<(), Self::Error> {
        self.stats.bytes_searched = f.byte_count();
        self.stats.binary_offset = f.binary_byte_offset();
        self.stats.elapsed = self.started.elapsed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse;
    use crate::model::{Described, FileMatches, Input};
    use crate::output::json::render_with_stats;
    use crate::processors::{Processor, markdown::Markdown};
    use crate::search::{matcher, search_source, searcher};
    use crate::source::Source;
    use termcolor::Buffer;

    /// The same file printed by `StreamSink` and by the printers from a Described model; with
    /// `markdown`, Trails come from the Markdown Processor. JSON elapsed times are masked.
    fn both(
        argv: &[&str],
        layout: Layout,
        color: bool,
        named: bool,
        content: &[u8],
        markdown: bool,
    ) -> (String, String) {
        let mut v = vec!["s4"];
        v.extend_from_slice(argv);
        let a = parse(v.iter().map(Into::into).collect()).unwrap();
        let explicit = !argv.contains(&"walked");
        let m = matcher(&a, &a.patterns().unwrap().patterns).unwrap();
        let ro = RenderOptions {
            layout,
            line_numbers: !a.no_line_number,
            with_filename: named,
            width: 40,
            max_columns: a.max_columns,
            context: a.before() > 0 || a.after() > 0,
            expand: false,
            binary: a.binary_choice,
            look: crate::output::Look::default(),
        };
        let jo = JsonOptions {
            line_numbers: ro.line_numbers,
            width: ro.width,
            expand: false,
        };
        let path = Path::new("docs/a.md");
        let src = Source::from_bytes(path.into(), content.to_vec(), explicit);
        let input = Input {
            bytes: src.bytes(),
            path,
            format: "markdown",
            disk: None,
        };
        let outline = markdown.then(|| {
            Markdown
                .outline_of(&input, &toml::Table::new())
                .unwrap()
                .unwrap()
        });
        let format = markdown.then_some("markdown");
        let new_buf = || {
            if color {
                Buffer::ansi()
            } else {
                Buffer::no_color()
            }
        };
        // The separator from a file before, as with one thread: both write it before the first
        // row, and not before a binary message alone.
        let lead = (layout == Layout::Flat).then_some(&b"--"[..]);
        let mut streamed = new_buf();
        let mut s = searcher(&a, explicit, true);
        let file = StreamFile {
            path,
            explicit,
            format,
            src: Some(src.bytes()),
            outline: outline.as_ref(),
            max_count: crate::search::max_count(&a),
            lead,
        };
        let mut sink = StreamSink::new(&mut streamed, &m, a.invert_match, &ro, &jo, file);
        s.search_reader(&m, std::io::Cursor::new(src.bytes()), &mut sink)
            .unwrap();
        sink.close().unwrap();
        let mut modelled = new_buf();
        if let Some(fm) =
            search_source(&mut s, &m, &src, false, false, crate::search::max_count(&a)).unwrap()
        {
            let trails = if markdown {
                let anchors: Vec<Vec<usize>> = fm.matches().map(|m| m.anchors()).collect();
                let all: Vec<usize> = anchors.iter().flatten().copied().collect();
                let mut it = Markdown
                    .trails(&input, &toml::Table::new(), &all)
                    .unwrap()
                    .into_iter();
                anchors
                    .iter()
                    .map(|a| it.by_ref().take(a.len()).collect())
                    .collect()
            } else {
                fm.matches()
                    .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
                    .collect()
            };
            let d = Described {
                file: FileMatches {
                    path: path.into(),
                    ..fm
                },
                format: format.map(Into::into),
                trails,
                sections: Vec::new(),
                offsets: false,
                replacements: Vec::new(),
            };
            match layout {
                Layout::Flat => {
                    super::super::flat::render(&d, src.bytes(), &ro, lead, &mut modelled).unwrap()
                }
                Layout::Tree => {
                    super::super::tree::render(&d, src.bytes(), &ro, &mut modelled).unwrap()
                }
                Layout::Json => {
                    let mut out = Vec::new();
                    render_with_stats(&d, src.bytes(), &jo, &mut out).unwrap();
                    std::io::Write::write_all(&mut modelled, &out).unwrap();
                }
            }
        }
        let text = |b: Buffer| {
            let s = String::from_utf8(b.into_inner()).unwrap();
            regex_lite::Regex::new(r#""elapsed":\{[^}]*\}"#)
                .unwrap()
                .replace_all(&s, "")
                .into_owned()
        };
        (text(streamed), text(modelled))
    }

    fn check(cases: &[(Vec<&str>, &[u8])], markdown: bool) {
        for (argv, content) in cases {
            let mut argv = argv.clone();
            if argv[0] == "walked" {
                argv.remove(0);
                argv.push("walked");
            }
            for layout in [Layout::Flat, Layout::Tree, Layout::Json] {
                for color in [false, true] {
                    for named in [true, false] {
                        let (s, m) = both(&argv, layout, color, named, content, markdown);
                        assert!(!m.is_empty(), "{argv:?} printed nothing");
                        assert_eq!(s, m, "{argv:?} {layout:?} color={color} named={named}");
                    }
                }
            }
        }
    }

    #[test]
    fn plain_rows_written_as_they_come_equal_the_printers() {
        let long = format!("{}needle{}", "x".repeat(60), "y".repeat(60));
        let mut content =
            format!("one needle\ntwo\nthree\nfour\nfive needle\n{long}\nsix\nneedle caf\u{e9}\n")
                .into_bytes();
        content.extend_from_slice(b"bad \xff needle\nseven\nlast needle");
        let mut binary = b"needle first\nmore\n".to_vec();
        binary.extend_from_slice(b"x\0binary needle\nneedle again\n");
        // A walked file stops at binary data: put it past the first buffer (64 KiB).
        let mut late = b"needle first\n".to_vec();
        late.extend(std::iter::repeat_n(&b"padding line\n"[..], 6000).flatten());
        late.extend_from_slice(b"x\0binary needle\n");
        let cases: Vec<(Vec<&str>, &[u8])> = vec![
            (vec!["needle"], &content),
            (vec!["-C1", "needle"], &content),
            (vec!["-A1", "-N", "needle"], &content),
            (vec!["-v", "needle"], &content),
            (vec!["-v", "-C1", "needle"], &content),
            (vec!["-M", "12", "-C1", "needle"], &content),
            (vec!["-m2", "-A1", "needle"], &content),
            (vec!["needle"], &binary),
            (vec!["walked", "needle"], &late),
            (vec!["walked", "-M", "5", "-C1", "needle"], &late),
            (vec!["-a", "needle"], &binary),
        ];
        check(&cases, false);
    }

    #[test]
    fn outlined_rows_written_as_they_come_equal_the_printers() {
        let guide = include_bytes!("../../tests/fixtures/md/guide.md");
        let long = format!("# Long\n\n{}needle{}\n", "x".repeat(60), "y".repeat(60));
        let nested = b"# A\n\n- ## B needle\n  more needle\n\n- # Head in item\n  needle\n";
        let cases: Vec<(Vec<&str>, &[u8])> = vec![
            (vec!["needle"], guide),
            (vec!["-C1", "needle"], guide),
            (vec!["-C2", "-e", "Linux", "-e", "needle-pkg"], guide),
            (vec!["-B3", "needle-a"], guide),
            (vec!["-v", "needle"], guide),
            (vec!["-v", "-C1", "needle"], guide),
            (vec!["-M", "15", "-C1", "needle"], guide),
            (vec!["-m3", "-A2", "needle"], guide),
            (vec!["-e", "\\| Pro", "-e", "needle-10"], guide),
            (vec!["$"], guide),
            (vec!["-C1", "needle"], long.as_bytes()),
            (vec!["-C1", "needle"], nested),
            (vec!["Head|needle"], nested),
        ];
        check(&cases, true);
    }
}
