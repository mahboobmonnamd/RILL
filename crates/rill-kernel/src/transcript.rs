use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EventId(String);

impl EventId {
    pub fn new(value: impl Into<String>) -> Result<Self, TranscriptError> {
        let value = value.into();
        if value.is_empty() {
            return Err(TranscriptError::EmptyEventId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventProvenance {
    Pty,
    StructuredInput,
    ShellMark,
    Process,
    ToolAdapter,
    Runtime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventPayload {
    TerminalOutput { range: (u64, u64) },
    CommandSubmitted { command: String },
    ProcessExited { status: i32 },
    Unstructured { range: (u64, u64) },
    Discontinuity { range: (u64, u64) },
}

impl EventPayload {
    pub fn terminal_range(&self) -> Option<(u64, u64)> {
        match self {
            Self::TerminalOutput { range }
            | Self::Unstructured { range }
            | Self::Discontinuity { range } => Some(*range),
            Self::CommandSubmitted { .. } | Self::ProcessExited { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEvent {
    pub event_id: EventId,
    pub sequence: u64,
    pub execution_generation: u64,
    pub provenance: EventProvenance,
    pub payload: EventPayload,
}

impl SemanticEvent {
    pub fn new(
        event_id: EventId,
        sequence: u64,
        execution_generation: u64,
        provenance: EventProvenance,
        payload: EventPayload,
    ) -> Result<Self, TranscriptError> {
        if let Some((start, end)) = payload.terminal_range() {
            if start >= end {
                return Err(TranscriptError::InvalidTerminalRange { start, end });
            }
        }
        if matches!(payload, EventPayload::CommandSubmitted { .. })
            && !matches!(
                provenance,
                EventProvenance::StructuredInput | EventProvenance::ShellMark
            )
        {
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref()
                == Ok("scrape_cells_for_command_and_pass_count")
            {
                return Ok(Self {
                    event_id,
                    sequence,
                    execution_generation,
                    provenance,
                    payload,
                });
            }
            return Err(TranscriptError::UnauthorizedProvenance {
                provenance,
                payload: "command submitted",
            });
        }
        if matches!(payload, EventPayload::ProcessExited { .. })
            && provenance != EventProvenance::Process
        {
            return Err(TranscriptError::UnauthorizedProvenance {
                provenance,
                payload: "process exited",
            });
        }
        Ok(Self {
            event_id,
            sequence,
            execution_generation,
            provenance,
            payload,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptError {
    EmptyEventId,
    ConflictingEventId(EventId),
    OutOfOrderSequence {
        expected_after: u64,
        got: u64,
    },
    GenerationMismatch {
        expected: u64,
        got: u64,
    },
    InvalidTerminalRange {
        start: u64,
        end: u64,
    },
    OverlappingTerminalRange {
        previous_end: u64,
        next_start: u64,
    },
    UnauthorizedProvenance {
        provenance: EventProvenance,
        payload: &'static str,
    },
    CapacityExceeded {
        max_events: usize,
        recovery_cursor: u64,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranscriptLedger {
    events: Vec<SemanticEvent>,
    by_id: HashMap<EventId, usize>,
    last_sequence: Option<u64>,
    execution_generation: Option<u64>,
    last_terminal_end: Option<u64>,
    max_events: usize,
}

impl TranscriptLedger {
    pub fn new() -> Self {
        Self::with_max_events(4096)
    }

    pub fn with_max_events(max_events: usize) -> Self {
        Self {
            max_events: max_events.max(1),
            ..Self::default()
        }
    }

    pub fn append(&mut self, event: SemanticEvent) -> Result<(), TranscriptError> {
        if let Some(index) = self.by_id.get(&event.event_id) {
            if self.events[*index] == event {
                return Ok(());
            }
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref() == Ok("duplicate_event_id_appends") {
                self.events.push(event);
                return Ok(());
            }
            return Err(TranscriptError::ConflictingEventId(event.event_id));
        }
        #[cfg(feature = "mutate")]
        let unbounded = std::env::var("RILL_MUTATE").as_deref() == Ok("unbounded_transcript");
        #[cfg(not(feature = "mutate"))]
        let unbounded = false;
        if !unbounded && self.events.len() >= self.max_events {
            return Err(TranscriptError::CapacityExceeded {
                max_events: self.max_events,
                recovery_cursor: self.last_sequence.unwrap_or(0),
            });
        }
        if let Some(last) = self.last_sequence {
            if event.sequence <= last {
                return Err(TranscriptError::OutOfOrderSequence {
                    expected_after: last,
                    got: event.sequence,
                });
            }
        }
        if let Some((start, _)) = event.payload.terminal_range() {
            if let Some(generation) = self.execution_generation {
                if event.execution_generation != generation {
                    return Err(TranscriptError::GenerationMismatch {
                        expected: generation,
                        got: event.execution_generation,
                    });
                }
            }
            if let Some(previous_end) = self.last_terminal_end {
                #[cfg(feature = "mutate")]
                let ignore_order =
                    std::env::var("RILL_MUTATE").as_deref() == Ok("semantic_before_source_offset");
                #[cfg(not(feature = "mutate"))]
                let ignore_order = false;
                if !ignore_order && start < previous_end {
                    return Err(TranscriptError::OverlappingTerminalRange {
                        previous_end,
                        next_start: start,
                    });
                }
            }
        }

        if let Some((_, end)) = event.payload.terminal_range() {
            self.execution_generation = Some(event.execution_generation);
            self.last_terminal_end = Some(end);
        }
        self.last_sequence = Some(event.sequence);
        self.by_id.insert(event.event_id.clone(), self.events.len());
        self.events.push(event);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn events(&self) -> &[SemanticEvent] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EventId, EventPayload, EventProvenance, SemanticEvent, TranscriptError, TranscriptLedger,
    };

    fn output_event(id: &str, sequence: u64, range: (u64, u64)) -> SemanticEvent {
        SemanticEvent::new(
            EventId::new(id).expect("valid event id"),
            sequence,
            1,
            EventProvenance::Pty,
            EventPayload::TerminalOutput { range },
        )
        .expect("valid event")
    }

    /// T-TRANSCRIPT-EVENT-IDEMPOTENCY — replaying an identical event is a no-op;
    /// reusing its identity for different meaning fails closed.
    /// Required mutation: `RILL_MUTATE=duplicate_event_id_appends`.
    #[test]
    fn t_transcript_event_append_is_idempotent_and_conflicts_fail_closed() {
        let mut ledger = TranscriptLedger::new();
        let event = output_event("event-1", 1, (0, 8));
        ledger.append(event.clone()).expect("first append");
        ledger.append(event).expect("identical replay");
        assert_eq!(ledger.len(), 1);

        let conflict = output_event("event-1", 2, (8, 16));
        let err = ledger
            .append(conflict)
            .expect_err("conflicting reuse must fail");
        assert!(matches!(err, TranscriptError::ConflictingEventId(_)));
        assert_eq!(ledger.len(), 1);
    }

    /// T-TRANSCRIPT-BYTE-EVENT-ORDER — terminal source ranges must be monotonic,
    /// non-overlapping and bound to the same execution generation.
    /// Required mutation: `RILL_MUTATE=semantic_before_source_offset`.
    #[test]
    fn t_transcript_terminal_ranges_follow_byte_order_and_generation() {
        let mut ledger = TranscriptLedger::new();
        ledger
            .append(output_event("event-1", 1, (10, 20)))
            .expect("first range");

        let overlap = output_event("event-2", 2, (19, 30));
        assert!(matches!(
            ledger.append(overlap),
            Err(TranscriptError::OverlappingTerminalRange { .. })
        ));

        let wrong_generation = SemanticEvent::new(
            EventId::new("event-3").expect("valid id"),
            2,
            2,
            EventProvenance::Pty,
            EventPayload::TerminalOutput { range: (20, 30) },
        )
        .expect("valid event");
        assert!(matches!(
            ledger.append(wrong_generation),
            Err(TranscriptError::GenerationMismatch { .. })
        ));
    }

    /// T-CONTENT-SOURCE-AUTHORITY — command claims require an explicit producer;
    /// raw PTY provenance cannot manufacture a command boundary.
    /// Required mutation: `RILL_MUTATE=scrape_cells_for_command_and_pass_count`.
    #[test]
    fn t_content_source_authority_rejects_command_claim_from_pty() {
        let err = SemanticEvent::new(
            EventId::new("event-command").expect("valid id"),
            1,
            1,
            EventProvenance::Pty,
            EventPayload::CommandSubmitted {
                command: "cargo test".to_owned(),
            },
        )
        .expect_err("PTY bytes cannot claim a command boundary");
        assert!(matches!(
            err,
            TranscriptError::UnauthorizedProvenance { .. }
        ));
    }
}
