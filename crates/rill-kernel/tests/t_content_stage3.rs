use rill_kernel::{
    ByteRing, CaptureOutcome, ContentEvent, ContentKind, ContentTimeline, Discipline,
    DurableTranscript, RedactionRule, ReplayState, ScreenMode, Session, SourceRange,
    TranscriptError, TranscriptLedger, Winsize,
};

/// T-CONTENT-RANGE-REQUIRES-STATE — a range beginning after stream origin is
/// unusable without a compatible checkpoint.
///
/// Required mutation: `allow_range_without_state`.
#[test]
fn t_content_range_requires_compatible_state() {
    let source = SourceRange::new(7, 40, 80, Some("checkpoint-1")).expect("valid source");
    assert!(source.validate_replay(&ReplayState::StreamOrigin).is_err());
    assert!(source
        .validate_replay(&ReplayState::checkpoint("checkpoint-1", 7, 40))
        .is_ok());
    assert!(source
        .validate_replay(&ReplayState::checkpoint("checkpoint-1", 8, 40))
        .is_err());
}

/// T-CONTENT-SURVIVES-RING-EVICTION — materialized presentation remains stable
/// after the raw source bytes leave the hot ring.
///
/// Required mutation: `timeline_reads_hot_ring`.
#[test]
fn t_content_materialized_output_survives_ring_eviction() {
    let mut ring = ByteRing::new(4);
    ring.append(b"old!");
    let source = SourceRange::new(1, 0, 4, None).expect("valid source");
    let mut timeline = ContentTimeline::new();
    timeline
        .append(ContentEvent::materialized_terminal_output(
            "item-1", 1, source, "old!",
        ))
        .expect("append materialized item");

    ring.append(b"new!");
    assert_eq!(ring.snapshot(), b"new!");
    assert_eq!(timeline.events()[0].materialized_text(), Some("old!"));
}

/// T-CONTENT-NO-PROMPT-HEURISTIC — prompt-shaped bytes remain ordinary output
/// without an explicit structured-input or shell-mark event.
///
/// Required mutation: `prompt_regex_creates_command`.
#[test]
fn t_content_prompt_shape_does_not_create_command_boundary() {
    let source = SourceRange::new(1, 0, 14, None).expect("valid source");
    let event = ContentEvent::materialized_terminal_output("item-1", 1, source, "$ cargo test ");
    assert_eq!(event.kind, ContentKind::TerminalOutput);
    assert!(!event.is_command_boundary());
}

/// T-CONTENT-ALT-SAME-PTY — alternate-screen output remains a mutable grid of
/// the same execution and does not become timeline content.
///
/// Required mutation: `alt_grid_becomes_timeline_item`.
#[test]
fn t_content_alternate_screen_creates_no_timeline_item() {
    let session = Session::spawn_with(
        "/bin/sh",
        &["-c", "sleep 0.1"],
        Winsize::default(),
        Discipline::Raw,
    )
    .expect("spawn real PTY session");
    let child_pid = session.child_pid();
    let mut timeline = ContentTimeline::new();
    timeline.set_screen_mode(ScreenMode::Alternate);
    assert!(!timeline.accepts_terminal_materialization());
    let source = SourceRange::new(1, 0, 4, None).expect("source");
    assert!(timeline
        .append(ContentEvent::materialized_terminal_output(
            "alt-item", 1, source, "tui!",
        ))
        .is_err());
    assert!(timeline.is_empty());
    assert_eq!(
        session.child_pid(),
        child_pid,
        "screen mode cannot replace the PTY"
    );
    timeline.set_screen_mode(ScreenMode::Primary);
    assert!(timeline.accepts_terminal_materialization());
}

/// T-CONTENT-RETENTION-DISABLED — disabling durable capture leaves the bounded
/// live timeline available while writing no durable event.
///
/// Required mutation: `disabled_policy_writes_transcript`.
#[test]
fn t_content_retention_disabled_writes_nothing_durable() {
    let mut durable = DurableTranscript::disabled();
    let event = ContentEvent::new("item-1", ContentKind::Unstructured, 1, None, None);
    assert_eq!(durable.capture(event), CaptureOutcome::Disabled);
    assert!(durable.is_empty());

    let mut live = ContentTimeline::new();
    live.append(ContentEvent::new(
        "live-1",
        ContentKind::Unstructured,
        1,
        None,
        None,
    ))
    .expect("live projection remains available");
    assert_eq!(live.len(), 1);
}

/// T-CONTENT-REDACTION-DERIVED — export redaction changes only the derived
/// output and leaves canonical materialized content byte-identical.
///
/// Required mutation: `redactor_mutates_canonical_record`.
#[test]
fn t_content_redaction_is_a_derived_sink() {
    let source = SourceRange::new(1, 0, 12, None).expect("valid source");
    let event = ContentEvent::materialized_terminal_output("item-1", 1, source, "token=secret");
    let rule = RedactionRule::literal("secret", "[redacted]", "rule-v1");
    let export = event.redacted_export(&[rule]);

    assert_eq!(event.materialized_text(), Some("token=secret"));
    assert_eq!(export.text, "token=[redacted]");
    assert_eq!(export.rule_versions, vec!["rule-v1"]);
}

/// T-CONTENT-BOUNDED-RECOVERY — the authoritative ledger refuses growth beyond
/// its declared event bound and reports the cursor needed for recovery.
///
/// Required mutation: `unbounded_transcript`.
#[test]
fn t_content_transcript_bound_fails_closed_with_cursor() {
    let mut ledger = TranscriptLedger::with_max_events(1);
    let first = rill_kernel::SemanticEvent::new(
        rill_kernel::EventId::new("event-1").expect("id"),
        1,
        1,
        rill_kernel::EventProvenance::Runtime,
        rill_kernel::EventPayload::Unstructured { range: (0, 4) },
    )
    .expect("event");
    let second = rill_kernel::SemanticEvent::new(
        rill_kernel::EventId::new("event-2").expect("id"),
        2,
        1,
        rill_kernel::EventProvenance::Runtime,
        rill_kernel::EventPayload::Unstructured { range: (4, 8) },
    )
    .expect("event");
    ledger.append(first).expect("within bound");
    assert!(matches!(
        ledger.append(second),
        Err(TranscriptError::CapacityExceeded {
            recovery_cursor: 1,
            ..
        })
    ));
    assert_eq!(ledger.len(), 1);
    assert_eq!(ledger.events()[0].payload.terminal_range(), Some((0, 4)));
}
