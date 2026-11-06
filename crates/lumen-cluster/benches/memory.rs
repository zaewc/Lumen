//! Peak heap memory while clustering (#778, #779): the most bytes allocated at
//! once during one `group` call, beyond the input vectors themselves. Run with
//! `cargo bench -p lumen-cluster --bench memory`; it prints one line per case.

// A measurement binary: it reports to the terminal and aborts on failure.
#![allow(clippy::expect_used, clippy::print_stdout, missing_docs)]
// The counting allocator below is the only way to see heap use from inside
// the process; it forwards every call to the system allocator unchanged.
#![allow(unsafe_code)]

mod data;

use std::alloc::{GlobalAlloc, Layout, System};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use data::{Kind, THRESHOLD, vectors};

/// The system allocator, counting live bytes and their peak.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: invariant: every call is forwarded to `System` with the same layout
// and pointer, so `Counting` upholds `GlobalAlloc`'s contract exactly when
// `System` does. Why unsafe: `GlobalAlloc` is an unsafe trait. Why it holds:
// the counters are atomics updated beside the forwarded calls and never
// change what is allocated or freed.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: invariant: `layout` is the caller's valid, non-zero layout.
        // Why unsafe: forwarding an unsafe allocator call. Why it holds: it is
        // passed on unchanged.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: invariant: `ptr` came from this allocator with `layout`. Why
        // unsafe: forwarding an unsafe allocator call. Why it holds: every
        // allocation here came from `System` with the same layout.
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: invariant: `ptr` came from this allocator with `layout`, and
        // `new_size` is the caller's valid size. Why unsafe: forwarding an
        // unsafe allocator call. Why it holds: both are passed on unchanged.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            let live = LIVE.fetch_add(new_size, Ordering::Relaxed) + new_size;
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn main() {
    let workers = std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    for (kind, count) in [
        (Kind::Sparse, 20_000),
        (Kind::Mixed, 20_000),
        (Kind::Dense, 5_000),
        (Kind::Dense, 10_000),
        (Kind::Dense, 20_000),
    ] {
        let data = vectors(kind, count, 128);
        let refs: Vec<&[f32]> = data.iter().map(Vec::as_slice).collect();
        let before = LIVE.load(Ordering::Relaxed);
        PEAK.store(before, Ordering::Relaxed);
        let started = std::time::Instant::now();
        let groups =
            lumen_cluster::group_with_workers(&refs, THRESHOLD, workers).expect("valid input");
        let elapsed = started.elapsed();
        let peak = PEAK.load(Ordering::Relaxed) - before;
        let distinct = groups.iter().enumerate().filter(|(i, g)| i == *g).count();
        println!(
            "{:>6} {count:>6} x 128: peak {:>8.1} MiB above the input, {distinct} groups, {:.2} s",
            kind.name(),
            f64::from(u32::try_from(peak >> 10).unwrap_or(u32::MAX)) / 1024.0,
            elapsed.as_secs_f64(),
        );
    }
}
