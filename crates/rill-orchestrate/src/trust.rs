//! SPEC-TRUST §2 (project-local trust) and §4 (redaction).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Dependency-free content digest (FNV-1a 64-bit). Sufficient for detecting
/// "this file changed since trust was granted" — not a security hash.
fn content_digest(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Trust bound to a path **and** its content (ADR 0026 D2). A grant does
/// not survive the file changing under it — `git pull` must not silently
/// keep a repository trusted after it gains a new action.
#[derive(Default)]
pub struct TrustStore {
    trusted: HashMap<PathBuf, u64>,
}

impl TrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trust(&mut self, path: impl Into<PathBuf>, content: &[u8]) {
        self.trusted.insert(path.into(), content_digest(content));
    }

    pub fn revoke(&mut self, path: &Path) {
        self.trusted.remove(path);
    }

    /// MUST re-check content, not only path. Under `trust_by_path_only` this
    /// checks presence alone — the regression SPEC-TRUST §2 forbids.
    pub fn is_trusted(&self, path: &Path, content: &[u8]) -> bool {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("trust_by_path_only") {
            return self.trusted.contains_key(path);
        }
        self.trusted.get(path) == Some(&content_digest(content))
    }
}

const SECRET_KEY_HINTS: &[&str] = &[
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "API_KEY",
    "APIKEY",
    "PRIVATE_KEY",
];

const SECRET_VALUE_PREFIXES: &[&str] = &["sk-", "ghp_", "gho_", "AKIA", "xoxb-", "xoxp-", "xoxa-"];

fn redact_line(line: &str) -> String {
    if let Some(eq_pos) = line.find('=') {
        let key_upper = line[..eq_pos].trim().to_uppercase();
        if SECRET_KEY_HINTS.iter().any(|h| key_upper.contains(h)) {
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            return format!("{}=[REDACTED]{newline}", &line[..eq_pos]);
        }
    }
    let mut out = line.to_string();
    for prefix in SECRET_VALUE_PREFIXES {
        if let Some(pos) = out.find(prefix) {
            let end = out[pos..]
                .find(|c: char| c.is_whitespace())
                .map(|o| pos + o)
                .unwrap_or(out.len());
            out.replace_range(pos..end, "[REDACTED]");
        }
    }
    out
}

/// Mask recognizable secrets. Applied at the sink, never at the source
/// (SPEC-TRUST §4) — call this from `persist`, not from wherever text is
/// produced.
pub fn redact(text: &str) -> String {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_redaction_on_export") {
        return text.to_string();
    }
    text.split_inclusive('\n').map(redact_line).collect()
}

/// Something that persists or transmits content (SPEC-TRUST §4). Every
/// implementor is a redaction boundary.
pub trait PersistingSink {
    fn write(&mut self, text: &str);
}

pub struct RecordingSink {
    pub written: String,
}

impl PersistingSink for RecordingSink {
    fn write(&mut self, text: &str) {
        self.written.push_str(text);
    }
}

/// The only way content reaches a persisting sink: through redaction first.
pub fn persist(sink: &mut dyn PersistingSink, text: &str) {
    sink.write(&redact(text));
}

/// The live terminal surface is explicitly not a persisting sink
/// (SPEC-TRUST §4, NFR-BYTES) — content the user is looking at is not
/// rewritten.
pub fn write_live_surface(sink: &mut dyn PersistingSink, text: &str) {
    sink.write(text);
}
