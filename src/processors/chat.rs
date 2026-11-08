//! chat: WhatsApp's text export (M6 spec §10.4). A message starts at a line that begins with a
//! date and a time in one of the export's forms and runs up to the next such line; consecutive
//! messages of one date, as written, form a day. Days are the top-level units: the cursor reads
//! them as the Matches reach them, never past the day of the last Match (plan D8, spec §11).
use super::{Capability, Cursor, Prepared, Processor};
use crate::model::{Input, Name, Segment, SharedTrail, Trail};
use std::ops::Range;
use std::rc::Rc;

pub struct Chat;

const LRM: &[u8] = "\u{200e}".as_bytes();
const NNBSP: &[u8] = "\u{202f}".as_bytes();
const NBSP: &[u8] = "\u{a0}".as_bytes();

/// What opens a message, in ranges of its line: the date and the time as written, the sender
/// (none for a system message), and where the bytes that name it end.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Opener {
    date: Range<usize>,
    time: Range<usize>,
    sender: Option<Range<usize>>,
    label_end: usize,
}

fn digits(s: &[u8], i: &mut usize, min: usize, max: usize) -> bool {
    let s0 = *i;
    while *i < s.len() && s[*i].is_ascii_digit() && *i - s0 < max {
        *i += 1;
    }
    *i - s0 >= min
}

/// The opener at the start of `line`, which holds no line terminator: Android's
/// `31/12/23, 21:41 - ` and `12/31/23, 9:41 PM - `, iOS's `[31/12/23, 21:41:05] `, a U+200E
/// before it and a U+202F (or a space) before AM/PM allowed. None when the line opens no
/// message.
fn opener(line: &[u8]) -> Option<Opener> {
    let mut i = 0;
    if line.starts_with(LRM) {
        i += LRM.len();
    }
    let bracket = line.get(i) == Some(&b'[');
    if bracket {
        i += 1;
    }
    let d0 = i;
    for (k, (min, max)) in [(1, 2), (1, 2), (2, 4)].into_iter().enumerate() {
        if !digits(line, &mut i, min, max) {
            return None;
        }
        if k < 2 {
            if line.get(i) != Some(&b'/') {
                return None;
            }
            i += 1;
        }
    }
    let date = d0..i;
    if !line[i..].starts_with(b", ") {
        return None;
    }
    i += 2;
    let t0 = i;
    if !digits(line, &mut i, 1, 2) {
        return None;
    }
    for k in 0..2 {
        if line.get(i) != Some(&b':') {
            if k == 0 {
                return None;
            }
            break;
        }
        i += 1;
        if !digits(line, &mut i, 2, 2) {
            return None;
        }
    }
    // AM/PM after a space, a U+202F or a U+00A0.
    for sp in [&b" "[..], NNBSP, NBSP] {
        let rest = &line[i..];
        if let Some(r) = rest.strip_prefix(sp)
            && r.len() >= 2
            && matches!(r[0], b'A' | b'a' | b'P' | b'p')
            && matches!(r[1], b'M' | b'm')
        {
            i += sp.len() + 2;
            break;
        }
    }
    let time = t0..i;
    if bracket {
        if !line[i..].starts_with(b"] ") {
            return None;
        }
        i += 2;
    } else {
        if !line[i..].starts_with(b" - ") {
            return None;
        }
        i += 3;
    }
    let text = i;
    let sender = memchr::memmem::find(&line[text..], b": ")
        .filter(|&k| k > 0)
        .map(|k| text..text + k);
    let label_end = sender.as_ref().map_or(time.end, |s| s.end);
    Some(Opener {
        date,
        time,
        sender,
        label_end,
    })
}

/// One message, in absolute offsets: its bytes, the bytes that name it, its time and sender,
/// and its lines. A few dozen bytes (spec §11).
#[derive(Debug, Clone)]
struct Message {
    range: Range<usize>,
    label: Range<usize>,
    time: Range<usize>,
    sender: Option<Range<usize>>,
    lines: (u64, u64),
}

/// One day: its bytes, its date as the first message writes it, where that message's name
/// ends, and its lines. Its messages are read again when a Match falls in it, so a day of
/// millions of them costs no more than one (review r1, Fable #15).
#[derive(Debug, Clone)]
struct Day {
    range: Range<usize>,
    date: Range<usize>,
    /// The end of the first message's label.
    first_label_end: usize,
    ordinal: usize,
    lines: (u64, u64),
}

/// Reads lines from `pos` up to `end` for the openers among them.
#[derive(Debug, Clone)]
struct Reader<'s> {
    src: &'s [u8],
    pos: usize,
    end: usize,
    /// The number of the line read last.
    line: u64,
}

