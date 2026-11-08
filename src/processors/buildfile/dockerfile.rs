//! Dockerfile (M6 spec §7.9): each `FROM` opens a `stage`, named by its `AS` name or `stage N`
//! (0-based, as `--from=N` counts), up to the next `FROM`; each other instruction is an
//! `instruction` named by its keyword as written, over its continuation lines and its heredocs.
//! The escape character is `\`, or the one a leading `# escape=` directive sets. Bodies are
//! kept verbatim: no shell is parsed and no file is read.
use super::{BuildIndex, Item, Line, Named, line_at, trim};
use std::ops::Range;

/// The value of a parser directive (`# key=value`, the key without case) on a leading line,
/// or None when the line is no directive.
fn directive(src: &[u8], line: &Line) -> Option<(Range<usize>, Range<usize>)> {
    let t = trim(src, line.start..line.end);
    if src.get(t.start) != Some(&b'#') {
        return None;
    }
    let body = trim(src, t.start + 1..t.end);
    let eq = memchr::memchr(b'=', &src[body.clone()])? + body.start;
    let key = trim(src, body.start..eq);
    if key.is_empty()
        || !src[key.clone()]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return None;
    }
    Some((key, trim(src, eq + 1..body.end)))
}

/// Whether the line ends with the escape character, spaces and tabs after it allowed.
fn continues(src: &[u8], line: &Line, escape: u8) -> bool {
    let t = trim(src, line.start..line.end);
    !t.is_empty() && src[t.end - 1] == escape
}

/// Whether the line is blank or a comment: inside a continuation, Docker skips it.
fn skipped(src: &[u8], line: &Line) -> bool {
    let t = trim(src, line.start..line.end);
    t.is_empty() || src[t.start] == b'#'
}

/// A heredoc marker: the word that closes its body, and whether `<<-` strips leading tabs.
struct Heredoc {
    word: Range<usize>,
    strip_tabs: bool,
}

/// The heredoc markers of a line, in order: `<<WORD`, `<<-WORD`, `<<"WORD"`, `<<'WORD'`. As
/// buildkit reads them, a marker is a word of the line, split at blanks outside quotes and
/// escapes, that starts with `<<`: a `<<` inside a quoted word (`echo "<<EOF"`) or inside
/// another word (`$((1<<3))`) is text (Codex r1 #6, Fable r1 #8).
fn heredocs(src: &[u8], line: &Line, escape: u8, out: &mut Vec<Heredoc>) {
    let s = &src[..line.end];
    let mut i = line.start;
    while i < s.len() {
        while i < s.len() && matches!(s[i], b' ' | b'\t') {
            i += 1;
        }
        if s[i..].starts_with(b"<<") {
            marker(s, i, out);
        }
        // Past the word: quotes and escapes keep its blanks.
        let mut quote: Option<u8> = None;
        while i < s.len() {
            let c = s[i];
            match quote {
                Some(q) if c == q => quote = None,
                Some(b'"') if c == escape => i += 1,
                Some(_) => {}
                None if matches!(c, b' ' | b'\t') => break,
                None if matches!(c, b'"' | b'\'') => quote = Some(c),
                None if c == escape => i += 1,
                None => {}
            }
            i += 1;
        }
    }
}

/// The marker of the word at `i`, which starts with `<<`, if it is one.
fn marker(s: &[u8], i: usize, out: &mut Vec<Heredoc>) {
    let mut j = i + 2;
    if s.get(j) == Some(&b'<') {
        // A here-string, `<<<`.
        return;
    }
    let strip_tabs = s.get(j) == Some(&b'-');
    if strip_tabs {
        j += 1;
    }
    let quote = match s.get(j) {
        Some(&q @ (b'"' | b'\'')) => {
            j += 1;
            Some(q)
        }
        _ => None,
    };
    let w0 = j;
    while j < s.len() && (s[j].is_ascii_alphanumeric() || s[j] == b'_') {
        j += 1;
    }
    let closed = match quote {
        Some(q) => s.get(j) == Some(&q),
        None => true,
    };
    if j > w0 && closed {
        out.push(Heredoc {
            word: w0..j,
            strip_tabs,
        });
    }
}

/// Where the body of `h` that starts at `pos` ends: after its terminator line, or at the end of
/// the file when it has none.
fn heredoc_end(src: &[u8], mut pos: usize, h: &Heredoc) -> usize {
    let word = &src[h.word.clone()];
    while let Some(l) = line_at(src, pos) {
        let mut a = l.start;
        while h.strip_tabs && a < l.end && src[a] == b'\t' {
            a += 1;
        }
        if &src[a..l.end] == word {
            return l.next;
        }
        pos = l.next;
    }
    src.len()
}

/// The words of `src[r]`, split at spaces and tabs.
fn words(src: &[u8], r: Range<usize>) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut i = r.start;
    std::iter::from_fn(move || {
        while i < r.end && matches!(src[i], b' ' | b'\t') {
            i += 1;
        }
        if i >= r.end {
            return None;
        }
        let a = i;
        while i < r.end && !matches!(src[i], b' ' | b'\t') {
            i += 1;
        }
        Some(a..i)
    })
}

