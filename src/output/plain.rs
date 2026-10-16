//! Plain files without structural flags, written row by row as the searcher reports them (code
//! review r1, Fable #1): no model of the file is kept, so a file where most lines match costs
//! its output and nothing more. Prints exactly what the printers print from a Described file
//! with empty Trails.
use super::json::{JsonFile, JsonOptions, RunStats};
use super::{Layout, RenderOptions, Row, flat, tree};
use crate::model::{FileStats, Line, Submatch};
use crate::search::{MaxCountBytes, context_submatches, match_submatches};
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink, SinkContext, SinkFinish, SinkMatch};
use std::path::Path;
use std::time::Instant;
use termcolor::WriteColor;

pub struct PlainSink<'a, W: WriteColor> {
    w: &'a mut W,
    matcher: &'a RegexMatcher,
    invert: bool,
    o: &'a RenderOptions,
    path: &'a Path,
    explicit: bool,
    /// The search reads through a reader (for `-m`'s byte count, see `MaxCountBytes`).
    reader: bool,
    head: flat::FileHead,
    json: Option<JsonFile<'a>>,
    max_count: MaxCountBytes,
    /// Line number of the last row printed, for the `--` between groups.
    prev: Option<u64>,
    /// The tree's path line is written.
    header: bool,
    /// Binary data was found: rows after it are not printed (design note 3).
    binary: bool,
    /// No colour and no -M: a short valid line can be written without the printers' machinery.
    fast: bool,
    /// The path and its separator for the flat layout (empty without file names), and the
    /// width of that path in characters.
    path_prefix: (Vec<u8>, usize),
    scratch: Vec<u8>,
    started: Instant,
    pub stats: FileStats,
    pub has_match: bool,
}

impl<'a, W: WriteColor> PlainSink<'a, W> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        w: &'a mut W,
        matcher: &'a RegexMatcher,
        invert: bool,
        o: &'a RenderOptions,
        jo: &'a JsonOptions,
        path: &'a Path,
        explicit: bool,
        reader: bool,
    ) -> Self {
        let fast = !w.supports_color() && o.max_columns.is_none();
        Self {
            w,
            matcher,
            invert,
            o,
            path,
            explicit,
            reader,
            head: flat::FileHead::new(path, o),
            json: (o.layout == Layout::Json).then(|| JsonFile::new(path, None, jo)),
            max_count: MaxCountBytes::default(),
            prev: None,
            header: false,
            binary: false,
            fast,
            path_prefix: if o.with_filename && o.layout == Layout::Flat {
                let p = super::path_bytes(path);
                let width = String::from_utf8_lossy(&p).chars().count();
                (p, width)
            } else {
                (Vec::new(), 0)
            },
            scratch: Vec::new(),
            started: Instant::now(),
            stats: FileStats::default(),
            has_match: false,
        }
    }

    /// One row. `subs` finds the line's submatches; it is not called for a row that does not
    /// need them (a short line without colour).
    fn write(
        &mut self,
        line_number: u64,
        absolute_offset: u64,
        bytes: &[u8],
        is_match: bool,
        subs: &dyn Fn() -> Vec<Submatch>,
    ) -> std::io::Result<()> {
        let line = |submatches| Line {
            line_number,
            absolute_offset,
            len: bytes.len(),
            owned: None,
            submatches,
        };
        if let Some(j) = self.json.as_mut() {
            let line = line(subs());
            return if is_match {
                self.stats.matches += line.submatches.len() as u64;
                j.matched(self.w, &line, bytes, &[&[]], &[])
            } else {
                j.context(self.w, &line, bytes)
            };
        }
        if self.binary {
            return Ok(());
        }
        let gap = self.prev.is_some_and(|p| p + 1 != line_number);
        self.prev = Some(line_number);
        if self.fast && self.write_fast(line_number, bytes, is_match, gap)? {
            return Ok(());
        }
        let line = line(subs());
        let subs: Vec<(usize, usize)> = line.submatches.iter().map(|s| (s.start, s.end)).collect();
        let gap_before = gap && self.o.context;
        if self.o.layout == Layout::Flat {
            let row = Row {
                trail: &[],
                line_number: line.line_number,
                bytes,
                subs,
                is_match,
                gap_before,
            };
            return flat::write_row(self.w, &self.head, &row, self.o);
        }
        if !self.header {
            self.header = true;
            tree::write_header(self.w, self.path)?;
        }
        if gap_before {
            self.w.write_all(b"  --\n")?;
        }
        tree::write_row(self.w, 0, line.line_number, bytes, &subs, is_match, self.o)
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
            tree::write_header(self.w, self.path)?;
        }
        if gap && self.o.context {
            self.w.write_all(if tree { b"  --\n" } else { b"--\n" })?;
        }
        self.w.write_all(&self.scratch)?;
        self.w.write_all(raw)?;
        self.w.write_all(b"\n")?;
        Ok(true)
    }

    /// After the search: the tree's path line if no row wrote it, ripgrep's binary message,
    /// or JSON's `end`. Returns the file's JSON stats, for the summary. Writes nothing for a
    /// file without a Match.
    pub fn close(mut self) -> std::io::Result<Option<RunStats>> {
        if !self.has_match {
            return Ok(None);
        }
        if let Some(j) = self.json.as_mut() {
            return j.end(self.w, &self.stats).map(Some);
        }
        if self.o.layout == Layout::Tree && !self.header {
            tree::write_header(self.w, self.path)?;
        }
        let msg = super::binary_message(self.stats.binary_offset, true, self.explicit, self.o);
        if let Some(msg) = msg {
            match self.o.layout {
                Layout::Tree => tree::write_binary_message(self.w, &msg)?,
                _ => flat::write_binary_message(self.w, self.path, &msg, self.o)?,
            }
        }
        Ok(None)
    }
}

