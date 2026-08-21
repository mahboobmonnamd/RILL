//! Host-owned checkpoint record for one TerminalExecution (#313).
//!
//! The blob is opaque VT codec output from `vt-engine`. The kernel does not
//! parse cells.

/// Format this kernel accepts. N/N-1 is documented as: this tree speaks 1
/// only; any other version is refused (SPEC-RUNTIME-SUPERVISION §4).
pub const CHECKPOINT_FORMAT_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredCheckpoint {
    pub format_version: u16,
    pub ending_offset: u64,
    pub hash: u64,
    pub blob: Vec<u8>,
}

impl StoredCheckpoint {
    pub fn new(ending_offset: u64, hash: u64, blob: Vec<u8>) -> Self {
        Self {
            format_version: CHECKPOINT_FORMAT_VERSION,
            ending_offset,
            hash,
            blob,
        }
    }

    pub fn check_compatible(&self) -> Result<(), crate::Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("accept_incompatible_checkpoint") {
            return Ok(());
        }
        if self.format_version != CHECKPOINT_FORMAT_VERSION {
            return Err(crate::Error::IncompatibleCheckpoint);
        }
        Ok(())
    }
}
