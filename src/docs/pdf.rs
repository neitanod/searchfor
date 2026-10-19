//! PDF (spec §5.8): `3 Results › 3.2 Costs › page 42`. lopdf reads the file, its fonts and its
//! content streams; the text is laid out here, from the positions the content streams give
//! each string (M4 plan, D13): a string lower or higher than the last one by half its size starts
//! a line, and one further right than the last one ended by 0.15 of its size gets a space before
//! it, as does a TJ kerning of more than 0.1 em. The bookmarks are chapters, each starting at the
//! line of the page it points to that starts with its title, or at the top of that page (D14).
use super::{Builder, DocError};
use lopdf::{DecompressError, Dictionary, Document, Encoding, Object, ObjectId, Stream};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// The most bytes one stream may decompress to (64 MiB): a page, Form, font or object stream
/// past it makes the document fail, as a decompression bomb (D17).
const MAX_STREAM: usize = 64 << 20;
/// The most bytes all the page, Form and font streams of a document may decompress to, together
/// (1 GiB): many pages sharing one large stream are a bomb too (D17).
const MAX_INFLATED: usize = 1 << 30;
/// The most work the content streams of a document may do, plus `PAGE_WORK` for each of its
/// pages: each operation a page or a Form runs spends one, and each Form drawn `FORM_DRAW`
/// more. A small Form drawn by Forms drawn by Forms, or a page of millions of operations, is a
/// bomb of work (D17; M4 plan r2, Fable #1 and #2: 10 000 pages each drawing five Forms of 200
/// operations fit).
const MAX_WORK: usize = 8_000_000;
const PAGE_WORK: usize = 20_000;
/// The pages that add `PAGE_WORK`: past them a document has the work of 10 000 pages, which
/// takes about 8 s at worst (M4 plan r3, Fable #4).
const MAX_PAGES_WORK: usize = 10_000;
/// The most bytes and tokens the object streams of a document may hold, together: lopdf
/// parses them whole when it loads the file, about 128 bytes of memory for each token (D17;
/// M4 plan r2, Fable #1).
const MAX_OBJSTM_BYTES: usize = 64 << 20;
const MAX_OBJSTM_TOKENS: usize = 2_000_000;
/// The work of drawing a Form, before its operations run.
const FORM_DRAW: usize = 16;
/// Form XObjects nested deeper than this are not read.
const MAX_FORM_DEPTH: usize = 8;
/// At most this many bookmarks are read.
const MAX_BOOKMARKS: usize = 100_000;

pub fn extract(bytes: &[u8], b: &mut Builder) -> Result<(), DocError> {
    let options = lopdf::LoadOptions {
        max_decompressed_size: Some(MAX_STREAM),
        filter: Some(objstm),
        ..Default::default()
    };
    OBJSTM.with(|c| c.set(ObjStmBudget::default()));
    let loaded = Document::load_mem_with_options(bytes, options);
    if let Some(e) = OBJSTM.with(|c| c.get()).refused {
        return Err(e.error());
    }
    let mut doc = loaded.map_err(load_error)?;
    if doc.is_encrypted() && doc.encryption_state.is_none() {
        return Err(DocError::Encrypted);
    }
    // Before any stream is read: in an encrypted file the load filter is not called.
    for o in doc.objects.values_mut() {
        if let Object::Stream(s) = o {
            unpredictable(s);
        }
    }
    lost_objects(&doc)?;
    let pages = pages(&doc);
    let numbers: HashMap<ObjectId, u32> = pages.iter().map(|(n, id)| (*id, *n)).collect();
    let mut marks = bookmarks(&doc, &numbers);
    marks.sort_by_key(|m| m.2);
    let mut marks = marks.into_iter().peekable();
    // The page tree is walked once (Fable M4 code r2 #8): the reader gets its page count.
    let r = Reader::new(&doc, pages.len());
    for (n, id) in &pages {
        let mut text = Layout::new(b.remaining());
        let res = Res::of_page(&r, *id)?;
        match page_content(&doc, *id) {
            Ok(content) => {
                if r.contents.borrow_mut().insert(doc.get_page_contents(*id)) {
                    r.inflated(content.len())?;
                }
                run(
                    &r,
                    &content,
                    &res,
                    State::new(Matrix::IDENTITY),
                    &mut text,
                    0,
                )?;
            }
            // A stream past the limit fails the document; any other error leaves the page
            // empty (lopdf reads a stream that does not decompress as it is, which draws
            // nothing), and the next page is read.
            Err(e) => {
                if let Ok(e) = too_large(e, "a page's content") {
                    return Err(e);
                }
            }
        }
        let lines: Vec<&str> = text.out.lines().collect();
        // Each bookmark of this page starts at the first line, from the previous one's, that
        // starts with its title; else at the top of the page (D14).
        let mut starts = Vec::new();
        let mut from = 0;
        while let Some((level, title, _)) = marks.next_if(|m| m.2 <= *n) {
            let want = squeeze(&title);
            let at = (from..lines.len())
                .find(|&i| !want.is_empty() && squeeze(lines[i]).starts_with(&want))
                .unwrap_or(from);
            from = at;
            starts.push((at, level, title));
        }
        let name = format!("page {n}");
        let mut starts = starts.into_iter().peekable();
        let mut page = None;
        for (i, line) in lines.iter().enumerate() {
            // A chapter that starts inside the page splits it: the page is a Segment under
            // each chapter it holds part of.
            while let Some((_, level, title)) = starts.next_if(|s| s.0 <= i) {
                if let Some(p) = page.take() {
                    b.close(p);
                }
                b.chapter(level, &title);
            }
            if page.is_none() {
                page = Some(b.open("page", &name));
            }
            b.line(line);
        }
        for (_, level, title) in starts {
            if let Some(p) = page.take() {
                b.close(p);
            }
            b.chapter(level, &title);
        }
        if let Some(p) = page {
            b.close(p);
        }
        if text.full {
            b.mark_cut();
        }
        if b.full() {
            break;
        }
    }
    Ok(())
}

/// The pages, numbered from 1 in the order of the page tree. lopdf's walk goes round a
/// `/Kids` that holds a node above it until it has spent a step per object, and lists a page
/// it reaches twice twice; this one visits each node once (Fable r1 #7), with lopdf's limit
/// of 256 levels that still have kids to read.
fn pages(doc: &Document) -> BTreeMap<u32, ObjectId> {
    const PENDING: usize = 256;
    let mut out = BTreeMap::new();
    let Ok(root) = doc
        .catalog()
        .and_then(|c| c.get(b"Pages"))
        .and_then(Object::as_reference)
    else {
        return out;
    };
    let kids = |id| {
        doc.get_dictionary(id)
            .and_then(|d| d.get_deref(b"Kids", doc))
            .and_then(Object::as_array)
            .map_or(&[][..], |k| k.as_slice())
    };
    let mut seen = HashSet::from([root]);
    let mut current = kids(root);
    let mut pending = Vec::new();
    loop {
        let Some((kid, rest)) = current.split_first() else {
            match pending.pop() {
                Some(p) => {
                    current = p;
                    continue;
                }
                None => break,
            }
        };
        current = rest;
        let Ok(id) = kid.as_reference() else { continue };
        if !seen.insert(id) {
            continue;
        }
        match doc.get_dictionary(id).and_then(Dictionary::get_type) {
            Ok(t) if t == b"Page" => {
                out.insert(out.len() as u32 + 1, id);
            }
            Ok(t) if t == b"Pages" && pending.len() < PENDING => {
                if !current.is_empty() {
                    pending.push(current);
                }
                current = kids(id);
            }
            _ => {}
        }
    }
    out
}

/// A page's content: its one stream decompressed, or its streams joined by lopdf, which
/// copies them (a page of one 63 MiB stream took 135 MiB that way; M4 plan r2).
fn page_content(doc: &Document, id: ObjectId) -> Result<Vec<u8>, lopdf::Error> {
    let ids = doc.get_page_contents(id);
    let [one] = ids.as_slice() else {
        return doc.get_page_content_with_limit(id, MAX_STREAM);
    };
    let Ok(s) = doc.get_object(*one).and_then(Object::as_stream) else {
        return Ok(Vec::new());
    };
    match s.decompressed_content_with_limit(MAX_STREAM) {
        Err(e @ lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. })) => Err(e),
        // As lopdf: a stream that does not decompress is read as it is.
        Err(_) if s.content.len() <= MAX_STREAM => Ok(s.content.clone()),
        Err(e) => Err(e),
        Ok(data) => Ok(data),
    }
}

/// An object stream lopdf left out without a word: in an encrypted file it reads them after
/// decrypting, where the load filter is not called, and drops one past `MAX_STREAM`, its
/// objects (pages included) with it (M4 plan r4, Codex #1). The objects the cross-reference
/// table puts in object streams and the document lacks name those streams; one past the limit
/// fails the document, as the filter would have.
fn lost_objects(doc: &Document) -> Result<(), DocError> {
    let mut seen = HashSet::new();
    for (id, entry) in &doc.reference_table.entries {
        let lopdf::xref::XrefEntry::Compressed { container, .. } = entry else {
            continue;
        };
        if doc.objects.contains_key(&(*id, 0)) || !seen.insert(*container) {
            continue;
        }
        let Ok(s) = doc.get_object((*container, 0)).and_then(Object::as_stream) else {
            continue;
        };
        if let Err(e) = s.decompressed_content_with_limit(MAX_STREAM)
            && let Ok(e) = too_large(e, "an object stream")
        {
            return Err(e);
        }
    }
    Ok(())
}

/// What the object streams of the document being loaded on this thread have spent, and
/// whether one was refused (lopdf's load filter is a plain function).
#[derive(Clone, Copy, Default)]
struct ObjStmBudget {
    bytes: usize,
    tokens: usize,
    refused: Option<Refused>,
}

#[derive(Clone, Copy)]
enum Refused {
    Bytes,
    Tokens,
    Overlap,
}

impl Refused {
    fn error(self) -> DocError {
        match self {
            Refused::Bytes => {
                DocError::TooLarge("its object streams".into(), MAX_OBJSTM_BYTES >> 20)
            }
            Refused::Tokens => DocError::broken(format!(
                "its object streams hold more than {MAX_OBJSTM_TOKENS} tokens"
            )),
            Refused::Overlap => DocError::broken("an object stream holds overlapping objects"),
        }
    }
}

thread_local! {
    static OBJSTM: Cell<ObjStmBudget> = Cell::new(ObjStmBudget::default());
}

/// lopdf's load filter: an object stream is decompressed and its tokens counted before lopdf
/// parses it, and past the document's `MAX_OBJSTM_BYTES` or `MAX_OBJSTM_TOKENS` it is left out
/// and the document refused.
fn objstm(id: ObjectId, o: &mut Object) -> Option<(ObjectId, Object)> {
    // lopdf keeps a file's object as the filter leaves it, and an object stream's object as
    // the filter returns it: both, then.
    let Object::Stream(s) = o else {
        return Some((id, o.clone()));
    };
    if unpredictable(s) || !s.dict.has_type(b"ObjStm") {
        return Some((id, o.clone()));
    }
    let mut b = OBJSTM.with(|c| c.get());
    if b.refused.is_some() {
        return None;
    }
    match s.decompressed_content_with_limit(MAX_OBJSTM_BYTES) {
        Ok(data) => {
            b.bytes += data.len();
            b.tokens += tokens(&data);
            if !separate(s, &data) {
                b.refused = Some(Refused::Overlap);
            }
        }
        Err(lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. })) => {
            b.bytes = usize::MAX;
        }
        // lopdf fails to read it too.
        Err(_) => {}
    }
    if b.refused.is_some() {
    } else if b.bytes > MAX_OBJSTM_BYTES {
        b.refused = Some(Refused::Bytes);
    } else if b.tokens > MAX_OBJSTM_TOKENS {
        b.refused = Some(Refused::Tokens);
    }
    OBJSTM.with(|c| c.set(b));
    b.refused.is_none().then(|| (id, o.clone()))
}

/// Empties the stream `s` when lopdf cannot apply the predictor of its `/DecodeParms`, as
/// lopdf reads it: `Columns × Colors × BitsPerComponent` bits a row, which lopdf multiplies
/// unchecked, past what a machine word holds (a panic in a debug build, a wrong row in a
/// release one: Codex r1 #7). Such a stream does not decompress, and draws or holds nothing.
/// True when it was emptied.
fn unpredictable(s: &mut Stream) -> bool {
    let Ok(p) = s.dict.get(b"DecodeParms").and_then(Object::as_dict) else {
        return false;
    };
    let predictor = p.get(b"Predictor").and_then(Object::as_i64).unwrap_or(1);
    if predictor != 2 && !(10..=15).contains(&predictor) {
        return false;
    }
    let n = |key: &[u8], default: i64| {
        p.get(key)
            .and_then(Object::as_i64)
            .unwrap_or(default)
            .max(1) as usize
    };
    let row = n(b"Columns", 1)
        .checked_mul(n(b"Colors", 1))
        .and_then(|c| c.checked_mul(n(b"BitsPerComponent", 8)));
    if row.is_some() {
        return false;
    }
    s.dict.remove(b"Filter");
    s.dict.remove(b"DecodeParms");
    s.set_content(Vec::new());
    true
}

/// How many tokens `data` holds, at most as lopdf reads them: each run of regular bytes, each
/// delimiter, and each sign, which starts a number inside a run (`+1+1`; M4 plan r3, Codex #2).
fn tokens(data: &[u8]) -> usize {
    let mut n = 0;
    let mut in_word = false;
    for &c in data {
        if white(c) {
            in_word = false;
        } else if delimiter(c) || c == b'+' || c == b'-' {
            n += 1;
            in_word = false;
        } else if !in_word {
            n += 1;
            in_word = true;
        }
    }
    n
}

/// Whether the objects of the object stream `s` (its header in `data`) start at offsets that
/// grow: lopdf parses each from its offset, so offsets that repeat or go back parse the same
/// bytes again, and `tokens` would count them once (M4 plan r3, Codex #2).
fn separate(s: &Stream, data: &[u8]) -> bool {
    // As lopdf reads the header: `First` an integer, its words numbers or not, and a pair with
    // one that is not skipped (M4 plan r4, Fable #1).
    let first = s
        .dict
        .get(b"First")
        .and_then(Object::as_i64)
        .unwrap_or(0)
        .max(0) as usize;
    let Some(head) = data.get(..first.min(data.len())) else {
        return true;
    };
    let words: Vec<Option<u32>> = String::from_utf8_lossy(head)
        .split_whitespace()
        .map(|w| w.parse().ok())
        .collect();
    let offsets: Vec<u32> = words
        .chunks(2)
        .filter_map(|p| match p {
            [Some(_), Some(off)] => Some(*off),
            _ => None,
        })
        .collect();
    offsets.windows(2).all(|w| w[0] < w[1])
}