impl Reader<'_> {
    /// Reads lines up to the next opener: it, and the number of the last line before it.
    fn next_opener(&mut self) -> (Option<(usize, u64, Opener)>, u64) {
        while self.pos < self.end {
            let start = self.pos;
            let (content, next) = line_at(self.src, start);
            self.pos = next;
            self.line += 1;
            if let Some(o) = opener(&self.src[content]) {
                return (Some((start, self.line, o)), self.line - 1);
            }
        }
        (None, self.line)
    }
}

/// The messages of a run of lines in order, with their ordinals: the opener read last and
/// the reader after it.
struct Messages<'s> {
    reader: Reader<'s>,
    /// The next message's opener, read already: (line start, line number, opener).
    pending: Option<(usize, u64, Opener)>,
    count: usize,
}

impl<'s> Messages<'s> {
    /// The messages of `day`.
    fn of(src: &'s [u8], day: &Day) -> Self {
        let mut reader = Reader {
            src,
            pos: day.range.start,
            end: day.range.end,
            line: day.lines.0 - 1,
        };
        let pending = reader.next_opener().0;
        Messages {
            reader,
            pending,
            count: 0,
        }
    }
}

impl Iterator for Messages<'_> {
    type Item = (usize, Message);

    fn next(&mut self) -> Option<(usize, Message)> {
        let (start, line, o) = self.pending.take()?;
        let (next, last_line) = self.reader.next_opener();
        let end = next.as_ref().map_or(self.reader.end, |n| n.0);
        self.pending = next;
        let m = Message {
            range: start..end,
            label: start..start + o.label_end,
            time: shift(&o.time, start),
            sender: o.sender.as_ref().map(|s| shift(s, start)),
            lines: (line, last_line.max(line)),
        };
        self.count += 1;
        Some((self.count - 1, m))
    }
}

/// The days of an export in order, read one line at a time as they are asked for. A file whose
/// first line, after a BOM and a U+200E, opens no message is no export: it has no day (spec
/// §10.4, Failure).
struct Days<'s> {
    reader: Reader<'s>,
    ordinal: usize,
    /// The next day's first opener, read already: (line start, line number, opener).
    pending: Option<(usize, u64, Opener)>,
}

impl<'s> Days<'s> {
    fn new(src: &'s [u8]) -> Self {
        let start = if src.starts_with(b"\xEF\xBB\xBF") {
            3
        } else {
            0
        };
        let mut d = Days {
            reader: Reader {
                src,
                pos: src.len(),
                end: src.len(),
                line: 0,
            },
            ordinal: 0,
            pending: None,
        };
        let (content, next) = line_at(src, start);
        if let Some(o) = opener(&src[content]) {
            d.pending = Some((start, 1, o));
            d.reader.pos = next;
            d.reader.line = 1;
        }
        d
    }
}

/// The line at `pos`: its content without `\n` or `\r\n`, and where the next one starts.
fn line_at(src: &[u8], pos: usize) -> (Range<usize>, usize) {
    let nl = memchr::memchr(b'\n', &src[pos..]).map_or(src.len(), |k| pos + k);
    let next = (nl + 1).min(src.len());
    let end = if nl < src.len() && nl > pos && src[nl - 1] == b'\r' {
        nl - 1
    } else {
        nl
    };
    (pos..end, next)
}

fn shift(r: &Range<usize>, by: usize) -> Range<usize> {
    r.start + by..r.end + by
}

