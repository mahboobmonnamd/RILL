//! Framed attach codec for `SOCK_STREAM`.
//!
//! Layout: one tag byte + u32 little-endian length + payload.
//! Not JSON. Not cells. Darwin has no `SOCK_SEQPACKET`.

use std::fmt;
use std::path::{Path, PathBuf};

pub const MAX_FRAME: usize = 4 * 1024 * 1024;
/// Attach protocol this tree speaks (ADR 0015 D1). 8- and 16-byte payloads imply 1.
pub const PROTOCOL_VERSION: u8 = 1;
/// Independent client credit, checkpoints and deltas (SPEC-ATTACH §5, SPEC-CLIENT-AUTHORITY §2).
pub const PROTOCOL_2: u8 = 2;
/// `flags` bit 0: observe (ADR 0015 D7). Not a second writer.
pub const ATTACH_FLAG_OBSERVE: u8 = 1;

/// The read-only companion socket for cold kernel identity queries. It is
/// deliberately not an attach frame: the attach stream remains DATA/CREDIT on
/// its warm path (SPEC-ATTACH §8).
pub fn cold_identity_socket_path(attach_socket: impl AsRef<Path>) -> PathBuf {
    let mut path = attach_socket.as_ref().as_os_str().to_os_string();
    path.push(".identity");
    PathBuf::from(path)
}

/// Cold container-tree projection for chrome (SPEC-NAV §1). Not an attach frame.
pub fn cold_nav_socket_path(attach_socket: impl AsRef<Path>) -> PathBuf {
    let mut path = attach_socket.as_ref().as_os_str().to_os_string();
    path.push(".nav");
    PathBuf::from(path)
}

