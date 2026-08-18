//! SPEC-TASK: the agent runtime object (ADR 0030). Every property here is
//! provider-independent — demonstrable against the replay adapter alone, no
//! network, no API key (SPEC-TASK §2).

use crate::trust;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

/// The closed set (SPEC-TASK §3). Adding a member requires an ADR amendment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    Prompt,
    Plan,
    Tool,
    Command,
    Output,
    Approval,
    Diff,
    Result,
    Question,
}

impl SectionKind {
    /// An adapter tag it cannot map becomes `Output` with the raw content —
    /// never invented, never dropped (SPEC-TASK §3).
    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "prompt" => Self::Prompt,
            "plan" => Self::Plan,
            "tool" => Self::Tool,
            "command" => Self::Command,
            "output" => Self::Output,
            "approval" => Self::Approval,
            "diff" => Self::Diff,
            "result" => Self::Result,
            "question" => Self::Question,
            _ => Self::Output,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Plan => "plan",
            Self::Tool => "tool",
            Self::Command => "command",
            Self::Output => "output",
            Self::Approval => "approval",
            Self::Diff => "diff",
            Self::Result => "result",
            Self::Question => "question",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Section {
    pub index: u64,
    pub kind: SectionKind,
    pub content: String,
    closed: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TaskError {
    UnknownSection,
    SectionClosed,
    StillBlocked,
    NotAnApproval,
    NotAQuestion,
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSection => write!(f, "unknown section index"),
            Self::SectionClosed => write!(f, "section is closed"),
            Self::StillBlocked => write!(f, "task is blocked on approval or question"),
            Self::NotAnApproval => write!(f, "section is not an approval"),
            Self::NotAQuestion => write!(f, "section is not a question"),
        }
    }
}

impl std::error::Error for TaskError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub decision: ApprovalDecision,
    pub actor: String,
    pub at_unix_ms: u64,
}

pub struct Task {
    id: TaskId,
    next_index: u64,
    sections: Vec<Section>,
    decisions: HashMap<u64, Decision>,
}