/// The `AS` name of a `FROM` line whose arguments are `args`: flags (`--platform=…`) skipped,
/// then the image, `AS` without case, the name.
fn stage_name(src: &[u8], args: Range<usize>) -> Option<Range<usize>> {
    let mut w = words(src, args).skip_while(|w| src[w.clone()].starts_with(b"--"));
    w.next()?;
    let as_word = w.next()?;
    if !src[as_word].eq_ignore_ascii_case(b"AS") {
        return None;
    }
    w.next()
}

pub(crate) fn scan(src: &[u8]) -> BuildIndex {
    let mut items = BuildIndex::default();
    let mut escape = b'\\';
    let mut leading = true;
    // The open stage, as an index into `items`, and how many stages opened.
    let mut stage: Option<usize> = None;
    let mut stages = 0;
    let mut marks = Vec::new();
    let mut pos = 0;
    while let Some(line) = line_at(src, pos) {
        pos = line.next;
        if leading {
            if let Some((key, value)) = directive(src, &line) {
                if src[key].eq_ignore_ascii_case(b"escape")
                    && let &[c @ (b'\\' | b'`')] = &src[value]
                {
                    escape = c;
                }
                continue;
            }
            leading = false;
        }
        if skipped(src, &line) {
            continue;
        }
        let t = trim(src, line.start..line.end);
        let kw_len = src[t.clone()]
            .iter()
            .take_while(|b| b.is_ascii_alphabetic())
            .count();
        let keyword = t.start..t.start + kw_len;
        // The logical line: continuation lines, the comments and blank lines among them.
        let is_heredoc_kw = [&b"RUN"[..], b"COPY", b"ADD"]
            .iter()
            .any(|k| src[keyword.clone()].eq_ignore_ascii_case(k));
        marks.clear();
        if is_heredoc_kw {
            heredocs(src, &line, escape, &mut marks);
        }
        let mut last = line;
        let mut end = line.next;
        while continues(src, &last, escape) {
            let Some(next) = line_at(src, end) else {
                break;
            };
            end = next.next;
            if skipped(src, &next) {
                continue;
            }
            if is_heredoc_kw {
                heredocs(src, &next, escape, &mut marks);
            }
            last = next;
        }
        for h in &marks {
            end = heredoc_end(src, end, h);
        }
        pos = end;
        if kw_len == 0 {
            // No keyword: text, as Docker would refuse it.
            continue;
        }
        if src[keyword.clone()].eq_ignore_ascii_case(b"FROM") {
            if let Some(open) = stage {
                items.set_end(open, line.start);
            }
            let name = stage_name(src, keyword.end..line.end);
            items.push(Item {
                kind: "stage",
                range: line.start..src.len(),
                label: name.clone().unwrap_or(line.start..line.start),
                name: match name {
                    Some(r) => Named::Source(r),
                    None => Named::Ordinal("stage", stages),
                },
                signature: Some(t.start..line.end),
            });
            stage = Some(items.len() - 1);
            stages += 1;
            continue;
        }
        items.push(Item {
            kind: "instruction",
            range: line.start..end,
            label: keyword.clone(),
            name: Named::Source(keyword),
            signature: None,
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &[u8]) -> Vec<(&'static str, String, Range<usize>)> {
        scan(src)
            .into_iter()
            .map(|i| {
                let n = match i.name {
                    Named::Source(r) => String::from_utf8_lossy(&src[r]).into_owned(),
                    Named::Ordinal(w, n) => format!("{w} {n}"),
                };
                (i.kind, n, i.range)
            })
            .collect()
    }

    #[test]
    fn heredocs_end_at_their_words_in_order_and_an_open_one_at_eof() {
        let src = b"RUN <<A cat <<-\"B\"\na\nA\n\tB\nENV x\n";
        let k = kinds(src);
        assert_eq!(k[0].1, "RUN");
        assert_eq!(k[0].2, 0..src.len() - 6);
        assert_eq!(k[1].1, "ENV");
        // `<<<` is a here-string and `<< A` no marker.
        let src = b"RUN cat <<<x << A\nENV y\n";
        assert_eq!(kinds(src).len(), 2);
        let src = b"RUN <<E\nFROM x\n";
        assert_eq!(kinds(src), [("instruction", "RUN".into(), 0..src.len())]);
    }

    #[test]
    fn stage_names_skip_flags_and_keep_their_text() {
        let src = b"FROM --platform=$X img as Build\nFROM img\nFROM img AS\n";
        let k = kinds(src);
        let names: Vec<&str> = k.iter().map(|k| k.1.as_str()).collect();
        assert_eq!(names, ["Build", "stage 1", "stage 2"]);
        assert_eq!(k[0].2, 0..32);
    }

    #[test]
    fn directives_are_leading_lines_only() {
        // After an instruction, `# escape=` is a comment.
        let src = b"FROM a\n# escape=`\nRUN x `\nENV y\n";
        assert_eq!(kinds(src).len(), 3);
        let src = b"# syntax=docker/dockerfile:1\n#escape=`\nFROM a\nRUN x `\nENV y\n";
        assert_eq!(kinds(src).len(), 2);
    }
}
