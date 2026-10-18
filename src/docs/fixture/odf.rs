//! The OpenDocument writers (M4 plan, D19). Stub of Chunk 4; Chunk 7 writes it.
use super::Block;

pub fn odt(_blocks: &[Block]) -> Vec<u8> {
    unimplemented!("written in Chunk 7")
}

pub fn ods(_sheets: &[(&str, &[&[&str]])]) -> Vec<u8> {
    unimplemented!("written in Chunk 7")
}

pub fn odp(_slides: &[(&[&str], Option<&str>)]) -> Vec<u8> {
    unimplemented!("written in Chunk 7")
}
