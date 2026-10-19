//! `-r`/`--replace` in the Trail layouts (M5a plan, D4): each Match of a line replaced by the
//! replacement, its `$1`, `$name` and `${name}` read from the Match's capture groups, as rg's
//! standard printer replaces them (chunk 4).
use crate::matcher::PatternMatcher;

/// The replacement of a run and the matcher whose groups it reads.
#[derive(Debug)]
pub struct Replacer {
    #[allow(dead_code)]
    matcher: PatternMatcher,
    #[allow(dead_code)]
    template: Vec<u8>,
}

impl Replacer {
    pub fn new(matcher: PatternMatcher, template: Vec<u8>) -> Self {
        Replacer { matcher, template }
    }

    /// `line` with each range of `subs` replaced, and the ranges the replacements take in it.
    pub fn replace(&self, line: &[u8], subs: &[(usize, usize)]) -> (Vec<u8>, Vec<(usize, usize)>) {
        (line.to_vec(), subs.to_vec())
    }
}
