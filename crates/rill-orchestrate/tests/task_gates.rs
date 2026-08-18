//! SPEC-TASK gates: T-TASK-SECTIONS, T-TASK-APPROVE, T-TASK-FORK,
//! T-TASK-QUEUE, T-TASK-REPLAY, T-TASK-PERSIST, T-TASK-ATTACH.

use rill_orchestrate::task::*;

// --------------------------------------------------------- T-TASK-SECTIONS

/// A closed section cannot be mutated; corrections are new sections. An
/// adapter tag outside the closed set maps to `Output`, never invented or
/// dropped.
///
/// Required mutation: `RILL_MUTATE=mutate_closed_section`.
#[test]
fn t_task_sections_are_append_only_and_unknown_tags_become_output() {
    let mut t = Task::new(TaskId(1));
    let i = t.append(SectionKind::Output, "first draft");
    t.mutate_content(i, "unclosed edit is fine")
        .expect("open section is editable");

    // decide()/close() require Approval kind for the close-path used
    // elsewhere; exercise close via answer() on a Question instead so this
    // test only depends on public API.
    let q = t.append(SectionKind::Question, "proceed?");
    t.answer(q, "yes").expect("answering closes the question");

    let err = t
        .mutate_content(q, "trying to rewrite a closed section")
        .expect_err("closed section must refuse mutation");
    assert_eq!(err, TaskError::SectionClosed);

    assert_eq!(SectionKind::from_tag("not-a-real-tag"), SectionKind::Output);
    assert_eq!(SectionKind::from_tag("diff"), SectionKind::Diff);
}

// ---------------------------------------------------------- T-TASK-APPROVE

/// No timeout, no default outcome — only an explicit `decide()` unblocks the
/// task, and the decision is recorded with actor and time.
///
/// Required mutation: `RILL_MUTATE=approval_times_out_to_yes`.
#[test]
fn t_task_approve_only_an_explicit_decision_unblocks() {
    let mut t = Task::new(TaskId(1));
    let a = t.append(SectionKind::Approval, "delete the branch?");
    assert!(t.is_blocked(), "an open approval must block the task");
    assert!(t.is_blocked(), "still blocked — nothing auto-approves");

    t.decide(a, ApprovalDecision::Approved, "crdileep", 1_700_000_000_000)
        .expect("explicit decision");
    assert!(
        !t.is_blocked(),
        "an explicit decision must unblock the task"
    );

    let d = t.decision_for(a).expect("decision must be recorded");
    assert_eq!(d.decision, ApprovalDecision::Approved);
    assert_eq!(d.actor, "crdileep");
    assert_eq!(d.at_unix_ms, 1_700_000_000_000);
}

// ------------------------------------------------------------- T-TASK-FORK

/// Fork is a copy-on-write prefix up to the cut point. Sections appended
/// after the cut must not appear in the fork, and each task continues
/// independently afterward.
///
/// Required mutation: `RILL_MUTATE=fork_includes_sections_after_cut`.
#[test]
fn t_task_fork_is_a_prefix_and_diverges_independently() {
    let mut original = Task::new(TaskId(1));
    original.append(SectionKind::Prompt, "start");
    let cut = original.append(SectionKind::Output, "state at fork point");
    original.append(
        SectionKind::Output,
        "happened after the fork, original only",
    );

    let mut forked = original.fork(TaskId(2), cut);
    assert!(
        forked.sections().iter().all(|s| s.index <= cut),
        "fork carried a section that did not exist yet at the cut point"
    );
    assert!(
        !forked
            .sections()
            .iter()
            .any(|s| s.content.contains("original only")),
        "fork carried a section appended to the source after the cut"
    );

    original.append(SectionKind::Output, "added to original after forking");
    forked.append(SectionKind::Output, "added to forked after forking");
    assert!(!forked
        .sections()
        .iter()
        .any(|s| s.content.contains("added to original")));
    assert!(!original
        .sections()
        .iter()
        .any(|s| s.content.contains("added to forked")));
}

// ------------------------------------------------------------ T-TASK-QUEUE

