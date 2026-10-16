//! The `title` of a YAML frontmatter block, read as YAML reads it (code review r1, Codex #9):
//! a plain scalar ending at a ` #` comment, a single-quoted one (`''` is a quote), a
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
    let i = lines.iter().position(|(_, l)| l.starts_with("title:"))?;
    let mut name = Name::default();
    let (start, line) = lines[i];
    let value = &line["title:".len()..];
    let blank = value.len() - value.trim_start_matches([' ', '\t']).len();
    let at = start + "title:".len() + blank;
    let value = &value[blank..];
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

    #[test]
    fn only_a_top_level_title() {
        assert!(title("  title: nested\n", 0).is_none());
        assert!(title("subtitle: x\n", 0).is_none());
    }
}
