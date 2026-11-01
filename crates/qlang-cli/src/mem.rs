//! Counting the memory the process holds, so that a run can be stopped before the machine
//! runs out (`R904`). The core cannot count by itself: it hands the job to this allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};

/// What every thread has handed over. Signed on purpose: memory allocated by one thread and freed by another is
/// counted as a debit that may arrive before the matching credit, and clamping it at zero would lose it for good
/// (the total then only grows, until `R904` stops a server that holds 14 MB).
static IN_USE: AtomicIsize = AtomicIsize::new(0);

/// Each thread counts on its own and hands its balance over by chunks: an atomic operation
/// on every allocation would show in the interpreter's speed.
const CHUNK: isize = 1 << 18;

/// A thread's balance, handed over when the thread ends too (a connection's thread is short-lived).
struct Local(std::cell::Cell<isize>);

impl Drop for Local {
    fn drop(&mut self) {
        IN_USE.fetch_add(self.0.get(), Relaxed);
    }
}

thread_local! {
    static LOCAL: Local = const { Local(std::cell::Cell::new(0)) };
}

#[inline]
fn count(delta: isize) {
    // `try_with`: the cell is gone while a thread is being torn down; the change then goes straight to the total
    let gone = LOCAL
        .try_with(|local| {
            let v = local.0.get() + delta;
            if v >= CHUNK || v <= -CHUNK {
                IN_USE.fetch_add(v, Relaxed);
                local.0.set(0);
            } else {
                local.0.set(v);
            }
        })
        .is_err();
    if gone {
        IN_USE.fetch_add(delta, Relaxed);
    }
}

/// The system allocator, counting the bytes it has handed out and not got back.
pub struct Counting;

// SAFETY: every call is forwarded unchanged to the system allocator; the counter is only
// bookkeeping on the side.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            count(layout.size() as isize);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            count(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        count(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            count(new_size as isize - layout.size() as isize);
        }
        p
    }
}

pub fn in_use() -> usize {
    IN_USE.load(Relaxed).max(0) as usize
}

/// Default limit: a gigabyte (`--max-memory MB` or `QLANG_MAX_MEMORY`; `0` removes it).
const DEFAULT_MB: usize = 1024;

/// Takes `--max-memory MB` out of the arguments and sets the limit.
pub fn configure(args: &mut Vec<String>) -> Result<(), String> {
    let mut mb = std::env::var("QLANG_MAX_MEMORY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MB);
    while let Some(i) = args.iter().position(|a| a == "--max-memory") {
        let value = args
            .get(i + 1)
            .and_then(|v| v.parse::<usize>().ok())
            .ok_or("--max-memory needs a number of megabytes (0: no limit)")?;
        mb = value;
        args.drain(i..i + 2);
    }
    if mb > 0 {
        qlang_core::host::set_memory_limit(in_use, mb.saturating_mul(1 << 20));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short-lived thread (a connection's) frees what another thread allocated and ends with a negative balance:
    /// that balance used to be dropped with the thread, leaving the total higher for good, until a server that held
    /// 14 MB was stopped by `R904`.
    #[test]
    fn the_balance_of_a_thread_that_ends_is_not_lost() {
        let before = in_use();
        for _ in 0..2000 {
            let block = vec![1u8; 100_000];
            std::thread::spawn(move || drop(block)).join().unwrap();
        }
        let after = in_use();
        assert!(
            after < before + (16 << 20),
            "{before} bytes before, {after} after"
        );
    }
}
