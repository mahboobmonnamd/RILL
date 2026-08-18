//! T-CHIP1-POD — `PodCell` is 16 bytes and holds no `String`.
//!
//! The previous prototype died by putting `String` on snapshot cells. This
//! gate is the size/layout lock ([SPEC-VT-TYPES] §2). Lint `no-cell-strings`
//! is the other half of the oracle.
//!
//! Required mutation: add a `String` field to `PodCell`.

use rill_vt_types::PodCell;
use std::mem::{align_of, size_of};

#[test]
fn t_chip1_pod_cell_is_sixteen_bytes() {
    assert_eq!(
        size_of::<PodCell>(),
        16,
        "PodCell must stay 16 bytes (SPEC-VT-TYPES §2)"
    );
    assert_eq!(
        align_of::<PodCell>(),
        4,
        "PodCell must stay align 4 (SPEC-VT-TYPES §2)"
    );
}
