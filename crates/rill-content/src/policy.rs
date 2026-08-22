#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RetentionTier {
    Disabled = 0,
    MemoryOnly = 1,
    BoundedDurable = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionClass {
    pub tier: RetentionTier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetentionPolicy {
    parent: RetentionTier,
    local: RetentionTier,
}

impl RetentionPolicy {
    pub fn new(parent: RetentionTier, local: RetentionTier) -> Self {
        Self { parent, local }
    }

    /// The most restrictive applicable rule wins. A closer Workspace setting
    /// cannot widen a corporate/parent prohibition.
    pub fn resolved(&self) -> RetentionTier {
        if mutate_widen() {
            return self.local;
        }
        self.parent.min(self.local)
    }

    pub fn allows_durable_capture(&self) -> bool {
        self.resolved() == RetentionTier::BoundedDurable
    }
}

fn mutate_widen() -> bool {
    #[cfg(feature = "mutate")]
    {
        std::env::var("RILL_MUTATE").as_deref() == Ok("closest_workspace_wins")
    }
    #[cfg(not(feature = "mutate"))]
    {
        false
    }
}

/// Redaction is a derived sink. It must not rewrite canonical bytes.
pub fn redact_export(canonical: &[u8], replacement: &[u8]) -> (Vec<u8>, [u8; 32]) {
    let export = if mutate_rewrite_source() {
        replacement.to_vec()
    } else {
        let mut out = canonical.to_vec();
        if let Some(pos) = find_subslice(&out, replacement) {
            for b in &mut out[pos..pos + replacement.len()] {
                *b = b'*';
            }
        }
        out
    };
    (export, hash32(canonical))
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

fn hash32(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    let mut h = 0x9e3779b97f4a7c15u64;
    for (i, b) in bytes.iter().enumerate() {
        h = h.wrapping_mul(0x100000001b3) ^ u64::from(*b) ^ (i as u64);
        out[i % 32] ^= *b;
        out[(i + 7) % 32] ^= h as u8;
    }
    out
}

fn mutate_rewrite_source() -> bool {
    #[cfg(feature = "mutate")]
    {
        std::env::var("RILL_MUTATE").as_deref() == Ok("redactor_mutates_canonical")
    }
    #[cfg(not(feature = "mutate"))]
    {
        false
    }
}
