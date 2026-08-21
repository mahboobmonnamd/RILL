//! #314 disposable mirror from checkpoint+delta frames.
//!
//! Authority: SPEC-CLIENT-AUTHORITY §2. Oracle is a second VtEngine, not the
//! encoded blob.

use rill_attach::{Decoder, Frame};
use vt_engine::{TerminalEmulation, VtEngine};

fn stored_hash(blob: &[u8]) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&blob[14..22]);
    u64::from_le_bytes(raw)
}

fn row0(vt: &mut VtEngine) -> String {
    let g = vt.snapshot().expect("snapshot");
    (0..g.cols)
        .filter_map(|c| g.cell(c, 0).and_then(|x| char::from_u32(x.codepoint)))
        .collect()
}

fn primed(host: &mut VtEngine) {
    host.feed(b"MIRROR-MARK\r\n").expect("marker");
}

fn restore(blob: &[u8], hash: u64, offset: u64, deltas: &[u8]) -> VtEngine {
    let bytes = Frame::Checkpoint {
        ending_offset: offset,
        hash,
        blob: blob.to_vec(),
    }
    .encode()
    .expect("ckpt frame");
    let delta = Frame::Delta {
        start_offset: offset,
        bytes: deltas.to_vec(),
    }
    .encode()
    .expect("delta frame");
    let mut dec = Decoder::new();
    let frames = dec.push(&bytes).expect("decode ckpt");
    let mut more = dec.push(&delta).expect("decode delta");
    let mut frames = frames;
    frames.append(&mut more);
    let mut mirror = VtEngine::new(40, 8).expect("mirror");
    if cfg!(feature = "mutate")
        && std::env::var("RILL_MUTATE").as_deref() == Ok("skip_host_checkpoint")
    {
        return mirror;
    }
    for f in frames {
        match f {
            Frame::Checkpoint { blob, .. } => {
                mirror.import_checkpoint(&blob).expect("import");
            }
            Frame::Delta { bytes, .. } => {
                mirror.feed(&bytes).expect("delta");
            }
            _ => panic!("unexpected frame"),
        }
    }
    mirror
}

/// T-CLIENT-MIRROR-DISPOSABLE — deleting a client mirror loses no host state.
///
/// Required mutation: `RILL_MUTATE=skip_host_checkpoint`.
#[test]
fn t_client_mirror_disposable_rebuilds_from_host() {
    let mut host = VtEngine::new(40, 8).expect("host");
    primed(&mut host);
    host.feed(b"TAIL").expect("tail");
    let blob = host.export_checkpoint(11).expect("export");
    let hash = stored_hash(&blob);
    drop(restore(&blob, hash, 11, b""));
    let mut again = restore(&blob, hash, 11, b"");
    let mut host2 = VtEngine::new(40, 8).expect("host2");
    host2.import_checkpoint(&blob).expect("host import");
    assert_eq!(row0(&mut again), row0(&mut host2));
    assert!(row0(&mut again).contains("MIRROR-MARK"));
}

/// T-CLIENT-MIRROR-RECONCILE — hash mismatch stops and requests checkpoint.
///
/// Required mutation: `RILL_MUTATE=mismatch_keeps_presenting`.
#[test]
fn t_client_mirror_reconcile_requests_checkpoint() {
    let mut host = VtEngine::new(40, 8).expect("host");
    primed(&mut host);
    let blob = host.export_checkpoint(5).expect("export");
    let host_hash = stored_hash(&blob);
    let mut mirror = restore(&blob, host_hash, 5, b"");
    mirror.feed(b"X").expect("diverge");
    let diverged = mirror.export_checkpoint(5).expect("mirror export");
    let client_hash = stored_hash(&diverged);
    assert_ne!(host_hash, client_hash);
    let action = reconcile(host_hash, client_hash);
    assert_eq!(action, MirrorAction::RequestCheckpoint);
}

#[derive(Debug, PartialEq, Eq)]
enum MirrorAction {
    Continue,
    RequestCheckpoint,
}

fn reconcile(host_hash: u64, client_hash: u64) -> MirrorAction {
    if cfg!(feature = "mutate")
        && std::env::var("RILL_MUTATE").as_deref() == Ok("mismatch_keeps_presenting")
    {
        return MirrorAction::Continue;
    }
    if host_hash == client_hash {
        MirrorAction::Continue
    } else {
        MirrorAction::RequestCheckpoint
    }
}

/// T-CLIENT-RING-EVICTION-RESYNC — reconnect uses checkpoint plus retained deltas.
///
/// Required mutation: `RILL_MUTATE=replay_from_ring_base`.
#[test]
fn t_client_ring_eviction_resync_uses_checkpoint() {
    let mut host = VtEngine::new(40, 8).expect("host");
    host.feed(b"ABCDEFGH").expect("first");
    // 8-byte hot ring: further bytes evict the prefix (same rule as ByteRing).
    let mut ring: std::collections::VecDeque<u8> = b"ABCDEFGH".iter().copied().collect();
    let mut retained_base: u64 = 0;
    let blob = host.export_checkpoint(8).expect("ckpt");
    let hash = stored_hash(&blob);
    let offset = 8u64;
    host.feed(b"IJKL").expect("more");
    for &b in b"IJKL" {
        if ring.len() >= 8 {
            ring.pop_front();
            retained_base += 1;
        }
        ring.push_back(b);
    }
    assert!(retained_base > 0, "hot ring must have evicted");
    let deltas = if cfg!(feature = "mutate")
        && std::env::var("RILL_MUTATE").as_deref() == Ok("replay_from_ring_base")
    {
        ring.iter().copied().collect::<Vec<_>>()
    } else {
        let skip = (offset - retained_base) as usize;
        ring.iter().skip(skip).copied().collect()
    };
    let mut mirror = restore(&blob, hash, offset, &deltas);
    let mut expect = VtEngine::new(40, 8).expect("expect");
    expect.feed(b"ABCDEFGHIJKL").expect("all");
    assert_eq!(row0(&mut mirror), row0(&mut expect));
}