impl<W: WriteColor> Sink for PlainSink<'_, W> {
    type Error = std::io::Error;

    fn begin(&mut self, s: &Searcher) -> Result<bool, Self::Error> {
        self.max_count = MaxCountBytes::for_searcher(s);
        self.started = Instant::now();
        Ok(true)
    }

    fn matched(&mut self, _s: &Searcher, m: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.max_count.matched(m);
        self.stats.matched_lines += m.lines().count() as u64;
        self.has_match = true;
        let (matcher, invert) = (self.matcher, self.invert);
        let subs = || match_submatches(matcher, invert, m);
        let n = m.line_number().unwrap_or(0);
        self.write(n, m.absolute_byte_offset(), m.bytes(), true, &subs)?;
        Ok(true)
    }

    fn context(&mut self, _s: &Searcher, c: &SinkContext<'_>) -> Result<bool, Self::Error> {
        self.max_count.context(c);
        let (matcher, invert) = (self.matcher, self.invert);
        let subs = || context_submatches(matcher, invert, c.bytes());
        let n = c.line_number().unwrap_or(0);
        self.write(n, c.absolute_byte_offset(), c.bytes(), false, &subs)?;
        Ok(true)
    }

    fn binary_data(&mut self, _s: &Searcher, _offset: u64) -> Result<bool, Self::Error> {
        self.binary = true;
        Ok(true)
    }

    fn finish(&mut self, _s: &Searcher, f: &SinkFinish) -> Result<(), Self::Error> {
        self.stats.bytes_searched = self.max_count.bytes_searched(f.byte_count(), self.reader);
        self.stats.binary_offset = f.binary_byte_offset();
        self.stats.elapsed = self.started.elapsed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse;
    use crate::model::{Described, FileMatches};
    use crate::output::json::render_with_stats;
    use crate::search::{matcher, search_source, searcher};
    use crate::source::Source;
    use termcolor::Buffer;

    /// The same file printed by `PlainSink` and by the printers from a Described model with
    /// empty Trails; JSON elapsed times are masked.
    fn both(
        argv: &[&str],
        layout: Layout,
        color: bool,
        named: bool,
        content: &[u8],
    ) -> (String, String) {
        let mut v = vec!["sfor"];
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
        };
        let jo = JsonOptions {
            line_numbers: ro.line_numbers,
            width: ro.width,
            expand: false,
        };
        let path = Path::new("docs/a.txt");
        let new_buf = || {
            if color {
                Buffer::ansi()
            } else {
                Buffer::no_color()
            }
        };
        let mut streamed = new_buf();
        let mut s = searcher(&a, explicit, true);
        let mut sink = PlainSink::new(
            &mut streamed,
            &m,
            a.invert_match,
            &ro,
            &jo,
            path,
            explicit,
            true,
        );
        s.search_reader(&m, std::io::Cursor::new(content), &mut sink)
            .unwrap();
        sink.close().unwrap();
        let src = Source::from_bytes(path.into(), content.to_vec(), explicit);
        let mut modelled = new_buf();
        if let Some(fm) = search_source(&mut s, &m, &src, false, false).unwrap() {
            let trails = fm
                .matches()
                .map(|m| vec![Vec::new(); m.submatches.len().max(1)])
                .collect();
            let d = Described {
                file: FileMatches {
                    path: path.into(),
                    ..fm
                },
                format: None,
                trails,
                sections: Vec::new(),
            };
            match layout {
                Layout::Flat => {
                    super::super::flat::render(&d, src.bytes(), &ro, &mut modelled).unwrap()
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

    #[test]
    fn rows_written_as_they_come_equal_the_printers() {
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
        for (argv, content) in &cases {
            let mut argv = argv.clone();
            if argv[0] == "walked" {
                argv.remove(0);
                argv.push("walked");
            }
            for layout in [Layout::Flat, Layout::Tree, Layout::Json] {
                for color in [false, true] {
                    for named in [true, false] {
                        let (s, m) = both(&argv, layout, color, named, content);
                        assert!(!m.is_empty(), "{argv:?} printed nothing");
                        assert_eq!(s, m, "{argv:?} {layout:?} color={color} named={named}");
                    }
                }
            }
        }
    }
}
