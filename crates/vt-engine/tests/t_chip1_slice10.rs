//! Slice 10 T-CHIP1-MODE.
//!
//! Authority: ADR 0036, SPEC-VT-MODE. Chip 0 stays live; host encoder not wired.

use rill_vt_types::{TerminalEmulation, TerminalModeState};
use vt_engine::VtEngine;

fn engine() -> VtEngine {
    VtEngine::new(80, 24).expect("vt-engine")
}

fn fresh() -> TerminalModeState {
    TerminalModeState::fresh()
}

/// T-CHIP1-MODE — DECCKM, keypad, paste, mouse, focus, alt, DECTCEM.
///
/// Required mutation: `RILL_MUTATE=ignore_mode_updates`.
#[test]
fn t_chip1_mode_tracks_host_encoder_flags() {
    let mut vt = engine();
    assert_eq!(vt.mode_state(), fresh());

    vt.feed(b"\x1b[?1h").expect("DECCKM on");
    assert!(vt.mode_state().application_cursor_keys);
    vt.feed(b"\x1b[?1l").expect("DECCKM off");
    assert!(!vt.mode_state().application_cursor_keys);

    vt.feed(b"\x1b=").expect("DECKPAM");
    assert!(vt.mode_state().application_keypad);
    vt.feed(b"\x1b>").expect("DECKPNM");
    assert!(!vt.mode_state().application_keypad);

    vt.feed(b"\x1b[?2004h").expect("bracketed paste on");
    assert!(vt.mode_state().bracketed_paste);
    vt.feed(b"\x1b[?2004l").expect("bracketed paste off");
    assert!(!vt.mode_state().bracketed_paste);

    vt.feed(b"\x1b[?1006h").expect("SGR mouse on");
    assert!(vt.mode_state().mouse_sgr);
    vt.feed(b"\x1b[?1006l").expect("SGR mouse off");
    assert!(!vt.mode_state().mouse_sgr);

    vt.feed(b"\x1b[?1000h").expect("X10 mouse on");
    assert!(vt.mode_state().mouse_x10);
    vt.feed(b"\x1b[?1002h").expect("button mouse on");
    assert!(vt.mode_state().mouse_button);
    vt.feed(b"\x1b[?1003h").expect("any mouse on");
    assert!(vt.mode_state().mouse_any);

    vt.feed(b"\x1b[?1004h").expect("focus on");
    assert!(vt.mode_state().focus_events);
    vt.feed(b"\x1b[?1004l").expect("focus off");
    assert!(!vt.mode_state().focus_events);

    vt.feed(b"\x1b[?1049h").expect("alt on");
    assert!(vt.mode_state().alternate_screen);
    vt.feed(b"\x1b[?1049l").expect("alt off");
    assert!(!vt.mode_state().alternate_screen);

    vt.feed(b"\x1b[?25l").expect("hide cursor");
    assert!(!vt.mode_state().cursor_visible);
    vt.feed(b"\x1b[?25h").expect("show cursor");
    assert!(vt.mode_state().cursor_visible);
}

/// `reset()` restores encoder defaults.
#[test]
fn t_chip1_mode_reset_restores_defaults() {
    let mut vt = engine();
    vt.feed(b"\x1b[?1h\x1b[?2004h\x1b[?1006h\x1b=")
        .expect("modes on");
    vt.reset().expect("reset");
    assert_eq!(vt.mode_state(), fresh());
}