/// Internal worker listener. Sibling of the public attach socket, never under
/// shared `/tmp` as the production parent (SPEC-RUNTIME-SUPERVISION §2).
pub fn worker_socket_path(attach_socket: impl AsRef<Path>) -> PathBuf {
    let attach = attach_socket.as_ref();
    match attach.file_name() {
        Some(name) => {
            let mut n = name.to_os_string();
            n.push(".worker");
            attach.with_file_name(n)
        }
        None => attach.with_extension("worker"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    Data = 1,
    Credit = 2,
    Resize = 3,
    Exit = 4,
    Attach = 5,
    Refused = 6,
    /// Cold checkpoint blob (SPEC-CLIENT-AUTHORITY §2, #314). Not warm-path.
    Checkpoint = 7,
    /// Ordered bytes strictly after a checkpoint offset.
    Delta = 8,
    /// Client requests a new checkpoint after hash mismatch.
    ResyncRequest = 9,
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
            7 => Ok(Self::Checkpoint),
            8 => Ok(Self::Delta),
            9 => Ok(Self::ResyncRequest),
            other => Err(Error::UnknownTag(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RefuseReason {
    AlreadyAttached = 1,
    Invalid = 2,
    ProtocolMismatch = 3,
}

impl RefuseReason {
    pub fn from_u8(v: u8) -> Result<Self, Error> {
        match v {
            1 => Ok(Self::AlreadyAttached),
            2 => Ok(Self::Invalid),
            3 => Ok(Self::ProtocolMismatch),
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
        /// `None` is the Spike 0 8-byte payload: attach the daemon's default
        /// leaf. `Some` is a kernel `SessionId` as `u64` (SPEC-GRAPH §2).
        session_id: Option<u64>,
        /// 8/16-byte payloads imply [`PROTOCOL_VERSION`].
        protocol: u8,
        /// ADR 0015 D7. Writer claim is unchanged.
        observe: bool,
    },
    Refused {
        reason: RefuseReason,
    },
    Checkpoint {
        ending_offset: u64,
        hash: u64,
        blob: Vec<u8>,
    },
    Delta {
        start_offset: u64,
        bytes: Vec<u8>,
    },
    ResyncRequest,
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
            Self::Checkpoint { .. } => Tag::Checkpoint,
            Self::Delta { .. } => Tag::Delta,
            Self::ResyncRequest => Tag::ResyncRequest,
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

    pub fn protocol_supported(protocol: u8) -> bool {
        protocol == PROTOCOL_VERSION || protocol == PROTOCOL_2
    }

    /// Spike 0 / named-id writer attach (protocol 1, not observe).
    pub fn attach(generation: u64, session_id: Option<u64>) -> Self {
        Self::Attach {
            generation,
            session_id,
            protocol: PROTOCOL_VERSION,
            observe: false,
        }
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
            Self::Attach {
                generation,
                session_id,
                protocol,
                observe,
            } => {
                let mut p = generation.to_le_bytes().to_vec();
                let extra = *protocol != PROTOCOL_VERSION || *observe;
                if let Some(id) = session_id {
                    p.extend_from_slice(&id.to_le_bytes());
                    if extra {
                        p.push(*protocol);
                        p.push(if *observe { ATTACH_FLAG_OBSERVE } else { 0 });
                    }
                } else if extra {
                    p.extend_from_slice(&0u64.to_le_bytes());
                    p.push(*protocol);
                    p.push(if *observe { ATTACH_FLAG_OBSERVE } else { 0 });
                }
                p
            }
            Self::Refused { reason } => vec![*reason as u8],
            Self::Checkpoint {
                ending_offset,
                hash,
                blob,
            } => {
                let mut p = Vec::with_capacity(16 + blob.len());
                p.extend_from_slice(&ending_offset.to_le_bytes());
                p.extend_from_slice(&hash.to_le_bytes());
                p.extend_from_slice(blob);
                p
            }
            Self::Delta {
                start_offset,
                bytes,
            } => {
                let mut p = Vec::with_capacity(8 + bytes.len());
                p.extend_from_slice(&start_offset.to_le_bytes());
                p.extend_from_slice(bytes);
                p
            }
            Self::ResyncRequest => Vec::new(),
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
            Tag::Attach => match payload.len() {
                8 => Ok(Self::Attach {
                    generation: read_u64(payload)?,
                    session_id: None,
                    protocol: PROTOCOL_VERSION,
                    observe: false,
                }),
                16 => {
                    let generation =
                        u64::from_le_bytes(payload[0..8].try_into().map_err(|_| Error::Truncated)?);
                    let session_id = u64::from_le_bytes(
                        payload[8..16].try_into().map_err(|_| Error::Truncated)?,
                    );
                    Ok(Self::Attach {
                        generation,
                        session_id: Some(session_id),
                        protocol: PROTOCOL_VERSION,
                        observe: false,
                    })
                }
                18 => {
                    let generation =
                        u64::from_le_bytes(payload[0..8].try_into().map_err(|_| Error::Truncated)?);
                    let raw_id = u64::from_le_bytes(
                        payload[8..16].try_into().map_err(|_| Error::Truncated)?,
                    );
                    let protocol = payload[16];
                    let observe = payload[17] & ATTACH_FLAG_OBSERVE != 0;
                    Ok(Self::Attach {
                        generation,
                        session_id: if raw_id == 0 { None } else { Some(raw_id) },
                        protocol,
                        observe,
                    })
                }
                _ => Err(Error::Truncated),
            },
            Tag::Refused => {
                if payload.len() != 1 {
                    return Err(Error::Truncated);
                }
                Ok(Self::Refused {
                    reason: RefuseReason::from_u8(payload[0])?,
                })
            }
            Tag::Checkpoint => {
                if payload.len() < 16 {
                    return Err(Error::Truncated);
                }
                let ending_offset =
                    u64::from_le_bytes(payload[0..8].try_into().map_err(|_| Error::Truncated)?);
                let hash =
                    u64::from_le_bytes(payload[8..16].try_into().map_err(|_| Error::Truncated)?);
                Ok(Self::Checkpoint {
                    ending_offset,
                    hash,
                    blob: payload[16..].to_vec(),
                })
            }
            Tag::Delta => {
                if payload.len() < 8 {
                    return Err(Error::Truncated);
                }
                let start_offset =
                    u64::from_le_bytes(payload[0..8].try_into().map_err(|_| Error::Truncated)?);
                Ok(Self::Delta {
                    start_offset,
                    bytes: payload[8..].to_vec(),
                })
            }
            Tag::ResyncRequest => {
                if !payload.is_empty() {
                    return Err(Error::Truncated);
                }
                Ok(Self::ResyncRequest)
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
            let len =
                u32::from_le_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
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
        stream.extend(Frame::Data(b"partial".to_vec()).encode().expect("data"));
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
        let full = Frame::attach(7, None).encode().expect("encode");
        let mut d = Decoder::new();
        assert!(d.push(&full[..3]).expect("p1").is_empty());
        let rest = d.push(&full[3..]).expect("p2");
        assert_eq!(rest, vec![Frame::attach(7, None)]);
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
        assert!(!Frame::attach(1, None).is_warm_path());
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
        assert!(!Frame::Checkpoint {
            ending_offset: 0,
            hash: 0,
            blob: vec![],
        }
        .is_warm_path());
        assert!(!Frame::Delta {
            start_offset: 0,
            bytes: vec![],
        }
        .is_warm_path());
        assert!(!Frame::ResyncRequest.is_warm_path());
    }

    #[test]
    fn unknown_tag_is_a_protocol_error_not_a_skip() {
        let mut d = Decoder::new();
        let err = d
            .push(&[99, 0, 0, 0, 0])
            .expect_err("unknown tag must fail");
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

    #[test]
    fn t_attach_eight_byte_payload_is_generation_only() {
        let bytes = Frame::attach(9, None).encode().expect("encode");
        assert_eq!(
            &bytes[1..5],
            &8u32.to_le_bytes(),
            "Spike 0 payload is 8 bytes"
        );
        let mut d = Decoder::new();
        assert_eq!(
            d.push(&bytes).expect("decode"),
            vec![Frame::attach(9, None)]
        );
    }

    #[test]
    fn cold_identity_socket_preserves_the_full_attach_socket_name() {
        let a = cold_identity_socket_path("/tmp/rill-a.sock");
        let b = cold_identity_socket_path("/tmp/rill-a.other");
        assert_eq!(a, PathBuf::from("/tmp/rill-a.sock.identity"));
        assert_eq!(b, PathBuf::from("/tmp/rill-a.other.identity"));
        assert_ne!(a, b, "distinct attach sockets need distinct cold sockets");
        let w = worker_socket_path("/tmp/rill-a.sock");
        assert_eq!(w, PathBuf::from("/tmp/rill-a.sock.worker"));
        assert_ne!(w, a);
        let n = cold_nav_socket_path("/tmp/rill-a.sock");
        assert_eq!(n, PathBuf::from("/tmp/rill-a.sock.nav"));
        assert_ne!(n, a);
    }

    #[test]
    fn t_attach_sixteen_byte_payload_names_a_session_id() {
        let bytes = Frame::attach(3, Some(42)).encode().expect("encode");
        assert_eq!(
            &bytes[1..5],
            &16u32.to_le_bytes(),
            "named leaf payload is 16 bytes"
        );
        let mut d = Decoder::new();
        assert_eq!(
            d.push(&bytes).expect("decode"),
            vec![Frame::attach(3, Some(42))]
        );
    }

    #[test]
    fn t_attach_eighteen_byte_payload_carries_protocol_and_observe() {
        let f = Frame::Attach {
            generation: 4,
            session_id: Some(7),
            protocol: PROTOCOL_VERSION,
            observe: true,
        };
        let bytes = f.encode().expect("encode");
        assert_eq!(&bytes[1..5], &18u32.to_le_bytes());
        let mut d = Decoder::new();
        assert_eq!(d.push(&bytes).expect("decode"), vec![f]);
    }
}
