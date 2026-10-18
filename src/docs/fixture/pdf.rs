//! The PDF writers (M4 plan, D19): lopdf's own writer, and its RC4 encryption.

/// A PDF with Helvetica text, one `Tj` per line 14 points apart, its streams compressed, and
/// `bookmarks` as (level, title, 1-based page).
pub fn pdf(pages: &[&[&str]], bookmarks: &[(u8, &str, usize)]) -> Vec<u8> {
    let mut doc = pdf_doc(pages, bookmarks);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// `pdf`, encrypted (RC4, 128 bits) with `user` as the password needed to open it ("" opens
/// it without one, as most encrypted PDFs).
pub fn pdf_encrypted(pages: &[&[&str]], user: &str) -> Vec<u8> {
    let mut doc = pdf_doc(pages, &[]);
    let version = lopdf::EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: user,
        key_length: 128,
        permissions: lopdf::Permissions::all(),
    };
    let state = lopdf::EncryptionState::try_from(version).unwrap();
    doc.encrypt(&state).unwrap();
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

fn pdf_doc(pages: &[&[&str]], bookmarks: &[(u8, &str, usize)]) -> lopdf::Document {
    use lopdf::content::{Content, Operation};
    use lopdf::{Dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    // The file identifier, which encryption keys are derived from.
    let id = Object::String(
        b"searchfor-fixtur".to_vec(),
        lopdf::StringFormat::Hexadecimal,
    );
    doc.trailer.set("ID", Object::Array(vec![id.clone(), id]));
    let pages_id = doc.new_object_id();
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let font_id = doc.add_object(font);
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));
    let mut res = Dictionary::new();
    res.set("Font", Object::Dictionary(fonts));
    let res_id = doc.add_object(res);
    let mut kids = Vec::new();
    for lines in pages {
        let mut ops = vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new("Td", vec![72.into(), 760.into()]),
        ];
        for (i, l) in lines.iter().enumerate() {
            if i > 0 {
                ops.push(Operation::new("Td", vec![0.into(), (-14).into()]));
            }
            // WinAnsi: the Latin-1 letters of the line, as one byte each.
            let bytes: Vec<u8> = l
                .chars()
                .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
                .collect();
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(bytes, lopdf::StringFormat::Literal)],
            ));
        }
        ops.push(Operation::new("ET", vec![]));
        let content = Content { operations: ops };
        let mut stream = Stream::new(Dictionary::new(), content.encode().unwrap());
        let _ = stream.compress();
        let content_id = doc.add_object(stream);
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set("Contents", Object::Reference(content_id));
        kids.push(Object::Reference(doc.add_object(page)));
    }
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Count", Object::Integer(kids.len() as i64));
    pages_dict.set("Kids", Object::Array(kids.clone()));
    pages_dict.set("Resources", Object::Reference(res_id));
    pages_dict.set(
        "MediaBox",
        Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
    );
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    if !bookmarks.is_empty() {
        let outlines_id = doc.new_object_id();
        // Items are written flat per level: each item's parent is the last item one level up.
        let ids: Vec<_> = bookmarks.iter().map(|_| doc.new_object_id()).collect();
        let mut dicts: Vec<Dictionary> = Vec::new();
        for (i, (level, title, page)) in bookmarks.iter().enumerate() {
            let mut d = Dictionary::new();
            d.set(
                "Title",
                Object::String(title.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            );
            let parent = (0..i)
                .rev()
                .find(|&j| bookmarks[j].0 < *level)
                .map_or(outlines_id, |j| ids[j]);
            d.set("Parent", Object::Reference(parent));
            d.set(
                "Dest",
                Object::Array(vec![kids[page - 1].clone(), Object::Name(b"Fit".to_vec())]),
            );
            dicts.push(d);
        }
        for i in 0..bookmarks.len() {
            let level = bookmarks[i].0;
            // The next sibling: the next item at this level before one at a higher level.
            if let Some(j) = (i + 1..bookmarks.len())
                .take_while(|&j| bookmarks[j].0 >= level)
                .find(|&j| bookmarks[j].0 == level)
            {
                dicts[i].set("Next", Object::Reference(ids[j]));
            }
            if let Some(j) = (i + 1..bookmarks.len())
                .next()
                .filter(|&j| bookmarks[j].0 == level + 1)
            {
                dicts[i].set("First", Object::Reference(ids[j]));
            }
        }
        for (id, d) in ids.iter().zip(dicts) {
            doc.objects.insert(*id, Object::Dictionary(d));
        }
        let mut outlines = Dictionary::new();
        outlines.set("Type", Object::Name(b"Outlines".to_vec()));
        outlines.set("First", Object::Reference(ids[0]));
        doc.objects
            .insert(outlines_id, Object::Dictionary(outlines));
        catalog.set("Outlines", Object::Reference(outlines_id));
    }
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc.compress();
    doc
}
