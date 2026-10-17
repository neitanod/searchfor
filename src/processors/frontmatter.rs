//! The `title` of a YAML frontmatter block, read as YAML reads it (code review r1, Codex #9):
//! the top-level key `title`, plain or quoted (code review r2, Codex #6), and its value after
//! any anchor or tag: a plain scalar ending at a ` #` comment, a single-quoted one (`''` is a quote), a
//! double-quoted one with its escapes, or a `|`/`>` block scalar, each over one or more lines.
//! The name keeps one line: every line break becomes a space, which maps to the `\n` it
//! replaces. Each piece of the name maps to its bytes in the Source, as `Name::pieces`.
use std::ops::Range;

pub type Pieces = Vec<(Range<usize>, Range<usize>)>;

/// The title of `block` (the frontmatter's text, which starts at byte `base` of the Source) and
/// its pieces; None when it has no top-level `title` key.
pub fn title(block: &str, base: usize) -> Option<(String, Pieces)> {
    let lines: Vec<(usize, &str)> = block
        .split_inclusive('\n')
        .scan(base, |at, l| {
            let start = *at;
            *at += l.len();
            Some((start, l.strip_suffix('\n').unwrap_or(l)))
        })
        .collect();
    let (i, colon) = lines
        .iter()
        .enumerate()
        .find_map(|(i, (_, l))| Some((i, title_key(l)?)))?;
    let mut name = Name::default();
    let (start, line) = lines[i];
    let skip = properties(&line[colon..]);
    let at = start + colon + skip;
    let value = &line[colon + skip..];
    if value.starts_with('*') {
        // An alias names another node: no title of its own.
        return None;
    }
    let rest = &lines[i + 1..];
    match value.chars().next() {
        Some('|') | Some('>') => name.block(value, rest),
        Some('"') => name.quoted(at + 1, &value[1..], rest, true),
        Some('\'') => name.quoted(at + 1, &value[1..], rest, false),
        // An empty value or a comment: a plain scalar may still follow on indented lines.
        None | Some('#') => name.plain(None, rest),
        Some(_) => name.plain(Some((at, value)), rest),
    }
    Some((name.text, name.pieces))
}

/// The bytes before the value of a top-level `title` key on `line` (code review r2, Codex #6):
/// the key plain, or quoted and `title` once decoded (code review r3, Codex #4), blanks before
/// the `:`, and a blank or the line's end after it. None for any other line.
fn title_key(line: &str) -> Option<usize> {
    let (key, len) = match line.as_bytes().first() {
        Some(&q @ (b'"' | b'\'')) => quoted_key(&line[1..], q == b'"').map(|(k, n)| (k, n + 1))?,
        _ if line.starts_with("title") => ("title".to_string(), "title".len()),
        _ => return None,
    };
    if key != "title" {
        return None;
    }
    let after = &line[len..];
    let blanks = after.len() - after.trim_start_matches([' ', '\t']).len();
    let colon = len + blanks;
    if !line[colon..].starts_with(':') {
        return None;
    }
    let value = &line[colon + 1..];
    (value.is_empty() || value.starts_with([' ', '\t'])).then_some(colon + 1)
}

/// A quoted key decoded (`s` starts after its opening quote) and the bytes it takes up to its
/// closing quote, included; None when it does not close on the line.
fn quoted_key(s: &str, double: bool) -> Option<(String, usize)> {
    let mut key = String::new();
    let mut k = 0;
    while let Some(c) = s[k..].chars().next() {
        match c {
            '"' if double => return Some((key, k + 1)),
            '\\' if double => {
                let (decoded, len) = escape(&s[k + 1..]);
                key.push_str(&decoded);
                k += 1 + len;
            }
            '\'' if !double && s[k + 1..].starts_with('\'') => {
                key.push('\'');
                k += 2;
            }
            '\'' if !double => return Some((key, k + 1)),
            _ => {
                key.push(c);
                k += c.len_utf8();
            }
        }
    }
    None
}

/// The bytes before the value in `s` (what follows the key's `:`): blanks, and the anchors
/// (`&name`) and tags (`!tag`, `!!str`) before it, each followed by blanks.
fn properties(s: &str) -> usize {
    let mut at = s.len() - s.trim_start_matches([' ', '\t']).len();
    while s[at..].starts_with(['&', '!']) {
        let token = s[at..].find([' ', '\t']).unwrap_or(s.len() - at);
        at += token;
        at += s[at..].len() - s[at..].trim_start_matches([' ', '\t']).len();
    }
    at
}

