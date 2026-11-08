//! diff: the commits, files and hunks of unified and combined diffs, `git log -p` and
//! `git format-patch` series (M6 spec §8.2). One pass over the lines indexes the delimiters
//! into an Outline; the cursor is the Outline's, with a commit's subject line counted as its own
//! name too (M6 spec §3, point 5).
use super::mail::names::{NameBuf, Unfolded, name_cap, push_decoded_text};
use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, SharedTrail, Trail};
use crate::text::CutName;
use crate::trail::{Outline, OutlineSegment, TrailCursor, names_of, trail_for};
use std::ops::Range;
use std::rc::Rc;

pub struct Diff;

fn name_chars(t: &toml::Table) -> usize {
    t.get("name_chars")
        .and_then(|v| v.as_integer())
        .map_or(40, |n| n.max(0) as usize)
}

/// One line: its bytes without `\n` and a `\r` before it, and where the next starts.
#[derive(Debug, Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    next: usize,
}

fn line_at(src: &[u8], start: usize) -> Line {
    let nl = memchr::memchr(b'\n', &src[start..]).map(|k| start + k);
    let next = nl.map_or(src.len(), |k| k + 1);
    let mut end = nl.unwrap_or(src.len());
    if end > start && src[end - 1] == b'\r' {
        end -= 1;
    }
    Line { start, end, next }
}

/// `commit <sha>` of `git log`, or `From <sha> …` of `git format-patch`: the sha's bytes.
/// A `From ` line carries the whole sha (40 digits, 64 in a SHA-256 repository), so a body
/// line `From deadbeef …` opens no commit.
fn commit_line(src: &[u8], l: &Line) -> Option<Range<usize>> {
    let text = &src[l.start..l.end];
    let (skip, whole) = if text.starts_with(b"commit ") {
        (7, false)
    } else if text.starts_with(b"From ") {
        (5, true)
    } else {
        return None;
    };
    let from = l.start + skip;
    let hex = src[from..l.end]
        .iter()
        .take_while(|b| b.is_ascii_hexdigit())
        .count();
    let after = src.get(from + hex).copied();
    let digits = if whole {
        hex == 40 || hex == 64
    } else {
        hex >= 7
    };
    (digits && (from + hex == l.end || after == Some(b' '))).then_some(from..from + hex)
}

fn starts(src: &[u8], l: &Line, prefix: &[u8]) -> bool {
    src[l.start..l.end].starts_with(prefix)
}

/// A hunk header `@@ -a,b +c,d @@ context` (`@@@` and two old ranges in a combined diff).
struct HunkHeader {
    /// Each parent's old start and count, then the new start and count; each with the bytes
    /// of its `-a`/`+c`.
    old: Vec<(u64, u64, Range<usize>)>,
    new: (u64, u64, Range<usize>),
    /// The context text after the closing `@@`.
    context: Range<usize>,
}

fn hunk_header(src: &[u8], l: &Line) -> Option<HunkHeader> {
    let text = &src[l.start..l.end];
    let ats = text.iter().take_while(|&&b| b == b'@').count();
    if ats < 2 || text.get(ats) != Some(&b' ') {
        return None;
    }
    let mut i = l.start + ats;
    let mut old = Vec::new();
    let mut new = None;
    loop {
        while i < l.end && src[i] == b' ' {
            i += 1;
        }
        if i >= l.end {
            return None;
        }
        if src[i] == b'@' {
            let close = src[i..l.end].iter().take_while(|&&b| b == b'@').count();
            if close != ats {
                return None;
            }
            i += close;
            break;
        }
        let sign = src[i];
        if sign != b'-' && sign != b'+' {
            return None;
        }
        let from = i;
        i += 1;
        let num = |i: &mut usize| -> Option<u64> {
            let s = *i;
            while *i < l.end && src[*i].is_ascii_digit() {
                *i += 1;
            }
            std::str::from_utf8(&src[s..*i]).ok()?.parse().ok()
        };
        let start = num(&mut i)?;
        let shown = from..i;
        let count = if src.get(i) == Some(&b',') && i < l.end {
            i += 1;
            num(&mut i)?
        } else {
            1
        };
        if sign == b'-' {
            old.push((start, count, shown));
        } else {
            new = Some((start, count, shown));
        }
    }
    let new = new?;
    if old.len() != ats - 1 {
        return None;
    }
    let mut c = i;
    if src.get(c) == Some(&b' ') && c < l.end {
        c += 1;
    }
    Some(HunkHeader {
        old,
        new,
        context: c..l.end,
    })
}

