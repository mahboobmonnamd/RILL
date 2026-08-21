//! Bounded byte history. Overwrite oldest when full. Never used as a
//! drop-on-full live pipe — live attach uses credit + stop-reading.

use crate::error::Error;
use std::collections::VecDeque;

#[derive(Debug)]
pub struct ByteRing {
    data: VecDeque<u8>,
    cap: usize,
    /// Monotonic offset of `data[0]`. Eviction advances this; it never
    /// renumbers surviving bytes (SPEC-CONTENT §2, #313).
    retained_base: u64,
    /// Next offset to assign (exclusive end).
    end: u64,
}

impl ByteRing {
    pub fn new(cap: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(cap.min(64 * 1024)),
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

    /// Bytes strictly after `start`, if `start` is still retained.
    pub fn deltas_after(&self, start: u64) -> Result<Vec<u8>, Error> {
        if start < self.retained_base {
            return Err(Error::CheckpointEvicted);
        }
        if start > self.end {
            return Err(Error::CheckpointEvicted);
        }
        let skip = (start - self.retained_base) as usize;
        Ok(self.data.iter().skip(skip).copied().collect())
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.data.iter().copied().collect()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn cap(&self) -> usize {
        self.cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_bytes_ring_stores_raw_invalid_utf8() {
        let mut ring = ByteRing::new(64);
        let fixture = include_bytes!("../../../fixtures/invalid_utf8.bin");
        ring.append(fixture);
        let got = ring.snapshot();
        assert_eq!(
            got, fixture,
            "ring must keep original bytes, not UTF-8 replacement"
        );
        assert!(!got.windows(3).any(|w| w == [0xef, 0xbf, 0xbd]));
    }

    #[test]
    fn bounded_ring_keeps_tail() {
        let mut ring = ByteRing::new(4);
        ring.append(b"abcdef");
        assert_eq!(ring.snapshot(), b"cdef");
        assert_eq!(ring.retained_base(), 2);
        assert_eq!(ring.end_offset(), 6);
        assert!(ring.deltas_after(0).is_err());
        assert_eq!(ring.deltas_after(2).expect("tail"), b"cdef");
    }
}
