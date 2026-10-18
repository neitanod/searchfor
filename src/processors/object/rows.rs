//! The rows of an object file (spec §5.7). Stub of C1; Chunk 2 writes it.
use crate::model::Line;
use crate::trail::LineIndex;
use std::ops::Range;

pub const SIDE: usize = 4096;

pub fn rows(
    _bytes: &[u8],
    _hits: &[Range<usize>],
    _limit: Option<u64>,
    _lines: &LineIndex,
) -> Vec<Line> {
    Vec::new()
}