/// A path of a `---`/`+++` line or of `rename from`/`to`, with its git quotes decoded: its
/// characters, each with its Source bytes, a tab and what follows (a timestamp) left out.
fn path_chars(src: &[u8], r: Range<usize>) -> Vec<(char, Range<usize>, bool)> {
    let mut bytes: Vec<(u8, Range<usize>)> = Vec::new();
    if src.get(r.start) == Some(&b'"') {
        let mut i = r.start + 1;
        while i < r.end && src[i] != b'"' {
            if src[i] == b'\\' && i + 1 < r.end {
                let e = src[i + 1];
                if (b'0'..=b'7').contains(&e) {
                    let mut j = i + 1;
                    let mut v: u32 = 0;
                    while j < r.end && j < i + 4 && (b'0'..=b'7').contains(&src[j]) {
                        v = v * 8 + u32::from(src[j] - b'0');
                        j += 1;
                    }
                    bytes.push((v as u8, i..j));
                    i = j;
                    continue;
                }
                let b = match e {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    b'a' => 7,
                    b'b' => 8,
                    b'f' => 12,
                    b'v' => 11,
                    other => other,
                };
                bytes.push((b, i..i + 2));
                i += 2;
                continue;
            }
            bytes.push((src[i], i..i + 1));
            i += 1;
        }
    } else {
        let end = memchr::memchr(b'\t', &src[r.clone()]).map_or(r.end, |k| r.start + k);
        bytes.extend((r.start..end).map(|i| (src[i], i..i + 1)));
    }
    // Characters from the bytes; a character is verbatim when its bytes are its Source.
    let mut out = Vec::new();
    let mut k = 0;
    while k < bytes.len() {
        let width = match bytes[k].0 {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => 0,
        };
        let seq: Vec<u8> = bytes[k..(k + width.max(1)).min(bytes.len())]
            .iter()
            .map(|b| b.0)
            .collect();
        let c = (width > 0 && seq.len() == width)
            .then(|| {
                std::str::from_utf8(&seq)
                    .ok()
                    .and_then(|s| s.chars().next())
            })
            .flatten();
        let n = if c.is_some() { width } else { 1 };
        let range = bytes[k].1.start..bytes[k + n - 1].1.end;
        let verbatim = c.is_some() && range.len() == n;
        out.push((c.unwrap_or('\u{FFFD}'), range, verbatim));
        k += n;
    }
    out
}

/// The path of `src[r]`, `prefix` (`a/`, `b/`) removed when `strip`.
fn push_path(src: &[u8], r: Range<usize>, strip: Option<&[u8]>, name: &mut CutName) {
    let mut chars = path_chars(src, r);
    if let Some(p) = strip {
        let lead: String = chars.iter().take(p.len()).map(|c| c.0).collect();
        if lead.as_bytes() == p {
            chars.drain(..p.len());
        }
    }
    // Verbatim runs as one piece each.
    let mut run: Option<Range<usize>> = None;
    for (c, range, verbatim) in chars {
        match (&mut run, verbatim) {
            (Some(r), true) if r.end == range.start => r.end = range.end,
            (_, true) => {
                if let Some(r) = run.replace(range) {
                    name.push_lossy(src, r);
                }
            }
            (_, false) => {
                if let Some(r) = run.take() {
                    name.push_lossy(src, r);
                }
                name.push_decoded(c, range);
            }
        }
    }
    if let Some(r) = run {
        name.push_lossy(src, r);
    }
}

fn path_text(src: &[u8], r: Range<usize>) -> String {
    path_chars(src, r).into_iter().map(|c| c.0).collect()
}

/// A file being read: its Segment, its header's paths.
struct File {
    seg: usize,
    /// The `---` and `+++` paths' bytes.
    old: Option<Range<usize>>,
    new: Option<Range<usize>>,
    /// `rename from`/`rename to` (or `copy`) paths.
    from: Option<Range<usize>>,
    to: Option<Range<usize>>,
    /// The `diff --git` line's bytes after `diff --git `.
    git: Option<Range<usize>>,
    /// A hunk was read: past it, a line that is no hunk ends the file.
    hunks: bool,
}

