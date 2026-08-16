//! Bounded byte history. Overwrite oldest when full. Never used as a
//! drop-on-full live pipe — live attach uses credit + stop-reading.

use std::collections::VecDeque;

#[derive(Debug)]
pub struct ByteRing {
    data: VecDeque<u8>,
    cap: usize,
}

impl ByteRing {
    pub fn new(cap: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(cap.min(64 * 1024)),
            cap: cap.max(1),
        }
    }

    pub fn append(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if bytes.len() >= self.cap {
            self.data.clear();
            self.data.extend(bytes[bytes.len() - self.cap..].iter().copied());
            return;
        }
        let overflow = self.data.len().saturating_add(bytes.len()).saturating_sub(self.cap);
        if overflow > 0 {
            self.data.drain(..overflow);
        }
        self.data.extend(bytes.iter().copied());
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
        assert_eq!(got, fixture, "ring must keep original bytes, not UTF-8 replacement");
        assert!(!got.windows(3).any(|w| w == [0xef, 0xbf, 0xbd]));
    }

    #[test]
    fn bounded_ring_keeps_tail() {
        let mut ring = ByteRing::new(4);
        ring.append(b"abcdef");
        assert_eq!(ring.snapshot(), b"cdef");
    }
}
