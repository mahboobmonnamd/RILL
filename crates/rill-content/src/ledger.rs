use crate::range::SourceRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    TerminalInput,
    TerminalOutput,
    BackgroundOutput,
    AgentMessage,
    ToolCall,
    ToolResult,
    Approval,
    Question,
    DiffResult,
    Lifecycle,
    Discontinuity,
    Unstructured,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEvent {
    pub id: EventId,
    pub kind: EventKind,
    pub sequence: u64,
    pub payload: Vec<u8>,
    pub range: Option<SourceRange>,
}

#[derive(Debug)]
pub enum TranscriptError {
    Conflict,
}

impl std::fmt::Display for TranscriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => write!(f, "event id reused with a different payload"),
        }
    }
}

impl std::error::Error for TranscriptError {}

#[derive(Debug, Default)]
pub struct SemanticLedger {
    events: Vec<SemanticEvent>,
    next: u64,
}

impl SemanticLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_id_raw(&mut self) -> u64 {
        let id = self.next;
        self.next = self.next.saturating_add(1);
        id
    }

    /// Append is idempotent for the same EventId and payload. A reused id with
    /// a different payload fails closed.
    pub fn append(&mut self, mut event: SemanticEvent) -> Result<EventId, TranscriptError> {
        if let Some(existing) = self.events.iter().find(|e| e.id == event.id) {
            if existing.payload != event.payload || existing.kind != event.kind {
                return Err(TranscriptError::Conflict);
            }
            return Ok(event.id);
        }
        if event.sequence == 0 {
            event.sequence = self.events.len() as u64 + 1;
        }
        let id = event.id;
        self.events.push(event);
        if id.0 >= self.next {
            self.next = id.0.saturating_add(1);
        }
        Ok(id)
    }

    pub fn get(&self, id: EventId) -> Option<&SemanticEvent> {
        self.events.iter().find(|e| e.id == id)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SemanticEvent> {
        self.events.iter()
    }

    pub fn hash_order(&self) -> u64 {
        let mut h = 0xcbf29ce484222325u64;
        for e in &self.events {
            h ^= e.id.0;
            h = h.wrapping_mul(0x100000001b3);
            h ^= e.sequence;
            h = h.wrapping_mul(0x100000001b3);
            h ^= e.payload.len() as u64;
        }
        h
    }
}