impl File {
    fn new(seg: usize) -> Self {
        Self {
            seg,
            old: None,
            new: None,
            from: None,
            to: None,
            git: None,
            hunks: false,
        }
    }
}

/// A hunk being read: its Segment and the lines it still counts, per parent and new.
struct Hunk {
    seg: usize,
    old: Vec<u64>,
    new: u64,
}

impl Hunk {
    fn done(&self) -> bool {
        self.new == 0 && self.old.iter().all(|&o| o == 0)
    }

    /// Counts a line of the hunk by its `prefix`, one column per parent: the new side has it
    /// unless a column removes it, a parent unless its column adds it.
    fn count(&mut self, prefix: &[u8]) {
        if !prefix.contains(&b'-') {
            self.new = self.new.saturating_sub(1);
        }
        for (i, o) in self.old.iter_mut().enumerate() {
            if prefix.get(i).copied().unwrap_or(b' ') != b'+' {
                *o = o.saturating_sub(1);
            }
        }
    }
}

/// Where a commit's subject is looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// `git log`'s header lines, up to a blank line.
    LogHeaders,
    /// `git log`'s message: its first line with text is the subject.
    LogMessage,
    /// A patch mail's headers: `Subject:` is the subject.
    MailHeaders,
    Done,
}

/// The commit being read.
struct Commit {
    seg: usize,
    sha: Range<usize>,
    subject: Option<Range<usize>>,
    phase: Phase,
}

/// The Outline of a diff and, by Segment, the subject of a commit, which is its own name too
/// (M6 spec §3, point 5: a Match there is outside the commit).
struct Index {
    outline: Outline,
    subjects: Vec<Option<Range<usize>>>,
    /// The commits' Segments, in file order.
    commits: Vec<usize>,
}

struct Builder<'s> {
    src: &'s [u8],
    name_chars: usize,
    segs: Vec<OutlineSegment>,
    /// (commit start, subject).
    subjects: Vec<(usize, Range<usize>)>,
    commit: Option<Commit>,
    file: Option<File>,
    hunk: Option<Hunk>,
    /// The hunk closed last and where it ends: a `\ No newline at end of file` line right
    /// there is its own, and the first such line takes it.
    closed: Option<(usize, usize)>,
}

/// git's extended header lines, between `diff --git` and `---`.
const EXTENDED: &[&[u8]] = &[
    b"index ",
    b"old mode ",
    b"new mode ",
    b"deleted file mode ",
    b"new file mode ",
    b"similarity index ",
    b"dissimilarity index ",
    b"rename from ",
    b"rename to ",
    b"copy from ",
    b"copy to ",
];

fn is_diff_line(src: &[u8], l: &Line) -> bool {
    starts(src, l, b"diff --git ")
        || starts(src, l, b"diff --cc ")
        || starts(src, l, b"diff --combined ")
}

impl<'s> Builder<'s> {
    fn new(src: &'s [u8], name_chars: usize) -> Self {
        Self {
            src,
            name_chars,
            segs: Vec::new(),
            subjects: Vec::new(),
            commit: None,
            file: None,
            hunk: None,
            closed: None,
        }
    }

    fn open(&mut self, kind: &str, l: &Line, label_end: usize) -> usize {
        let mut s = OutlineSegment::new(kind, "", l.start..self.src.len(), l.start..label_end);
        s.name_anchor = l.start;
        self.segs.push(s);
        self.segs.len() - 1
    }

    fn named(&mut self, seg: usize, name: CutName) {
        self.named_as(seg, name.finish());
    }

    fn named_as(&mut self, seg: usize, (text, pieces): (String, crate::text::Pieces)) {
        let s = &mut self.segs[seg];
        s.segment.name = text;
        s.name = (!pieces.is_empty()).then_some(pieces);
    }

    fn close_hunk(&mut self, at: usize) {
        if let Some(h) = self.hunk.take() {
            self.segs[h.seg].range.end = at;
            self.closed = Some((h.seg, at));
        }
    }

    fn close_file(&mut self, at: usize) {
        self.close_hunk(at);
        if let Some(f) = self.file.take() {
            self.name_file(&f);
            self.segs[f.seg].range.end = at;
        }
    }

    fn close_commit(&mut self, at: usize) {
        self.close_file(at);
        if let Some(c) = self.commit.take() {
            self.name_commit(&c);
            self.segs[c.seg].range.end = at;
        }
    }

