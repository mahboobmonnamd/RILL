//! SPEC-ATTENTION gates: T-ATT-ONEQUEUE, T-ATT-ROLLUP, T-ATT-READ, plus the
//! bounded-queue half of T-ATT-COLD (§2's never-drop-actionable rule; the
//! zero-control-plane-RPC half needs a running daemon and stays open).

use rill_orchestrate::attention::*;

// ------------------------------------------------------------ T-ATT-ONEQUEUE

/// Two independent projections of the queue — a badge count and the mailbox
/// listing — must always agree, because both read the same entries. This is
/// the concrete content of "no surface may derive attention independently."
///
/// Required mutation: `RILL_MUTATE=second_attention_classifier`.
#[test]
fn t_att_onequeue_badge_and_mailbox_agree() {
    let mut q = AttentionQueue::new(16);
    q.enqueue("workspace-a", AttentionState::NeedsInput, "cli-notify", 0);
    q.enqueue(
        "workspace-a",
        AttentionState::CompletedUnread,
        "shell-integration",
        0,
    );

    let badge = q.badge_count("workspace-a");
    let mailbox = q.mailbox_entries("workspace-a").len();
    assert_eq!(
        badge, mailbox,
        "badge count and mailbox listing disagree — they must derive from the same queue"
    );
    assert_eq!(badge, 2);
}

// --------------------------------------------------------- bounded queue (§2)

/// On overflow, the oldest non-actionable entry is dropped first;
/// `needs_input`/`approval` are never evicted for capacity.
///
/// Required mutation: `RILL_MUTATE=overflow_drops_actionable`.
#[test]
fn t_att_bounded_queue_never_evicts_needs_input_for_capacity() {
    let mut q = AttentionQueue::new(2);
    let needs_input = q.enqueue("pane-1", AttentionState::NeedsInput, "password-prompt", 0);
    q.enqueue("pane-1", AttentionState::CompletedUnread, "long-command", 1);
    q.enqueue("pane-1", AttentionState::CompletedUnread, "long-command", 2);

    assert_eq!(
        q.mailbox_entries("pane-1").len(),
        2,
        "queue did not respect its capacity"
    );
    assert!(
        q.mailbox_entries("pane-1")
            .iter()
            .any(|e| e.id == needs_input),
        "the actionable entry was evicted to make room for a completed_unread"
    );
}

// -------------------------------------------------------------- T-ATT-ROLLUP

/// Rollup reports the highest-urgency state among the given targets, per the
/// fixed order: approval > needs_input > failed > disconnected >
/// completed_unread.
///
/// Required mutation: `RILL_MUTATE=rollup_wrong_order`.
#[test]
fn t_att_rollup_reports_highest_urgency() {
    let mut q = AttentionQueue::new(16);
    q.enqueue("child-a", AttentionState::Disconnected, "remote", 0);
    q.enqueue("child-b", AttentionState::Approval, "agent", 0);
    q.enqueue("child-b", AttentionState::CompletedUnread, "agent", 1);

    let rollup = q.rollup(&["child-a", "child-b"]);
    assert_eq!(
        rollup,
        Some(AttentionState::Approval),
        "rollup did not report the highest-urgency state among the children"
    );
}

// ---------------------------------------------------------------- T-ATT-READ

/// An entry clears only on a real view: visible, window key, minimum dwell.
/// A hidden render, an unfocused render, and a too-brief render must not
/// clear it.
///
/// Required mutation: `RILL_MUTATE=clear_on_any_render`.
#[test]
fn t_att_read_clears_only_on_a_real_view() {
    let mut q = AttentionQueue::new(16);
    let id = q.enqueue("pane-1", AttentionState::CompletedUnread, "long-command", 0);

    q.observe_view(
        id,
        ViewEvent {
            visible: false,
            key: true,
            dwell_ms: 1000,
        },
    );
    assert!(!q.is_read(id), "a hidden render must not clear the entry");

    q.observe_view(
        id,
        ViewEvent {
            visible: true,
            key: false,
            dwell_ms: 1000,
        },
    );
    assert!(
        !q.is_read(id),
        "an unfocused-window render must not clear the entry"
    );

    q.observe_view(
        id,
        ViewEvent {
            visible: true,
            key: true,
            dwell_ms: 10,
        },
    );
    assert!(
        !q.is_read(id),
        "a too-brief render must not clear the entry"
    );

    q.observe_view(
        id,
        ViewEvent {
            visible: true,
            key: true,
            dwell_ms: 300,
        },
    );
    assert!(
        q.is_read(id),
        "a real view (visible, key, dwelled) must clear the entry"
    );
}
