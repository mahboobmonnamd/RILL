//! Slice 8 T-CHIP1-BOUNDS.
//!
//! Authority: SPEC-VT-PARSER §6, ADR 0012 D9. The counting allocator is
//! thread-local so sibling tests cannot pollute the measurement.

use rill_vt_types::TerminalEmulation;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use vt_engine::VtEngine;

struct CountingAlloc;

thread_local! {
    static COUNT: Cell<bool> = const { Cell::new(false) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() && COUNT.with(Cell::get) {
            BYTES.with(|n| n.set(n.get().saturating_add(layout.size())));
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn engine() -> VtEngine {
    VtEngine::new(80, 24).expect("vt-engine")
}

fn allocs_during(f: impl FnOnce()) -> usize {
    BYTES.with(|n| n.set(0));
    COUNT.with(|c| c.set(true));
    f();
    COUNT.with(|c| c.set(false));
    BYTES.with(Cell::get)
}

/// Hostile inputs of this size must not grow the heap like the payload.
const HOSTILE: usize = 8 * 1024 * 1024;
/// Anything near the payload is growth; a few KB of runtime noise is not.
const SLACK: usize = 64 * 1024;

fn assert_feed_bounded(vt: &mut VtEngine, bytes: &[u8], name: &str) {
    let grew = allocs_during(|| {
        vt.feed(bytes).expect("feed hostile");
    });
    assert!(
        grew < SLACK,
        "{name}: feed allocated {grew} bytes (cap {SLACK}); hostile input must not grow the heap"
    );
}

/// T-CHIP1-BOUNDS — hostile sequences stay bounded.
///
/// Required mutation: `RILL_MUTATE=unbounded_osc`.
#[test]
fn t_chip1_bounds_hostile_sequences_stay_bounded() {
    let mut osc = Vec::with_capacity(HOSTILE + 2);
    osc.extend_from_slice(b"\x1b]");
    osc.resize(HOSTILE + 2, b'x');

    let mut vt = engine();
    assert_feed_bounded(&mut vt, &osc, "unterminated OSC");

    let mut dcs = Vec::with_capacity(HOSTILE + 2);
    dcs.extend_from_slice(b"\x1bP");
    dcs.resize(HOSTILE + 2, b'x');
    assert_feed_bounded(&mut vt, &dcs, "unterminated DCS");

    let mut csi = Vec::with_capacity(2_000_000);
    csi.extend_from_slice(b"\x1b[");
    for i in 0..1_000_000 {
        if i > 0 {
            csi.push(b';');
        }
        csi.push(b'1');
    }
    csi.push(b'm');
    assert_feed_bounded(&mut vt, &csi, "CSI 1e6 params");

    vt.feed(b"A").expect("print after flood");
    let grid = vt.snapshot().expect("snapshot");
    assert!(
        (0..grid.cols).any(|c| grid.cell(c, 0).expect("cell").codepoint == u32::from(b'A')),
        "feed(b\"A\") after hostile input must still print"
    );
}
