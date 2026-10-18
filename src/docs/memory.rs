//! The readers' memory counter (M4 plan, D31): the allocator of every s4 process is the
//! system's, with a count of the bytes a document reader holds. `isolated::child` arms it
//! with the reader's limit; past it an allocation is refused, the same on Linux, macOS and
//! Windows, and the reader gives the memory warning, as one past `RLIMIT_DATA` gives it:
//!
//! - a `try_reserve` sees `Err`, which a reader returns as an error ending `out of memory`,
//!   and `isolated::refused_memory` turns into the warning;
//! - any other allocation goes to `std::alloc::handle_alloc_error`, whose default prints
//!   `memory allocation of N bytes failed` and aborts the reader: its parent reads that line in
//!   the reader's stderr and gives the warning (`isolated::why`);
//! - and a reader that saw the refusal and went on (lopdf ends a stream where its
//!   `read_to_end` failed, and returns what it read) finds it in `take_refused` after the
//!   document, and gives the warning for whatever it answered (M4 plan r8, Codex #1).
//!
//! Not armed, in s4's own process and in every test binary, each allocation and free costs
//! one relaxed load of `limit`, a value no one writes there, and a branch: the count itself
//! is never touched.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering::Relaxed};

/// The system's allocator, counting the live bytes once armed.
pub struct Counter {
    /// The most bytes live at once; 0 until `arm`, and then nothing is counted.
    limit: AtomicUsize,
    /// The bytes allocated since `arm` and not yet freed: below 0 when blocks allocated
    /// before it are freed after it, which loosens the limit by those few bytes and never
    /// makes it refuse everything.
    live: AtomicIsize,
    /// Whether an allocation was refused, by the count or by the system, since the last
    /// `take_refused`.
    refused: AtomicBool,
}

/// The allocator of every binary that links this library: such a binary, as `helpers/example`
/// and `helpers/grammar` are, cannot declare a `#[global_allocator]` of its own (M4 plan, D31;
/// r8, Fable #4).
#[global_allocator]
static COUNTER: Counter = Counter::new();

/// From now on, in this process, an allocation that takes the live bytes past `bytes` is
/// refused. Called once, by a reader, before its first document.
pub fn arm(bytes: usize) {
    COUNTER.arm(bytes);
}

/// Whether this process was refused an allocation since the last call: a reader asks after
/// each document, and before the next (M4 plan r8, Codex #1).
pub fn take_refused() -> bool {
    COUNTER.take_refused()
}

/// `mib` MiB in bytes, at most `isize::MAX` (`SEARCHFOR_READER_MIB` may say anything).
pub fn bytes_of_mib(mib: u64) -> usize {
    mib.saturating_mul(1 << 20).min(isize::MAX as u64) as usize
}

impl Counter {
    pub const fn new() -> Counter {
        Counter {
            limit: AtomicUsize::new(0),
            live: AtomicIsize::new(0),
            refused: AtomicBool::new(false),
        }
    }

    fn take_refused(&self) -> bool {
        self.refused.swap(false, Relaxed)
    }

    fn arm(&self, bytes: usize) {
        self.live.store(0, Relaxed);
        self.limit
            .store(bytes.clamp(1, isize::MAX as usize), Relaxed);
    }

    /// Whether the counter counts: false until `arm`, and so in s4's own process.
    #[inline(always)]
    fn armed(&self) -> bool {
        self.limit.load(Relaxed) != 0
    }

    /// Whether `n` more bytes fit under the armed limit, counting them when they do.
    fn take(&self, n: usize) -> bool {
        let limit = self.limit.load(Relaxed) as isize;
        // A Layout's size is at most isize::MAX.
        let n = n as isize;
        let before = self.live.fetch_add(n, Relaxed);
        if before.saturating_add(n) > limit {
            self.live.fetch_sub(n, Relaxed);
            self.refused.store(true, Relaxed);
            return false;
        }
        true
    }

    fn give(&self, n: usize) {
        self.live.fetch_sub(n as isize, Relaxed);
    }

    /// An allocation while armed: counted first, given back if the system refuses it.
    #[inline]
    unsafe fn counted(&self, layout: Layout, zeroed: bool) -> *mut u8 {
        if !self.take(layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: the caller's layout, as GlobalAlloc::alloc requires; calloc for zeroed
        // blocks, whose pages come zeroed.
        let p = unsafe {
            if zeroed {
                System.alloc_zeroed(layout)
            } else {
                System.alloc(layout)
            }
        };
        if p.is_null() {
            self.give(layout.size());
            self.refused.store(true, Relaxed);
        }
        p
    }

    #[inline]
    unsafe fn counted_realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        if new_size > old && !self.take(new_size - old) {
            // Refused: the block stays as it was, as a failed realloc leaves it.
            return std::ptr::null_mut();
        }
        // SAFETY: `ptr` came from `System` with `layout`; `new_size` is the caller's.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if p.is_null() {
            if new_size > old {
                self.give(new_size - old);
            }
            self.refused.store(true, Relaxed);
        } else if new_size < old {
            self.give(old - new_size);
        }
        p
    }
}

impl Default for Counter {
    fn default() -> Counter {
        Counter::new()
    }
}