/// A queued prompt must not be sent while the task is blocked on an
/// unanswered approval or question.
///
/// Required mutation: `RILL_MUTATE=queue_sends_while_blocked`.
#[test]
fn t_task_queue_refuses_to_send_while_blocked() {
    let mut t = Task::new(TaskId(1));
    let a = t.append(SectionKind::Approval, "run the migration?");
    let mut q = PromptQueue::new();
    q.push("go ahead once approved");

    let err = q.try_send(&t).expect_err("must refuse while blocked");
    assert_eq!(err, TaskError::StillBlocked);
    assert_eq!(
        q.len(),
        1,
        "the refused prompt must stay queued, not be consumed"
    );

    t.decide(a, ApprovalDecision::Approved, "crdileep", 0)
        .unwrap();
    let sent = q.try_send(&t).expect("must send once unblocked");
    assert_eq!(sent.as_deref(), Some("go ahead once approved"));
}

// ----------------------------------------------------------- T-TASK-REPLAY

/// Every gate in this suite runs against the replay adapter with no
/// provider and no network — this test is that adapter itself, demonstrated
/// against a fixture transcript.
#[test]
fn t_task_replay_adapter_drives_a_task_from_a_transcript() {
    let mut adapter = ReplayAdapter::from_transcript(vec![
        ReplayEvent {
            tag: "prompt".into(),
            content: "add a test".into(),
        },
        ReplayEvent {
            tag: "plan".into(),
            content: "write the gate".into(),
        },
        ReplayEvent {
            tag: "vendor-specific-tag".into(),
            content: "raw passthrough".into(),
        },
    ]);
    let mut task = Task::new(TaskId(1));
    let n = adapter.drive(&mut task);
    assert_eq!(n, 3);
    assert_eq!(task.sections().len(), 3);
    assert_eq!(task.sections()[0].kind, SectionKind::Prompt);
    assert_eq!(task.sections()[1].kind, SectionKind::Plan);
    assert_eq!(
        task.sections()[2].kind,
        SectionKind::Output,
        "an unmapped tag must land as Output, not be dropped"
    );
    assert_eq!(task.sections()[2].content, "raw passthrough");
}

// ---------------------------------------------------------- T-TASK-PERSIST

/// A task and its prompt queue both survive a window restart — modeled here
/// as a byte round-trip through `persist_task_and_queue`.
///
/// Required mutation: `RILL_MUTATE=skip_persisting_queue`.
#[test]
fn t_task_persist_round_trips_sections_decisions_and_queue() {
    let mut t = Task::new(TaskId(7));
    t.append(SectionKind::Prompt, "do the thing");
    let a = t.append(SectionKind::Approval, "sure?");
    t.decide(a, ApprovalDecision::Approved, "crdileep", 42)
        .unwrap();

    let mut q = PromptQueue::new();
    q.push("next step");
    q.push("then this");

    let text = persist_task_and_queue(&t, &q);
    let (restored, restored_queue) = restore_task_and_queue(TaskId(7), &text);

    assert_eq!(restored.sections().len(), t.sections().len());
    assert_eq!(restored.sections()[0].content, "do the thing");
    let d = restored
        .decision_for(a)
        .expect("decision must survive restart");
    assert_eq!(d.actor, "crdileep");
    assert_eq!(
        restored_queue.len(),
        2,
        "prompt queue did not survive the restart — queued follow-ups were lost"
    );
}

// ---------------------------------------------------------- T-TASK-ATTACH

/// Attaching context requires an explicit scope and redacts at attach time,
/// because attaching is transmission.
///
/// Required mutation: `RILL_MUTATE=attach_skips_redaction`.
#[test]
fn t_task_attach_requires_scope_and_redacts_at_attach_time() {
    let err = attach_context("", "irrelevant").expect_err("empty scope must be refused");
    assert_eq!(err, AttachError::EmptyScope);

    let att = attach_context(
        "file:src/main.rs lines 1-4",
        "API_KEY=sk-abcdef1234567890\nfn main() {}",
    )
    .expect("valid scope");
    assert_eq!(att.scope, "file:src/main.rs lines 1-4");
    assert!(
        !att.content.contains("sk-abcdef1234567890"),
        "attaching context leaked an unredacted secret"
    );
    assert!(
        att.content.contains("fn main"),
        "redaction over-corrected non-secret content"
    );
}
