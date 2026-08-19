#![no_main]

use libfuzzer_sys::fuzz_target;
use vt_engine::{TerminalEmulation, VtEngine};

fuzz_target!(|data: &[u8]| {
    let mut vt = match VtEngine::new(80, 24) {
        Ok(v) => v,
        Err(_) => return,
    };
    let _ = vt.feed(data);
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cells.len(), 80 * 24, "grid must stay cols*rows");
    let replies = vt.take_replies().expect("take_replies");
    assert!(replies.len() <= 1024, "reply buffer cap");
});