/// The words of `s` joined by single spaces.
fn squeeze(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `e` as the error of a stream of `what` over its 64 MiB, when it is that.
fn too_large(e: lopdf::Error, what: &str) -> Result<DocError, lopdf::Error> {
    match e {
        lopdf::Error::Decompress(DecompressError::MemoryLimitExceeded { .. }) => {
            Ok(DocError::TooLarge(what.into(), MAX_STREAM >> 20))
        }
        e => Err(e),
    }
}

fn load_error(e: lopdf::Error) -> DocError {
    match too_large(e, "a stream of the file") {
        Ok(e) => e,
        Err(lopdf::Error::Decryption(_) | lopdf::Error::InvalidPassword) => DocError::Encrypted,
        Err(lopdf::Error::Decompress(e)) => DocError::broken(e),
        Err(e) => DocError::broken(format!("not a PDF lopdf can read ({e})")),
    }
}

/// The bookmarks as (level, title, page number), in outline order.
fn bookmarks(doc: &Document, numbers: &HashMap<ObjectId, u32>) -> Vec<(u8, String, u32)> {
    let mut out = Vec::new();
    let Ok(catalog) = doc.catalog() else {
        return out;
    };
    let Some(first) = catalog
        .get_deref(b"Outlines", doc)
        .and_then(Object::as_dict)
        .ok()
        .and_then(|o| o.get(b"First").ok())
        .and_then(|f| f.as_reference().ok())
    else {
        return out;
    };
    let mut seen = HashSet::new();
    // (node, level)
    let mut stack = vec![(first, 1u8)];
    while let Some((id, level)) = stack.pop() {
        if out.len() >= MAX_BOOKMARKS || !seen.insert(id) {
            continue;
        }
        let Ok(item) = doc.get_dictionary(id) else {
            continue;
        };
        // Siblings after this one, then its children first (depth first, in order).
        if let Ok(next) = item.get(b"Next").and_then(Object::as_reference) {
            stack.push((next, level));
        }
        if let Ok(child) = item.get(b"First").and_then(Object::as_reference) {
            stack.push((child, level.saturating_add(1)));
        }
        let title = item
            .get_deref(b"Title", doc)
            .ok()
            .and_then(|t| lopdf::decode_text_string(t).ok())
            .unwrap_or_default();
        if let Some(page) = destination(doc, item).and_then(|p| numbers.get(&p)) {
            out.push((level, title, *page));
        }
    }
    out
}

/// The page a bookmark points to: its `/Dest`, or the `/D` of a GoTo action, an explicit array
/// or a named destination.
fn destination(doc: &Document, item: &Dictionary) -> Option<ObjectId> {
    let dest = match item.get_deref(b"Dest", doc) {
        Ok(d) => d,
        Err(_) => item
            .get_deref(b"A", doc)
            .ok()?
            .as_dict()
            .ok()?
            .get_deref(b"D", doc)
            .ok()?,
    };
    page_of(doc, dest, 0)
}

fn page_of(doc: &Document, dest: &Object, depth: usize) -> Option<ObjectId> {
    if depth > 4 {
        return None;
    }
    match dest {
        Object::Array(a) => a.first()?.as_reference().ok(),
        Object::Dictionary(d) => page_of(doc, d.get_deref(b"D", doc).ok()?, depth + 1),
        Object::Name(n) | Object::String(n, _) => {
            let target = named(doc, n)?;
            page_of(doc, target, depth + 1)
        }
        Object::Reference(r) => page_of(doc, doc.get_object(*r).ok()?, depth + 1),
        _ => None,
    }
}

/// A named destination: in the catalog's `/Dests` dictionary, or its `/Names /Dests` tree.
fn named<'a>(doc: &'a Document, name: &[u8]) -> Option<&'a Object> {
    let catalog = doc.catalog().ok()?;
    if let Ok(dests) = catalog.get_deref(b"Dests", doc).and_then(Object::as_dict)
        && let Ok(d) = dests.get_deref(name, doc)
    {
        return Some(d);
    }
    let tree = catalog
        .get_deref(b"Names", doc)
        .ok()?
        .as_dict()
        .ok()?
        .get_deref(b"Dests", doc)
        .ok()?
        .as_dict()
        .ok()?;
    let mut nodes = vec![(tree, 0)];
    let mut visits = 0;
    while let Some((node, depth)) = nodes.pop() {
        visits += 1;
        if visits > 10_000 || depth > 32 {
            return None;
        }
        if let Ok(names) = node.get_deref(b"Names", doc).and_then(Object::as_array) {
            for pair in names.chunks(2) {
                if let [k, v] = pair
                    && k.as_str().ok() == Some(name)
                {
                    return doc.dereference(v).ok().map(|(_, o)| o);
                }
            }
        }
        if let Ok(kids) = node.get_deref(b"Kids", doc).and_then(Object::as_array) {
            for k in kids {
                if let Ok((_, Object::Dictionary(d))) = doc.dereference(k) {
                    nodes.push((d, depth + 1));
                }
            }
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix([f64; 6]);

impl Matrix {
    const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self × other`, in PDF's row-vector convention: apply `self`, then `other`.
    fn then(self, o: Matrix) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = o.0;
        Matrix([
            a * a2 + b * c2,
            a * b2 + b * d2,
            c * a2 + d * c2,
            c * b2 + d * d2,
            e * a2 + f * c2 + e2,
            e * b2 + f * d2 + f2,
        ])
    }

    /// This matrix, or `old` when a value of it is no finite number: a NaN in a text matrix
    /// puts every string after it on the line before (Codex M4 code r9 #5).
    fn or(self, old: Matrix) -> Matrix {
        if self.0.iter().all(|v| v.is_finite()) {
            self
        } else {
            old
        }
    }

    fn translate(x: f64, y: f64) -> Matrix {
        Matrix([1.0, 0.0, 0.0, 1.0, x, y])
    }

    fn of(ops: &[Operand]) -> Option<Matrix> {
        let v: Vec<f64> = ops.iter().filter_map(Operand::num).collect();
        (v.len() == 6 && v.iter().all(|x| x.is_finite()))
            .then(|| Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]))
    }

    /// A matrix written in a dictionary, None when a value is no finite number: lopdf reads a
    /// real as an `f32`, which 39 digits overflow (Fable M4 code r8 #5).
    fn from(ops: &[Object]) -> Option<Matrix> {
        let v: Vec<f64> = ops.iter().filter_map(num).collect();
        (v.len() == 6 && v.iter().all(|x| x.is_finite()))
            .then(|| Matrix([v[0], v[1], v[2], v[3], v[4], v[5]]))
    }
}

fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// A font's metric: a number, None when it is not finite, as lopdf reads a real that
/// overflows its `f32` (Codex M4 code r9 #5).
fn metric(o: &Object) -> Option<f64> {
    num(o).filter(|v| v.is_finite())
}

/// What a document's pages share as they are read: its fonts and Forms, read once each, and
/// the budgets all its pages spend (D17).
struct Reader<'a> {
    doc: &'a Document,
    fonts: RefCell<HashMap<ObjectId, Rc<Font<'a>>>>,
    /// Each Form, read the first time it is drawn.
    forms: RefCell<HashMap<ObjectId, Rc<Form<'a>>>>,
    /// The Forms being drawn, outermost first: a `Do` of one of them draws nothing, as
    /// `pdftotext` reads it (Fable M4 code r8 #2).
    drawing: RefCell<Vec<ObjectId>>,
    inflated: Cell<usize>,
    work: Cell<usize>,
    /// `MAX_WORK` and the `PAGE_WORK` of the document's pages.
    limit: usize,
    /// The content streams of the pages read so far: one a page shares with another is
    /// counted once in `MAX_INFLATED` (M4 plan r2, Fable #3).
    contents: RefCell<HashSet<Vec<ObjectId>>>,
}

impl<'a> Reader<'a> {
    /// A reader of `doc`, which has `pages` pages.
    fn new(doc: &'a Document, pages: usize) -> Self {
        Reader {
            doc,
            limit: MAX_WORK.saturating_add(PAGE_WORK * pages.min(MAX_PAGES_WORK)),
            contents: RefCell::default(),
            fonts: RefCell::default(),
            forms: RefCell::default(),
            drawing: RefCell::default(),
            inflated: Cell::new(0),
            work: Cell::new(0),
        }
    }

    /// Spends `n` decompressed bytes of the document's `MAX_INFLATED`.
    fn inflated(&self, n: usize) -> Result<(), DocError> {
        let total = self.inflated.get().saturating_add(n);
        self.inflated.set(total);
        if total > MAX_INFLATED {
            return Err(DocError::TooLarge(
                "the whole document, decompressed,".into(),
                MAX_INFLATED >> 20,
            ));
        }
        Ok(())
    }

    /// Spends `n` units of the work of the document's content streams.
    fn work(&self, n: usize) -> Result<(), DocError> {
        let total = self.work.get().saturating_add(n);
        self.work.set(total);
        if total > self.limit {
            return Err(DocError::broken(format!(
                "its content streams run more than {} operations",
                self.limit
            )));
        }
        Ok(())
    }

    /// The font `d`, object `id`.
    fn font(&self, id: Option<ObjectId>, d: &'a Dictionary) -> Result<Rc<Font<'a>>, DocError> {
        let Some(id) = id else {
            return Ok(Rc::new(font(self, d)?));
        };
        if let Some(f) = self.fonts.borrow().get(&id) {
            return Ok(f.clone());
        }
        let f = Rc::new(font(self, d)?);
        self.fonts.borrow_mut().insert(id, f.clone());
        Ok(f)
    }

    /// The Form `s`, object `id`: its content stream decompressed (counted in the document's
    /// `MAX_INFLATED`, which bounds what the cache holds), or none when it does not decompress.
    fn form(&self, id: Option<ObjectId>, s: &'a Stream) -> Result<Rc<Form<'a>>, DocError> {
        if let Some(f) = id.and_then(|id| self.forms.borrow().get(&id).cloned()) {
            return Ok(f);
        }
        let data = match s.decompressed_content_with_limit(MAX_STREAM) {
            Ok(data) => {
                self.inflated(data.len())?;
                data
            }
            Err(e) => match too_large(e, "a Form") {
                Ok(e) => return Err(e),
                Err(_) => Vec::new(),
            },
        };
        let matrix = s
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(|m| m.as_array().ok())
            .and_then(|a| Matrix::from(a))
            .unwrap_or(Matrix::IDENTITY);
        // A Form without resources of its own uses its parent's.
        let res = match s
            .dict
            .get_deref(b"Resources", self.doc)
            .and_then(Object::as_dict)
        {
            Ok(own) => Some(Res::new(self, vec![own])?),
            Err(_) => None,
        };
        let f = Rc::new(Form { data, matrix, res });
        if let Some(id) = id {
            self.forms.borrow_mut().insert(id, f.clone());
        }
        Ok(f)
    }
}

/// A Form XObject: its content stream, its matrix, and its own resources if it has them.
struct Form<'a> {
    data: Vec<u8>,
    matrix: Matrix,
    res: Option<Res<'a>>,
}

/// A font of a page or form: how its strings decode and how wide each code is.
struct Font<'a> {
    decode: Decode<'a>,
    /// Bytes per code: 2 for a composite (Type0) font, 1 otherwise.
    bytes: usize,
    /// Code → width in thousandths of the size; `default` for the others.
    widths: HashMap<u32, f64>,
    default: f64,
}

/// The resources a content stream runs with: its resource dictionaries (a page's own and the
/// ones it inherits, or a form's), and their fonts.
struct Res<'a> {
    dicts: Vec<&'a Dictionary>,
    fonts: BTreeMap<Vec<u8>, Rc<Font<'a>>>,
}

impl<'a> Res<'a> {
    fn of_page(r: &Reader<'a>, page: ObjectId) -> Result<Res<'a>, DocError> {
        let doc = r.doc;
        let mut dicts = Vec::new();
        if let Ok((own, inherited)) = doc.get_page_resources(page) {
            dicts.extend(own);
            dicts.extend(
                inherited
                    .iter()
                    .filter_map(|id| doc.get_dictionary(*id).ok()),
            );
        }
        Res::new(r, dicts)
    }

    fn new(r: &Reader<'a>, dicts: Vec<&'a Dictionary>) -> Result<Res<'a>, DocError> {
        let doc = r.doc;
        let mut fonts = BTreeMap::new();
        for d in &dicts {
            let Ok(f) = d.get_deref(b"Font", doc).and_then(Object::as_dict) else {
                continue;
            };
            for (k, v) in f.iter() {
                if fonts.contains_key(k) {
                    continue;
                }
                let Ok((id, Object::Dictionary(d))) = doc.dereference(v) else {
                    continue;
                };
                fonts.insert(k.clone(), r.font(id, d)?);
            }
        }
        Ok(Res { dicts, fonts })
    }

    /// The Form XObject `name`, and its object.
    fn form(&self, doc: &'a Document, name: &[u8]) -> Option<(Option<ObjectId>, &'a Stream)> {
        for r in &self.dicts {
            let Ok(xo) = r.get_deref(b"XObject", doc).and_then(Object::as_dict) else {
                continue;
            };
            if let Ok((id, Object::Stream(s))) = xo.get(name).and_then(|o| doc.dereference(o)) {
                let is_form =
                    s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form");
                return is_form.then_some((id, s));
            }
        }
        None
    }
}

/// How a font's strings decode.
enum Decode<'a> {
    /// Through its ToUnicode map, read by our code, and for a 1-byte code the map does not
    /// hold through the table of the font's `/Encoding` (`table`).
    Map(ToUnicode, Option<Vec<String>>),
    /// Through the encoding lopdf reads from `/Encoding`, for a font without a ToUnicode map.
    Encoding(Encoding<'a>),
    /// Each byte through the table of the font's `/Encoding`: its map does not read.
    Table(Vec<String>),
    /// As UTF-8.
    Raw,
}

/// How the strings of the font `d` decode: through its ToUnicode map when it has one that
/// reads, which says what each code means even in a subset font whose `/Encoding` does not
/// (D13); else through its `/Encoding`. lopdf is never given a font that has a map: it would
/// read the map again, and its reader lists every code of every range (M4 plan r2, Codex #1;
/// r3, Codex #1: also through an `/Encoding` dictionary whose base is Identity). A stream of
/// either past `MAX_STREAM` fails the document.
fn decode<'a>(r: &Reader<'a>, d: &'a Dictionary) -> Result<Decode<'a>, DocError> {
    let doc = r.doc;
    if let Ok(map) = d.get_deref(b"ToUnicode", doc).and_then(Object::as_stream) {
        match map.get_plain_content_with_limit(MAX_STREAM) {
            Ok(plain) => {
                r.inflated(plain.len())?;
                let m = ToUnicode::parse(&plain)?;
                if !m.chars.is_empty() || !m.ranges.is_empty() {
                    return Ok(Decode::Map(m, table(d, doc)));
                }
            }
            Err(e) => {
                if let Ok(e) = too_large(e, "a font's ToUnicode map") {
                    return Err(e);
                }
            }
        }
        return Ok(table(d, doc).map_or(Decode::Raw, Decode::Table));
    }
    match d.get_font_encoding_with_limit(doc, MAX_STREAM) {
        Ok(e) => Ok(Decode::Encoding(e)),
        Err(e) => match too_large(e, "a font's ToUnicode map") {
            Ok(e) => Err(e),
            Err(_) => Ok(Decode::Raw),
        },
    }
}

/// The text of each byte through the `/Encoding` of the font `d`, which lopdf reads from a
/// copy of the font without its ToUnicode map. None without an `/Encoding`.
fn table(d: &Dictionary, doc: &Document) -> Option<Vec<String>> {
    d.get(b"Encoding").ok()?;
    let mut copy = d.clone();
    copy.remove(b"ToUnicode");
    let e = copy.get_font_encoding_with_limit(doc, MAX_STREAM).ok()?;
    Some(
        (0..=255u8)
            .map(|b| Document::decode_text(&e, &[b]).unwrap_or_default())
            .collect(),
    )
}

impl Font<'_> {
    /// The text of the string `s`, at most `room` bytes of it (and one code more); true when
    /// it stopped there. Nothing is decoded past the room (M4 plan r2, Codex #2).
    fn text(&self, s: &[u8], room: usize) -> (String, bool) {
        let mut out = String::new();
        match &self.decode {
            Decode::Map(m, fallback) => {
                for (at, len) in Codes::new(s, Some(self)) {
                    if out.len() > room {
                        return (out, true);
                    }
                    let piece = &s[at..at + len];
                    if !m.write(len, code(piece), &mut out) {
                        match (fallback, piece) {
                            (Some(t), [b]) => out.push_str(&t[*b as usize]),
                            _ => out.push('\u{fffd}'),
                        }
                    }
                }
            }
            Decode::Encoding(e) => {
                for piece in s.chunks(4096 * self.bytes) {
                    if out.len() > room {
                        return (out, true);
                    }
                    out.push_str(&Document::decode_text(e, piece).unwrap_or_default());
                }
            }
            Decode::Table(t) => {
                for b in s {
                    if out.len() > room {
                        return (out, true);
                    }
                    out.push_str(&t[*b as usize]);
                }
            }
            Decode::Raw => return lossy(s, room),
        }
        let cut = out.len() > room;
        (out, cut)
    }
}

fn font<'a>(r: &Reader<'a>, d: &'a Dictionary) -> Result<Font<'a>, DocError> {
    let doc = r.doc;
    let decode = decode(r, d)?;
    let composite = d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type0");
    let mut widths = HashMap::new();
    let mut default = 500.0;
    if composite {
        let cid = d
            .get_deref(b"DescendantFonts", doc)
            .and_then(Object::as_array)
            .ok()
            .and_then(|a| a.first())
            .and_then(|f| doc.dereference(f).ok())
            .and_then(|(_, f)| f.as_dict().ok());
        if let Some(cid) = cid {
            default = cid.get(b"DW").ok().and_then(metric).unwrap_or(1000.0);
            if let Ok(w) = cid.get_deref(b"W", doc).and_then(Object::as_array) {
                let mut i = 0;
                while i + 1 < w.len() && widths.len() < 100_000 {
                    let Some(first) = num(&w[i]) else { break };
                    let first = first as u32;
                    match doc.dereference(&w[i + 1]).map(|(_, o)| o) {
                        Ok(Object::Array(list)) => {
                            // Codes past the last one there is are none (Fable r1 #3).
                            for (k, v) in list.iter().enumerate() {
                                if let (Some(v), Some(c)) = (metric(v), first.checked_add(k as u32))
                                {
                                    widths.insert(c, v);
                                }
                            }
                            i += 2;
                        }
                        _ => {
                            let (Some(last), Some(v)) =
                                (num(&w[i + 1]), w.get(i + 2).and_then(metric))
                            else {
                                break;
                            };
                            for c in first..=(last as u32).min(first.saturating_add(65_535)) {
                                widths.insert(c, v);
                            }
                            i += 3;
                        }
                    }
                }
            }
        }
    } else {
        let scale = if d.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type3") {
            d.get_deref(b"FontMatrix", doc)
                .and_then(Object::as_array)
                .ok()
                .and_then(|m| m.first().and_then(metric))
                .map(|s| s * 1000.0)
                .filter(|s| s.is_finite())
                .unwrap_or(1.0)
        } else {
            1.0
        };
        let first = d.get(b"FirstChar").ok().and_then(num).unwrap_or(0.0) as u32;
        if let Ok(w) = d.get_deref(b"Widths", doc).and_then(Object::as_array) {
            for (k, v) in w.iter().enumerate().take(256) {
                if let (Some(v), Some(c)) = (
                    doc.dereference(v)
                        .ok()
                        .and_then(|(_, o)| metric(o))
                        .map(|v| v * scale)
                        .filter(|v| v.is_finite()),
                    first.checked_add(k as u32),
                ) {
                    widths.insert(c, v);
                }
            }
        }
        if let Some(missing) = d
            .get_deref(b"FontDescriptor", doc)
            .and_then(Object::as_dict)
            .ok()
            .and_then(|fd| fd.get(b"MissingWidth").ok())
            .and_then(metric)
            .map(|w| w * scale)
            .filter(|w| *w > 0.0 && w.is_finite())
        {
            default = missing;
        }
    }
    Ok(Font {
        decode,
        bytes: if composite { 2 } else { 1 },
        widths,
        default,
    })
}

/// The text of a page as it is laid out: where the last string ended, to decide whether the
/// next one starts a line or needs a space.
struct Layout {
    out: String,
    /// The last string: where it ended (x, y, in device space), its size, and whether its
    /// font gave the widths of its glyphs.
    last: Option<Last>,
    /// The bytes of text the document has room for still: the page stops being drawn when
    /// its text reaches them (D17).
    room: usize,
    full: bool,
}

#[derive(Clone, Copy)]
struct Last {
    x: f64,
    y: f64,
    size: f64,
    widths: bool,
}

impl Layout {
    fn new(room: usize) -> Self {
        Layout {
            out: String::new(),
            last: None,
            room,
            full: false,
        }
    }

    /// `space` is a TJ kerning wide enough to be a space before `s`. Without the widths of the
    /// last string's glyphs, where it ended is a guess, so only that kerning makes a space.
    fn push(&mut self, s: &str, x: f64, y: f64, last: Last, space: bool) {
        if s.is_empty() || self.full {
            return;
        }
        // A ligature's letters take no more bytes than it does, and a string writes at most
        // one break or space before it. A string that does not fit writes what fits.
        let mut s = s;
        if self.out.len() + s.len() + 1 > self.room {
            self.full = true;
            let mut fits = self.room.saturating_sub(self.out.len() + 1).min(s.len());
            while !s.is_char_boundary(fits) {
                fits -= 1;
            }
            s = &s[..fits];
            if s.is_empty() {
                return;
            }
        }
        if let Some(l) = self.last {
            let tall = last.size.max(l.size);
            let gap = l.widths && (x - l.x > 0.15 * tall || x < l.x - tall);
            if (y - l.y).abs() > 0.5 * tall {
                self.out.push('\n');
            } else if (space || gap) && !self.out.ends_with([' ', '\n']) && !s.starts_with(' ') {
                self.out.push(' ');
            }
        }
        // The ligatures of Unicode's Alphabetic Presentation Forms, as their letters.
        for c in s.chars() {
            match c {
                '\u{fb00}' => self.out.push_str("ff"),
                '\u{fb01}' => self.out.push_str("fi"),
                '\u{fb02}' => self.out.push_str("fl"),
                '\u{fb03}' => self.out.push_str("ffi"),
                '\u{fb04}' => self.out.push_str("ffl"),
                '\u{fb05}' | '\u{fb06}' => self.out.push_str("st"),
                c => self.out.push(c),
            }
        }
        self.last = Some(last);
    }
}

/// The graphics state a content stream runs with: its CTM, and the text state, whose font is
/// the font itself rather than its name in some resources, so that a Form drawn with it shows
/// with the font of the stream that draws it even when the Form's resources name another
/// (Codex M4 code r7 #2). The text rendering mode (`Tr`) is not kept: invisible text is text
/// to search too, as `pdftotext` reads it.
#[derive(Clone)]
struct State<'a> {
    ctm: Matrix,
    font: Option<Rc<Font<'a>>>,
    size: f64,
    tc: f64,
    tw: f64,
    th: f64,
    tl: f64,
    rise: f64,
}

impl State<'_> {
    /// The state a page starts with: its CTM, and the text state's defaults.
    fn new(ctm: Matrix) -> Self {
        State {
            ctm,
            font: None,
            size: 0.0,
            tc: 0.0,
            tw: 0.0,
            th: 1.0,
            tl: 0.0,
            rise: 0.0,
        }
    }
}

/// The most bytes of text a string may write for each of its bytes (D17).
const MAX_GROWTH: usize = 8;

/// The most `q` a content stream may have open: past it a `q` saves nothing, and its `Q`
/// restores nothing (D17).
const MAX_SAVES: usize = 256;

/// Runs a content stream from the graphics state `gs`, writing the text it shows into `out`,
/// until the text is full. Its operations are read one at a time (`Ops`), so a stream never
/// becomes a list of them, and each one spends the document's work as it is read (D17). A Form
/// it draws runs from a copy of its state, so what the Form changes stays in it.
fn run<'a>(
    r: &Reader<'a>,
    data: &[u8],
    res: &Res<'a>,
    gs: State<'a>,
    out: &mut Layout,
    depth: usize,
) -> Result<(), DocError> {
    let doc = r.doc;
    let mut gs = gs;
    let mut stack = Vec::new();
    // The `q` past `MAX_SAVES`, which their `Q` match first.
    let mut lost = 0usize;
    let (mut tm, mut tlm) = (Matrix::IDENTITY, Matrix::IDENTITY);
    // A TJ kerning wide enough to be a space, waiting for the next string.
    let mut kern_space = false;
    let mut ops = Ops::new(data);
    ops.spaces = spaces(res, doc);
    let mut o = Vec::new();
    while let Some(op) = ops.next(&mut o)? {
        if out.full {
            break;
        }
        r.work(1)?;
        // A number past an `f64` is none (Codex M4 code r9 #5).
        let n = |i: usize| {
            o.get(i)
                .and_then(Operand::num)
                .filter(|v| v.is_finite())
                .unwrap_or(0.0)
        };
        match op {
            b"q" if stack.len() < MAX_SAVES => stack.push(gs.clone()),
            b"q" => lost += 1,
            b"Q" if lost > 0 => lost -= 1,
            b"Q" => {
                if let Some(saved) = stack.pop() {
                    gs = saved;
                }
            }
            b"cm" => {
                if let Some(m) = Matrix::of(&o) {
                    gs.ctm = m.then(gs.ctm).or(gs.ctm);
                }
            }
            b"BT" => {
                tm = Matrix::IDENTITY;
                tlm = tm;
            }
            b"Tf" => {
                gs.font = match o.first() {
                    Some(Operand::Name(f)) => res.fonts.get(f.as_ref()).cloned(),
                    _ => None,
                };
                gs.size = n(1);
            }
            b"Tc" => gs.tc = n(0),
            b"Tw" => gs.tw = n(0),
            b"Tz" => gs.th = n(0) / 100.0,
            b"TL" => gs.tl = n(0),
            b"Ts" => gs.rise = n(0),
            b"Td" => {
                tlm = Matrix::translate(n(0), n(1)).then(tlm).or(tlm);
                tm = tlm;
            }
            b"TD" => {
                gs.tl = -n(1);
                tlm = Matrix::translate(n(0), n(1)).then(tlm).or(tlm);
                tm = tlm;
            }
            b"Tm" => {
                if let Some(m) = Matrix::of(&o) {
                    tm = m;
                    tlm = m;
                }
            }
            b"T*" => {
                tlm = Matrix::translate(0.0, -gs.tl).then(tlm).or(tlm);
                tm = tlm;
            }
            b"Tj" | b"'" | b"\"" | b"TJ" => {
                if op == b"'" || op == b"\"" {
                    if op == b"\"" {
                        gs.tw = n(0);
                        gs.tc = n(1);
                    }
                    tlm = Matrix::translate(0.0, -gs.tl).then(tlm).or(tlm);
                    tm = tlm;
                }
                let items: &[Operand] = match (op, o.last()) {
                    (b"TJ", Some(Operand::Array(a))) => a,
                    (_, Some(s)) => std::slice::from_ref(s),
                    _ => continue,
                };
                let font = gs.font.as_deref();
                for item in items {
                    if let Some(s) = item.string() {
                        show(&s, font, &gs, &mut tm, out, kern_space);
                        kern_space = false;
                    } else {
                        let k = item.num().unwrap_or(0.0);
                        tm = Matrix::translate(-k / 1000.0 * gs.size * gs.th, 0.0)
                            .then(tm)
                            .or(tm);
                        if k < -100.0 {
                            kern_space = true;
                        }
                    }
                }
            }
            b"Do" if depth < MAX_FORM_DEPTH => {
                let Some(Operand::Name(name)) = o.first() else {
                    continue;
                };
                let Some((id, stream)) = res.form(doc, name) else {
                    continue;
                };
                // A Form inside itself, which no PDF draws: `MAX_FORM_DEPTH` stays for the Forms
                // that are no objects of their own.
                if id.is_some_and(|id| r.drawing.borrow().contains(&id)) {
                    continue;
                }
                let form = r.form(id, stream)?;
                r.work(FORM_DRAW)?;
                let inner = form.res.as_ref().unwrap_or(res);
                let mut drawn = gs.clone();
                drawn.ctm = form.matrix.then(gs.ctm).or(gs.ctm);
                r.drawing.borrow_mut().extend(id);
                let ran = run(r, &form.data, inner, drawn, out, depth + 1);
                if id.is_some() {
                    r.drawing.borrow_mut().pop();
                }
                ran?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Writes one string: decoded, placed where the text matrix puts it, and the matrix advanced
/// by its glyphs' widths. The text it decodes stops at what is left of the document's room.
fn show(s: &[u8], font: Option<&Font>, gs: &State, tm: &mut Matrix, out: &mut Layout, space: bool) {
    if out.full {
        return;
    }
    let trm = Matrix([gs.size * gs.th, 0.0, 0.0, gs.size, 0.0, gs.rise])
        .then(*tm)
        .then(gs.ctm);
    let size = (trm.0[2] * trm.0[2] + trm.0[3] * trm.0[3]).sqrt();
    let (x, y) = (trm.0[4], trm.0[5]);
    // A string writes at most `MAX_GROWTH` bytes of text for each of its bytes: a ToUnicode
    // map may give a code 256 letters, which made a 5 KB file 512 MiB of text (M4 plan r2).
    let room = out.room.saturating_sub(out.out.len());
    let most = s.len().saturating_mul(MAX_GROWTH).saturating_add(64);
    let (text, cut) = match font {
        Some(f) => f.text(s, room.min(most)),
        None => lossy(s, room.min(most)),
    };
    let cut = cut && room <= most;
    let text = if text.len() > most {
        let mut end = most;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        &text[..]
    };
    let (widths, default) = font.map_or((None, 500.0), |f| (Some(&f.widths), f.default));
    let known = widths.is_some_and(|w| !w.is_empty());
    let mut advance = 0.0;
    for (at, len) in Codes::new(s, font) {
        let c = code(&s[at..at + len]);
        let w = widths.and_then(|w| w.get(&c)).copied().unwrap_or(default);
        let word = if len == 1 && c == 32 { gs.tw } else { 0.0 };
        let step = |w: f64| (w / 1000.0 * gs.size + gs.tc + word) * gs.th;
        // A glyph whose advance is no finite number has the default width, or none.
        advance += [step(w), step(default)]
            .into_iter()
            .find(|a| a.is_finite())
            .unwrap_or(0.0);
    }
    *tm = Matrix::translate(advance, 0.0).then(*tm).or(*tm);
    let end = Matrix::translate(0.0, gs.rise).then(*tm).then(gs.ctm).0[4];
    let last = Last {
        x: end,
        y,
        size: size.abs().max(0.1),
        widths: known,
    };
    out.push(text, x, y, last, space);
    if cut {
        out.full = true;
    }
}

/// A code's bytes as a number, big-endian.
fn code(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |a, b| a << 8 | *b as u32)
}

/// `s` as UTF-8, invalid bytes replaced, at most `room` bytes of it; true when it stopped
/// there.
fn lossy(s: &[u8], room: usize) -> (String, bool) {
    let mut out = String::new();
    for piece in s.chunks(4096) {
        if out.len() >= room {
            return (out, true);
        }
        out.push_str(&String::from_utf8_lossy(piece));
    }
    let cut = out.len() > room;
    (out, cut)
}

/// The codes of a string in a font, as (start, length): by its ToUnicode map's codespace
/// ranges when it has them, else by its code size.
struct Codes<'s, 'f> {
    s: &'s [u8],
    at: usize,
    bytes: usize,
    map: Option<&'f ToUnicode>,
}

impl<'s, 'f> Codes<'s, 'f> {
    fn new(s: &'s [u8], font: Option<&'f Font>) -> Self {
        let map = font.and_then(|f| match &f.decode {
            Decode::Map(m, _) => Some(m),
            _ => None,
        });
        Codes {
            s,
            at: 0,
            bytes: font.map_or(1, |f| f.bytes),
            map,
        }
    }
}

impl Iterator for Codes<'_, '_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        let rest = &self.s[self.at..];
        if rest.is_empty() {
            return None;
        }
        // A simple font's codes are 1 byte; a composite font's are 2, unless its map says
        // otherwise (M4 plan r3, Fable #1).
        let len = match self.bytes {
            1 => 1,
            b => self.map.and_then(|m| m.code_len(rest)).unwrap_or(b),
        }
        .min(rest.len());
        let at = self.at;
        self.at += len;
        Some((at, len))
    }
}

// --- Content streams ---

/// The most operands one operation may have, the items of its arrays and dictionaries counted
/// (D17): a `TJ` of a line has tens.
const MAX_OPERANDS: usize = 65_536;
/// Arrays and dictionaries nested deeper than this end the stream.
const MAX_NESTING: usize = 32;

/// An operand of a content stream (or of a CMap), borrowing the stream's bytes.
#[derive(Debug, Clone, PartialEq)]
enum Operand<'s> {
    Num(f64),
    /// A literal string, between its parentheses, escapes not yet read.
    Literal(&'s [u8]),
    /// A hexadecimal string's digits, between `<` and `>`.
    Hex(&'s [u8]),
    /// A name, its `#xx` escapes read.
    Name(std::borrow::Cow<'s, [u8]>),
    Array(Vec<Operand<'s>>),
    Bool(bool),
    /// A dictionary or `null`: read past, never used.
    Other,
}

impl Operand<'_> {
    fn num(&self) -> Option<f64> {
        match self {
            Operand::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// A string's bytes, its escapes or hexadecimal digits read.
    fn string(&self) -> Option<Vec<u8>> {
        match self {
            Operand::Literal(b) => Some(literal(b)),
            Operand::Hex(h) => Some(hex(h)),
            _ => None,
        }
    }
}

fn literal(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        i += 1;
        if c != b'\\' {
            out.push(c);
            continue;
        }
        let Some(&e) = b.get(i) else { break };
        i += 1;
        match e {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'0'..=b'7' => {
                let mut v = (e - b'0') as u32;
                for _ in 0..2 {
                    match b.get(i) {
                        Some(d @ b'0'..=b'7') => {
                            v = v * 8 + (d - b'0') as u32;
                            i += 1;
                        }
                        _ => break,
                    }
                }
                out.push(v as u8);
            }
            // A backslash at the end of a line continues the string on the next.
            b'\r' => {
                if b.get(i) == Some(&b'\n') {
                    i += 1;
                }
            }
            b'\n' => {}
            e => out.push(e),
        }
    }
    out
}

fn hex(h: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> = h
        .iter()
        .filter_map(|c| (*c as char).to_digit(16).map(|d| d as u8))
        .collect();
    digits
        .chunks(2)
        .map(|p| p[0] << 4 | p.get(1).copied().unwrap_or(0))
        .collect()
}

/// `true`, `false`, or anything else a bare word means as an operand.
fn keyword(w: &[u8]) -> Operand<'_> {
    match w {
        b"true" => Operand::Bool(true),
        b"false" => Operand::Bool(false),
        _ => Operand::Other,
    }
}

/// A name's bytes, its `#xx` escapes read.
fn name(w: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if !w.contains(&b'#') {
        return std::borrow::Cow::Borrowed(w);
    }
    let mut out = Vec::with_capacity(w.len());
    let mut i = 0;
    while i < w.len() {
        let pair = w
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match pair.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(b) if w[i] == b'#' => {
                out.push(b);
                i += 3;
            }
            _ => {
                out.push(w[i]);
                i += 1;
            }
        }
    }
    std::borrow::Cow::Owned(out)
}

fn white(c: u8) -> bool {
    matches!(c, 0 | b'\t' | b'\n' | 12 | b'\r' | b' ')
}

fn delimiter(c: u8) -> bool {
    matches!(
        c,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

/// The operations of a content stream, one at a time: each operator, after its operands. What
/// it cannot read is skipped a byte at a time, as a reader does.
struct Ops<'s> {
    data: &'s [u8],
    at: usize,
    /// Operands and items read for the current operation.
    items: usize,
    /// The components of the color spaces the resources name, for inline images.
    spaces: HashMap<Vec<u8>, usize>,
}

impl<'s> Ops<'s> {
    fn new(data: &'s [u8]) -> Self {
        Ops {
            data,
            at: 0,
            items: 0,
            spaces: HashMap::new(),
        }
    }

    /// The next operator, its operands in `operands`; None at the end.
    fn next(&mut self, operands: &mut Vec<Operand<'s>>) -> Result<Option<&'s [u8]>, DocError> {
        operands.clear();
        self.items = 0;
        loop {
            self.skip();
            let Some(&c) = self.data.get(self.at) else {
                return Ok(None);
            };
            if c == b')' || c == b'>' || c == b']' || c == b'}' || c == b'{' {
                self.at += 1;
                continue;
            }
            if !delimiter(c) {
                let word = self.word();
                match Self::number(word) {
                    Some(n) => self.push(operands, Operand::Num(n))?,
                    None if matches!(word, b"true" | b"false" | b"null") => {
                        self.push(operands, keyword(word))?
                    }
                    None => {
                        if word == b"ID" {
                            self.skip_image(operands);
                        }
                        return Ok(Some(word));
                    }
                }
                continue;
            }
            let o = self.object(0)?;
            self.push(operands, o)?;
        }
    }

    fn push(&mut self, operands: &mut Vec<Operand<'s>>, o: Operand<'s>) -> Result<(), DocError> {
        self.count()?;
        operands.push(o);
        Ok(())
    }

    fn count(&mut self) -> Result<(), DocError> {
        self.items += 1;
        if self.items > MAX_OPERANDS {
            return Err(DocError::broken(format!(
                "an operation has more than {MAX_OPERANDS} operands"
            )));
        }
        Ok(())
    }

    /// White space and comments.
    fn skip(&mut self) {
        while let Some(&c) = self.data.get(self.at) {
            if white(c) {
                self.at += 1;
            } else if c == b'%' {
                while self
                    .data
                    .get(self.at)
                    .is_some_and(|c| *c != b'\n' && *c != b'\r')
                {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
    }

    /// The bytes up to the next white space or delimiter.
    fn word(&mut self) -> &'s [u8] {
        let start = self.at;
        while self
            .data
            .get(self.at)
            .is_some_and(|c| !white(*c) && !delimiter(*c))
        {
            self.at += 1;
        }
        &self.data[start..self.at]
    }

    fn number(w: &[u8]) -> Option<f64> {
        let first = *w.first()?;
        if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
            return None;
        }
        std::str::from_utf8(w).ok()?.parse().ok()
    }

    /// An object that starts with a delimiter (or, inside an array, anything).
    fn object(&mut self, depth: usize) -> Result<Operand<'s>, DocError> {
        let data = self.data;
        let c = data[self.at];
        match c {
            b'(' => {
                self.at += 1;
                let start = self.at;
                let mut open = 1usize;
                while let Some(&c) = data.get(self.at) {
                    self.at += 1;
                    match c {
                        b'\\' => self.at += 1,
                        b'(' => open += 1,
                        b')' => {
                            open -= 1;
                            if open == 0 {
                                return Ok(Operand::Literal(&data[start..self.at - 1]));
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Operand::Literal(&data[start.min(data.len())..]))
            }
            b'<' if data.get(self.at + 1) == Some(&b'<') => {
                self.at += 2;
                self.nested(depth, b">>")?;
                Ok(Operand::Other)
            }
            b'<' => {
                self.at += 1;
                let start = self.at;
                while data.get(self.at).is_some_and(|c| *c != b'>') {
                    self.at += 1;
                }
                let end = self.at.min(data.len());
                self.at += 1;
                Ok(Operand::Hex(&data[start..end]))
            }
            b'[' => {
                self.at += 1;
                Ok(Operand::Array(self.nested(depth, b"]")?))
            }
            b'/' => {
                self.at += 1;
                Ok(Operand::Name(name(self.word())))
            }
            _ if !delimiter(c) => {
                let w = self.word();
                Ok(Self::number(w).map_or_else(|| keyword(w), Operand::Num))
            }
            _ => {
                self.at += 1;
                Ok(Operand::Other)
            }
        }
    }

    /// The items of an array or a dictionary, up to `end`.
    fn nested(&mut self, depth: usize, end: &[u8]) -> Result<Vec<Operand<'s>>, DocError> {
        if depth >= MAX_NESTING {
            self.at = self.data.len();
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.at >= self.data.len() {
                return Ok(items);
            }
            if self.data[self.at..].starts_with(end) {
                self.at += end.len();
                return Ok(items);
            }
            if end == b"]" && self.data[self.at] == b'>' {
                self.at += 1;
                continue;
            }
            self.count()?;
            items.push(self.object(depth + 1)?);
        }
    }

    /// After `ID`, the bytes of an inline image, up to its `EI`: an image without a filter is
    /// as long as its width, height, components and bits say, so `EI` inside its bytes ends
    /// nothing (M4 plan r3, Codex #5); a filtered one ends at the first `EI` between white
    /// space, as lopdf and pdf.js read it.
    fn skip_image(&mut self, dict: &[Operand]) {
        let d = self.data;
        let mut i = self.at + 1;
        if let Some(n) = image_len(dict, &self.spaces) {
            i = i.saturating_add(n).min(d.len());
        }
        while i + 2 <= d.len() {
            if &d[i..i + 2] == b"EI"
                && white(d[i - 1])
                && d.get(i + 2).is_none_or(|c| white(*c) || delimiter(*c))
            {
                self.at = i;
                return;
            }
            i += 1;
        }
        self.at = d.len();
    }
}

/// The bytes of an inline image without a filter, from the operands of its `ID` (its
/// dictionary): None when it has a filter or its size is not given.
fn image_len(dict: &[Operand], spaces: &HashMap<Vec<u8>, usize>) -> Option<usize> {
    let get = |keys: &[&[u8]]| {
        dict.chunks(2).find_map(|p| match p {
            [Operand::Name(k), v] if keys.contains(&k.as_ref()) => Some(v),
            _ => None,
        })
    };
    if get(&[b"F", b"Filter"]).is_some() {
        return None;
    }
    let n = |keys: &[&[u8]]| {
        get(keys)
            .and_then(Operand::num)
            .filter(|v| *v >= 0.0)
            .map(|v| v as usize)
    };
    let (w, h) = (n(&[b"W", b"Width"])?, n(&[b"H", b"Height"])?);
    let mask = matches!(get(&[b"IM", b"ImageMask"]), Some(Operand::Bool(true)));
    let bits = if mask {
        1
    } else {
        n(&[b"BPC", b"BitsPerComponent"]).unwrap_or(8)
    };
    // The color space: by its name, or one the resources name (M4 plan r4, Codex #5), or an
    // array; one not known leaves the length unknown.
    let colors = if mask {
        1
    } else {
        match get(&[b"CS", b"ColorSpace"]) {
            None => 1,
            Some(Operand::Name(c)) => named_space(c).or_else(|| spaces.get(c.as_ref()).copied())?,
            Some(Operand::Array(a)) => match a.first() {
                Some(Operand::Name(f)) => array_space(f, a.len())?,
                _ => return None,
            },
            Some(_) => return None,
        }
    };
    let row = w.checked_mul(colors)?.checked_mul(bits)?.div_ceil(8);
    row.checked_mul(h)
}

/// The components of a color space named by itself (the abbreviations of inline images too).
fn named_space(n: &[u8]) -> Option<usize> {
    Some(match n {
        b"G" | b"DeviceGray" | b"CalGray" | b"I" | b"Indexed" => 1,
        b"RGB" | b"DeviceRGB" | b"CalRGB" | b"Lab" => 3,
        b"CMYK" | b"DeviceCMYK" => 4,
        _ => return None,
    })
}

/// The components of an array color space, by its family: an Indexed space's samples are
/// indexes, one component.
fn array_space(family: &[u8], _len: usize) -> Option<usize> {
    Some(match family {
        b"I" | b"Indexed" | b"Separation" | b"CalGray" => 1,
        b"CalRGB" | b"Lab" => 3,
        _ => return None,
    })
}

/// The color spaces of `res`, by name, with their components: what an inline image's `/CS`
/// may name.
fn spaces(res: &Res, doc: &Document) -> HashMap<Vec<u8>, usize> {
    let mut out = HashMap::new();
    for d in &res.dicts {
        let Ok(cs) = d.get_deref(b"ColorSpace", doc).and_then(Object::as_dict) else {
            continue;
        };
        for (k, v) in cs.iter() {
            let n = match doc.dereference(v).map(|(_, o)| o) {
                Ok(Object::Name(n)) => named_space(n),
                Ok(Object::Array(a)) => match a.first().and_then(|f| f.as_name().ok()) {
                    Some(b"ICCBased") => a
                        .get(1)
                        .and_then(|s| doc.dereference(s).ok())
                        .and_then(|(_, s)| s.as_stream().ok())
                        .and_then(|s| s.dict.get(b"N").ok())
                        .and_then(num)
                        .map(|n| n as usize),
                    Some(b"DeviceN") => a
                        .get(1)
                        .and_then(|n| doc.dereference(n).ok())
                        .and_then(|(_, n)| n.as_array().ok())
                        .map(|n| n.len()),
                    Some(f) => array_space(f, a.len()),
                    None => None,
                },
                _ => None,
            };
            if let Some(n) = n {
                out.entry(k.clone()).or_insert(n);
            }
        }
    }
    out
}

// --- ToUnicode maps ---

/// The most entries a ToUnicode map may hold: its single codes, its ranges and the strings of
/// its range arrays (D17). A range counts as one, whatever its size: ranges are kept as
/// ranges, never one entry per code.
const MAX_MAP: usize = 200_000;
/// The most UTF-16 units a code may map to (the CMap format's 512 bytes).
const MAX_DEST: usize = 256;
/// The most codespace ranges a ToUnicode map keeps (real ones have 1 to 4); each counts in
/// `MAX_MAP`.
const MAX_SPACES: usize = 256;

/// A font's ToUnicode CMap, read by our code (D13): its codespace ranges tell how many bytes
/// each code takes, and its `bfchar` and `bfrange` entries what text each code means.
#[derive(Debug, Default)]
struct ToUnicode {
    /// (bytes, low, high), at most `MAX_SPACES`.
    spaces: Vec<(usize, u32, u32)>,
    /// The length of a code by its first byte, from `spaces` (0: none says); built when a
    /// space is not 2 bytes long (M4 plan r3, Codex #4, Fable #1 and #3).
    lens: Option<Box<[u8; 256]>>,
    /// The lengths of the codes `chars` and `ranges` hold.
    lengths: Vec<usize>,
    chars: HashMap<(usize, u32), String>,
    /// (bytes, low, high, what the codes mean), sorted.
    ranges: Vec<(usize, u32, u32, Dest)>,
}

#[derive(Debug)]
enum Dest {
    /// The text of the range's first code; each next code adds one to its last unit.
    From(Vec<u16>),
    /// The text of each code of the range.
    Each(Vec<String>),
}

impl ToUnicode {
    fn parse(data: &[u8]) -> Result<ToUnicode, DocError> {
        let mut m = ToUnicode::default();
        let mut entries = 0usize;
        let mut ops = Ops::new(data);
        let mut o = Vec::new();
        let code_of = |o: &Operand| -> Option<(usize, u32)> {
            let b = o.string()?;
            (1..=4).contains(&b.len()).then(|| (b.len(), code(&b)))
        };
        let units = |o: &Operand| -> Option<Vec<u16>> {
            let b = o.string()?;
            (b.len() <= 2 * MAX_DEST).then(|| {
                b.chunks(2)
                    .map(|p| (p[0] as u16) << 8 | p.get(1).copied().unwrap_or(0) as u16)
                    .collect()
            })
        };
        while let Some(op) = ops.next(&mut o)? {
            match op {
                b"endcodespacerange" => {
                    for p in o.chunks(2) {
                        if let [lo, hi] = p
                            && let (Some((n, lo)), Some((_, hi))) = (code_of(lo), code_of(hi))
                        {
                            entries += 1;
                            if m.spaces.len() < MAX_SPACES {
                                m.spaces.push((n, lo, hi));
                            }
                        }
                    }
                }
                b"endbfchar" => {
                    for p in o.chunks(2) {
                        if let [src, dst] = p
                            && let (Some(src), Some(dst)) = (code_of(src), units(dst))
                        {
                            entries += 1;
                            m.chars.insert(src, String::from_utf16_lossy(&dst));
                        }
                    }
                }
                b"endbfrange" => {
                    for p in o.chunks(3) {
                        let [lo, hi, dst] = p else { continue };
                        let (Some((n, lo)), Some((_, hi))) = (code_of(lo), code_of(hi)) else {
                            continue;
                        };
                        if hi < lo {
                            continue;
                        }
                        entries += 1;
                        let dest = match dst {
                            Operand::Array(list) => {
                                entries += list.len();
                                Dest::Each(
                                    list.iter()
                                        .filter_map(units)
                                        .map(|u| String::from_utf16_lossy(&u))
                                        .collect(),
                                )
                            }
                            d => match units(d) {
                                Some(u) if !u.is_empty() => Dest::From(u),
                                _ => continue,
                            },
                        };
                        m.ranges.push((n, lo, hi, dest));
                    }
                }
                _ => {}
            }
            if entries > MAX_MAP {
                return Err(DocError::broken(format!(
                    "a font's ToUnicode map has more than {MAX_MAP} entries"
                )));
            }
        }
        m.ranges.sort_by_key(|r| (r.0, r.1));
        if m.spaces.iter().any(|s| s.0 != 2) {
            let mut lens = Box::new([0u8; 256]);
            for &(n, lo, hi) in &m.spaces {
                let shift = 8 * (n - 1) as u32;
                for b in (lo >> shift)..=(hi >> shift).min(255) {
                    let l = &mut lens[b as usize];
                    if *l == 0 || (n as u8) < *l {
                        *l = n as u8;
                    }
                }
            }
            m.lens = Some(lens);
        }
        let mut lengths: Vec<usize> = m
            .chars
            .keys()
            .map(|k| k.0)
            .chain(m.ranges.iter().map(|r| r.0))
            .collect();
        lengths.sort_unstable();
        lengths.dedup();
        m.lengths = lengths;
        Ok(m)
    }

    /// How many bytes the code at the start of `s` takes, by the codespace ranges of a map
    /// whose codes are not all of 2 bytes: one lookup by the first byte.
    fn code_len(&self, s: &[u8]) -> Option<usize> {
        let l = *self.lens.as_ref()?.get(*s.first()? as usize)?;
        (l > 0).then_some(l as usize)
    }

    /// What the code `c` of `n` bytes means, written into `out`; false when the map does not
    /// say.
    /// The map's codes of other lengths are tried too: a map that writes a simple font's
    /// codes in 2 bytes (Adobe's `<0000> <FFFF>` template), or the other way round, still says
    /// what they mean (M4 plan r3, Fable #1).
    fn write(&self, n: usize, c: u32, out: &mut String) -> bool {
        if self.write_as(n, c, out) {
            return true;
        }
        self.lengths
            .iter()
            .filter(|&&m| m != n && (m >= 4 || c < 1 << (8 * m)))
            .any(|&m| self.write_as(m, c, out))
    }

    fn write_as(&self, n: usize, c: u32, out: &mut String) -> bool {
        if let Some(t) = self.chars.get(&(n, c)) {
            out.push_str(t);
            return true;
        }
        let i = self.ranges.partition_point(|r| (r.0, r.1) <= (n, c));
        if let Some((rn, lo, hi, dest)) = i.checked_sub(1).map(|i| &self.ranges[i])
            && *rn == n
            && c <= *hi
        {
            let k = (c - lo) as usize;
            match dest {
                Dest::From(u) => {
                    let mut u = u.clone();
                    if let Some(last) = u.last_mut() {
                        *last = last.wrapping_add(k as u16);
                    }
                    out.push_str(&String::from_utf16_lossy(&u));
                }
                Dest::Each(list) => match list.get(k) {
                    Some(t) => out.push_str(t),
                    None => out.push('\u{fffd}'),
                },
            }
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture;
    use super::super::{DocError, Kind, extract};
    use crate::trail::trail_for;

    fn trail(doc: &[u8], needle: &str) -> Vec<String> {
        let e = extract(Kind::Pdf, doc).unwrap();
        let at = memchr::memmem::find(&e.text, needle.as_bytes())
            .unwrap_or_else(|| panic!("{needle} not in {:?}", String::from_utf8_lossy(&e.text)));
        trail_for(&e.outline, at)
            .into_iter()
            .map(|s| s.name)
            .collect()
    }

    fn report() -> Vec<u8> {
        fixture::pdf(
            &[
                &["Report", "front matter needle"],
                &["1 Intro", "the intro needle"],
                &["2 Costs", "costs needle"],
                &["2.1 Taxes", "taxes needle", "año café"],
            ],
            &[(1, "1 Intro", 2), (1, "2 Costs", 3), (2, "2.1 Taxes", 4)],
        )
    }

    #[test]
    fn bookmarks_are_chapters_holding_their_pages() {
        let d = report();
        assert_eq!(trail(&d, "front"), ["page 1"]);
        assert_eq!(trail(&d, "intro needle"), ["1 Intro", "page 2"]);
        assert_eq!(trail(&d, "costs needle"), ["2 Costs", "page 3"]);
        assert_eq!(
            trail(&d, "taxes needle"),
            ["2 Costs", "2.1 Taxes", "page 4"]
        );
    }

    /// D14: several bookmarks on one page start at the lines that hold their titles, and the
    /// page is a Segment under each of them.
    #[test]
    fn bookmarks_inside_a_page_split_it() {
        let d = fixture::pdf(
            &[
                &[
                    "Title page",
                    "1 Intro",
                    "intro needle",
                    "2 Costs",
                    "costs needle",
                ],
                &["more costs needle"],
            ],
            &[(1, "1 Intro", 1), (1, "2 Costs", 1), (1, "3 Missing", 2)],
        );
        assert_eq!(trail(&d, "Title page"), ["page 1"]);
        assert_eq!(trail(&d, "intro needle"), ["1 Intro", "page 1"]);
        assert_eq!(trail(&d, "costs needle"), ["2 Costs", "page 1"]);
        // A title not on its page starts the chapter at the top of the page.
        assert_eq!(trail(&d, "more costs"), ["3 Missing", "page 2"]);
    }

    #[test]
    fn lines_and_winansi_letters_come_through() {
        let e = extract(Kind::Pdf, &report()).unwrap();
        let text = String::from_utf8(e.text).unwrap();
        assert!(
            text.starts_with("Report\nfront matter needle\n1 Intro\n"),
            "{text}"
        );
        assert!(text.contains("año café\n"), "{text}");
    }

    #[test]
    fn an_empty_user_password_opens_and_another_one_does_not() {
        let open = fixture::pdf_encrypted(&[&["secret needle"]], "");
        assert_eq!(trail(&open, "secret"), ["page 1"]);
        let locked = fixture::pdf_encrypted(&[&["secret needle"]], "pw");
        assert_eq!(
            extract(Kind::Pdf, &locked).unwrap_err(),
            DocError::Encrypted
        );
    }

    #[test]
    fn a_broken_pdf_is_an_error() {
        assert!(matches!(
            extract(Kind::Pdf, b"%PDF-1.5\ngarbage"),
            Err(DocError::Broken(_))
        ));
        // A cut file: lopdf may recover part of it or fail, never panic.
        let d = report();
        for cut in [d.len() / 3, d.len() / 2, d.len() - 10] {
            let _ = extract(Kind::Pdf, &d[..cut]);
        }
    }

    use lopdf::{Dictionary, Document, Object, Stream, StringFormat};

    /// A one-page PDF whose page draws `content` with the font `font` as `/F1` and the Form
    /// XObjects `forms` (name, content, compressed) as `/Fm0`, `/Fm1`…, each with the page's
    /// resources.
    fn pdf_with(font: Dictionary, content: &[u8], forms: &[(&[u8], bool)]) -> Vec<u8> {
        let forms: Vec<_> = forms.iter().map(|(b, c)| (*b, *c, None)).collect();
        pdf_forms(font, content, &forms)
    }

    /// A Form of `pdf_forms`: its content, whether it is compressed, and its own fonts, by
    /// name, when it has resources of its own.
    type TestForm<'f> = (&'f [u8], bool, Option<Vec<(&'f str, Dictionary)>>);

    /// `pdf_with`, each Form with resources of its own when it has fonts: those fonts and the
    /// page's Forms.
    fn pdf_forms(font: Dictionary, content: &[u8], forms: &[TestForm]) -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let id = Object::String(b"searchfor-adverse".to_vec(), StringFormat::Hexadecimal);
        doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
        let pages_id = doc.new_object_id();
        let res_id = doc.new_object_id();
        // A stream is an object of its own, referred to.
        let mut font = font;
        for (_, v) in font.iter_mut() {
            if let Object::Stream(st) = v {
                *v = Object::Reference(doc.add_object(st.clone()));
            }
        }
        let font_id = doc.add_object(font);
        let xo_id = doc.new_object_id();
        let mut xo = Dictionary::new();
        for (i, (body, compressed, own)) in forms.iter().enumerate() {
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"XObject".to_vec()));
            d.set("Subtype", Object::Name(b"Form".to_vec()));
            // Required, and pdftotext draws nothing of a Form without it.
            d.set(
                "BBox",
                Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            );
            match own {
                Some(own) => {
                    let mut fonts = Dictionary::new();
                    for (name, f) in own {
                        let mut f = f.clone();
                        for (_, v) in f.iter_mut() {
                            if let Object::Stream(st) = v {
                                *v = Object::Reference(doc.add_object(st.clone()));
                            }
                        }
                        fonts.set(*name, Object::Reference(doc.add_object(f)));
                    }
                    let mut r = Dictionary::new();
                    r.set("Font", Object::Dictionary(fonts));
                    r.set("XObject", Object::Reference(xo_id));
                    d.set("Resources", Object::Dictionary(r));
                }
                None => d.set("Resources", Object::Reference(res_id)),
            }
            let mut st = Stream::new(d, body.to_vec());
            if *compressed {
                st.compress().unwrap();
            }
            xo.set(format!("Fm{i}"), Object::Reference(doc.add_object(st)));
        }
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font_id));
        let mut res = Dictionary::new();
        res.set("Font", Object::Dictionary(fonts));
        doc.objects.insert(xo_id, Object::Dictionary(xo));
        res.set("XObject", Object::Reference(xo_id));
        doc.objects.insert(res_id, Object::Dictionary(res));
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.to_vec()));
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        page.set("Resources", Object::Reference(res_id));
        let page_id = doc.add_object(page);
        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Count", Object::Integer(1));
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set(
            "MediaBox",
            Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        );
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let mut cat = Dictionary::new();
        cat.set("Type", Object::Name(b"Catalog".to_vec()));
        cat.set("Pages", Object::Reference(pages_id));
        let cat_id = doc.add_object(cat);
        doc.trailer.set("Root", Object::Reference(cat_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    /// A ToUnicode CMap mapping each `(code, text)` (codes of `width` bytes).
    fn cmap(width: usize, map: &[(u32, &str)]) -> Vec<u8> {
        let hex = |c: u32| format!("{c:0w$X}", w = width * 2);
        let mut s = format!(
            "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n/CMapName /T def /CMapType 2 def\n1 begincodespacerange <{}> <{}> endcodespacerange\n{} beginbfchar\n",
            hex(0),
            "FF".repeat(width),
            map.len()
        );
        for (c, t) in map {
            let u: String = t.encode_utf16().map(|u| format!("{u:04X}")).collect();
            s.push_str(&format!("<{}> <{u}>\n", hex(*c)));
        }
        s.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop end end\n");
        s.into_bytes()
    }

    fn simple_font(extra: impl FnOnce(&mut Dictionary)) -> Dictionary {
        let mut f = Dictionary::new();
        f.set("Type", Object::Name(b"Font".to_vec()));
        f.set("Subtype", Object::Name(b"Type1".to_vec()));
        f.set("BaseFont", Object::Name(b"ABCDEF+Subset".to_vec()));
        extra(&mut f);
        f
    }

    fn text_of(pdf: &[u8]) -> String {
        String::from_utf8(extract(Kind::Pdf, pdf).unwrap().text).unwrap()
    }

    /// A code the ToUnicode map does not hold reads through the font's `/Encoding` (LaTeX's
    /// `circlecopyrt` in a font whose map leaves it out was U+FFFD in the probe).
    #[test]
    fn a_code_the_map_leaves_out_reads_through_the_encoding() {
        let map = cmap(1, &[(0x6e, "n"), (0x65, "e")]);
        let font = simple_font(|f| {
            f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            );
        });
        let pdf = pdf_with(font, b"BT /F1 12 Tf 72 700 Td (needle) Tj ET", &[]);
        assert_eq!(text_of(&pdf), "needle\n");
    }

    /// Codex r1 #5: a simple font that declares both `/Encoding` and `/ToUnicode` is read
    /// through its ToUnicode map, which says what a subset font's codes mean; its ligatures
    /// expand.
    #[test]
    fn to_unicode_comes_before_the_encoding() {
        let map = cmap(
            1,
            &[
                (0x41, "n"),
                (0x42, "e"),
                (0x43, "d"),
                (0x44, "l"),
                (0x45, "\u{fb01}"),
            ],
        );
        let font = simple_font(|f| {
            f.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            );
        });
        let pdf = pdf_with(
            font,
            b"BT /F1 12 Tf 72 700 Td (ABBCDB) Tj 0 -20 Td (EAB) Tj ET",
            &[],
        );
        assert_eq!(text_of(&pdf), "needle\nfine\n");
    }

    /// A one-page PDF drawing `content` with a Type0 font, Identity-H, whose ToUnicode map is
    /// `map`.
    fn type0(map: Vec<u8>, content: &[u8]) -> Vec<u8> {
        let mut cid = Dictionary::new();
        cid.set("Type", Object::Name(b"Font".to_vec()));
        cid.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
        cid.set("DW", Object::Integer(500));
        let mut font = Dictionary::new();
        font.set("Type", Object::Name(b"Font".to_vec()));
        font.set("Subtype", Object::Name(b"Type0".to_vec()));
        font.set("Encoding", Object::Name(b"Identity-H".to_vec()));
        font.set(
            "DescendantFonts",
            Object::Array(vec![Object::Dictionary(cid)]),
        );
        font.set(
            "ToUnicode",
            Object::Stream(Stream::new(Dictionary::new(), map)),
        );
        pdf_with(font, content, &[])
    }

    /// A Type0 font with Identity-H and a ToUnicode map, its words separated by `TJ` kerning.
    #[test]
    fn a_type0_font_reads_its_two_byte_codes() {
        let map = cmap(
            2,
            &[(1, "n"), (2, "e"), (3, "d"), (4, "l"), (5, "h"), (6, "r")],
        );
        let mut cid = Dictionary::new();
        cid.set("Type", Object::Name(b"Font".to_vec()));
        cid.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
        cid.set("DW", Object::Integer(500));
        let mut font = Dictionary::new();
        font.set("Type", Object::Name(b"Font".to_vec()));
        font.set("Subtype", Object::Name(b"Type0".to_vec()));
        font.set("Encoding", Object::Name(b"Identity-H".to_vec()));
        font.set(
            "DescendantFonts",
            Object::Array(vec![Object::Dictionary(cid)]),
        );
        font.set(
            "ToUnicode",
            Object::Stream(Stream::new(Dictionary::new(), map)),
        );
        let pdf = pdf_with(
            font,
            b"BT /F1 12 Tf 72 700 Td [<000100020002000300040002> -400 <0005000200060002>] TJ ET",
            &[],
        );
        assert_eq!(text_of(&pdf), "needle here\n");
    }

    /// Fable r1 #3: a `/FirstChar` or a `/W` code at the end of the codes gives its widths to
    /// the codes there are, never wrapping to code 0 or panicking.
    #[test]
    fn widths_past_the_last_code_are_dropped() {
        let max = Object::Integer(i64::from(u32::MAX));
        let font = simple_font(|f| {
            f.set("FirstChar", max.clone());
            f.set("Widths", Object::Array(vec![600.into(), 600.into()]));
        });
        let pdf = pdf_with(font, b"BT /F1 12 Tf 72 700 Td (needle) Tj ET", &[]);
        assert_eq!(text_of(&pdf), "needle\n");
        let map = cmap(2, &[(1, "n"), (2, "e")]);
        for w in [
            vec![max.clone(), Object::Array(vec![600.into(), 600.into()])],
            vec![max.clone(), max.clone(), 600.into()],
        ] {
            let mut doc =
                Document::load_mem(&type0(map.clone(), b"BT /F1 12 Tf <0001> Tj ET")).unwrap();
            for o in doc.objects.values_mut() {
                if let Ok(d) = o.as_dict_mut()
                    && let Ok(fonts) = d.get_mut(b"DescendantFonts")
                    && let Ok(a) = fonts.as_array_mut()
                    && let Ok(cid) = a[0].as_dict_mut()
                {
                    cid.set("W", Object::Array(w.clone()));
                }
            }
            let mut out = Vec::new();
            doc.save_to(&mut out).unwrap();
            assert_eq!(text_of(&out), "n\n");
        }
    }

    /// `data` and 400 spaces (so lopdf deflates them) as rows of one byte each under the PNG
    /// predictor (filter byte 0, then the byte), with `/DecodeParms` of `columns`.
    fn predicted(data: &[u8], columns: i64) -> Stream {
        let padded = [data, &[b' '; 400]].concat();
        let rows: Vec<u8> = padded.iter().flat_map(|&b| [0, b]).collect();
        let mut st = Stream::new(Dictionary::new(), rows);
        st.compress().unwrap();
        assert!(st.dict.get(b"Filter").is_ok());
        let mut parms = Dictionary::new();
        parms.set("Predictor", Object::Integer(12));
        parms.set("Columns", Object::Integer(columns));
        parms.set("Colors", Object::Integer(1));
        parms.set("BitsPerComponent", Object::Integer(8));
        st.dict.set("DecodeParms", Object::Dictionary(parms));
        st
    }

    /// Codex r1 #7: a predictor whose row (`Columns × Colors × BitsPerComponent` bits) is past
    /// what a machine word holds does not decompress, and its stream draws nothing, in debug
    /// and in release alike (lopdf multiplies them unchecked: 2³ × (2⁶¹ + 1) wraps to rows of
    /// one byte); a predictor that fits reads.
    #[test]
    fn a_predictor_past_the_largest_row_draws_nothing() {
        let content = b"BT /F1 12 Tf 72 700 Td (needle) Tj ET";
        for (columns, want) in [(1, "needle\n"), ((1i64 << 61) + 1, "")] {
            let pdf = pdf_with(simple_font(|_| {}), b"", &[]);
            let mut doc = Document::load_mem(&pdf).unwrap();
            let page = *doc.get_pages().get(&1).unwrap();
            let id = doc.get_page_contents(page)[0];
            doc.objects
                .insert(id, Object::Stream(predicted(content, columns)));
            let mut out = Vec::new();
            doc.save_to(&mut out).unwrap();
            assert_eq!(text_of(&out), want, "columns {columns}");
        }
        // An object stream too, in the load filter, before it inflates it.
        let mut o = Object::Stream(predicted(b"1 0 2 0 (needle)", (1i64 << 61) + 1));
        if let Object::Stream(s) = &mut o {
            s.dict.set("Type", Object::Name(b"ObjStm".to_vec()));
        }
        super::OBJSTM.with(|c| c.set(super::ObjStmBudget::default()));
        let kept = super::objstm((9, 0), &mut o);
        assert!(kept.is_some());
        assert!(super::OBJSTM.with(|c| c.get()).refused.is_none());
    }

    /// Fable r1 #7: a page tree whose `/Kids` holds the node itself reads the pages after it
    /// (lopdf's walk spent its steps going round and read none), and a page reached twice is
    /// one page.
    #[test]
    fn a_page_tree_that_loops_reads_each_page_once() {
        let pdf = pdf_with(simple_font(|_| {}), b"BT /F1 12 Tf (needle) Tj ET", &[]);
        let mut doc = Document::load_mem(&pdf).unwrap();
        let page = *doc.get_pages().get(&1).unwrap();
        let tree = doc
            .catalog()
            .unwrap()
            .get(b"Pages")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_object_mut(tree)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "Kids",
                Object::Array(vec![
                    Object::Reference(tree),
                    Object::Reference(page),
                    Object::Reference(page),
                ]),
            );
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        assert_eq!(text_of(&out), "needle\n");
        // A chain of 1000 nodes of one kid each: no level has kids left to read, so the depth
        // is no limit, as in lopdf.
        let mut below = page;
        for _ in 0..1000 {
            let mut node = Dictionary::new();
            node.set("Type", Object::Name(b"Pages".to_vec()));
            node.set("Kids", Object::Array(vec![Object::Reference(below)]));
            below = doc.add_object(node);
        }
        doc.get_object_mut(tree)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Kids", Object::Array(vec![Object::Reference(below)]));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        assert_eq!(text_of(&out), "needle\n");
    }

    fn compressed_zeros(n: usize) -> Stream {
        let mut st = Stream::new(Dictionary::new(), vec![b' '; n]);
        st.compress().unwrap();
        st
    }

    /// Codex r1 #1: a ToUnicode map and a Form that decompress past 64 MiB make the document
    /// fail, with the limit, instead of being read whole.
    #[test]
    fn a_bomb_in_a_font_or_a_form_is_refused() {
        let font = simple_font(|f| f.set("ToUnicode", Object::Stream(compressed_zeros(65 << 20))));
        let pdf = pdf_with(font, b"BT /F1 12 Tf (A) Tj ET", &[]);
        let e = extract(Kind::Pdf, &pdf).unwrap_err();
        assert!(matches!(&e, DocError::TooLarge(..)), "{e}");
        let font = simple_font(|_| {});
        let bomb = vec![b' '; 65 << 20];
        let pdf = pdf_with(font, b"/Fm0 Do", &[(&bomb, true)]);
        let e = extract(Kind::Pdf, &pdf).unwrap_err();
        assert!(matches!(&e, DocError::TooLarge(..)), "{e}");
    }

    /// Fable r1 #3: the streams of all the pages, Forms and fonts of a document spend one
    /// budget.
    #[test]
    fn the_streams_of_a_document_share_a_budget() {
        let doc = Document::with_version("1.5");
        let r = super::Reader::new(&doc, 0);
        r.inflated(super::MAX_INFLATED).unwrap();
        let e = r.inflated(1).unwrap_err();
        assert!(matches!(&e, DocError::TooLarge(_, 1024)), "{e}");
    }

    /// The load filter of the object streams keeps every object: a file saved with object
    /// streams reads as it did (the probe's first filter lost them all, and hwloc's manual
    /// gave no text).
    #[test]
    fn a_file_with_object_streams_reads_whole() {
        let pdf = pdf_with(simple_font(|_| {}), b"BT /F1 12 Tf (needle) Tj ET", &[]);
        let mut doc = Document::load_mem(&pdf).unwrap();
        let mut packed = Vec::new();
        let options = lopdf::SaveOptions::builder()
            .use_object_streams(true)
            .use_xref_streams(true)
            .build();
        doc.save_with_options(&mut packed, options).unwrap();
        assert!(memchr::memmem::find(&packed, b"/ObjStm").is_some());
        assert_eq!(text_of(&packed), "needle\n");
    }

    /// Codex r4 #1: an encrypted file whose object stream, holding its page, passes the limit:
    /// lopdf drops the stream without a word, and the document fails with the limit. The file
    /// is encrypted by qpdf (which writes object streams in an encrypted file); without it the
    /// test says so and passes.
    #[test]
    fn an_encrypted_object_stream_past_the_limit_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let plain = pdf_with(simple_font(|_| {}), b"BT /F1 12 Tf (needle) Tj ET", &[]);
        let mut doc = Document::load_mem(&plain).unwrap();
        let pad = doc.add_object(Object::String(vec![b'a'; 65 << 20], StringFormat::Literal));
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_object_mut(root)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Pad", Object::Reference(pad));
        doc.save(dir.path().join("plain.pdf")).unwrap();
        let made = std::process::Command::new("qpdf")
            .args([
                "--encrypt",
                "",
                "",
                "128",
                "--use-aes=y",
                "--",
                "--object-streams=generate",
            ])
            .arg(dir.path().join("plain.pdf"))
            .arg(dir.path().join("enc.pdf"))
            .status();
        if !made.is_ok_and(|s| s.success()) {
            eprintln!("skipping: qpdf is missing");
            return;
        }
        let enc = std::fs::read(dir.path().join("enc.pdf")).unwrap();
        let e = extract(Kind::Pdf, &enc).unwrap_err();
        assert!(
            matches!(&e, DocError::TooLarge(what, 64) if what == "an object stream"),
            "{e}"
        );
    }

    /// Fable r1 #10: a Form whose stream does not decompress draws nothing, and the page goes on.
    #[test]
    fn a_form_that_does_not_decompress_draws_nothing() {
        let page = b"/Fm0 Do BT /F1 12 Tf (needle) Tj ET";
        let pdf = pdf_with(simple_font(|_| {}), page, &[(&[b' '; 4096], true)]);
        let mut doc = Document::load_mem(&pdf).unwrap();
        for o in doc.objects.values_mut() {
            if let Object::Stream(st) = o
                && st.dict.get(b"Filter").is_ok()
            {
                st.set_content(b"not flate".to_vec());
            }
        }
        let mut broken = Vec::new();
        doc.save_to(&mut broken).unwrap();
        assert_eq!(text_of(&broken), "needle\n");
    }

    /// Eight nested Forms, each drawing the next sixteen times: 16⁷ draws of the last one.
    fn fan_out(text: bool) -> Vec<u8> {
        let mut forms: Vec<Vec<u8>> = Vec::new();
        for i in 0..8 {
            let mut body = if text {
                b"BT /F1 12 Tf (needle ) Tj ET ".to_vec()
            } else {
                Vec::new()
            };
            if i < 7 {
                for _ in 0..16 {
                    body.extend(format!("/Fm{} Do ", i + 1).as_bytes());
                }
            }
            forms.push(body);
        }
        let refs: Vec<(&[u8], bool)> = forms.iter().map(|f| (f.as_slice(), false)).collect();
        pdf_with(simple_font(|_| {}), b"/Fm0 Do", &refs)
    }

    /// Codex r1 #2: the text limit holds while the page is drawn, and the work of the content
    /// streams has a budget, so a page of Forms drawing Forms ends quickly either way.
    #[test]
    fn a_page_of_forms_drawing_forms_stops_at_its_budgets() {
        let started = std::time::Instant::now();
        let e = super::super::extract_with(Kind::Pdf, &fan_out(true), 1 << 20).unwrap();
        assert!(e.cut);
        assert!(e.text.len() <= 1 << 20, "{}", e.text.len());
        let e = extract(Kind::Pdf, &fan_out(false)).unwrap_err();
        assert!(
            matches!(&e, DocError::Broken(m) if m.contains("operations")),
            "{e}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "{:?}",
            started.elapsed()
        );
    }

    /// The content reader (M4 plan r2): strings with escapes and nested parentheses, hex
    /// strings, arrays, names, dictionaries, comments and inline images.
    #[test]
    fn the_content_reader_reads_one_operation_at_a_time() {
        use super::{Operand, Ops};
        let data = b"q 1 0 0 1 72 700 cm BT /F1 12 Tf (a\\)b\\050(n)) Tj <6E 65 6> Tj\n\
            [(x) -250 (y)] TJ ET % a comment\n/P << /MCID [1 2] >> BDC EMC\n\
            BI /W 1 /H 1 ID \0EI\xff EI Q";
        let mut ops = Ops::new(data);
        let mut o = Vec::new();
        let mut seen = Vec::new();
        while let Some(op) = ops.next(&mut o).unwrap() {
            seen.push(String::from_utf8_lossy(op).into_owned());
            match op {
                b"cm" => assert_eq!(o.len(), 6),
                b"Tj" => assert!(matches!(o[0], Operand::Literal(_) | Operand::Hex(_))),
                b"TJ" => assert!(matches!(&o[0], Operand::Array(a) if a.len() == 3)),
                b"BDC" => assert_eq!(o, [Operand::Name(b"P"[..].into()), Operand::Other]),
                _ => {}
            }
            if op == b"Tj" {
                let t = o[0].string().unwrap();
                assert!(t == b"a)b((n)" || t == b"ne`", "{t:?}");
            }
        }
        assert_eq!(
            seen,
            [
                "q", "cm", "BT", "Tf", "Tj", "Tj", "TJ", "ET", "BDC", "EMC", "BI", "ID", "EI", "Q"
            ]
        );
    }

    /// Codex r2 #1: a range of a ToUnicode map stays a range, whatever its size: lopdf's
    /// reader listed each of its 2³² codes.
    #[test]
    fn a_map_range_is_read_as_a_range() {
        let mut map = cmap(2, &[]);
        let ranges =
            b"2 beginbfrange\n<0000> <FFFF> <0041>\n<00000000> <FFFFFFFF> <0061>\nendbfrange\n";
        let at = memchr::memmem::find(&map, b"endcmap").unwrap();
        map.splice(at..at, ranges.iter().copied());
        let started = std::time::Instant::now();
        let pdf = type0(map, b"BT /F1 12 Tf 72 700 Td <000000010005> Tj ET");
        assert_eq!(text_of(&pdf), "ABF\n");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    /// Codex r2 #2: a code that means 256 letters, repeated, is decoded only as far as the
    /// text has room, and what fits is kept.
    #[test]
    fn a_code_of_long_text_is_decoded_as_far_as_there_is_room() {
        let long = "n".repeat(256);
        let map = cmap(1, &[(0x41, long.as_str())]);
        let font = simple_font(|f| {
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            )
        });
        let mut content = b"BT /F1 12 Tf (".to_vec();
        content.extend(vec![b'A'; 64 << 10]);
        content.extend(b") Tj ET");
        let pdf = pdf_with(font, &content, &[]);
        let e = super::super::extract_with(Kind::Pdf, &pdf, 256 << 10).unwrap();
        assert!(e.cut);
        let n = e.text.len();
        assert!(n <= 256 << 10 && n > 128 << 10, "{n}");
        // Without the limit, a string writes at most `MAX_GROWTH` bytes for each of its bytes.
        let e = extract(Kind::Pdf, &pdf).unwrap();
        assert!(!e.cut);
        assert!(
            e.text.len() <= (64 << 10) * super::MAX_GROWTH + 65,
            "{}",
            e.text.len()
        );
    }

    /// Codex r2 #3: a Form of millions of operations spends its budget as they are read, and
    /// never becomes a list of them; `q` past `MAX_SAVES` saves nothing.
    #[test]
    fn a_form_of_millions_of_operations_is_refused_as_it_is_read() {
        let started = std::time::Instant::now();
        let body = b"q Q ".repeat(8 << 20);
        let pdf = pdf_with(simple_font(|_| {}), b"/Fm0 Do", &[(&body, true)]);
        let e = extract(Kind::Pdf, &pdf).unwrap_err();
        assert!(
            matches!(&e, DocError::Broken(m) if m.contains("operations")),
            "{e}"
        );
        let mut page = b"q ".repeat(1 << 20);
        page.extend(b"BT /F1 12 Tf (needle) Tj ET");
        assert_eq!(
            text_of(&pdf_with(simple_font(|_| {}), &page, &[])),
            "needle\n"
        );
        let mut tj = b"BT /F1 12 Tf [".to_vec();
        tj.extend(b"1 ".repeat(70_000));
        tj.extend(b"] TJ ET");
        let e = extract(Kind::Pdf, &pdf_with(simple_font(|_| {}), &tj, &[])).unwrap_err();
        assert!(
            matches!(&e, DocError::Broken(m) if m.contains("operands")),
            "{e}"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "{:?}",
            started.elapsed()
        );
    }

    /// Codex r3 #1: a font whose `/Encoding` is a dictionary based on Identity-H, and whose
    /// map has a range of 2³² codes: lopdf never gets the font, so it never lists the range.
    #[test]
    fn lopdf_never_reads_a_map_through_an_encoding_dictionary() {
        let started = std::time::Instant::now();
        let mut map = cmap(1, &[(0x41, "x")]);
        let at = memchr::memmem::find(&map, b"endcmap").unwrap();
        let range = b"1 beginbfrange\n<00000000> <FFFFFFFF> <0041>\nendbfrange\n";
        map.splice(at..at, range.iter().copied());
        let font = simple_font(|f| {
            let mut e = Dictionary::new();
            e.set("Type", Object::Name(b"Encoding".to_vec()));
            e.set("BaseEncoding", Object::Name(b"Identity-H".to_vec()));
            e.set("Differences", Object::Array(vec![]));
            f.set("Encoding", Object::Dictionary(e));
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            );
        });
        let pdf = pdf_with(font, b"BT /F1 12 Tf 72 700 Td (AB) Tj ET", &[]);
        let _ = extract(Kind::Pdf, &pdf);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    /// Codex r3 #2: an object stream whose objects start at offsets that repeat is refused,
    /// and signs count as tokens (`+1+1` is two numbers to lopdf).
    #[test]
    fn object_streams_count_what_lopdf_parses() {
        let st = |first: i64| {
            let mut d = Dictionary::new();
            d.set("First", Object::Integer(first));
            Stream::new(d, Vec::new())
        };
        assert!(super::separate(&st(12), b"7 0 8 5 9 9 [1] [2] [3]"));
        assert!(!super::separate(&st(12), b"7 0 8 0 9 0 [1 1 1 1]"));
        assert!(!super::separate(&st(12), b"7 4 8 2 9 9 [1] [2] [3]"));
        // Fable r4 #1: a pair that is not numbers is skipped, as lopdf skips it.
        assert!(!super::separate(&st(13), b"-1 -1 7 0 8 0 [1 1 1]"));
        // At least what lopdf parses: `[`, three numbers and `]`.
        assert!(super::tokens(b"[+1+1+1]") >= 5);
        assert!(super::tokens(b"[1+1+1]") >= 5);
    }

    /// Codex r3 #4, Fable r3 #3: a map of 100 000 codespace ranges reads a code's length in
    /// one lookup: each code walked all of them.
    #[test]
    fn many_codespace_ranges_cost_one_lookup_a_code() {
        let mut map = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n".to_vec();
        for _ in 0..1000 {
            map.extend(b"100 begincodespacerange\n");
            map.extend(b"<00> <FF>\n".repeat(100));
            map.extend(b"endcodespacerange\n");
        }
        map.extend(b"1 beginbfchar\n<0041> <006E>\nendbfchar\nendcmap end end\n");
        let mut content = b"BT /F1 12 Tf 72 700 Td <".to_vec();
        content.extend(b"0041".repeat(32 << 10));
        content.extend(b"> Tj ET");
        let started = std::time::Instant::now();
        let e = extract(Kind::Pdf, &type0(map, &content)).unwrap();
        assert!(
            e.text.starts_with(b"nnnn"),
            "{:?}",
            &e.text[..e.text.len().min(20)]
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    /// Codex r3 #5, Fable r3 #7: a font named with an escape (`/F#31` is `/F1`) is found, and
    /// an inline image without a filter ends where its size says, whatever its bytes hold.
    #[test]
    fn escaped_names_and_inline_images_keep_the_text() {
        let map = cmap(1, &[(0x41, "n"), (0x42, "e"), (0x43, "d"), (0x44, "l")]);
        let font = simple_font(|f| {
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            );
        });
        let pdf = pdf_with(
            font.clone(),
            b"BT /F#31 12 Tf 72 700 Td (ABBCDB) Tj ET",
            &[],
        );
        assert_eq!(text_of(&pdf), "needle\n");
        let page = b"BI /W 5 /H 1 /CS /G /BPC 8 ID  EI (\nEI BT /F1 12 Tf 72 700 Td (ABBCDB) Tj ET";
        assert_eq!(text_of(&pdf_with(font, page, &[])), "needle\n");
    }

    /// Codex r4 #5: an inline image whose color space the resources name (`/C1`, RGB: three
    /// bytes a pixel) ends where its size says, past an `EI` inside its bytes.
    #[test]
    fn an_inline_image_of_a_named_color_space_keeps_the_text() {
        let map = cmap(1, &[(0x41, "n"), (0x42, "e"), (0x43, "d"), (0x44, "l")]);
        let font = simple_font(|f| {
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            );
        });
        let page = b"BI /W 5 /H 1 /CS /C1 /BPC 8 ID abcde EI (zzzzz\nEI BT /F1 12 Tf 72 700 Td (ABBCDB) Tj ET";
        let mut doc = Document::load_mem(&pdf_with(font, page, &[])).unwrap();
        let page_id = *doc.get_pages().values().next().unwrap();
        let res = doc
            .get_dictionary(page_id)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_reference()
            .unwrap();
        let mut cs = Dictionary::new();
        cs.set("C1", Object::Name(b"DeviceRGB".to_vec()));
        doc.get_object_mut(res)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("ColorSpace", Object::Dictionary(cs));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        assert_eq!(text_of(&out), "needle\n");
    }

    /// Fable r3 #1: a simple font's codes are 1 byte, whatever its map's codespace says (Adobe's
    /// `<0000> <FFFF>` template over 1-byte `bfchar`s), and its glyph names lopdf does not know
    /// come from the map.
    #[test]
    fn a_simple_font_reads_one_byte_codes_whatever_its_codespace() {
        let map = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
            1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
            2 beginbfchar\n<1A> <0031>\n<1B> <0037>\nendbfchar\nendcmap end end\n";
        let font = simple_font(|f| {
            let mut e = Dictionary::new();
            e.set("Type", Object::Name(b"Encoding".to_vec()));
            let diff = vec![
                26.into(),
                Object::Name(b"one.LT".to_vec()),
                Object::Name(b"seven.LT".to_vec()),
            ];
            e.set("Differences", Object::Array(diff));
            f.set("Encoding", Object::Dictionary(e));
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map.to_vec())),
            );
        });
        let pdf = pdf_with(font, b"BT /F1 12 Tf 72 700 Td (B\x1a\x1b) Tj ET", &[]);
        assert_eq!(text_of(&pdf), "B17\n");
    }

    /// A simple font whose ToUnicode map reads the code `X` as `text`.
    fn x_font(text: &str) -> Dictionary {
        let map = cmap(1, &[(0x58, text)]);
        simple_font(|f| {
            f.set(
                "ToUnicode",
                Object::Stream(Stream::new(Dictionary::new(), map)),
            )
        })
    }

    /// The words `pdftotext -raw` reads from `pdf`, if it is installed: in the order the
    /// content streams show them.
    fn pdftotext_words(pdf: &[u8]) -> Option<Vec<String>> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.pdf");
        std::fs::write(&path, pdf).unwrap();
        let out = std::process::Command::new("pdftotext")
            .args(["-raw", "-q"])
            .arg(&path)
            .arg("-")
            .output()
            .ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .map(str::to_string)
                .collect()
        })
    }

    /// `text_of(pdf)` is `want`, and its words are `pdftotext`'s when it is installed.
    fn reads_as_pdftotext(pdf: &[u8], want: &str) {
        let got = text_of(pdf);
        assert_eq!(got, want);
        match pdftotext_words(pdf) {
            Some(words) => assert_eq!(got.split_whitespace().collect::<Vec<_>>(), words),
            None => eprintln!("note: pdftotext is missing; the comparison is skipped"),
        }
    }

    /// Codex M4 code r7 #2: a Form runs with the graphics state in force where `Do` draws it:
    /// the page's font and size, which the Form uses without a `Tf` of its own, its text
    /// parameters, and its CTM, with the Form's /Matrix applied.
    #[test]
    fn a_form_inherits_the_state_it_is_drawn_with() {
        let form: &[u8] = b"BT 72 700 Td (X) Tj ET";
        let pdf = pdf_with(
            x_font("needle"),
            b"BT /F1 12 Tf ET /Fm0 Do",
            &[(form, false)],
        );
        reads_as_pdftotext(&pdf, "needle\n");
        // The font is the page's even when the Form has resources of its own, which name
        // another font /F1 or none.
        for own in [vec![("F1", x_font("wrong"))], vec![("F2", x_font("wrong"))]] {
            let pdf = pdf_forms(
                x_font("needle"),
                b"BT /F1 12 Tf ET /Fm0 Do",
                &[(form, false, Some(own))],
            );
            reads_as_pdftotext(&pdf, "needle\n");
        }
        // The leading (TL) and the rise (Ts): T* moves 14 down, and a rise of 20 moves the
        // string up, so both start a line.
        let pdf = pdf_with(
            x_font("needle"),
            b"BT /F1 12 Tf 14 TL ET /Fm0 Do",
            &[(b"BT 72 700 Td (X) Tj T* (X) Tj ET", false)],
        );
        reads_as_pdftotext(&pdf, "needle\nneedle\n");
        let pdf = pdf_with(
            x_font("needle"),
            b"BT /F1 12 Tf 72 700 Td (X) Tj ET 20 Ts /Fm0 Do",
            &[(form, false)],
        );
        reads_as_pdftotext(&pdf, "needle\nneedle\n");
        // The size: at 40, a string 10 lower is on the same line.
        let pdf = pdf_with(
            x_font("needle"),
            b"BT /F1 40 Tf ET /Fm0 Do",
            &[(b"BT 72 700 Td (X) Tj 0 -10 Td (X) Tj ET", false)],
        );
        assert_eq!(text_of(&pdf), "needleneedle\n");
    }

    /// Codex M4 code r7 #2: what a Form changes of the state stays in it: after `Do`, the
    /// page's font, size and CTM are the ones it had.
    #[test]
    fn a_forms_changes_to_the_state_stay_in_it() {
        let page = b"BT /F1 12 Tf 72 700 Td ET /Fm0 Do BT 72 600 Td (X) Tj ET";
        // The Form shows with the page's font, then with its own at 30, and moves the CTM 50
        // up, which would put the page's last string on the line of the Form's.
        let form: &[u8] =
            b"BT 72 700 Td (X) Tj ET BT /F2 30 Tf 72 650 Td (X) Tj ET 1 0 0 1 0 50 cm";
        let pdf = pdf_forms(
            x_font("needle"),
            page,
            &[(form, false, Some(vec![("F2", x_font("inner"))]))],
        );
        reads_as_pdftotext(&pdf, "needle\ninner\nneedle\n");
    }

    /// Fable M4 code r8 #2: a Form that draws itself, directly or through another, is not drawn
    /// again while it is being drawn, as `pdftotext` reads it; one that draws itself 64 times no
    /// longer spends the whole document's work. A Form drawn again after its draw ends is drawn
    /// again.
    #[test]
    fn a_form_is_not_drawn_inside_itself() {
        let font = || simple_font(|_| {});
        let page: &[u8] = b"BT /F1 12 Tf 72 700 Td (page) Tj ET /Fm0 Do";
        let pdf = pdf_with(
            font(),
            page,
            &[(b"BT /F1 12 Tf 72 650 Td (self) Tj ET /Fm0 Do", false)],
        );
        reads_as_pdftotext(&pdf, "page\nself\n");
        let pdf = pdf_with(
            font(),
            b"BT /F1 12 Tf 72 700 Td (page) Tj ET /Fm0 Do BT 72 100 Td (after) Tj ET",
            &[
                (b"BT 72 650 Td (A) Tj ET /Fm1 Do", false),
                (
                    b"BT /F1 30 Tf 72 600 Td (B) Tj ET 2 0 0 2 0 0 cm /Fm0 Do",
                    false,
                ),
            ],
        );
        reads_as_pdftotext(&pdf, "page\nA\nB\nafter\n");
        let fan = [
            b"BT 72 650 Td (fan) Tj ET ".to_vec(),
            b"/Fm0 Do ".repeat(64),
        ]
        .concat();
        let pdf = pdf_with(font(), page, &[(&fan, false)]);
        reads_as_pdftotext(&pdf, "page\nfan\n");
        let pdf = pdf_with(
            font(),
            b"BT /F1 12 Tf 72 700 Td (page) Tj ET /Fm0 Do 1 0 0 1 0 -100 cm /Fm0 Do",
            &[(b"BT 72 650 Td (twice) Tj ET", false)],
        );
        reads_as_pdftotext(&pdf, "page\ntwice\ntwice\n");
    }

    /// Fable M4 code r8 #5: lopdf reads a real as an `f32`, so a /Matrix of 39 digits is
    /// infinite, and its products NaN, which glued every line of the page into one. A matrix
    /// with a value that is no number is no matrix, and the Form draws with the identity.
    /// `pdftotext`, which reads the real as a double, leaves out `big`, drawn off the page.
    #[test]
    fn a_form_matrix_past_f32_draws_with_the_identity() {
        let pdf = pdf_with(
            simple_font(|_| {}),
            b"BT /F1 12 Tf 72 700 Td (page) Tj ET /Fm0 Do BT 72 100 Td (after) Tj ET",
            &[
                (b"BT 72 650 Td (big) Tj ET 0 0 0 0 0 0 cm /Fm1 Do", false),
                (b"BT 72 600 Td (zero) Tj ET", false),
            ],
        );
        // A name as long as the real and a space before it, which replace it where lopdf wrote
        // it, without a space after the value before.
        let huge = format!(" {}.0", "9".repeat(39));
        let name = format!("P{}", "x".repeat(huge.len() - 2));
        let mut doc = Document::load_mem(&pdf).unwrap();
        let form = doc
            .objects
            .values_mut()
            .find_map(|o| match o {
                Object::Stream(st) if st.content.starts_with(b"BT 72 650") => Some(st),
                _ => None,
            })
            .unwrap();
        let n = || Object::Name(name.clone().into_bytes());
        let matrix = vec![n(), 0.into(), 0.into(), n(), 0.into(), 0.into()];
        form.dict.set("Matrix", Object::Array(matrix));
        let mut pdf = Vec::new();
        doc.save_to(&mut pdf).unwrap();
        let from = format!("/{name}");
        let mut at = 0;
        while let Some(i) = memchr::memmem::find(&pdf[at..], from.as_bytes()) {
            pdf[at + i..at + i + huge.len()].copy_from_slice(huge.as_bytes());
            at += i + huge.len();
        }
        assert!(memchr::memmem::find(&pdf, huge.as_bytes()).is_some());
        assert_eq!(text_of(&pdf), "page\nbig\nzero\nafter\n");
        if let Some(words) = pdftotext_words(&pdf) {
            assert_eq!(words, ["page", "zero", "after"]);
        }
    }

    /// `pdf` with each name `/P…` lopdf wrote for `huge` replaced by it: lopdf writes no real
    /// that overflows its `f32`, and reads one as infinite. `huge` starts with a space, which
    /// stands for the name's `/` without a space after the value before.
    fn with_real(pdf: &[u8], huge: &str) -> Vec<u8> {
        let from = format!("/P{}", "x".repeat(huge.len() - 2));
        let mut pdf = pdf.to_vec();
        let mut at = 0;
        while let Some(i) = memchr::memmem::find(&pdf[at..], from.as_bytes()) {
            pdf[at + i..at + i + huge.len()].copy_from_slice(huge.as_bytes());
            at += i + huge.len();
        }
        assert!(
            memchr::memmem::find(&pdf, huge.as_bytes()).is_some(),
            "{}",
            String::from_utf8_lossy(&pdf)
        );
        pdf
    }

    /// Codex M4 code r9 #5: a font metric lopdf reads as infinite (a real of 40 digits) made
    /// the text's advance NaN, which glued the lines of the page into one. A metric that is
    /// no finite number, or that scaled is none, is left out: the glyph has the default
    /// width. In `/Widths`, `/DW`, `/W`, `/MissingWidth` and a Type3 font's `/FontMatrix`, and in
    /// the operands of the content stream.
    #[test]
    fn a_font_metric_past_f32_has_the_default_width() {
        // The NaN of the advance after `A` was the y of `needle`, which no line is apart from.
        let lines = b"BT /F1 12 Tf 72 700 Td (A) Tj (needle) Tj 0 -30 Td (after) Tj ET";
        for huge in [
            format!(" {}.0", "9".repeat(40)),
            format!(" -{}.0", "9".repeat(40)),
        ] {
            let name = || Object::Name(format!("P{}", "x".repeat(huge.len() - 2)).into_bytes());
            // `Z` has a width, `A` the one of a code without.
            let simple = |key: &'static str, value: Object| {
                simple_font(move |f| {
                    f.set("FirstChar", 90);
                    f.set("Widths", Object::Array(vec![600.into()]));
                    f.set(key, value);
                })
            };
            let mut fd = Dictionary::new();
            fd.set("MissingWidth", name());
            let mut type3 = simple(
                "FontMatrix",
                Object::Array(vec![name(), 0.into(), 0.into()]),
            );
            type3.set("Subtype", Object::Name(b"Type3".to_vec()));
            type3.set("FirstChar", 65);
            let mut widths = simple("Widths", Object::Array(vec![name()]));
            widths.set("FirstChar", 65);
            for font in [
                widths,
                simple("FontDescriptor", Object::Dictionary(fd)),
                type3,
            ] {
                let pdf = with_real(&pdf_with(font, lines, &[]), &huge);
                assert_eq!(text_of(&pdf), "Aneedle\nafter\n", "{huge}");
            }
            let map = cmap(2, &[(0x41, "A"), (0x6e, "n")]);
            let mut pdf = type0(
                map,
                b"BT /F1 12 Tf 72 700 Td <0041> Tj <006e> Tj 0 -30 Td <006e> Tj ET",
            );
            let mut doc = Document::load_mem(&pdf).unwrap();
            for o in doc.objects.values_mut() {
                if let Object::Dictionary(d) = o
                    && let Ok(Object::Array(fonts)) = d.get_mut(b"DescendantFonts")
                    && let Some(Object::Dictionary(cid)) = fonts.first_mut()
                {
                    cid.set("DW", name());
                    cid.set(
                        "W",
                        Object::Array(vec![65.into(), Object::Array(vec![name()])]),
                    );
                }
            }
            pdf.clear();
            doc.save_to(&mut pdf).unwrap();
            assert_eq!(text_of(&with_real(&pdf, &huge)), "An\nn\n", "{huge}");
        }
        let huge = "9".repeat(400);
        // A kerning past an `f64`, wide enough for a space, and a finite scale whose products
        // are not.
        for (content, want) in [
            (
                format!("BT /F1 12 Tf 72 700 Td [(A) -{huge} (needle)] TJ 0 -30 Td (after) Tj ET"),
                "A needle\nafter\n",
            ),
            (
                "BT /F1 12 Tf 1e200 Tz 72 700 Td (A) Tj (needle) Tj 0 -30 Td (after) Tj ET".into(),
                "Aneedle\nafter\n",
            ),
        ] {
            let pdf = pdf_with(simple_font(|_| {}), content.as_bytes(), &[]);
            assert_eq!(text_of(&pdf), want, "{content}");
        }
        for op in ["Tf", "Tc", "Tz"] {
            let content = match op {
                "Tf" => {
                    format!("BT /F1 {huge} Tf 72 700 Td (A) Tj (needle) Tj 0 -30 Td (after) Tj ET")
                }
                _ => format!(
                    "BT /F1 12 Tf {huge} {op} 72 700 Td (A) Tj (needle) Tj 0 -30 Td (after) Tj ET"
                ),
            };
            let pdf = pdf_with(simple_font(|_| {}), content.as_bytes(), &[]);
            assert_eq!(text_of(&pdf), "Aneedle\nafter\n", "{op}");
        }
    }

    #[test]
    fn tj_kerning_wide_enough_is_a_space() {
        let data = b"BT /F1 10 Tf [(ne) -20 (edle) -300 (here)] TJ ET";
        let doc = lopdf::Document::with_version("1.5");
        let r = super::Reader::new(&doc, 0);
        let res = super::Res::new(&r, Vec::new()).unwrap();
        let mut out = super::Layout::new(usize::MAX);
        let gs = super::State::new(super::Matrix::IDENTITY);
        super::run(&r, data, &res, gs, &mut out, 0).unwrap();
        assert_eq!(out.out, "needle here");
    }
}
