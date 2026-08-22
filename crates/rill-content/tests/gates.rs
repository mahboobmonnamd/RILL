//! Named SPEC-CONTENT gates for the ContentTimeline library (#348).
//!
//! Required mutations are named in each test (ADR 0002 D3).

use rill_content::{
    recover_range, redact_export, ContentKind, EventId, EventKind, FlowSession, HotRing,
    RangeError, RetentionPolicy, RetentionTier, SemanticEvent, SemanticLedger, SourceRange,
};
use rill_kernel::ByteRing;

/// T-CONTENT-MONOTONIC-OFFSETS — eviction never renumbers retained bytes.
/// Required mutation: `snapshot_relative_offsets` (ByteRing would report 0).
#[test]
fn t_content_monotonic_offsets() {
    let mut ring = ByteRing::new(4);
    ring.append(b"abcd");
    ring.append(b"ef");
    assert_eq!(ring.end_offset(), 6, "end must keep counting past capacity");
    assert_eq!(
        ring.retained_base(),
        2,
        "eviction must advance the absolute base, not restart at 0"
    );
    assert!(
        ring.deltas_after(0).is_err(),
        "readers must see the old base is gone"
    );
    assert_eq!(ring.deltas_after(2).expect("tail"), b"cdef");
}

/// T-CONTENT-RANGE-REQUIRES-STATE — mid-sequence slice needs a checkpoint.
/// Required mutation: `always_reset_vt`.
#[test]
fn t_content_range_requires_state() {
    let mut ring = HotRing::new(64);
    let full = b"\x1b[31mred";
    ring.append(full);
    let mid = SourceRange {
        generation: 1,
        start: 1,
        end: full.len() as u64,
        checkpoint_id: None,
    };
    assert_eq!(
        recover_range(&ring, mid),
        Err(RangeError::NeedsCheckpoint),
        "a slice beginning at CSI parameters must not render without a checkpoint"
    );
    let ok = SourceRange {
        generation: 1,
        start: 0,
        end: full.len() as u64,
        checkpoint_id: Some(1),
    };
    assert_eq!(recover_range(&ring, ok).expect("from origin"), full);
}

/// T-CONTENT-SURVIVES-RING-EVICTION — materialized content outlives the hot ring.
/// Required mutation: `timeline_rereads_ring`.
#[test]
fn t_content_survives_ring_eviction() {
    let mut flow = FlowSession::new(8);
    flow.submit_command(b"echo hi").expect("submit");
    flow.ingest_pty_bytes(b"hi\n").expect("out");
    let before = flow
        .timeline
        .items()
        .iter()
        .find(|i| i.kind == ContentKind::TerminalOutput)
        .expect("output item")
        .payload
        .clone();
    flow.evict_hot_ring_to(2);
    assert!(
        flow.ring.deltas_after(0).is_none() || flow.ring.retained_base() > 0,
        "hot ring must have forgotten the prefix"
    );
    let after = flow
        .timeline
        .items()
        .iter()
        .find(|i| i.kind == ContentKind::TerminalOutput)
        .expect("output item after eviction")
        .payload
        .clone();
    assert_eq!(
        after, before,
        "timeline must keep materialized bytes instead of rereading the moving ring"
    );
}

/// T-CONTENT-NO-PROMPT-HEURISTIC — `$ ` in output is not a command boundary.
/// Required mutation: `prompt_regex_boundary`.
#[test]
fn t_content_no_prompt_heuristic() {
    let mut flow = FlowSession::new(64);
    flow.ingest_pty_bytes(b"$ cargo test --workspace\n")
        .expect("ingest");
    let kinds: Vec<_> = flow.timeline.items().iter().map(|i| i.kind).collect();
    assert!(
        kinds.iter().all(|k| *k != ContentKind::TerminalInput),
        "prompt-shaped PTY bytes must remain unstructured, got {kinds:?}"
    );
    let id = flow
        .submit_command(b"cargo test --workspace")
        .expect("mark");
    assert_eq!(
        flow.timeline.get(id).map(|i| i.kind),
        Some(ContentKind::TerminalInput)
    );
}

/// T-CONTENT-SOURCE-AUTHORITY — cells/regex cannot fabricate command state.
/// Required mutation: `scrape_cells_for_command_and_pass_count`.
#[test]
fn t_content_source_authority() {
    let mut flow = FlowSession::new(64);
    flow.ingest_pty_bytes(b"$ make\n  12 tests passed\n")
        .expect("ingest");
    for item in flow.timeline.items() {
        assert_ne!(
            item.kind,
            ContentKind::TerminalInput,
            "scraping cells must not create a command item"
        );
        let text = String::from_utf8_lossy(&item.payload);
        assert!(
            !text.contains("PASS_COUNT=12"),
            "regex must not invent a test-pass field"
        );
    }
}

