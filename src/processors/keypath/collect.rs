//! The keypath collector: open and close events in file order, kept only where an anchor
//! needs them (design note 6). C2 fills it.

pub struct Collector<'a> {
    _anchors: &'a [usize],
}
