//! Framed attach codec for `SOCK_STREAM`.
//!
//! Layout: one tag byte + u32 little-endian length + payload.
//! Not JSON. Not cells. Darwin has no `SOCK_SEQPACKET`.

use std::fmt;

pub const MAX_FRAME: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    Data = 1,
    Credit = 2,
    Resize = 3,
    Exit = 4,
    Attach = 5,
    Refused = 6,
}

impl Tag {
    pub fn from_u8(v: u8) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::Data),
            2 => Ok(Self::Credit),
            3 => Ok(Self::Resize),
            4 => Ok(Self::Exit),
            5 => Ok(Self::Attach),
            6 => Ok(Self::Refused),
            other => Err(Error::UnknownTag(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RefuseReason {
    AlreadyAttached = 1,
    Invalid = 2,
}

impl RefuseReason {
    pub fn from_u8(v: u8) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::AlreadyAttached),
            2 => Ok(Self::Invalid),
            other => Err(Error::UnknownRefuse(other)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Data(Vec<u8>),
    Credit(u32),
    Resize {
        cols: u16,
        rows: u16,
        px_w: u16,
        px_h: u16,
    },
    Exit {
        status: i32,
    },
    Attach {
        generation: u64,
    },
    Refused {
        reason: RefuseReason,
    },
}

impl Frame {
    pub fn tag(&self) -> Tag {
        match self {
            Self::Data(_) => Tag::Data,
            Self::Credit(_) => Tag::Credit,
            Self::Resize { .. } => Tag::Resize,
            Self::Exit { .. } => Tag::Exit,
            Self::Attach { .. } => Tag::Attach,
            Self::Refused { .. } => Tag::Refused,
        }
    }

    /// Frames permitted on the warm path (SPEC-ATTACH §8).
    ///
    /// Replaces `is_control_rpc`, which returned a hardcoded `false` and was
    /// never called — the reported `control_rpc=0` was guaranteed by the
    /// literal, not by the absence of RPCs (docs/SPIKE-0-AUDIT.md S1-3).
    /// T-NFR now asserts this at the client's single `send` chokepoint.
    pub fn is_warm_path(&self) -> bool {
        matches!(self, Self::Data(_) | Self::Credit(_))
    }

    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let payload = match self {
            Self::Data(b) => b.clone(),
            Self::Credit(n) => n.to_le_bytes().to_vec(),
            Self::Resize {
                cols,
                rows,
                px_w,
                px_h,
            } => {
                let mut p = Vec::with_capacity(8);
                p.extend_from_slice(&cols.to_le_bytes());
                p.extend_from_slice(&rows.to_le_bytes());
                p.extend_from_slice(&px_w.to_le_bytes());
                p.extend_from_slice(&px_h.to_le_bytes());
                p
            }
            Self::Exit { status } => status.to_le_bytes().to_vec(),
            Self::Attach { generation } => generation.to_le_bytes().to_vec(),
            Self::Refused { reason } => vec![*reason as u8],
        };
        if payload.len() > MAX_FRAME {
            return Err(Error::TooLarge(payload.len()));
        }
        let mut out = Vec::with_capacity(5 + payload.len());
        out.push(self.tag() as u8);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&payload);
        Ok(out)
    }

    pub fn decode(tag: Tag, payload: &[u8]) -> Result<Self, Error> {
        match tag {
            Tag::Data => Ok(Self::Data(payload.to_vec())),
            Tag::Credit => {
                let n = read_u32(payload)?;
                Ok(Self::Credit(n))
            }
            Tag::Resize => {
                if payload.len() != 8 {
                    return Err(Error::Truncated);
                }
                Ok(Self::Resize {
                    cols: u16::from_le_bytes([payload[0], payload[1]]),
                    rows: u16::from_le_bytes([payload[2], payload[3]]),
                    px_w: u16::from_le_bytes([payload[4], payload[5]]),
                    px_h: u16::from_le_bytes([payload[6], payload[7]]),
                })
            }
            Tag::Exit => {
                let status = read_i32(payload)?;
                Ok(Self::Exit { status })
            }
            Tag::Attach => {
                let generation = read_u64(payload)?;
                Ok(Self::Attach { generation })
            }
            Tag::Refused => {
                if payload.len() != 1 {
                    return Err(Error::Truncated);
                }
                Ok(Self::Refused {
                    reason: RefuseReason::from_u8(payload[0])?,
                })
            }
        }
    }
}

fn read_u32(p: &[u8]) -> Result<u32, Error> {
    if p.len() != 4 {
        return Err(Error::Truncated);
    }
    Ok(u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
}

fn read_i32(p: &[u8]) -> Result<i32, Error> {
    if p.len() != 4 {
        return Err(Error::Truncated);
    }
    Ok(i32::from_le_bytes([p[0], p[1], p[2], p[3]]))
}

fn read_u64(p: &[u8]) -> Result<u64, Error> {
    if p.len() != 8 {
        return Err(Error::Truncated);
    }
    Ok(u64::from_le_bytes([
        p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7],
    ]))
}

