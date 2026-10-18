//! Word, Excel and PowerPoint (spec §5.8). Stub of Chunk 4; Chunk 6 writes it.
use super::{Builder, DocError};

pub fn docx(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 6"))
}

pub fn xlsx(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 6"))
}

pub fn pptx(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 6"))
}