impl Task {
    pub fn new(id: TaskId) -> Self {
        Self {
            id,
            next_index: 0,
            sections: Vec::new(),
            decisions: HashMap::new(),
        }
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// Append-only (SPEC-TASK §3).
    pub fn append(&mut self, kind: SectionKind, content: impl Into<String>) -> u64 {
        let index = self.next_index;
        self.next_index += 1;
        self.sections.push(Section {
            index,
            kind,
            content: content.into(),
            closed: false,
        });
        index
    }

    fn close(&mut self, index: u64) -> Result<(), TaskError> {
        let s = self
            .sections
            .iter_mut()
            .find(|s| s.index == index)
            .ok_or(TaskError::UnknownSection)?;
        s.closed = true;
        Ok(())
    }

    /// A closed section MUST NOT be mutated; corrections are new sections
    /// (SPEC-TASK §3). Under `mutate_closed_section` this check is skipped.
    pub fn mutate_content(
        &mut self,
        index: u64,
        new_content: impl Into<String>,
    ) -> Result<(), TaskError> {
        let s = self
            .sections
            .iter_mut()
            .find(|s| s.index == index)
            .ok_or(TaskError::UnknownSection)?;
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("mutate_closed_section") {
            s.content = new_content.into();
            return Ok(());
        }
        if s.closed {
            return Err(TaskError::SectionClosed);
        }
        s.content = new_content.into();
        Ok(())
    }

    /// True while the most recent `approval`/`question` is unresolved
    /// (SPEC-TASK §4). Under `approval_times_out_to_yes` an open approval
    /// stops blocking on its own — the auto-approve regression D4 forbids.
    pub fn is_blocked(&self) -> bool {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("approval_times_out_to_yes") {
            return false;
        }
        self.sections
            .iter()
            .rev()
            .find(|s| matches!(s.kind, SectionKind::Approval | SectionKind::Question))
            .map(|s| match s.kind {
                SectionKind::Approval => !self.decisions.contains_key(&s.index),
                SectionKind::Question => !self.has_answer(s.index),
                _ => false,
            })
            .unwrap_or(false)
    }

    fn has_answer(&self, question_index: u64) -> bool {
        let marker = format!("[answers #{question_index}]");
        self.sections.iter().any(|s| s.content.starts_with(&marker))
    }

    /// Explicit decision only. There is no timer and no default outcome
    /// (SPEC-TASK §4) — nothing in this type can advance a blocked task
    /// except this call.
    pub fn decide(
        &mut self,
        index: u64,
        decision: ApprovalDecision,
        actor: &str,
        at_unix_ms: u64,
    ) -> Result<(), TaskError> {
        let kind = self
            .sections
            .iter()
            .find(|s| s.index == index)
            .ok_or(TaskError::UnknownSection)?
            .kind;
        if !matches!(kind, SectionKind::Approval) {
            return Err(TaskError::NotAnApproval);
        }
        self.decisions.insert(
            index,
            Decision {
                decision,
                actor: actor.to_string(),
                at_unix_ms,
            },
        );
        self.close(index)
    }

    pub fn decision_for(&self, index: u64) -> Option<&Decision> {
        self.decisions.get(&index)
    }

    pub fn answer(
        &mut self,
        question_index: u64,
        answer: impl Into<String>,
    ) -> Result<u64, TaskError> {
        let kind = self
            .sections
            .iter()
            .find(|s| s.index == question_index)
            .ok_or(TaskError::UnknownSection)?
            .kind;
        if !matches!(kind, SectionKind::Question) {
            return Err(TaskError::NotAQuestion);
        }
        let idx = self.append(
            SectionKind::Result,
            format!("[answers #{question_index}] {}", answer.into()),
        );
        self.close(question_index)?;
        Ok(idx)
    }

    /// Copy-on-write **prefix** up to `up_to_index` (SPEC-TASK §6). Under
    /// `fork_includes_sections_after_cut` the fork instead carries every
    /// section including ones appended after the cut — the regression
    /// ADR 0030 D6 forbids.
    pub fn fork(&self, new_id: TaskId, up_to_index: u64) -> Task {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("fork_includes_sections_after_cut") {
            return Task {
                id: new_id,
                next_index: self.next_index,
                sections: self.sections.clone(),
                decisions: self.decisions.clone(),
            };
        }
        let sections: Vec<Section> = self
            .sections
            .iter()
            .filter(|s| s.index <= up_to_index)
            .cloned()
            .collect();
        let next_index = sections.last().map(|s| s.index + 1).unwrap_or(0);
        let decisions = self
            .decisions
            .iter()
            .filter(|(k, _)| **k <= up_to_index)
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        Task {
            id: new_id,
            next_index,
            sections,
            decisions,
        }
    }
}

/// Queued follow-ups, visible and editable before they send (SPEC-TASK §7).
pub struct PromptQueue {
    items: VecDeque<String>,
}

impl Default for PromptQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptQueue {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
        }
    }

    pub fn push(&mut self, prompt: impl Into<String>) {
        self.items.push_back(prompt.into());
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Refuses while `task` is blocked — a queued prompt MUST NOT answer a
    /// question the user has not seen (SPEC-TASK §7).
    pub fn try_send(&mut self, task: &Task) -> Result<Option<String>, TaskError> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("queue_sends_while_blocked") {
            return Ok(self.items.pop_front());
        }
        if task.is_blocked() {
            return Err(TaskError::StillBlocked);
        }
        Ok(self.items.pop_front())
    }
}

/// One recorded transcript event (SPEC-TASK §2).
pub struct ReplayEvent {
    pub tag: String,
    pub content: String,
}

/// Drives a `Task` from a recorded transcript, emitting exactly the events a
/// live provider would. Every gate in this module is demonstrable against
/// this adapter alone.
pub struct ReplayAdapter {
    events: VecDeque<ReplayEvent>,
}

impl ReplayAdapter {
    pub fn from_transcript(events: Vec<ReplayEvent>) -> Self {
        Self {
            events: events.into(),
        }
    }

    pub fn drive(&mut self, task: &mut Task) -> usize {
        let mut n = 0;
        while let Some(ev) = self.events.pop_front() {
            task.append(SectionKind::from_tag(&ev.tag), ev.content);
            n += 1;
        }
        n
    }
}

// ------------------------------------------------------------- persistence