    /// Ends the search for the commit's subject: a file or a hunk starts.
    fn no_subject(&mut self) {
        if let Some(c) = &mut self.commit {
            c.phase = Phase::Done;
        }
    }

    /// Names a file by its new path, else its old one when the new is `/dev/null`; a rename's
    /// signature is `old → new` (M6 spec §8.2). `a/` and `b/` are removed only where git adds
    /// them: on old and new paths that have them both (or `/dev/null`).
    fn name_file(&mut self, f: &File) {
        let src = self.src;
        let dev_null = |r: &Option<Range<usize>>| {
            r.as_ref()
                .is_some_and(|r| src[r.clone()].starts_with(b"/dev/null"))
        };
        let prefixed = |r: &Option<Range<usize>>, p: &str| {
            r.as_ref()
                .is_some_and(|r| path_text(src, r.clone()).starts_with(p))
        };
        let git = (f.git.is_some() || f.old.is_some() || f.new.is_some())
            && (f.old.is_none() || dev_null(&f.old) || prefixed(&f.old, "a/"))
            && (f.new.is_none() || dev_null(&f.new) || prefixed(&f.new, "b/"));
        let (path, strip): (Option<Range<usize>>, &[u8]) = if f.new.is_some() && !dev_null(&f.new) {
            (f.new.clone(), b"b/")
        } else if f.old.is_some() && !dev_null(&f.old) {
            (f.old.clone(), b"a/")
        } else if f.to.is_some() {
            (f.to.clone(), b"")
        } else {
            // `diff --git a/x b/x` alone: the path after its last ` b/`.
            let r = f.git.clone().and_then(|g| {
                memchr::memmem::rfind(&src[g.clone()], b" b/").map(|k| g.start + k + 1..g.end)
            });
            (r, b"b/")
        };
        let mut name = CutName::new(self.name_chars);
        if let Some(p) = path {
            push_path(
                src,
                p,
                (git && !strip.is_empty()).then_some(strip),
                &mut name,
            );
        }
        self.named(f.seg, name);
        if let (Some(from), Some(to)) = (&f.from, &f.to) {
            self.segs[f.seg].segment.signature = Some(format!(
                "{} → {}",
                path_text(src, from.clone()),
                path_text(src, to.clone())
            ));
        }
    }

    /// Names a commit by its sha's first seven digits and its subject, a leading
    /// `[PATCH n/m]` removed.
    fn name_commit(&mut self, c: &Commit) {
        let src = self.src;
        let mut name = CutName::new(self.name_chars);
        name.push_lossy(src, c.sha.start..c.sha.start + 7);
        if let Some(subject) = &c.subject {
            self.subjects
                .push((self.segs[c.seg].range.start, subject.clone()));
            let mut r = subject.clone();
            if src[r.clone()].starts_with(b"[")
                && let Some(k) = memchr::memchr(b']', &src[r.clone()])
                && src[r.start..r.start + k]
                    .windows(5)
                    .any(|w| w.eq_ignore_ascii_case(b"PATCH"))
            {
                r.start += k + 1;
                while r.start < r.end && src[r.start] == b' ' {
                    r.start += 1;
                }
            }
            if memchr::memmem::find(&src[r.clone()], b"=?").is_some() {
                // RFC 2047 encoded words, decoded as mail decodes a header (review r1,
                // Fable #66).
                let mut out = NameBuf::new(self.name_chars);
                out.push_source(src, c.sha.start..c.sha.start + 7);
                out.push_generated(" ");
                let mut lines = Vec::new();
                let mut at = r.start;
                for nl in memchr::memchr_iter(b'\n', &src[r.clone()]).map(|k| r.start + k) {
                    let end = if nl > at && src[nl - 1] == b'\r' {
                        nl - 1
                    } else {
                        nl
                    };
                    lines.push(at..end);
                    at = nl + 1;
                }
                lines.push(at..r.end);
                let u = Unfolded::new(src, &lines, name_cap(self.name_chars));
                push_decoded_text(src, &u, &mut out);
                self.named_as(c.seg, out.finish());
                return;
            }
            if !r.is_empty() {
                name.push_str(" ");
                // A folded `Subject:` header: its lines joined by one space.
                let mut at = r.start;
                for nl in memchr::memchr_iter(b'\n', &src[r.clone()]).map(|k| r.start + k) {
                    let mut end = nl;
                    if end > at && src[end - 1] == b'\r' {
                        end -= 1;
                    }
                    name.push_lossy(src, at..end);
                    name.push_str(" ");
                    at = nl + 1;
                    while at < r.end && matches!(src[at], b' ' | b'\t') {
                        at += 1;
                    }
                }
                name.push_lossy(src, at..r.end);
            }
        }
        self.named(c.seg, name);
    }

