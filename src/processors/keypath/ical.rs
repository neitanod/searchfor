//! iCalendar (RFC 5545) and vCard (RFC 6350), which share the content-line syntax (M6 spec
//! §10.2). `BEGIN:X` … `END:X` is a `component` Segment named `X` and the value of its
//! SUMMARY, FN or TZID, wherever in the component that property comes, or else `X[N]` among
//! its siblings of that `X` when there are two or more, `X` alone otherwise. A single VCALENDAR around the whole file is hidden. A property is a
//! `key` Segment named without its parameters, over its folded lines. An `END` closes the
//! innermost open component of its name and those inside it; an `END` with none open is text;
//! components open at the end close there.
use super::lines::{self, Builder, Kind, Lines, NameSrc};
use crate::text::CutName;
use std::collections::HashMap;
use std::ops::Range;

pub(crate) fn prepare<'a>(
    input: crate::model::Input<'a>,
    params: &'a ::toml::Table,
) -> anyhow::Result<Box<dyn crate::processors::Prepared + 'a>> {
    lines::prepare(input, params, scan)
}

pub(crate) fn outline(
    src: &[u8],
    params: &::toml::Table,
    anchors: Option<&[usize]>,
) -> anyhow::Result<crate::trail::Outline> {
    lines::outline(src, params, anchors, scan)
}

/// A component open in the scan.
struct Open {
    /// Its `X`, uppercased.
    x: Vec<u8>,
    /// Its Segment; None past `MAX_DEPTH`.
    entry: Option<usize>,
    /// The Segments open outside it.
    depth: usize,
    /// Whether its name already took a SUMMARY, FN or TZID.
    named: bool,
    /// How many children of each `X` it has opened so far.
    children: Option<HashMap<Vec<u8>, u32>>,
    /// Its place among the components opened, which its children's ordinals are counted by.
    serial: u64,
}

/// A content line: its name, and where its value starts. None for any other line.
fn content(src: &[u8], line: Range<usize>) -> Option<(Range<usize>, usize)> {
    let mut i = line.start;
    while i < line.end && (src[i].is_ascii_alphanumeric() || src[i] == b'-') {
        i += 1;
    }
    if i == line.start || i >= line.end || !matches!(src[i], b';' | b':') {
        return None;
    }
    let name = line.start..i;
    let mut quoted = false;
    while i < line.end {
        match src[i] {
            b'"' => quoted = !quoted,
            b':' if !quoted => return Some((name, i + 1)),
            _ => {}
        }
        i += 1;
    }
    None
}

fn upper(src: &[u8], r: Range<usize>) -> Vec<u8> {
    src[r].to_ascii_uppercase()
}