impl Iterator for Days<'_> {
    type Item = Day;

    fn next(&mut self) -> Option<Day> {
        let (start, line, o) = self.pending.take()?;
        let src = self.reader.src;
        let mut day = Day {
            range: start..start,
            date: shift(&o.date, start),
            first_label_end: start + o.label_end,
            ordinal: self.ordinal,
            lines: (line, line),
        };
        self.ordinal += 1;
        loop {
            let (next, last_line) = self.reader.next_opener();
            day.range.end = next.as_ref().map_or(src.len(), |n| n.0);
            day.lines.1 = last_line.max(day.lines.1);
            match next {
                Some((s, _, n)) if src[shift(&n.date, s)] == src[day.date.clone()] => {}
                Some(p) => {
                    self.pending = Some(p);
                    return Some(day);
                }
                None => return Some(day),
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Params {
    name_chars: usize,
}

impl Params {
    fn of(t: &toml::Table) -> Self {
        let name_chars = t
            .get("name_chars")
            .and_then(|v| v.as_integer())
            .map_or(40, |n| n.max(0) as usize);
        Params { name_chars }
    }
}

impl Day {
    fn name(&self, src: &[u8], max: usize) -> (String, crate::text::Pieces) {
        let mut n = crate::text::CutName::new(max);
        n.push_lossy(src, self.date.clone());
        n.finish()
    }

    fn segment(&self, src: &[u8], max: usize) -> Segment {
        Segment {
            kind: "day".into(),
            name: self.name(src, max).0,
            line: self.lines.0,
            end_line: self.lines.1,
            level: None,
            signature: None,
        }
    }
}

impl Message {
    /// Its time as written and its sender (`21:41 Ana`), or its time alone; the space between
    /// them is text of the name only.
    fn name(&self, src: &[u8], max: usize) -> (String, crate::text::Pieces) {
        let mut n = crate::text::CutName::new(max);
        n.push_lossy(src, self.time.clone());
        if let Some(s) = &self.sender {
            n.push_str(" ");
            n.push_lossy(src, s.clone());
        }
        n.finish()
    }

    fn segment(&self, src: &[u8], max: usize) -> Segment {
        Segment {
            kind: "message".into(),
            name: self.name(src, max).0,
            line: self.lines.0,
            end_line: self.lines.1,
            level: None,
            signature: None,
        }
    }
}

/// A chat export prepared for its cursor: nothing is read ahead of time.
struct ChatPrepared<'a> {
    src: &'a [u8],
    p: Params,
}

impl Prepared for ChatPrepared<'_> {
    fn cursor(&self) -> Box<dyn Cursor + '_> {
        Box::new(ChatCursor {
            src: self.src,
            p: self.p,
            days: Days::new(self.src),
            day: None,
            messages: None,
            message: None,
            at: 0,
            last: None,
        })
    }

    /// Every day's and message's name: the whole file, the one path that reads it all. A Match
    /// in a name is outside the Segment it names (rule 5), the day's date on its first line
    /// included.
    fn names(&self) -> anyhow::Result<Vec<Name>> {
        let (src, max) = (self.src, self.p.name_chars);
        let mut out = Vec::new();
        for day in Days::new(src) {
            let ds = day.segment(src, max);
            let day_label = day.date.start..day.first_label_end.min(day.date.end);
            let (text, pieces) = day.name(src, max);
            out.push(Name {
                text,
                pieces,
                trail: vec![ds.clone()],
                labels: vec![day_label.clone()],
                anchor: day.date.start,
            });
            for (_, m) in Messages::of(src, &day) {
                let (text, pieces) = m.name(src, max);
                out.push(Name {
                    text,
                    pieces,
                    trail: vec![ds.clone(), m.segment(src, max)],
                    labels: vec![day_label.clone(), m.label.clone()],
                    anchor: m.time.start,
                });
            }
        }
        Ok(out)
    }

    fn streams(&self) -> bool {
        true
    }
}

/// The Trails of an export in file order: the day that holds or follows the last offset, and
/// the Trail last given by (day, message, which labels hold the offset).
struct ChatCursor<'s> {
    src: &'s [u8],
    p: Params,
    days: Days<'s>,
    day: Option<Day>,
    /// The messages of `day`, read as far as `message`, the one the last offset is in.
    messages: Option<Messages<'s>>,
    message: Option<(usize, Message)>,
    at: usize,
    #[allow(clippy::type_complexity)]
    last: Option<((usize, usize, bool, bool), SharedTrail)>,
}

impl Cursor for ChatCursor<'_> {
    fn trail_at(&mut self, byte: usize) -> anyhow::Result<SharedTrail> {
        if byte < self.at {
            self.days = Days::new(self.src);
            self.day = None;
        }
        self.at = byte;
        while self.day.as_ref().is_none_or(|d| d.range.end <= byte) {
            self.day = self.days.next();
            self.messages = None;
            self.message = None;
            if self.day.is_none() {
                break;
            }
        }
        let (src, max) = (self.src, self.p.name_chars);
        let found = match self.day.as_ref().filter(|d| d.range.start <= byte) {
            Some(d) => {
                if self
                    .message
                    .as_ref()
                    .is_some_and(|(_, m)| byte < m.range.start)
                {
                    self.messages = None;
                    self.message = None;
                }
                let messages = self.messages.get_or_insert_with(|| Messages::of(src, d));
                while self
                    .message
                    .as_ref()
                    .is_none_or(|(_, m)| m.range.end <= byte)
                {
                    match messages.next() {
                        Some(m) => self.message = Some(m),
                        None => break,
                    }
                }
                self.message.as_ref().map(|(k, m)| (d, *k, m))
            }
            None => None,
        };
        let key = match found {
            Some((d, k, m)) => {
                let in_day = k == 0 && d.date.contains(&byte);
                (d.ordinal, k, !in_day, !m.label.contains(&byte))
            }
            None => (usize::MAX, 0, false, false),
        };
        if let Some((k, t)) = &self.last
            && *k == key
        {
            return Ok(t.clone());
        }
        let mut trail = Vec::new();
        if let Some((d, _, m)) = found {
            if key.2 {
                trail.push(d.segment(src, max));
            }
            if key.3 {
                trail.push(m.segment(src, max));
            }
        }
        let t: SharedTrail = Rc::from(trail);
        self.last = Some((key, t.clone()));
        Ok(t)
    }

    fn open_from(&mut self) -> Option<usize> {
        Some(
            self.day
                .as_ref()
                .filter(|d| d.range.contains(&self.at))
                .map_or(self.at, |d| d.range.start),
        )
    }
}