    /// Whether the line at `l`, in a hunk whose counts are not done, starts a commit, a file
    /// or a hunk. A `---`/`+++` pair there is a file's header only before a hunk header: else
    /// it is two changed lines that start with `--` and `++`, a SQL or Lua comment (review r1,
    /// Fable #9).
    fn header(&self, l: &Line) -> bool {
        let src = self.src;
        commit_line(src, l).is_some()
            || is_diff_line(src, l)
            || hunk_header(src, l).is_some()
            || (starts(src, l, b"--- ") && l.next < src.len() && {
                let plus = line_at(src, l.next);
                starts(src, &plus, b"+++ ")
                    && plus.next < src.len()
                    && hunk_header(src, &line_at(src, plus.next)).is_some()
            })
    }

    /// A line of the hunk being read, counted: true when it is one. A line that is no hunk
    /// line, or a header, ends the hunk whatever its counts (M6 spec §8.2, Failure).
    fn in_hunk(&mut self, l: &Line) -> bool {
        let src = self.src;
        if self.hunk.is_none() {
            return false;
        }
        let text = &src[l.start..l.end];
        let content = text.is_empty() || matches!(text[0], b' ' | b'+' | b'-' | b'\\');
        if !content || self.header(l) {
            self.close_hunk(l.start);
            return false;
        }
        let h = self.hunk.as_mut().expect("a hunk");
        if text.first() != Some(&b'\\') {
            let parents = h.old.len().min(text.len());
            h.count(&text[..parents]);
        }
        if h.done() {
            self.close_hunk(l.next);
        }
        true
    }

    /// Reads a commit's header and message lines for its subject.
    fn subject(&mut self, l: &Line) {
        let src = self.src;
        let Some(c) = &mut self.commit else { return };
        let text = &src[l.start..l.end];
        match c.phase {
            Phase::LogHeaders if text.is_empty() => c.phase = Phase::LogMessage,
            Phase::LogMessage => {
                let lead = text
                    .iter()
                    .take_while(|&&b| matches!(b, b' ' | b'\t'))
                    .count();
                if lead < text.len() {
                    c.subject = Some(l.start + lead..l.end);
                    c.phase = Phase::Done;
                }
            }
            Phase::MailHeaders if text.is_empty() => c.phase = Phase::Done,
            Phase::MailHeaders => {
                if let Some(rest) = text.strip_prefix(b"Subject: ") {
                    let mut r = l.end - rest.len()..l.end;
                    // A folded header goes on in lines that start with white space.
                    let mut next = l.next;
                    while next < src.len() {
                        let f = line_at(src, next);
                        if f.start == f.end || !matches!(src[f.start], b' ' | b'\t') {
                            break;
                        }
                        r.end = f.end;
                        next = f.next;
                    }
                    c.subject = Some(r);
                    c.phase = Phase::Done;
                }
            }
            _ => {}
        }
    }

