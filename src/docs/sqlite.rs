//! SQLite (spec §5.8). Stub of Chunk 4; Chunk 10 writes it.
use super::{Builder, DocError};

pub fn extract(_bytes: &[u8], _b: &mut Builder) -> Result<(), DocError> {
    Err(DocError::broken("written in Chunk 10"))
}
