//! Counting the memory the process holds, so that a run can be stopped before the machine
//! runs out (`R904`). The core cannot count by itself: it hands the job to this allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

static IN_USE: AtomicUsize = AtomicUsize::new(0);

/// Each thread counts on its own and hands its balance over by chunks: an atomic operation
/// on every allocation would show in the interpreter's speed.
const CHUNK: isize = 1 << 18;

thread_local! {
    static LOCAL: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

#[inline]
fn count(delta: isize) {
    // `try_with`: the cell is gone while a thread is being torn down
    let _ = LOCAL.try_with(|c| {
        let v = c.get() + delta;
        if v >= CHUNK || v <= -CHUNK {
            if v >= 0 {
                IN_USE.fetch_add(v as usize, Relaxed);
            } else {
                // memory freed here that another thread allocated: never below zero
                let mut cur = IN_USE.load(Relaxed);
                while let Err(seen) = IN_USE.compare_exchange_weak(
                    cur,
                    cur.saturating_sub((-v) as usize),
                    Relaxed,
                    Relaxed,
                ) {
                    cur = seen;
                }
            }
            c.set(0);
        } else {
            c.set(v);
        }
    });
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
    IN_USE.load(Relaxed)
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