#[derive(Serialize, Deserialize)]
struct SectionEntry {
    index: u64,
    kind: String,
    content: String,
    closed: bool,
}

#[derive(Serialize, Deserialize)]
struct DecisionEntry {
    section_index: u64,
    decision: String,
    actor: String,
    at_unix_ms: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct SnapshotDoc {
    id: u64,
    next_index: u64,
    #[serde(default)]
    section: Vec<SectionEntry>,
    #[serde(default)]
    decision: Vec<DecisionEntry>,
    #[serde(default)]
    queue: Vec<String>,
}

fn decision_to_str(d: ApprovalDecision) -> &'static str {
    match d {
        ApprovalDecision::Approved => "approved",
        ApprovalDecision::Rejected => "rejected",
        ApprovalDecision::Cancelled => "cancelled",
    }
}

fn decision_from_str(s: &str) -> ApprovalDecision {
    match s {
        "approved" => ApprovalDecision::Approved,
        "rejected" => ApprovalDecision::Rejected,
        _ => ApprovalDecision::Cancelled,
    }
}

/// Encode a task and its prompt queue so both survive a window restart
/// (SPEC-TASK §1). Under `skip_persisting_queue` the queue is dropped from
/// the encoding — the regression that would lose queued follow-ups.
pub fn persist_task_and_queue(task: &Task, queue: &PromptQueue) -> String {
    let doc = SnapshotDoc {
        id: task.id.0,
        next_index: task.next_index,
        section: task
            .sections
            .iter()
            .map(|s| SectionEntry {
                index: s.index,
                kind: s.kind.as_str().to_string(),
                content: s.content.clone(),
                closed: s.closed,
            })
            .collect(),
        decision: task
            .decisions
            .iter()
            .map(|(i, d)| DecisionEntry {
                section_index: *i,
                decision: decision_to_str(d.decision).to_string(),
                actor: d.actor.clone(),
                at_unix_ms: d.at_unix_ms,
            })
            .collect(),
        queue: {
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_persisting_queue") {
                Vec::new()
            } else {
                queue.items.iter().cloned().collect()
            }
            #[cfg(not(feature = "mutate"))]
            {
                queue.items.iter().cloned().collect()
            }
        },
    };
    toml::to_string_pretty(&doc).unwrap_or_default()
}

pub fn restore_task_and_queue(id: TaskId, text: &str) -> (Task, PromptQueue) {
    let doc: SnapshotDoc = toml::from_str(text).unwrap_or(SnapshotDoc {
        id: id.0,
        ..Default::default()
    });
    let mut task = Task::new(TaskId(doc.id));
    task.next_index = doc.next_index;
    task.sections = doc
        .section
        .iter()
        .map(|e| Section {
            index: e.index,
            kind: SectionKind::from_tag(&e.kind),
            content: e.content.clone(),
            closed: e.closed,
        })
        .collect();
    task.decisions = doc
        .decision
        .iter()
        .map(|e| {
            (
                e.section_index,
                Decision {
                    decision: decision_from_str(&e.decision),
                    actor: e.actor.clone(),
                    at_unix_ms: e.at_unix_ms,
                },
            )
        })
        .collect();
    let mut queue = PromptQueue::new();
    for p in doc.queue {
        queue.push(p);
    }
    (task, queue)
}

// ------------------------------------------------------------ attachments

#[derive(Debug, PartialEq, Eq)]
pub enum AttachError {
    EmptyScope,
}

#[derive(Debug)]
pub struct ContextAttachment {
    pub scope: String,
    pub content: String,
}

/// Explicit and scoped (SPEC-TASK §9). Redaction applies at attach, because
/// attaching is transmission. Under `attach_skips_redaction` the raw content
/// is attached unredacted.
pub fn attach_context(
    scope: impl Into<String>,
    raw_content: &str,
) -> Result<ContextAttachment, AttachError> {
    let scope = scope.into();
    if scope.trim().is_empty() {
        return Err(AttachError::EmptyScope);
    }
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("attach_skips_redaction") {
        return Ok(ContextAttachment {
            scope,
            content: raw_content.to_string(),
        });
    }
    Ok(ContextAttachment {
        scope,
        content: trust::redact(raw_content),
    })
}