#[derive(Debug)]
pub enum Error {
    UnknownTag(u8),
    UnknownRefuse(u8),
    Truncated,
    TooLarge(usize),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTag(t) => write!(f, "unknown attach tag {t}"),
            Self::UnknownRefuse(r) => write!(f, "unknown refuse reason {r}"),
            Self::Truncated => write!(f, "truncated attach frame"),
            Self::TooLarge(n) => write!(f, "attach frame too large ({n} bytes)"),
            Self::Io(e) => write!(f, "attach io: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Incremental decoder for a byte stream (SOCK_STREAM, not seqpacket).
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Frame>, Error> {
        self.buf.extend_from_slice(bytes);
        let mut frames = Vec::new();
        loop {
            if self.buf.len() < 5 {
                break;
            }
            let tag = Tag::from_u8(self.buf[0])?;
            let len = u32::from_le_bytes([
                self.buf[1],
                self.buf[2],
                self.buf[3],
                self.buf[4],
            ]) as usize;
            if len > MAX_FRAME {
                return Err(Error::TooLarge(len));
            }
            if self.buf.len() < 5 + len {
                // Fail closed rather than buffering without bound: a peer that
                // declares a large frame and never sends it must not be able to
                // grow the daemon's memory (SPEC-ATTACH §3).
                if self.buf.len() > MAX_FRAME + 5 {
                    return Err(Error::TooLarge(self.buf.len()));
                }
                break;
            }
            let payload = self.buf[5..5 + len].to_vec();
            self.buf.drain(..5 + len);
            frames.push(Frame::decode(tag, &payload)?);
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_attach_second_attach_frame_is_refused_payload() {
        let f = Frame::Refused {
            reason: RefuseReason::AlreadyAttached,
        };
        let bytes = f.encode().expect("encode");
        let mut d = Decoder::new();
        let got = d.push(&bytes).expect("decode");
        assert_eq!(
            got,
            vec![Frame::Refused {
                reason: RefuseReason::AlreadyAttached
            }]
        );
    }

    #[test]
    fn t_resize_frame_is_ordered_with_data_on_the_stream() {
        let mut d = Decoder::new();
        let mut stream = Vec::new();
        stream.extend(
            Frame::Data(b"partial".to_vec())
                .encode()
                .expect("data"),
        );
        stream.extend(
            Frame::Resize {
                cols: 100,
                rows: 40,
                px_w: 800,
                px_h: 600,
            }
            .encode()
            .expect("resize"),
        );
        stream.extend(Frame::Data(b"more".to_vec()).encode().expect("data2"));
        let got = d.push(&stream).expect("decode");
        assert!(matches!(got[0], Frame::Data(_)));
        assert!(matches!(
            got[1],
            Frame::Resize {
                cols: 100,
                rows: 40,
                ..
            }
        ));
        assert!(matches!(got[2], Frame::Data(_)));
    }

    #[test]
    fn t_exit_frame_decodes_status() {
        let bytes = Frame::Exit { status: 0 }.encode().expect("encode");
        let mut d = Decoder::new();
        let got = d.push(&bytes).expect("decode");
        assert_eq!(got, vec![Frame::Exit { status: 0 }]);
    }

    #[test]
    fn sock_stream_partial_reads_reassemble() {
        let full = Frame::Attach { generation: 7 }
            .encode()
            .expect("encode");
        let mut d = Decoder::new();
        assert!(d.push(&full[..3]).expect("p1").is_empty());
        let rest = d.push(&full[3..]).expect("p2");
        assert_eq!(rest, vec![Frame::Attach { generation: 7 }]);
    }

    #[test]
    fn no_json_on_the_wire() {
        let bytes = Frame::Data(b"hi".to_vec()).encode().expect("encode");
        assert_ne!(bytes[0], b'{');
        assert_ne!(bytes[0], b'[');
    }

    /// SPEC-ATTACH §8. Only DATA and CREDIT belong on a keystroke.
    #[test]
    fn only_data_and_credit_are_warm_path_frames() {
        assert!(Frame::Data(vec![1]).is_warm_path());
        assert!(Frame::Credit(1).is_warm_path());
        assert!(!Frame::Attach { generation: 1 }.is_warm_path());
        assert!(!Frame::Exit { status: 0 }.is_warm_path());
        assert!(!Frame::Resize {
            cols: 1,
            rows: 1,
            px_w: 1,
            px_h: 1
        }
        .is_warm_path());
        assert!(!Frame::Refused {
            reason: RefuseReason::Invalid
        }
        .is_warm_path());
    }

    #[test]
    fn unknown_tag_is_a_protocol_error_not_a_skip() {
        let mut d = Decoder::new();
        let err = d.push(&[99, 0, 0, 0, 0]).expect_err("unknown tag must fail");
        assert!(matches!(err, Error::UnknownTag(99)));
    }

    #[test]
    fn declared_length_beyond_max_frame_is_rejected() {
        let mut d = Decoder::new();
        let mut bytes = vec![Tag::Data as u8];
        bytes.extend_from_slice(&(MAX_FRAME as u32 + 1).to_le_bytes());
        assert!(matches!(d.push(&bytes), Err(Error::TooLarge(_))));
    }

    /// Every byte boundary, not just one. This is the hazard SOCK_STREAM
    /// introduces over seqpacket and it is the decoder's whole purpose.
    #[test]
    fn every_split_point_reassembles() {
        let full = Frame::Resize {
            cols: 100,
            rows: 40,
            px_w: 800,
            px_h: 600,
        }
        .encode()
        .expect("encode");
        for split in 0..=full.len() {
            let mut d = Decoder::new();
            let mut got = d.push(&full[..split]).expect("first half");
            got.extend(d.push(&full[split..]).expect("second half"));
            assert_eq!(got.len(), 1, "split at {split} lost or duplicated a frame");
        }
    }
}
