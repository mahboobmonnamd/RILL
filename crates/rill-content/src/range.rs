use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceRange {
    pub generation: u64,
    pub start: u64,
    pub end: u64,
    pub checkpoint_id: Option<u64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RangeError {
    Evicted,
    NeedsCheckpoint,
    Truncated,
}

impl std::fmt::Display for RangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evicted => write!(f, "source range evicted from the hot ring"),
            Self::NeedsCheckpoint => {
                write!(
                    f,
                    "range starts mid-sequence and needs a compatible checkpoint"
                )
            }
            Self::Truncated => write!(f, "source range truncated"),
        }
    }
}

impl std::error::Error for RangeError {}

/// Bounded hot bytes for offset correlation. Not a PTY and not the kernel ring.
#[derive(Debug)]
pub struct HotRing {
    data: VecDeque<u8>,
    cap: usize,
    retained_base: u64,
    end: u64,
}

impl HotRing {
    pub fn new(cap: usize) -> Self {
        Self {
            data: VecDeque::new(),
            cap: cap.max(1),
            retained_base: 0,
            end: 0,
        }
    }

    pub fn retained_base(&self) -> u64 {
        self.retained_base
    }

    pub fn end_offset(&self) -> u64 {
        self.end
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.data.iter().copied().collect()
    }

    pub fn append(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.end = self.end.saturating_add(bytes.len() as u64);
        if bytes.len() >= self.cap {
            self.data.clear();
            self.data
                .extend(bytes[bytes.len() - self.cap..].iter().copied());
            self.retained_base = self.end.saturating_sub(self.data.len() as u64);
            return;
        }
        let overflow = self
            .data
            .len()
            .saturating_add(bytes.len())
            .saturating_sub(self.cap);
        if overflow > 0 {
            self.data.drain(..overflow);
        }
        self.data.extend(bytes.iter().copied());
        self.retained_base = self.end.saturating_sub(self.data.len() as u64);
    }

    pub fn deltas_after(&self, start: u64) -> Option<Vec<u8>> {
        if start < self.retained_base || start > self.end {
            return None;
        }
        let skip = (start - self.retained_base) as usize;
        Some(self.data.iter().skip(skip).copied().collect())
    }
}

/// Reconstruct bytes for a range. A slice that begins in the middle of an
/// escape sequence is refused unless `checkpoint_id` is present.
pub fn recover_range(ring: &HotRing, range: SourceRange) -> Result<Vec<u8>, RangeError> {
    if range.start < ring.retained_base() {
        return Err(RangeError::Evicted);
    }
    let bytes = ring.deltas_after(range.start).ok_or(RangeError::Evicted)?;
    let take = (range.end.saturating_sub(range.start)) as usize;
    let slice = if take > bytes.len() {
        return Err(RangeError::Truncated);
    } else {
        bytes[..take].to_vec()
    };
    if mutate_reset_vt() {
        return Ok(slice);
    }
    if range.checkpoint_id.is_none() && starts_mid_sequence(&slice) {
        return Err(RangeError::NeedsCheckpoint);
    }
    Ok(slice)
}

fn starts_mid_sequence(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }
    matches!(bytes[0], b'[' | b';' | b'0'..=b'9' | b'm' | b'H')
        && bytes.iter().any(|&b| b == 0x1b || b.is_ascii_digit())
}

fn mutate_reset_vt() -> bool {
    #[cfg(feature = "mutate")]
    {
        std::env::var("RILL_MUTATE").as_deref() == Ok("always_reset_vt")
    }
    #[cfg(not(feature = "mutate"))]
    {
        false
    }
}
