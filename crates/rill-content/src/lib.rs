//! ContentTimeline and the semantic event ledger (ADR 0053 D7, SPEC-CONTENT).
//!
//! This crate does not paint, does not own a PTY, and must not depend on
//! AppKit. Command boundaries come only from explicit marks or structured
//! submissions — never from a prompt regex.

mod ledger;
mod policy;
mod range;
mod timeline;

pub use ledger::{EventId, EventKind, SemanticEvent, SemanticLedger, TranscriptError};
pub use policy::{redact_export, RetentionClass, RetentionPolicy, RetentionTier};
pub use range::{recover_range, HotRing, RangeError, SourceRange};
pub use timeline::{
    ContentItem, ContentItemId, ContentKind, ContentTimeline, FlowBlock, TimelineError,
};

/// Host-side Flow session: ledger + timeline + a non-durable composer draft.
#[derive(Debug)]
pub struct FlowSession {
    pub ledger: SemanticLedger,
    pub timeline: ContentTimeline,
    pub ring: HotRing,
    pub generation: u64,
    draft: String,
    open_output: Option<ContentItemId>,
    checkpoint_id: u64,
}

impl Default for FlowSession {
    fn default() -> Self {
        Self::new(64 * 1024)
    }
}

impl FlowSession {
    pub fn new(ring_cap: usize) -> Self {
        Self {
            ledger: SemanticLedger::new(),
            timeline: ContentTimeline::new(),
            ring: HotRing::new(ring_cap),
            generation: 1,
            draft: String::new(),
            open_output: None,
            checkpoint_id: 1,
        }
    }

    /// Unsent composer text. Never written to the ledger or ring.
    pub fn draft(&self) -> &str {
        &self.draft
    }

    pub fn set_draft(&mut self, text: String) {
        if mutate("persist_composer_draft") {
            let id = EventId(self.ledger.next_id_raw());
            let _ = self.ledger.append(SemanticEvent {
                id,
                kind: EventKind::Lifecycle,
                sequence: 0,
                payload: text.clone().into_bytes(),
                range: None,
            });
        }
        self.draft = text;
    }

    pub fn clear_draft(&mut self) {
        self.draft.clear();
    }

    /// Explicit structured submission. The accepted bytes are the boundary.
    pub fn submit_command(&mut self, command: &[u8]) -> Result<ContentItemId, TimelineError> {
        if mutate("prompt_regex_boundary") {
            return self.ingest_pty_bytes(command);
        }
        let start = self.ring.end_offset();
        let ev = SemanticEvent {
            id: EventId(self.ledger.next_id_raw()),
            kind: EventKind::TerminalInput,
            sequence: 0,
            payload: command.to_vec(),
            range: Some(SourceRange {
                generation: self.generation,
                start,
                end: start,
                checkpoint_id: Some(self.checkpoint_id),
            }),
        };
        let event_id = self.ledger.append(ev.clone())?;
        let item = self.timeline.append_from_event(&ev, event_id)?;
        self.open_output = None;
        self.draft.clear();
        Ok(item)
    }

    /// PTY bytes after a submission attach to that command. Unmarked bytes
    /// stay an honest terminal region, even if they look like `$ cmd`.
    pub fn ingest_pty_bytes(&mut self, bytes: &[u8]) -> Result<ContentItemId, TimelineError> {
        if bytes.is_empty() {
            return Err(TimelineError::Empty);
        }
        let start = self.ring.end_offset();
        self.ring.append(bytes);
        let end = self.ring.end_offset();
        let range = SourceRange {
            generation: self.generation,
            start,
            end,
            checkpoint_id: Some(self.checkpoint_id),
        };
        let fake_command = (mutate("scrape_cells_for_command_and_pass_count")
            || mutate("prompt_regex_boundary"))
            && looks_like_prompt(bytes);
        if fake_command {
            let ev = SemanticEvent {
                id: EventId(self.ledger.next_id_raw()),
                kind: EventKind::TerminalInput,
                sequence: 0,
                payload: bytes.to_vec(),
                range: Some(range),
            };
            let event_id = self.ledger.append(ev.clone())?;
            return self.timeline.append_from_event(&ev, event_id);
        }
        let kind = if self.timeline.last_input().is_some() {
            EventKind::TerminalOutput
        } else {
            EventKind::Unstructured
        };
        let ev = SemanticEvent {
            id: EventId(self.ledger.next_id_raw()),
            kind,
            sequence: 0,
            payload: bytes.to_vec(),
            range: Some(range),
        };
        if let Some(id) = self.open_output {
            if !mutate("timeline_rereads_ring") {
                self.timeline.extend_output(id, bytes, range)?;
            }
            let _ = self.ledger.append(ev);
            return Ok(id);
        }
        let event_id = self.ledger.append(ev.clone())?;
        let item = self.timeline.append_from_event(&ev, event_id)?;
        if kind == EventKind::TerminalOutput {
            self.open_output = Some(item);
        }
        Ok(item)
    }

    pub fn close_open_output(&mut self) {
        self.open_output = None;
    }

    pub fn mark_checkpoint(&mut self) {
        self.checkpoint_id = self.checkpoint_id.saturating_add(1);
    }

    /// Materialized items remain after the hot ring forgets the source bytes.
    pub fn evict_hot_ring_to(&mut self, cap: usize) {
        let snap = self.ring.snapshot();
        self.ring = HotRing::new(cap);
        self.ring.append(&snap);
        if mutate("timeline_rereads_ring") {
            for item in self.timeline.items_mut() {
                if let Some(r) = item.range {
                    item.payload = self.ring.deltas_after(r.start).unwrap_or_default();
                }
            }
        }
    }
}

fn looks_like_prompt(bytes: &[u8]) -> bool {
    bytes.windows(2).any(|w| w == b"$ ")
}

fn mutate(name: &str) -> bool {
    #[cfg(feature = "mutate")]
    {
        std::env::var("RILL_MUTATE").as_deref() == Ok(name)
    }
    #[cfg(not(feature = "mutate"))]
    {
        let _ = name;
        false
    }
}