#[derive(Default)]
struct Name {
    text: String,
    pieces: Pieces,
}

/// `s` up to a comment (a `#` at its start or after a blank), without trailing blanks, and
/// whether a comment cut it.
fn uncommented(s: &str) -> (&str, bool) {
    let b = s.as_bytes();
    let cut =
        (0..b.len()).find(|&k| b[k] == b'#' && (k == 0 || b[k - 1] == b' ' || b[k - 1] == b'\t'));
    let s = cut.map_or(s, |k| &s[..k]);
    (s.trim_end_matches([' ', '\t']), cut.is_some())
}

fn indentation(s: &str) -> usize {
    s.len() - s.trim_start_matches(' ').len()
}

impl Name {
    /// Text that is its Source bytes, starting at byte `at`.
    fn verbatim(&mut self, s: &str, at: usize) {
        if !s.is_empty() {
            self.decoded(s, at..at + s.len());
        }
    }

    /// Text decoded from the Source range `src`.
    fn decoded(&mut self, s: &str, src: Range<usize>) {
        let t = self.text.len();
        self.text.push_str(s);
        self.pieces.push((t..self.text.len(), src));
    }

    /// A line break, shown as one space, over the `\n` at byte `at` (none at the start).
    fn space(&mut self, at: usize) {
        if !self.text.is_empty() {
            self.decoded(" ", at..at + 1);
        }
    }

    /// A plain scalar: `first` (its offset and text) on the key's line, then the indented
    /// lines that follow, until a comment or a line at the key's indentation.
    fn plain(&mut self, first: Option<(usize, &str)>, rest: &[(usize, &str)]) {
        if let Some((at, s)) = first {
            let (s, comment) = uncommented(s);
            self.verbatim(s, at);
            if comment {
                return;
            }
        }
        for &(start, line) in rest {
            let trimmed = line.trim_start_matches([' ', '\t']);
            if trimmed.is_empty() {
                continue;
            }
            if indentation(line) == 0 || trimmed.starts_with('#') {
                break;
            }
            let (s, comment) = uncommented(trimmed);
            // The `\n` before this line: the end of the previous one.
            self.space(start.saturating_sub(1));
            self.verbatim(s, start + (line.len() - trimmed.len()));
            if comment {
                break;
            }
        }
    }

    /// A quoted scalar from byte `at` (after the opening quote): `first` is the rest of the
    /// key's line, `rest` the lines after it. Double quotes decode escapes; single quotes only
    /// `''`. A line break folds into one space, and the blanks around it are dropped.
    fn quoted(&mut self, at: usize, first: &str, rest: &[(usize, &str)], double: bool) {
        let lines = std::iter::once((at, first)).chain(rest.iter().copied());
        let mut pending_break: Option<usize> = None;
        let mut join = true;
        for (start, line) in lines {
            let mut s = line;
            let mut off = start;
            if pending_break.is_some() {
                let lead = s.len() - s.trim_start_matches([' ', '\t']).len();
                s = &s[lead..];
                off += lead;
                if s.is_empty() {
                    continue;
                }
            }
            if let Some(nl) = pending_break.take()
                && join
            {
                self.space(nl);
            }
            join = true;
            // Start of the verbatim run not yet written.
            let mut run_start = 0;
            let b = s.as_bytes();
            let mut k = 0;
            let mut closed = false;
            while k < b.len() {
                let c = b[k];
                if double && c == b'"' || !double && c == b'\'' && b.get(k + 1) != Some(&b'\'') {
                    self.verbatim(&s[run_start..k], off + run_start);
                    closed = true;
                    break;
                }
                if !double && c == b'\'' {
                    self.verbatim(&s[run_start..k], off + run_start);
                    self.decoded("'", off + k..off + k + 2);
                    k += 2;
                    run_start = k;
                    continue;
                }
                if double && c == b'\\' {
                    self.verbatim(&s[run_start..k], off + run_start);
                    if k + 1 == b.len() {
                        // An escaped line break joins the lines without a space.
                        join = false;
                        k += 1;
                        run_start = k;
                        break;
                    }
                    let (decoded, len) = escape(&s[k + 1..]);
                    self.decoded(&decoded, off + k..off + k + 1 + len);
                    k += 1 + len;
                    run_start = k;
                    continue;
                }
                k += 1;
            }
            if closed {
                return;
            }
            // Trailing blanks before a line break are not part of the value.
            let run = s[run_start.min(s.len())..].trim_end_matches([' ', '\t']);
            self.verbatim(run, off + run_start);
            pending_break = Some(start + line.len());
        }
    }