fn scan(src: &[u8], b: &mut Builder) {
    let mut stack: Vec<Open> = Vec::new();
    // Of the components open, how many of each `X`: an END finds its component, or that it
    // has none, without walking the stack.
    let mut open_x: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut top: HashMap<Vec<u8>, u32> = HashMap::new();
    let mut tops: Vec<usize> = Vec::new();
    // Each component's Segment, its parent's serial (`u64::MAX` at the top) and its `X`: one
    // alone of its kind among its siblings takes no ordinal, which only the end of the scan
    // knows.
    let mut kinds: Vec<(usize, u64, Vec<u8>)> = Vec::new();
    let mut serial = 0u64;
    let mut lines = Lines::new(src).peekable();
    while let Some((start, end, mut next)) = lines.next() {
        // The folded lines that continue this one.
        let mut last_end = end;
        while let Some(&(s, e, n)) = lines.peek() {
            if s < e && matches!(src[s], b' ' | b'\t') {
                last_end = e;
                next = n;
                lines.next();
            } else {
                break;
            }
        }
        // Parameters may fold before the `:`.
        let Some((name, value)) = content(src, start..last_end) else {
            continue;
        };
        let word = &src[name.clone()];
        let value_end = if value <= end {
            end
        } else {
            Lines::starting(src, value)
                .next()
                .map_or(last_end, |(_, e, _)| e.min(last_end))
        };
        let x = lines::trim(src, value..value_end);
        if word.eq_ignore_ascii_case(b"BEGIN") && !x.is_empty() {
            let key = upper(src, x.clone());
            let depth = b.depth();
            let siblings = match stack.last_mut() {
                Some(p) => p.children.get_or_insert_with(HashMap::new),
                None => &mut top,
            };
            let n = siblings.entry(key.clone()).or_insert(0);
            *n += 1;
            let ordinal = *n;
            let entry = b.open(
                start,
                start..end,
                Kind::Component,
                NameSrc::Component {
                    x: x.clone(),
                    value: None,
                    ordinal,
                },
            );
            if stack.is_empty() {
                tops.extend(entry);
                if entry.is_none() {
                    tops.push(usize::MAX);
                }
            }
            *open_x.entry(key.clone()).or_insert(0) += 1;
            if let Some(i) = entry {
                let parent = stack.last().map_or(u64::MAX, |p| p.serial);
                kinds.push((i, parent, key.clone()));
            }
            serial += 1;
            stack.push(Open {
                x: key,
                entry,
                depth,
                named: false,
                children: None,
                serial,
            });
            continue;
        }
        if word.eq_ignore_ascii_case(b"END") {
            let key = upper(src, x);
            if open_x.get(&key).is_some_and(|&n| n > 0) {
                while let Some(o) = stack.pop() {
                    *open_x.get_mut(&o.x).expect("counted") -= 1;
                    if o.x == key {
                        b.close_to(o.depth, next);
                        break;
                    }
                }
            }
            continue;
        }
        let depth = b.depth();
        b.open(start, name.clone(), Kind::Key, NameSrc::Verbatim(name));
        b.close_to(depth, next);
        if (word.eq_ignore_ascii_case(b"SUMMARY")
            || word.eq_ignore_ascii_case(b"FN")
            || word.eq_ignore_ascii_case(b"TZID"))
            && value < last_end
            && let Some(o) = stack.last_mut()
            && !o.named
        {
            o.named = true;
            if let Some(i) = o.entry
                && let NameSrc::Component { value: v, .. } = &mut b.entry_mut(i).name
            {
                *v = Some(value..last_end);
            }
        }
    }
    let mut count: HashMap<(u64, &[u8]), u32> = HashMap::new();
    for (_, parent, x) in &kinds {
        *count.entry((*parent, x.as_slice())).or_insert(0) += 1;
    }
    for (i, parent, x) in &kinds {
        if count[&(*parent, x.as_slice())] == 1
            && let NameSrc::Component { ordinal, .. } = &mut b.entry_mut(*i).name
        {
            *ordinal = 0;
        }
    }
    // A single VCALENDAR around the whole file is hidden, as HTML's `body`.
    if let [only] = tops[..]
        && only != usize::MAX
    {
        let e = b.entry_mut(only);
        if let NameSrc::Component { x, .. } = &e.name
            && src[x.clone()].eq_ignore_ascii_case(b"VCALENDAR")
        {
            e.hidden = true;
        }
    }
}

