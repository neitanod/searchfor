//! OpenDocument (spec §5.8). Stub of Chunk 4; Chunk 7 writes it.
use super::{Builder, DocError};

pub fn odt(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 7"))
}

pub fn ods(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 7"))
}

pub fn odp(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 7"))
}