    /// A `|` or `>` block scalar: the lines after the header indented more than the key, each
    /// without its indentation, joined by spaces.
    fn block(&mut self, header: &str, rest: &[(usize, &str)]) {
        let (header, _) = uncommented(header);
        let explicit = header
            .chars()
            .find_map(|c| c.to_digit(10))
            .map(|d| d as usize);
        let indent = explicit.or_else(|| {
            rest.iter()
                .find(|(_, l)| !l.trim().is_empty())
                .map(|(_, l)| indentation(l))
        });
        let Some(indent) = indent.filter(|&n| n > 0) else {
            return;
        };
        let mut last_break = None;
        for &(start, line) in rest {
            if line.trim().is_empty() {
                last_break = last_break.or(Some(start.saturating_sub(1)));
                continue;
            }
            if indentation(line) < indent {
                break;
            }
            if let Some(nl) = last_break.take() {
                self.space(nl);
            }
            self.verbatim(line[indent..].trim_end_matches([' ', '\t']), start + indent);
            last_break = Some(start + line.len());
        }
    }
}

/// The text of a double-quoted escape (`s` starts after the backslash) and the bytes it takes
/// after the backslash. A line break or tab escape is shown as a space; an unknown or broken
/// escape stays as it is.
fn escape(s: &str) -> (String, usize) {
    let Some(c) = s.chars().next() else {
        return ("\\".into(), 0);
    };
    let hex = |n: usize| {
        s.get(1..1 + n)
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .and_then(char::from_u32)
            .map(|ch| (ch.to_string(), 1 + n))
    };
    let simple = |t: &str| Some((t.to_string(), c.len_utf8()));
    let out = match c {
        '0' => simple("\0"),
        'a' => simple("\u{7}"),
        'b' => simple("\u{8}"),
        'e' => simple("\u{1b}"),
        'f' => simple("\u{c}"),
        'v' => simple("\u{b}"),
        't' | '\t' | 'n' | 'r' | 'L' | 'P' => simple(" "),
        ' ' => simple(" "),
        '"' => simple("\""),
        '/' => simple("/"),
        '\\' => simple("\\"),
        'N' => simple("\u{85}"),
        '_' => simple("\u{a0}"),
        'x' => hex(2),
        'u' => hex(4),
        'U' => hex(8),
        _ => None,
    };
    out.unwrap_or_else(|| (format!("\\{c}"), c.len_utf8()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Name as SegmentName;

    /// The title of `block` placed at byte 4 of a Source (after `---\n`), checked against it.
    fn title_of(block: &str) -> (String, SegmentName) {
        let src = format!("---\n{block}---\n");
        let (text, pieces) = title(block, 4).expect("a title");
        let name = SegmentName {
            text: text.clone(),
            pieces,
            trail: Vec::new(),
            labels: Vec::new(),
            anchor: 0,
        };
        // Every verbatim piece is its Source bytes.
        for (t, s) in &name.pieces {
            if t.len() == s.len() && &text[t.clone()] != " " {
                assert_eq!(&text[t.clone()], &src[s.clone()], "{block:?}");
            }
        }
        (text, name)
    }

    /// The Source bytes of the first hit of `needle` in the title.
    fn hit(block: &str, needle: &str) -> String {
        let src = format!("---\n{block}---\n");
        let (text, name) = title_of(block);
        let at = text.find(needle).unwrap();
        name.source_ranges(at..at + needle.len())
            .into_iter()
            .map(|r| src[r].to_string())
            .collect()
    }

    #[test]
    fn plain_title_ends_at_a_comment() {
        assert_eq!(title_of("title: Hello # comment\n").0, "Hello");
        assert_eq!(title_of("title: C# and F#\n").0, "C# and F#");
        assert_eq!(title_of("date: x\ntitle:   Spaced  \n").0, "Spaced");
        assert_eq!(hit("title: Hello # comment\n", "Hello"), "Hello");
    }

    #[test]
    fn plain_title_over_indented_lines() {
        let block = "title: First\n  second # note\nnext: x\n";
        assert_eq!(title_of(block).0, "First second");
        assert_eq!(hit(block, " "), "\n");
        assert_eq!(
            title_of("title:\n  On the next line\n").0,
            "On the next line"
        );
    }

    #[test]
    fn quoted_titles_decode() {
        assert_eq!(title_of("title: \"Deploy guide\"\n").0, "Deploy guide");
        assert_eq!(title_of("title: 'It''s here' # c\n").0, "It's here");
        assert_eq!(hit("title: 'It''s here'\n", "'"), "''");
        assert_eq!(title_of("title: \"a \\\"b\\\" c\"\n").0, "a \"b\" c");
        assert_eq!(title_of("title: \"x\\u00e9y\"\n").0, "xéy");
        assert_eq!(hit("title: \"x\\u00e9y\"\n", "é"), "\\u00e9");
        assert_eq!(title_of("title: \"tab\\there\"\n").0, "tab here");
        assert_eq!(title_of("title: \"two\n  lines\"\n").0, "two lines");
        assert_eq!(title_of("title: \"joined\\\n  word\"\n").0, "joinedword");
        assert_eq!(title_of("title: 'two\n  lines'\n").0, "two lines");
    }

    #[test]
    fn block_scalars_join_their_lines() {
        let block = "title: |\n  Hello\n  World\nother: x\n";
        assert_eq!(title_of(block).0, "Hello World");
        assert_eq!(hit(block, "World"), "World");
        assert_eq!(hit(block, " "), "\n");
        assert_eq!(title_of("title: >-\n  Folded\n\n  text\n").0, "Folded text");
        assert_eq!(title_of("title: |2 # c\n    indented\n").0, "  indented");
        assert_eq!(title_of("title: |\nnext: x\n").0, "");
    }

    /// Code review r2, Codex #6: the YAML key `title`, quoted or not, with blanks before `:`.
    #[test]
    fn the_title_key_as_yaml_reads_it() {
        assert_eq!(title_of("\"title\": Hello\n").0, "Hello");
        assert_eq!(hit("\"title\": Hello\n", "Hello"), "Hello");
        assert_eq!(title_of("'title': Hello\n").0, "Hello");
        assert_eq!(title_of("title : Hello\n").0, "Hello");
        assert_eq!(title_of("title\t:\tHello\n").0, "Hello");
        assert_eq!(title_of("\"title\" : 'Quoted'\n").0, "Quoted");
        assert!(title("titles: x\n", 0).is_none());
        assert!(title("title2: x\n", 0).is_none());
        assert!(title("title x: y\n", 0).is_none());
        assert!(title("\"titles\": x\n", 0).is_none());
    }

    /// Code review r3, Codex #4: a quoted key is compared once decoded.
    #[test]
    fn a_quoted_title_key_is_decoded() {
        assert_eq!(title_of("\"ti\\u0074le\": Hello\n").0, "Hello");
        assert_eq!(hit("\"ti\\u0074le\": Hello\n", "Hello"), "Hello");
        assert_eq!(title_of("\"\\x74itle\" : Hi\n").0, "Hi");
        assert_eq!(title_of("'ti''tle': x\ntitle: Real\n").0, "Real");
        assert!(title("\"ti\\u0074les\": x\n", 0).is_none());
        assert!(title("\"title: x\n", 0).is_none());
        assert!(title("'title: x\n", 0).is_none());
    }

    /// Code review r2, Codex #6: an anchor or a tag before the value is not part of it.
    #[test]
    fn anchors_and_tags_are_not_part_of_the_title() {
        assert_eq!(title_of("title: &mytitle Hello\n").0, "Hello");
        assert_eq!(hit("title: &mytitle Hello\n", "Hello"), "Hello");
        assert_eq!(title_of("title: !!str Hello\n").0, "Hello");
        assert_eq!(title_of("title: !!str &a Hello # c\n").0, "Hello");
        assert_eq!(title_of("title: &a \"Quoted\"\n").0, "Quoted");
        assert_eq!(hit("title: &a \"Quoted\"\n", "Quoted"), "Quoted");
        assert_eq!(title_of("title: &a |\n  Block\n").0, "Block");
        assert_eq!(title_of("title: &a\n  Next line\n").0, "Next line");
        // An alias names another node: no title of its own.
        assert!(title("title: *other\n", 0).is_none());
    }

    #[test]
    fn only_a_top_level_title() {
        assert!(title("  title: nested\n", 0).is_none());
        assert!(title("subtitle: x\n", 0).is_none());
    }
}