    fn build(mut self) -> Index {
        let src = self.src;
        let mut at = 0;
        while at < src.len() {
            let l = line_at(src, at);
            at = l.next;
            if self.in_hunk(&l) {
                continue;
            }
            let text = &src[l.start..l.end];
            if text.first() == Some(&b'\\')
                && self.file.is_some()
                && let Some((seg, end)) = self.closed
                && end == l.start
            {
                self.segs[seg].range.end = l.next;
                // A hunk has one such mark: a second is text (code round 2, Fable #17).
                self.closed = None;
                continue;
            }
            if let Some(sha) = commit_line(src, &l) {
                self.close_commit(l.start);
                let seg = self.open("commit", &l, l.end);
                self.segs[seg].segment.signature = Some(String::from_utf8_lossy(text).into_owned());
                let phase = if text.starts_with(b"From ") {
                    Phase::MailHeaders
                } else {
                    Phase::LogHeaders
                };
                self.commit = Some(Commit {
                    seg,
                    sha,
                    subject: None,
                    phase,
                });
                continue;
            }
            if is_diff_line(src, &l) {
                self.close_file(l.start);
                self.no_subject();
                let seg = self.open("file", &l, l.end);
                let mut f = File::new(seg);
                f.git = starts(src, &l, b"diff --git ").then(|| l.start + 11..l.end);
                self.file = Some(f);
                continue;
            }
            if starts(src, &l, b"--- ") && l.next < src.len() {
                let plus = line_at(src, l.next);
                if starts(src, &plus, b"+++ ") {
                    // The `---`/`+++` pair of the `diff --git` line before it, else a file.
                    let reuse = self
                        .file
                        .as_ref()
                        .is_some_and(|f| !f.hunks && f.old.is_none() && f.new.is_none());
                    if !reuse {
                        self.close_file(l.start);
                        let seg = self.open("file", &l, plus.end);
                        self.file = Some(File::new(seg));
                    }
                    self.no_subject();
                    let f = self.file.as_mut().expect("a file");
                    f.old = Some(l.start + 4..l.end);
                    f.new = Some(plus.start + 4..plus.end);
                    self.segs[f.seg].label.end = plus.end;
                    at = plus.next;
                    continue;
                }
            }
            if let Some(h) = hunk_header(src, &l) {
                self.no_subject();
                let deleted = self.file.as_ref().is_some_and(|f| {
                    f.new
                        .as_ref()
                        .is_some_and(|r| src[r.clone()].starts_with(b"/dev/null"))
                });
                let seg = self.open("hunk", &l, l.end);
                let mut name = CutName::new(self.name_chars);
                let shown = if deleted { &h.old[0].2 } else { &h.new.2 };
                name.push_lossy(src, shown.clone());
                if !h.context.is_empty() {
                    name.push_str(" ");
                    name.push_lossy(src, h.context.clone());
                }
                self.named(seg, name);
                self.segs[seg].segment.signature = Some(String::from_utf8_lossy(text).into_owned());
                if let Some(f) = &mut self.file {
                    f.hunks = true;
                }
                let hunk = Hunk {
                    seg,
                    old: h.old.iter().map(|o| o.1).collect(),
                    new: h.new.1,
                };
                self.hunk = Some(hunk);
                if self.hunk.as_ref().is_some_and(Hunk::done) {
                    self.close_hunk(l.next);
                }
                continue;
            }
            if let Some(f) = &mut self.file {
                if f.hunks {
                    // Past its hunks, a line that is no hunk ends the file: it is the commit's.
                    self.close_file(l.start);
                } else {
                    // An extended header line is part of the file's own name, as the
                    // `diff --git` line and the `---`/`+++` pair around it (review r1,
                    // Fable #69).
                    if EXTENDED.iter().any(|k| text.starts_with(k)) {
                        self.segs[f.seg].label.end = l.end;
                    }
                    for (key, from) in [
                        (&b"rename from "[..], true),
                        (b"rename to ", false),
                        (b"copy from ", true),
                        (b"copy to ", false),
                    ] {
                        if let Some(rest) = text.strip_prefix(key) {
                            let r = l.end - rest.len()..l.end;
                            if from {
                                f.from = Some(r);
                            } else {
                                f.to = Some(r);
                            }
                        }
                    }
                    continue;
                }
            }
            self.subject(&l);
        }
        self.close_commit(src.len());
        let subjects: std::collections::HashMap<usize, Range<usize>> =
            self.subjects.into_iter().collect();
        let outline = Outline::build(self.segs, src);
        let subjects = outline
            .segments
            .iter()
            .map(|s| {
                (s.segment.kind == "commit")
                    .then(|| subjects.get(&s.range.start).cloned())
                    .flatten()
            })
            .collect();
        let commits = (0..outline.segments.len())
            .filter(|&i| outline.segments[i].segment.kind == "commit")
            .collect();
        Index {
            outline,
            subjects,
            commits,
        }
    }
}

impl Index {
    fn new(src: &[u8], name_chars: usize) -> Self {
        Builder::new(src, name_chars).build()
    }

    /// `trail::names_of`, with a commit's subject counted in its label: a Match in it leaves
    /// the commit out of its Trail.
    fn names(&self) -> Vec<Name> {
        let mut names = names_of(&self.outline);
        let segs = &self.outline.segments;
        for n in names.iter_mut().filter(|n| n.trail.len() == 1) {
            let k = self
                .commits
                .partition_point(|&c| segs[c].range.start <= n.anchor);
            let Some(&c) = k.checked_sub(1).map(|k| &self.commits[k]) else {
                continue;
            };
            if segs[c].name_anchor == n.anchor
                && let (Some(subject), Some(label)) = (&self.subjects[c], n.labels.last_mut())
            {
                label.end = label.end.max(subject.end);
            }
        }
        names
    }

