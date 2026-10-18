//! The PDF writers (M4 plan, D19). Stub of Chunk 4; Chunk 9 writes it.
pub fn pdf(_pages: &[&[&str]], _bookmarks: &[(u8, &str, usize)]) -> Vec<u8> {
    unimplemented!("written in Chunk 9")
}

pub fn pdf_encrypted(_pages: &[&[&str]], _user: &str) -> Vec<u8> {
    unimplemented!("written in Chunk 9")
}