/// The name of a component into `n`: `X`, then one space and the value `value` unfolded, one
/// piece per physical line, or else `[ordinal]`, which 0 leaves out (a component alone of its
/// kind among its siblings). The space and the ordinal are added words: they map to no Source
/// byte (M6 spec §3 rule 4).
pub(super) fn push_component(
    src: &[u8],
    x: &Range<usize>,
    value: Option<&Range<usize>>,
    ordinal: u32,
    n: &mut CutName,
) {
    n.push_lossy(src, x.clone());
    let Some(v) = value else {
        if ordinal > 0 {
            n.push_str(&format!("[{ordinal}]"));
        }
        return;
    };
    n.push_str(" ");
    let mut first = true;
    for (s, e, _) in Lines::starting(src, v.start) {
        if s >= v.end || n.full() {
            break;
        }
        // A folded line drops the space or tab that folds it.
        let from = if first { s } else { s + 1 };
        first = false;
        let to = e.min(v.end);
        if from < to {
            n.push_lossy(src, from..to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::lines::tests::cursor_is_model;
    use super::super::tests::needles;

    #[test]
    fn components_properties_and_late_names() {
        let src = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART;TZID=\"a:b\":needle\nBEGIN:VALARM\nX:needle\nEND:VALARM\nSUMMARY:Lunch wi\n th\n\t friends\nEND:vevent\nEND:VCALENDAR\n";
        assert_eq!(
            needles(src, "ical"),
            vec![
                vec!["VEVENT Lunch with friends", "DTSTART"],
                vec!["VEVENT Lunch with friends", "VALARM", "X"]
            ]
        );
        // Two top-level calendars are both shown; a stray END is text.
        let src =
            b"BEGIN:VCALENDAR\nEND:X\nA:needle\nEND:VCALENDAR\nBEGIN:VCALENDAR\nEND:VCALENDAR\n";
        assert_eq!(needles(src, "ical"), vec![vec!["VCALENDAR[1]", "A"]]);
    }

    #[test]
    fn a_match_in_the_naming_value_stays_inside() {
        // The label is the BEGIN line: a Match in SUMMARY or FN is inside the component,
        // under its key (orchestrator's decision 1 on code round 1); one in BEGIN is not, and
        // one in END is.
        let src =
            b"BEGIN:VEVENT\nSUMMARY:needle sync\nEND:VEVENT\nBEGIN:needle\nFN:x\nEND:needle\n";
        assert_eq!(
            needles(src, "ical"),
            vec![
                vec!["VEVENT needle sync", "SUMMARY"],
                vec![],
                vec!["needle x"]
            ]
        );
        cursor_is_model("ical", src);
    }

    #[test]
    fn an_ordinal_only_among_two_or_more_of_a_kind() {
        // A component alone of its kind among its siblings is named `X`, as markup's elements
        // (code round 1, Fable #42); two or more of a kind are `X[N]`.
        let src = b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nBEGIN:VALARM\nX:needle\nEND:VALARM\nEND:VEVENT\nBEGIN:VTODO\nX:needle\nEND:VTODO\nBEGIN:VTODO\nX:needle\nEND:VTODO\nEND:VCALENDAR\n";
        assert_eq!(
            needles(src, "ical"),
            vec![
                vec!["VEVENT", "VALARM", "X"],
                vec!["VTODO[1]", "X"],
                vec!["VTODO[2]", "X"]
            ]
        );
        cursor_is_model("ical", src);
    }

    #[test]
    fn parameters_folded_before_the_colon() {
        // A property whose parameters fold before its `:` is a key (Fable #43), and so is a
        // BEGIN or a SUMMARY folded the same way.
        let src = b"BEGIN:VEVENT\nDTSTART;TZID=Europe/\n Madrid:needle\nSUMMARY;LANGUAGE=\n en:Team\nEND:VEVENT\n";
        assert_eq!(needles(src, "ical"), vec![vec!["VEVENT Team", "DTSTART"]]);
        cursor_is_model("ical", src);
    }

    #[test]
    fn deep_components_stop_opening_segments() {
        let n = 100_000;
        let mut src = "BEGIN:X\n".repeat(n);
        src.push_str("A:needle\n");
        src.push_str(&"END:X\n".repeat(n));
        let t = &needles(src.as_bytes(), "ical")[0];
        assert_eq!(t.len(), super::lines::MAX_DEPTH);
    }

    #[test]
    fn the_cursor_gives_the_model_trails_at_every_byte() {
        for src in [
            &b"BEGIN:VCALENDAR\nBEGIN:VEVENT\nDESCRIPTION:Bring the needle\n folded needle\nSUMMARY:Team sync\nEND:VEVENT\nEND:VCALENDAR\n"[..],
            b"BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDESCRIPTION:needle\r\nEND:UNKNOWN\r\nSUMMARY:One\r\nEND:VCALENDAR\r\nneedle\r\n",
            b"BEGIN:VCARD\nEMAIL:needle\nEND:VCARD\nBEGIN:VCARD\nEMAIL:needle\nEND:VCARD\n",
            b"BEGIN:\nEND:\n:x\n x\nBEGIN:A",
            b"",
        ] {
            cursor_is_model("ical", src);
        }
    }
}
