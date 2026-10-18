//! The Word, Excel and PowerPoint writers (M4 plan, D19). Stub of Chunk 4; Chunk 6 writes it.
use super::Block;

pub fn docx(_blocks: &[Block]) -> Vec<u8> {
    unimplemented!("written in Chunk 6")
}

pub fn xlsx(_sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    unimplemented!("written in Chunk 6")
}

pub fn pptx(_slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    unimplemented!("written in Chunk 6")
}