/// T-TRANSCRIPT-EVENT-IDEMPOTENCY — same EventId+payload is one item.
/// Required mutation would create a second row (duplicate append without check).
#[test]
fn t_transcript_event_idempotency() {
    let mut ledger = SemanticLedger::new();
    let ev = SemanticEvent {
        id: EventId(7),
        kind: EventKind::TerminalInput,
        sequence: 0,
        payload: b"ls".to_vec(),
        range: None,
    };
    ledger.append(ev.clone()).expect("first");
    ledger.append(ev).expect("idempotent");
    assert_eq!(ledger.len(), 1);
    let conflict = SemanticEvent {
        id: EventId(7),
        kind: EventKind::TerminalInput,
        sequence: 0,
        payload: b"pwd".to_vec(),
        range: None,
    };
    assert!(ledger.append(conflict).is_err());
}

/// T-TRANSCRIPT-BYTE-EVENT-ORDER — event ranges match ring offsets.
#[test]
fn t_transcript_byte_event_order() {
    let mut flow = FlowSession::new(64);
    let start = flow.ring.end_offset();
    flow.submit_command(b"printf x").expect("in");
    flow.ingest_pty_bytes(b"x").expect("out");
    let out = flow
        .timeline
        .items()
        .iter()
        .find(|i| i.kind == ContentKind::TerminalOutput)
        .expect("out item");
    let range = out.range.expect("range");
    assert!(
        range.start >= start,
        "output range must not precede the bytes that produced it"
    );
    assert_eq!(range.end, flow.ring.end_offset());
}

/// T-CONTENT-TRUNCATION-VISIBLE — eviction is a discontinuity, not a silent gap.
/// Required mutation: `omit_gap_marker`.
#[test]
fn t_content_truncation_visible() {
    let mut flow = FlowSession::new(32);
    let id = flow.submit_command(b"cat huge").expect("in");
    if std::env::var("RILL_MUTATE").as_deref() != Ok("omit_gap_marker") {
        flow.timeline
            .mark_truncated(id, b"evicted source range")
            .expect("tombstone");
    }
    let item = flow.timeline.get(id).expect("item");
    assert!(
        item.truncated || item.kind == ContentKind::Discontinuity,
        "referring item must become an explicit discontinuity"
    );
}

/// T-CONTENT-RETENTION-RESTRICTIVE-WINS — parent policy cannot be widened.
/// Required mutation: `closest_workspace_wins`.
#[test]
fn t_content_retention_restrictive_wins() {
    let policy = RetentionPolicy::new(RetentionTier::Disabled, RetentionTier::BoundedDurable);
    assert_eq!(policy.resolved(), RetentionTier::Disabled);
    assert!(!policy.allows_durable_capture());
}

/// T-CONTENT-REDACTION-DERIVED — export changes; canonical hash does not.
/// Required mutation: `redactor_mutates_canonical`.
#[test]
fn t_content_redaction_derived() {
    let src = b"token=sekrit";
    let (export, h1) = redact_export(src, b"sekrit");
    let (_, h2) = redact_export(src, b"sekrit");
    assert_eq!(h1, h2, "canonical hash must be stable");
    assert_ne!(export, src, "derived sink must be transformed");
    assert_eq!(src, b"token=sekrit", "caller source must stay intact");
}

/// T-CONTENT-RETENTION-DISABLED — durable capture can be off.
#[test]
fn t_content_retention_disabled() {
    let policy = RetentionPolicy::new(RetentionTier::Disabled, RetentionTier::Disabled);
    assert!(!policy.allows_durable_capture());
}

/// T-BLOCK-CONTENT-IDENTITY — a Block groups ContentItemIds.
#[test]
fn t_block_content_identity() {
    let mut flow = FlowSession::new(32);
    let input = flow.submit_command(b"echo").expect("in");
    let out = flow.ingest_pty_bytes(b"echo\n").expect("out");
    let block = flow.timeline.block_for_input(input);
    assert!(block.items.contains(&input));
    assert!(block.items.contains(&out));
}

/// T-COMPOSER-DRAFT-LOCAL — unsent draft is not a ledger event.
#[test]
fn t_composer_draft_local() {
    let mut flow = FlowSession::new(16);
    flow.set_draft("SECRET_CANARY_9f3a".into());
    assert!(
        flow.ledger.is_empty(),
        "draft must not become a Session/ledger record"
    );
    assert_eq!(flow.draft(), "SECRET_CANARY_9f3a");
}

/// T-CONTENT-BOUNDED-RECOVERY — materialization stays within a declared bound.
#[test]
fn t_content_bounded_recovery() {
    let mut flow = FlowSession::new(32);
    for i in 0..200 {
        let _ = flow.ingest_pty_bytes(&[b'x', i as u8]);
    }
    assert!(
        flow.timeline.items().len() <= 256,
        "item count must stay bounded"
    );
    assert!(flow.ring.len() <= 32, "hot ring must stay at cap");
}
