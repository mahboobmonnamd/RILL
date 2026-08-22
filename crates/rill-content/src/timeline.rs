use crate::ledger::{EventId, EventKind, SemanticEvent, TranscriptError};
use crate::range::SourceRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentItemId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentKind {
    TerminalInput,
    TerminalOutput,
    Unstructured,
    Discontinuity,
    Other,
}

impl From<EventKind> for ContentKind {
    fn from(k: EventKind) -> Self {
        match k {
            EventKind::TerminalInput => Self::TerminalInput,
            EventKind::TerminalOutput => Self::TerminalOutput,
            EventKind::Unstructured => Self::Unstructured,
            EventKind::Discontinuity => Self::Discontinuity,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentItem {
    pub id: ContentItemId,
    pub event_id: EventId,
    pub kind: ContentKind,
    pub payload: Vec<u8>,
    pub range: Option<SourceRange>,
    pub truncated: bool,
}

/// Derived grouping over stable ContentItemIds — not a `(start,end)` only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlowBlock {
    pub items: Vec<ContentItemId>,
}

#[derive(Debug)]
pub enum TimelineError {
    Empty,
    MissingItem,
}

impl std::fmt::Display for TimelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty content"),
            Self::MissingItem => write!(f, "content item missing"),
        }
    }
}

impl std::error::Error for TimelineError {}

impl From<TranscriptError> for TimelineError {
    fn from(_: TranscriptError) -> Self {
        Self::Empty
    }
}

#[derive(Debug, Default)]
pub struct ContentTimeline {
    items: Vec<ContentItem>,
    next: u64,
}

impl ContentTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append_from_event(
        &mut self,
        event: &SemanticEvent,
        event_id: EventId,
    ) -> Result<ContentItemId, TimelineError> {
        let id = ContentItemId(self.next);
        self.next = self.next.saturating_add(1);
        self.items.push(ContentItem {
            id,
            event_id,
            kind: ContentKind::from(event.kind),
            payload: event.payload.clone(),
            range: event.range,
            truncated: false,
        });
        Ok(id)
    }

    pub fn extend_output(
        &mut self,
        id: ContentItemId,
        bytes: &[u8],
        range: SourceRange,
    ) -> Result<(), TimelineError> {
        let item = self
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or(TimelineError::MissingItem)?;
        item.payload.extend_from_slice(bytes);
        if let Some(r) = item.range.as_mut() {
            r.end = range.end;
        } else {
            item.range = Some(range);
        }
        Ok(())
    }

    pub fn mark_truncated(
        &mut self,
        id: ContentItemId,
        reason: &[u8],
    ) -> Result<(), TimelineError> {
        let item = self
            .items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or(TimelineError::MissingItem)?;
        item.truncated = true;
        if item.kind != ContentKind::Discontinuity {
            item.kind = ContentKind::Discontinuity;
            item.payload = reason.to_vec();
        }
        Ok(())
    }

    pub fn get(&self, id: ContentItemId) -> Option<&ContentItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn items(&self) -> &[ContentItem] {
        &self.items
    }

    pub fn items_mut(&mut self) -> &mut [ContentItem] {
        &mut self.items
    }

    pub fn last_input(&self) -> Option<&ContentItem> {
        self.items
            .iter()
            .rev()
            .find(|i| i.kind == ContentKind::TerminalInput)
    }

    /// Block identity is the set of ContentItemIds, not only ring offsets.
    pub fn block_for_input(&self, input: ContentItemId) -> FlowBlock {
        let mut items = vec![input];
        if mutate_range_only() {
            return FlowBlock { items };
        }
        if let Some(idx) = self.items.iter().position(|i| i.id == input) {
            for rest in &self.items[idx + 1..] {
                if rest.kind == ContentKind::TerminalInput {
                    break;
                }
                items.push(rest.id);
            }
        }
        FlowBlock { items }
    }

    pub fn visible_window(&self, start: usize, limit: usize) -> &[ContentItem] {
        let s = start.min(self.items.len());
        let e = s.saturating_add(limit).min(self.items.len());
        &self.items[s..e]
    }
}

fn mutate_range_only() -> bool {
    #[cfg(feature = "mutate")]
    {
        std::env::var("RILL_MUTATE").as_deref() == Ok("block_stores_only_offsets")
    }
    #[cfg(not(feature = "mutate"))]
    {
        false
    }
}