    fn own_subject(&self, seg: usize, byte: usize) -> bool {
        self.subjects[seg]
            .as_ref()
            .is_some_and(|r| r.contains(&byte))
    }

    fn trail(&self, byte: usize) -> Trail {
        let mut t = trail_for(&self.outline, byte);
        // Commits never nest, and only a commit holds its subject, before its files.
        let segs = &self.outline.segments;
        let k = self
            .commits
            .partition_point(|&c| segs[c].range.start <= byte);
        if let Some(&c) = k.checked_sub(1).map(|k| &self.commits[k])
            && self.own_subject(c, byte)
        {
            t.clear();
        }
        t
    }
}

/// A diff prepared once: its Outline.
struct DiffPrepared {
    index: Index,
}

impl Prepared for DiffPrepared {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(DiffCursor {
            index: &self.index,
            trails: TrailCursor::new(&self.index.outline),
            indices: Vec::new(),
            last: None,
        })
    }

    fn names(&self) -> anyhow::Result<Vec<Name>> {
        Ok(self.index.names())
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Outline's cursor, a commit left out of the Trails of its subject's bytes.
struct DiffCursor<'p> {
    index: &'p Index,
    trails: TrailCursor<'p>,
    indices: Vec<usize>,
    last: Option<(Vec<usize>, SharedTrail)>,
}

impl Cursor for DiffCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        self.trails.indices(byte, &mut self.indices);
        let index = self.index;
        self.indices.retain(|&i| !index.own_subject(i, byte));
        match &self.last {
            Some((ix, t)) if *ix == self.indices => Ok(t.clone()),
            _ => {
                let t: SharedTrail = Rc::from(self.trails.segments(&self.indices));
                self.last = Some((self.indices.clone(), t.clone()));
                Ok(t)
            }
        }
    }

    /// The Outline's answer, a subject's bytes counted as its commit's own, which can only
    /// keep more Matches.
    fn open_from(&mut self) -> Option<usize> {
        Some(self.trails.open_from())
    }

    fn open_from_with(
        &mut self,
        first: &mut dyn FnMut(Range<usize>) -> Option<usize>,
    ) -> Option<usize> {
        Some(self.trails.open_from_with(first))
    }
}

