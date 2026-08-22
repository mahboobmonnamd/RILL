use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRange {
    pub generation: u64,
    pub start: u64,
    pub end: u64,
    pub checkpoint_id: Option<String>,
}

impl SourceRange {
    pub fn new(
        generation: u64,
        start: u64,
        end: u64,
        checkpoint_id: Option<&str>,
    ) -> Result<Self, ContentError> {
        if start >= end {
            return Err(ContentError::InvalidSourceRange { start, end });
        }
        Ok(Self {
            generation,
            start,
            end,
            checkpoint_id: checkpoint_id.map(str::to_owned),
        })
    }

    pub fn validate_replay(&self, state: &ReplayState) -> Result<(), ContentError> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("allow_range_without_state") {
            return Ok(());
        }
        match state {
            ReplayState::StreamOrigin if self.start == 0 => Ok(()),
            ReplayState::StreamOrigin => Err(ContentError::ReplayStateRequired),
            ReplayState::Checkpoint {
                checkpoint_id,
                generation,
                ending_offset,
            } if self.checkpoint_id.as_deref() == Some(checkpoint_id.as_str())
                && self.generation == *generation
                && self.start == *ending_offset =>
            {
                Ok(())
            }
            ReplayState::Checkpoint { .. } => Err(ContentError::IncompatibleReplayState),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayState {
    StreamOrigin,
    Checkpoint {
        checkpoint_id: String,
        generation: u64,
        ending_offset: u64,
    },
}

impl ReplayState {
    pub fn checkpoint(checkpoint_id: &str, generation: u64, ending_offset: u64) -> Self {
        Self::Checkpoint {
            checkpoint_id: checkpoint_id.to_owned(),
            generation,
            ending_offset,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetentionMode {
    Disabled,
    MemoryOnly,
    Durable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub mode: RetentionMode,
    pub limit_bytes: Option<u64>,
}

impl RetentionPolicy {
    pub fn disabled() -> Self {
        Self {
            mode: RetentionMode::Disabled,
            limit_bytes: None,
        }
    }

    pub fn memory_only(limit_bytes: u64) -> Self {
        Self {
            mode: RetentionMode::MemoryOnly,
            limit_bytes: Some(limit_bytes),
        }
    }

    pub fn durable(limit_bytes: Option<u64>) -> Self {
        Self {
            mode: RetentionMode::Durable,
            limit_bytes,
        }
    }

    pub fn allows_capture(&self) -> bool {
        !matches!(self.mode, RetentionMode::Disabled)
    }

    pub fn restrictive_wins(self, other: Self) -> Self {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("closest_workspace_setting_wins") {
            return self;
        }
        match (self.mode, other.mode) {
            (RetentionMode::Disabled, _) => self,
            (_, RetentionMode::Disabled) => other,
            (RetentionMode::MemoryOnly, RetentionMode::MemoryOnly) => {
                let cap = self
                    .limit_bytes
                    .unwrap_or(u64::MAX)
                    .min(other.limit_bytes.unwrap_or(u64::MAX));
                Self {
                    mode: RetentionMode::MemoryOnly,
                    limit_bytes: Some(cap),
                }
            }
            (RetentionMode::MemoryOnly, RetentionMode::Durable) => self,
            (RetentionMode::Durable, RetentionMode::MemoryOnly) => other,
            (RetentionMode::Durable, RetentionMode::Durable) => {
                let cap = self
                    .limit_bytes
                    .unwrap_or(u64::MAX)
                    .min(other.limit_bytes.unwrap_or(u64::MAX));
                Self {
                    mode: RetentionMode::Durable,
                    limit_bytes: Some(cap),
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentKind {
    TerminalInput,
    TerminalOutput,
    BackgroundOutput,
    AgentMessage,
    ToolCall,
    ToolResult,
    Approval,
    Question,
    DiffResult,
    LifecycleEvent,
    Discontinuity,
    Truncation,
    Unstructured,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventAvailability {
    Available,
    Truncated,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScreenMode {
    #[default]
    Primary,
    Alternate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentEvent {
    pub event_id: String,
    pub kind: ContentKind,
    pub sequence: u64,
    pub parent_event_id: Option<String>,
    pub source_range: Option<(u64, u64)>,
    pub availability: EventAvailability,
    source: Option<SourceRange>,
    materialized_text: Option<String>,
}

impl ContentEvent {
    pub fn new(
        event_id: impl Into<String>,
        kind: ContentKind,
        sequence: u64,
        parent_event_id: Option<&str>,
        source_range: Option<(u64, u64)>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            kind,
            sequence,
            parent_event_id: parent_event_id.map(str::to_owned),
            source_range,
            availability: EventAvailability::Available,
            source: None,
            materialized_text: None,
        }
    }

    pub fn materialized_terminal_output(
        event_id: impl Into<String>,
        sequence: u64,
        source: SourceRange,
        text: impl Into<String>,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            kind: ContentKind::TerminalOutput,
            sequence,
            parent_event_id: None,
            source_range: Some((source.start, source.end)),
            availability: EventAvailability::Available,
            source: Some(source),
            materialized_text: Some(text.into()),
        }
    }

    pub fn truncated(event_id: impl Into<String>, sequence: u64, range: (u64, u64)) -> Self {
        Self {
            event_id: event_id.into(),
            kind: ContentKind::Truncation,
            sequence,
            parent_event_id: None,
            source_range: Some(range),
            availability: EventAvailability::Truncated,
            source: None,
            materialized_text: None,
        }
    }

    pub fn materialized_text(&self) -> Option<&str> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("timeline_reads_hot_ring") {
            return None;
        }
        self.materialized_text.as_deref()
    }

    pub fn is_command_boundary(&self) -> bool {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("prompt_regex_creates_command") {
            return self
                .materialized_text
                .as_deref()
                .is_some_and(|text| text.starts_with("$ "));
        }
        false
    }

    pub fn redacted_export(&self, rules: &[RedactionRule]) -> RedactedExport {
        let mut text = self.materialized_text.clone().unwrap_or_default();
        let mut rule_versions = Vec::new();
        for rule in rules {
            if text.contains(&rule.literal) {
                text = text.replace(&rule.literal, &rule.replacement);
                rule_versions.push(rule.version.clone());
            }
        }
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("redactor_mutates_canonical_record") {
            text = self.materialized_text.clone().unwrap_or_default();
        }
        RedactedExport {
            text,
            rule_versions,
        }
    }

    pub fn size_hint(&self) -> u64 {
        match self.source_range {
            Some((start, end)) => end.saturating_sub(start).max(1),
            None => self.event_id.len() as u64 + 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentError {
    DuplicateEventId(String),
    OutOfOrderSequence {
        event_id: String,
        expected_after: u64,
        got: u64,
    },
    CaptureDisabled,
    RetentionLimitExceeded {
        limit: u64,
        attempted: u64,
    },
    InvalidSourceRange {
        start: u64,
        end: u64,
    },
    ReplayStateRequired,
    IncompatibleReplayState,
    AlternateScreenNotMaterialized,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentTimeline {
    events: Vec<ContentEvent>,
    ids: HashSet<String>,
    last_sequence: Option<u64>,
    bytes_used: u64,
    screen_mode: ScreenMode,
}

impl ContentTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, event: ContentEvent) -> Result<(), ContentError> {
        if event.kind == ContentKind::TerminalOutput && !self.accepts_terminal_materialization() {
            return Err(ContentError::AlternateScreenNotMaterialized);
        }
        if self.ids.contains(&event.event_id) {
            return Err(ContentError::DuplicateEventId(event.event_id.clone()));
        }
        if let Some(last) = self.last_sequence {
            if event.sequence <= last {
                return Err(ContentError::OutOfOrderSequence {
                    event_id: event.event_id.clone(),
                    expected_after: last,
                    got: event.sequence,
                });
            }
        }
        self.ids.insert(event.event_id.clone());
        self.last_sequence = Some(event.sequence);
        self.bytes_used = self.bytes_used.saturating_add(event.size_hint());
        self.events.push(event);
        Ok(())
    }

    pub fn append_with_policy(
        &mut self,
        policy: &RetentionPolicy,
        event: ContentEvent,
    ) -> Result<(), ContentError> {
        if !policy.allows_capture() {
            return Err(ContentError::CaptureDisabled);
        }
        if let Some(limit) = policy.limit_bytes {
            let attempted = self.bytes_used.saturating_add(event.size_hint());
            if attempted > limit {
                return Err(ContentError::RetentionLimitExceeded { limit, attempted });
            }
        }
        self.append(event)
    }

    pub fn append_discontinuity(
        &mut self,
        event_id: &str,
        sequence: u64,
        start: u64,
        end: u64,
    ) -> Result<(), ContentError> {
        self.append(ContentEvent::new(
            event_id,
            ContentKind::Discontinuity,
            sequence,
            None,
            Some((start, end)),
        ))
    }

    pub fn append_truncation(
        &mut self,
        event_id: &str,
        sequence: u64,
        start: u64,
        end: u64,
    ) -> Result<(), ContentError> {
        self.append(ContentEvent::truncated(event_id, sequence, (start, end)))
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn events(&self) -> &[ContentEvent] {
        &self.events
    }

    pub fn bytes_used(&self) -> u64 {
        self.bytes_used
    }

    pub fn has_discontinuity(&self) -> bool {
        self.events
            .iter()
            .any(|event| event.kind == ContentKind::Discontinuity)
    }

    pub fn has_truncation(&self) -> bool {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("omit_truncation_marker") {
            return false;
        }
        self.events
            .iter()
            .any(|event| matches!(event.kind, ContentKind::Truncation))
    }

    pub fn set_screen_mode(&mut self, mode: ScreenMode) {
        self.screen_mode = mode;
    }

    pub fn accepts_terminal_materialization(&self) -> bool {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("alt_grid_becomes_timeline_item") {
            return true;
        }
        self.screen_mode == ScreenMode::Primary
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactionRule {
    literal: String,
    replacement: String,
    version: String,
}

impl RedactionRule {
    pub fn literal(literal: &str, replacement: &str, version: &str) -> Self {
        Self {
            literal: literal.to_owned(),
            replacement: replacement.to_owned(),
            version: version.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactedExport {
    pub text: String,
    pub rule_versions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureOutcome {
    Disabled,
    Captured,
    LimitExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableTranscript {
    policy: RetentionPolicy,
    events: Vec<ContentEvent>,
    bytes_used: u64,
}

impl DurableTranscript {
    pub fn disabled() -> Self {
        Self {
            policy: RetentionPolicy::disabled(),
            events: Vec::new(),
            bytes_used: 0,
        }
    }

    pub fn capture(&mut self, event: ContentEvent) -> CaptureOutcome {
        if !self.policy.allows_capture() {
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref() == Ok("disabled_policy_writes_transcript") {
                self.events.push(event);
                return CaptureOutcome::Captured;
            }
            return CaptureOutcome::Disabled;
        }
        let attempted = self.bytes_used.saturating_add(event.size_hint());
        if self
            .policy
            .limit_bytes
            .is_some_and(|limit| attempted > limit)
        {
            return CaptureOutcome::LimitExceeded;
        }
        self.bytes_used = attempted;
        self.events.push(event);
        CaptureOutcome::Captured
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ContentEvent, ContentKind, ContentTimeline, EventAvailability, RetentionMode,
        RetentionPolicy,
    };

    #[test]
    /// T-CONTENT-RETENTION-RESTRICTIVE-WINS — a child policy cannot widen a
    /// disabled parent policy and enable capture.
    /// Required mutation: `RILL_MUTATE=closest_workspace_setting_wins`.
    fn t_content_retention_policy_restrictive_wins() {
        let parent = RetentionPolicy::disabled();
        let child = RetentionPolicy::memory_only(1024);
        let resolved = child.restrictive_wins(parent.clone());
        assert_eq!(resolved.mode, RetentionMode::Disabled);
        assert_eq!(resolved, parent);
    }

    #[test]
    fn t_content_timeline_rejects_duplicate_event_id() {
        let mut timeline = ContentTimeline::new();
        let first = ContentEvent::new("evt-1", ContentKind::TerminalOutput, 1, None, None);
        let dup = ContentEvent::new("evt-1", ContentKind::TerminalOutput, 2, None, None);

        timeline.append(first).expect("first event should append");
        let err = timeline
            .append(dup)
            .expect_err("duplicate event id should fail");
        assert!(matches!(err, super::ContentError::DuplicateEventId(_)));
        assert_eq!(timeline.len(), 1);
    }

    #[test]
    fn t_content_timeline_rejects_out_of_order_sequence() {
        let mut timeline = ContentTimeline::new();
        timeline
            .append(ContentEvent::new(
                "evt-1",
                ContentKind::TerminalOutput,
                10,
                None,
                None,
            ))
            .expect("first event should append");
        let err = timeline
            .append(ContentEvent::new(
                "evt-2",
                ContentKind::TerminalOutput,
                9,
                None,
                None,
            ))
            .expect_err("sequence must remain monotonic");
        assert!(matches!(
            err,
            super::ContentError::OutOfOrderSequence { .. }
        ));
    }

    #[test]
    fn t_content_policy_disabled_rejects_capture() {
        let policy = RetentionPolicy::disabled();
        let mut timeline = ContentTimeline::new();
        let event = ContentEvent::new("evt-1", ContentKind::TerminalOutput, 1, None, None);
        assert!(!policy.allows_capture());
        let err = timeline
            .append_with_policy(&policy, event)
            .expect_err("disabled capture must fail closed");
        assert!(matches!(err, super::ContentError::CaptureDisabled));
    }

    #[test]
    fn t_content_timeline_tracks_discontinuity_markers() {
        let mut timeline = ContentTimeline::new();
        timeline
            .append(ContentEvent::new(
                "evt-gap",
                ContentKind::Discontinuity,
                1,
                None,
                Some((100, 200)),
            ))
            .expect("discontinuity marker should append");
        assert!(timeline.has_discontinuity());
    }

    #[test]
    fn t_content_policy_byte_limit_is_enforced() {
        let policy = RetentionPolicy::memory_only(12);
        let mut timeline = ContentTimeline::new();
        let first = ContentEvent::new("evt-1", ContentKind::TerminalOutput, 1, None, Some((0, 8)));
        let second =
            ContentEvent::new("evt-2", ContentKind::TerminalOutput, 2, None, Some((8, 16)));

        timeline
            .append_with_policy(&policy, first)
            .expect("first item should fit");
        let err = timeline
            .append_with_policy(&policy, second)
            .expect_err("second item should exceed the byte limit");
        assert!(matches!(
            err,
            super::ContentError::RetentionLimitExceeded { .. }
        ));
    }

    #[test]
    fn t_content_timeline_supports_discontinuity_helper() {
        let mut timeline = ContentTimeline::new();
        timeline
            .append_discontinuity("gap-1", 1, 40, 50)
            .expect("discontinuity helper should append");
        assert!(timeline.has_discontinuity());
        assert_eq!(timeline.bytes_used(), 10);
    }

    #[test]
    /// T-CONTENT-TRUNCATION-VISIBLE — removed source is represented by an
    /// explicit unavailable timeline item rather than a silent gap.
    /// Required mutation: `RILL_MUTATE=omit_truncation_marker`.
    fn t_content_timeline_marks_truncation_explicitly() {
        let mut timeline = ContentTimeline::new();
        timeline
            .append_truncation("trunc-1", 3, 220, 320)
            .expect("truncation should be visible");
        assert!(timeline.has_truncation());
        let event = &timeline.events()[0];
        assert_eq!(event.availability, EventAvailability::Truncated);
        assert_eq!(event.kind, ContentKind::Truncation);
    }
}
