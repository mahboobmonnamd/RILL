//! T-LIVE-* gates — M7 warm-path wiring (SPEC-VT-LIVE-SWAP §6).

use rill_host::{skip_mode_poll_mutate, skip_reply_drain_mutate};
use rill_vt_types::{TerminalEmulation, TerminalModeState};
use vt_engine::VtEngine;

/// T-LIVE-REPLY — DSR bytes are available to drain after feed.
#[test]
fn t_live_swap_dsr_reply_is_available_after_feed() {
    let mut chip = VtEngine::new(80, 24).expect("vt");
    chip.feed(b"\x1b[5;3H\x1b[6n").expect("feed");
    let reply = chip.take_replies().expect("take");
    let text = String::from_utf8_lossy(&reply);
    assert!(
        text.contains("5;3R"),
        "DSR must be queued for attach DATA drain, got {text:?}"
    );
    assert!(
        chip.take_replies().expect("drain").is_empty(),
        "second take_replies must be empty"
    );
}

/// T-LIVE-REPLY — replies must be forwarded as attach DATA unless mutated.
#[test]
fn t_live_swap_outbound_data_includes_dsr() {
    let mut chip = VtEngine::new(80, 24).expect("vt");
    chip.feed(b"\x1b[6n").expect("feed");
    let forward = !skip_reply_drain_mutate() && chip.has_replies();
    assert!(
        forward,
        "DSR must be forwarded as attach DATA (skip_reply_drain breaks this)"
    );
}

/// T-LIVE-MODE — DECCKM is tracked after feed (same path as `Client::after_feed`).
#[test]
fn t_live_swap_mode_state_tracks_decckm() {
    let mut chip = VtEngine::new(80, 24).expect("vt");
    chip.feed(b"\x1b[?1h").expect("feed");
    let modes = if skip_mode_poll_mutate() {
        TerminalModeState::fresh()
    } else {
        chip.mode_state()
    };
    assert!(
        modes.application_cursor_keys,
        "host encoder must read application cursor keys from mode_state"
    );
}
