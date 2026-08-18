//! SPEC-ATTENTION: the orchestration-plane queue (ADR 0029).

/// Priority order, lowest to highest (SPEC-ATTENTION §8): approval outranks
/// everything, completed_unread outranks nothing. Declared in ascending
/// order so `Ord`'s derived comparison matches the spec's priority exactly —
/// `max()` over a set of states picks the correct rollup value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionState {
    CompletedUnread,
    Disconnected,
    Failed,
    NeedsInput,
    Approval,
}

fn is_actionable(s: AttentionState) -> bool {
    matches!(s, AttentionState::NeedsInput | AttentionState::Approval)
}

#[derive(Clone, Debug)]
pub struct AttentionEntry {
    pub id: u64,
    pub target: String,
    pub state: AttentionState,
    pub origin: String,
    pub seq: u64,
    pub at_unix_ms: u64,
    read: bool,
}

pub struct ViewEvent {
    pub visible: bool,
    pub key: bool,
    pub dwell_ms: u64,
}

const MIN_DWELL_MS: u64 = 250;

/// One queue. Every surface — badges, mailbox, rollup, next-attention — MUST
/// read this, never derive attention independently (SPEC-ATTENTION §1).
pub struct AttentionQueue {
    entries: Vec<AttentionEntry>,
    next_id: u64,
    next_seq: u64,
    cap_per_target: usize,
}

impl AttentionQueue {
    pub fn new(cap_per_target: usize) -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            next_seq: 1,
            cap_per_target,
        }
    }

    /// Cold enqueue (SPEC-ATTENTION §2). On overflow, the oldest
    /// **non-actionable** entry for this target is dropped first;
    /// `needs_input`/`approval` are never dropped for capacity. Under
    /// `overflow_drops_actionable` the first entry for the target is dropped
    /// regardless of state.
    pub fn enqueue(
        &mut self,
        target: impl Into<String>,
        state: AttentionState,
        origin: impl Into<String>,
        at_unix_ms: u64,
    ) -> u64 {
        let target = target.into();
        let id = self.next_id;
        self.next_id += 1;
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(AttentionEntry {
            id,
            target: target.clone(),
            state,
            origin: origin.into(),
            seq,
            at_unix_ms,
            read: false,
        });

        let count = self.entries.iter().filter(|e| e.target == target).count();
        if count > self.cap_per_target {
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref() == Ok("overflow_drops_actionable") {
                if let Some(pos) = self.entries.iter().position(|e| e.target == target) {
                    self.entries.remove(pos);
                }
                return id;
            }
            if let Some(pos) = self
                .entries
                .iter()
                .position(|e| e.target == target && !is_actionable(e.state))
            {
                self.entries.remove(pos);
            }
        }
        id
    }

    /// Highest-urgency state among unread entries for `targets`
    /// (SPEC-ATTENTION §8). Under `rollup_wrong_order` this returns the
    /// *lowest* urgency instead.
    pub fn rollup(&self, targets: &[&str]) -> Option<AttentionState> {
        let unread = self
            .entries
            .iter()
            .filter(|e| targets.contains(&e.target.as_str()) && !e.read)
            .map(|e| e.state);
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("rollup_wrong_order") {
            return unread.min();
        }
        unread.max()
    }

    /// Clears only on a real view: visible, window key, minimum dwell
    /// (SPEC-ATTENTION §7). Under `clear_on_any_render` any render clears
    /// it, including hidden or momentary ones.
    pub fn observe_view(&mut self, id: u64, view: ViewEvent) {
        #[cfg(feature = "mutate")]
        let clears = if std::env::var("RILL_MUTATE").as_deref() == Ok("clear_on_any_render") {
            true
        } else {
            view.visible && view.key && view.dwell_ms >= MIN_DWELL_MS
        };
        #[cfg(not(feature = "mutate"))]
        let clears = view.visible && view.key && view.dwell_ms >= MIN_DWELL_MS;

        if clears {
            if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                e.read = true;
            }
        }
    }

    pub fn is_read(&self, id: u64) -> bool {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.read)
            .unwrap_or(false)
    }

    /// One of two independent projections of the same queue (badges).
    /// Under `second_attention_classifier` this stops reading `entries` —
    /// the "second classifier" ADR 0029 D1 forbids.
    pub fn badge_count(&self, target: &str) -> usize {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("second_attention_classifier") {
            return 0;
        }
        self.entries
            .iter()
            .filter(|e| e.target == target && !e.read)
            .count()
    }

    /// The other projection (mailbox). Must always agree with `badge_count`
    /// because both read the same `entries`.
    pub fn mailbox_entries(&self, target: &str) -> Vec<&AttentionEntry> {
        self.entries
            .iter()
            .filter(|e| e.target == target && !e.read)
            .collect()
    }
}