// SAFETY: every block comes from `System` and goes back to it with the same layout; the
// counter only refuses, it never changes a block. Not armed, each method is one load, one
// branch and a jump to the system's own function, the last thing it does. `inline(never)`:
// inlined at every allocation site by LTO, the counter made the release binary about 400 KB
// larger (M4 plan, D31).
unsafe impl GlobalAlloc for Counter {
    #[inline(never)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if self.armed() {
            // SAFETY: the caller's contract, passed on.
            return unsafe { self.counted(layout, false) };
        }
        // SAFETY: the caller's layout, as GlobalAlloc::alloc requires.
        unsafe { System.alloc(layout) }
    }

    #[inline(never)]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if self.armed() {
            // SAFETY: the caller's contract, passed on.
            return unsafe { self.counted(layout, true) };
        }
        // SAFETY: as in alloc; the system's calloc.
        unsafe { System.alloc_zeroed(layout) }
    }

    #[inline(never)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.armed() {
            self.give(layout.size());
        }
        // SAFETY: `ptr` came from `System` with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    #[inline(never)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if self.armed() {
            // SAFETY: the caller's contract, passed on.
            return unsafe { self.counted_realloc(ptr, layout, new_size) };
        }
        // SAFETY: `ptr` came from `System` with `layout`; `new_size` is the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: usize = 1 << 20;

    fn layout(n: usize) -> Layout {
        Layout::from_size_align(n, 8).unwrap()
    }

    /// D31: an armed counter refuses the allocation that would take its live bytes past the
    /// limit, and counts the bytes freed, so the same request fits again after a free.
    #[test]
    fn an_armed_counter_refuses_past_its_limit() {
        let c = Counter::new();
        c.arm(4 * MIB);
        // SAFETY: each block is freed once, with the layout it was allocated with.
        unsafe {
            let a = c.alloc(layout(3 * MIB));
            assert!(!a.is_null());
            assert!(c.alloc(layout(2 * MIB)).is_null(), "3 + 2 MiB is past 4");
            let z = c.alloc_zeroed(layout(MIB));
            assert!(!z.is_null(), "3 + 1 MiB is the limit itself");
            assert!(c.alloc_zeroed(layout(1)).is_null());
            c.dealloc(a, layout(3 * MIB));
            let b = c.alloc(layout(2 * MIB));
            assert!(!b.is_null());
            c.dealloc(b, layout(2 * MIB));
            c.dealloc(z, layout(MIB));
        }
        assert_eq!(c.live.load(Relaxed), 0);
    }

    /// D31: a realloc counts only what it adds or gives back, and a refused one keeps the block.
    #[test]
    fn a_realloc_counts_its_difference() {
        let c = Counter::new();
        c.arm(4 * MIB);
        // SAFETY: the block is valid until its dealloc, with the layout of its last size.
        unsafe {
            let a = c.alloc(layout(MIB));
            a.write(7);
            let a = c.realloc(a, layout(MIB), 3 * MIB);
            assert!(!a.is_null());
            assert_eq!(c.live.load(Relaxed), 3 * MIB as isize);
            assert!(
                c.realloc(a, layout(3 * MIB), 5 * MIB).is_null(),
                "past 4 MiB"
            );
            assert_eq!(a.read(), 7, "the refused realloc left the block as it was");
            let a = c.realloc(a, layout(3 * MIB), MIB);
            assert_eq!(c.live.load(Relaxed), MIB as isize);
            c.dealloc(a, layout(MIB));
        }
        assert_eq!(c.live.load(Relaxed), 0);
    }

    /// D31: a counter that was never armed, as in s4's own process, counts nothing and
    /// refuses nothing; a block allocated before `arm` and freed after it leaves the count
    /// below 0, never wrapped around to a refusal of everything.
    #[test]
    fn a_counter_not_armed_counts_nothing() {
        let c = Counter::new();
        // SAFETY: each block is freed once, with its layout.
        unsafe {
            let before = c.alloc(layout(2 * MIB));
            assert_eq!(c.live.load(Relaxed), 0);
            c.arm(MIB);
            c.dealloc(before, layout(2 * MIB));
            assert_eq!(c.live.load(Relaxed), -2 * MIB as isize);
            let a = c.alloc(layout(MIB));
            assert!(!a.is_null());
            c.dealloc(a, layout(MIB));
        }
    }

    /// M4 plan r8, Codex #1: a refusal is remembered until it is taken, so a reader whose
    /// document went on without the memory it was refused (lopdf ends a stream where its
    /// `read_to_end` failed) still gives the memory warning; taking it clears it for the next
    /// document.
    #[test]
    fn a_refusal_is_remembered_until_taken() {
        let c = Counter::new();
        c.arm(4 * MIB);
        assert!(!c.take_refused());
        // SAFETY: the one block is freed once, with the layout of its last size.
        unsafe {
            assert!(c.alloc(layout(5 * MIB)).is_null());
            assert!(c.take_refused(), "the refused alloc");
            assert!(!c.take_refused(), "taken, the next document starts clean");
            let a = c.alloc(layout(MIB));
            assert!(!a.is_null());
            assert!(c.realloc(a, layout(MIB), 8 * MIB).is_null());
            assert!(c.take_refused(), "the refused realloc");
            c.dealloc(a, layout(MIB));
        }
        assert!(!c.take_refused(), "what fits refuses nothing");
    }

    /// D31: the limit in MiB, as `child` arms it, saturates instead of overflowing.
    #[test]
    fn a_limit_in_mib_saturates() {
        assert_eq!(bytes_of_mib(1024), 1024 * MIB);
        assert_eq!(bytes_of_mib(u64::MAX), isize::MAX as usize);
    }
}