impl Processor for Diff {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let index = Index::new(input.bytes, name_chars(params));
        Ok(anchors.iter().map(|&a| index.trail(a)).collect())
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        Ok(Index::new(input.bytes, name_chars(params)).names())
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        Ok(Box::new(DiffPrepared {
            index: Index::new(input.bytes, name_chars(params)),
        }))
    }

    fn capability(&self, _params: &toml::Table) -> Capability {
        Capability::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trails(src: &[u8], pattern: &str) -> Vec<String> {
        let index = Index::new(src, 40);
        let prepared = DiffPrepared {
            index: Index::new(src, 40),
        };
        let mut c = prepared.cursor();
        memchr::memmem::find_iter(src, pattern.as_bytes())
            .map(|a| {
                let t = c.trail_at(a).unwrap();
                assert_eq!(t.as_ref(), index.trail(a).as_slice(), "byte {a}");
                t.iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" › ")
            })
            .collect()
    }

    #[test]
    fn format_patch_series_renames_and_quoted_paths() {
        let src = "From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\n\
            From: A <a@x>\nSubject: [PATCH 1/2] Rename the\n needle\n\nneedle body\n---\n\
            diff --git a/old.rs b/new.rs\nsimilarity index 90%\nrename from old.rs\nrename to new.rs\n\
            --- a/old.rs\n+++ b/new.rs\n@@ -1,2 +1,2 @@\n-x\n+needle\n y\n-- \n2.43.0 needle\n\n\
            From 1123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\n\
            Subject: [PATCH 2/2] Two\n\n\
            diff --git \"a/caf\\303\\251\" \"b/caf\\303\\251\"\n--- \"a/caf\\303\\251\"\n+++ \"b/caf\\303\\251\"\n\
            @@ -0,0 +1 @@\n+needle\n";
        assert_eq!(
            trails(src.as_bytes(), "needle"),
            [
                "",
                "0123456 Rename the needle",
                "0123456 Rename the needle › new.rs › +1",
                "0123456 Rename the needle",
                "1123456 Two › café › +1",
            ]
        );
        let index = Index::new(src.as_bytes(), 40);
        let file = index
            .outline
            .segments
            .iter()
            .find(|s| s.segment.name == "new.rs")
            .unwrap();
        assert_eq!(file.segment.signature.as_deref(), Some("old.rs → new.rs"));
        let names = index.names();
        let cafe = names.iter().find(|n| n.text == "café").unwrap();
        assert_eq!(cafe.pieces.len(), 2);
    }

    /// Review r1, Fable #9: in a hunk whose counts are not done, a `---`/`+++` pair is a
    /// changed line that starts with `--` or `++` (a SQL comment); it is a file's header only
    /// before a hunk header, which ends a hunk whose counts run past it (M6 spec §8.2, Failure).
    #[test]
    fn a_dash_pair_inside_a_hunk_is_content() {
        let src = b"diff --git a/q.sql b/q.sql\n--- a/q.sql\n+++ b/q.sql\n@@ -1,3 +1,3 @@\n\
            --- old needle comment\n+++ new needle comment\n SELECT needle2;\n";
        assert_eq!(
            trails(src, "needle"),
            ["q.sql › +1", "q.sql › +1", "q.sql › +1"]
        );
        let src = b"--- a/x\n+++ b/x\n@@ -1,99 +1,99 @@\n+x\n--- a/next\n+++ b/next\n@@ -1 +1 @@\n+needle\n";
        assert_eq!(trails(src, "needle"), ["next › +1"]);
    }

    /// Review r1, Fable #44: a patch mail's `From ` line carries the whole 40-digit sha, so
    /// a body line `From deadbeef …` opens no commit.
    #[test]
    fn a_from_line_needs_the_whole_sha() {
        let src = b"From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\n\
            Subject: [PATCH] One\n\nFrom deadbeef we learned the needle\n---\n\
            --- a/x\n+++ b/x\n@@ -1 +1 @@\n+needle\n";
        assert_eq!(
            trails(src, "needle"),
            ["0123456 One", "0123456 One › x › +1"]
        );
    }

    /// Review r1, Fable #48: `\ No newline at end of file` after a hunk's last counted line
    /// belongs to that hunk.
    #[test]
    fn no_newline_marks_stay_in_their_hunk() {
        let src = b"--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n\
            \\ No newline at end of file\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-c\n+d\n\\ No newline at end of file\n";
        assert_eq!(trails(src, "No newline"), ["x › +1", "x › +1", "y › +1"]);
        // One mark per hunk: a second `\` line after it is text after the hunk, as any other
        // line there (code round 2, Fable #17).
        let src = b"--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n\\ No newline at end of file\n\\ needle\nneedle\n";
        assert_eq!(trails(src, "needle"), ["", ""]);
    }

    /// Review r1, Fable #66: a commit's RFC 2047 subject is decoded, as mail's are.
    #[test]
    fn encoded_subjects_are_decoded() {
        let src = b"From 0123456789abcdef0123456789abcdef01234567 Mon Sep 17 00:00:00 2001\n\
            Subject: [PATCH] =?UTF-8?q?caf=C3=A9_fix?=\n\nneedle\n";
        assert_eq!(trails(src, "needle"), ["0123456 café fix"]);
    }

    /// Review r1, Fable #69: a file's own name runs over all its header lines (`index`,
    /// modes, renames), whether or not a `---`/`+++` pair follows.
    #[test]
    fn a_files_label_holds_its_extended_headers() {
        let src = b"diff --git a/x b/x\nindex 0000needle..1111 100644\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n+y\n\
            diff --git a/m b/m\nold mode 100644\nnew mode 100755\nindex needle\n\
            diff --git a/bin b/bin\nindex 1..2\nGIT binary patch\nliteral 6\nneedle\n";
        assert_eq!(trails(src, "needle"), ["", "", "bin"]);
    }

    #[test]
    fn plain_unified_diffs_keep_their_paths() {
        let src = b"--- a.txt\t2024-01-01\n+++ b.txt\t2024-01-02\n@@ -1 +1 @@\n-x\n+needle\n";
        assert_eq!(trails(src, "needle"), ["b.txt › +1"]);
    }
}