impl Processor for Chat {
    fn trails(
        &self,
        input: &Input,
        params: &toml::Table,
        anchors: &[usize],
    ) -> anyhow::Result<Vec<Trail>> {
        let prepared = self.prepare(*input, params)?;
        let mut cursor = prepared.cursor();
        let mut order: Vec<usize> = (0..anchors.len()).collect();
        order.sort_by_key(|&i| anchors[i]);
        let mut out = vec![Vec::new(); anchors.len()];
        for i in order {
            out[i] = cursor.trail_at(anchors[i])?.to_vec();
        }
        Ok(out)
    }

    fn names(&self, input: &Input, params: &toml::Table) -> anyhow::Result<Vec<Name>> {
        self.prepare(*input, params)?.names()
    }

    fn prepare<'a>(
        &'a self,
        input: Input<'a>,
        params: &'a toml::Table,
    ) -> anyhow::Result<Box<dyn Prepared + 'a>> {
        Ok(Box::new(ChatPrepared {
            src: input.bytes,
            p: Params::of(params),
        }))
    }

    fn capability(&self, _: &toml::Table) -> Capability {
        Capability::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn openers_of_android_and_ios() {
        let o = opener(b"31/12/23, 21:41 - Ana: hi: there").unwrap();
        assert_eq!((o.date, o.time, o.sender), (0..8, 10..15, Some(18..21)));
        let o = opener("12/31/23, 9:41\u{202f}PM - Ana: x".as_bytes()).unwrap();
        assert_eq!(o.time, 10..19);
        let o = opener(b"12/31/23, 9:41 pm - system").unwrap();
        assert_eq!((o.time, o.sender, o.label_end), (10..17, None, 17));
        let o = opener("\u{200e}[31/12/23, 21:41:05] Ana: x".as_bytes()).unwrap();
        assert_eq!((o.date, o.time, o.sender), (4..12, 14..22, Some(24..27)));
        for no in [
            &b"31/12/23 21:41 - Ana: x"[..],
            b"31/12/23, 21:41 Ana: x",
            b"99/no date",
            b"31.12.23, 21:41 - Ana: x",
            b"",
        ] {
            assert!(opener(no).is_none(), "{}", String::from_utf8_lossy(no));
        }
    }

    #[test]
    fn days_group_consecutive_dates_and_keep_reappearing_ones_apart() {
        let src = b"1/1/24, 9:00 - A: x\nmore\n1/1/24, 9:01 - B: y\n2/1/24, 9:00 - A: z\n1/1/24, 9:02 - A: w\n";
        let days: Vec<_> = Days::new(src).collect();
        assert_eq!(days.len(), 3);
        let messages: Vec<_> = Messages::of(src, &days[0]).collect();
        assert_eq!(messages.len(), 2);
        assert_eq!(days[0].lines, (1, 3));
        assert_eq!(messages[0].1.lines, (1, 2));
        assert_eq!(messages[1].1.lines, (3, 3));
        assert_eq!(days[2].lines, (5, 5));
        assert!(
            Days::new(b"no export\n1/1/24, 9:00 - A: x\n")
                .next()
                .is_none()
        );
    }

    #[test]
    fn the_cursor_reads_only_up_to_the_day_of_the_last_offset() {
        let src = b"1/1/24, 9:00 - A: needle\n1/1/24, 9:01 - B: x\n2/1/24, 9:00 - B: y\n";
        let input = Input {
            bytes: src,
            path: Path::new("x"),
            format: "whatsapp",
            disk: None,
        };
        let p = toml::Table::new();
        let prepared = Chat.prepare(input, &p).unwrap();
        let mut c = prepared.cursor();
        let t = c.trail_at(18).unwrap();
        let names: Vec<_> = t.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["1/1/24", "9:00 A"]);
        assert!(Rc::ptr_eq(&t, &c.trail_at(19).unwrap()));
        assert_eq!(c.open_from(), Some(0));
        // A Match in the first opener's date is outside its day and its message.
        assert!(c.trail_at(0).unwrap().is_empty());
        // A later message's date is inside its day only.
        assert_eq!(c.trail_at(26).unwrap().len(), 1);
    }
}
