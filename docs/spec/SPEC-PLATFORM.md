# SPEC-PLATFORM — core boundary and per-platform obligations

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0027](../adr/0027-one-core-native-ui-per-os.md)
- **Requires:** [SPEC-KERNEL](SPEC-KERNEL.md), [SPEC-DISPLAY](SPEC-DISPLAY.md),
  [SPEC-ATTACH](SPEC-ATTACH.md)
- **Crates:** all; boundary is `crates/rill-host/ffi.rs` +
  `host/macos/rill_ffi.h`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Core boundary

- Kernel, attach codec, Chip 0 adapter and the daemon MUST NOT contain
  platform-specific code beyond genuine OS differences (PTY creation, reaping,
  socket details), and those MUST sit behind narrow named `cfg` boundaries in
  the crate that owns them.
- The UI boundary is the existing FFI. A second platform adds a second consumer
  of that FFI. It MUST NOT add a second core.
- The no-UI-dependency check MUST be a build-level check on the dependency
  graph, not a source grep (the standard NFR-SPAWN sets).

## 2. Native UI per platform

- Each platform's UI MUST be native to that platform.
- There MUST NOT be a cross-platform UI toolkit between the core and the screen.
- The presenter MUST NOT be abstracted. No portable-presenter trait is written
  until a second presenter exists and is measured.

## 3. Per-platform evidence

- A platform is supported when **its own** packaged build has demonstrated the
  named gates red-then-green, including NFR-KEY on that platform's hardware, on
  battery where the concept applies.
- macOS Proven MUST NOT be cited for Linux or Windows.
- Each platform declares its own NFR-KEY instrument. The definition is portable;
  the measurement is not, and MUST NOT be approximated to produce one number.
- A platform that cannot meet NFR-KEY MUST NOT ship as a platform (PRD §7).

## 4. Per-platform obligations

- Every FR in PRD §5 MUST hold on every supported platform.
- FR-KILL MUST be re-proven per platform. A Windows port MUST demonstrate the
  equivalent property on ConPTY; restating the requirement in platform terms is
  allowed, dropping it is not.
- NFR-SPAWN remains a link-level gate per platform.

## 5. No speculative portability

Until a second platform is actually being built, this tree MUST NOT:

- add a UI abstraction layer,
- widen the FFI "for portability",
- introduce a windowing or rendering trait with one implementor,
- move macOS-specific behaviour behind a `cfg` with no other arm.

## 6. Gates

| ID | Status | Closes |
|---|---|---|
| T-PLAT-CORE | Red | §1 |
| T-PLAT-FFI | Red | §1 |
| T-PLAT-GATES | Red | §3 |

T-NFR, T-KILL, T-SPAWN remain the macOS closers and are not re-cut.

## 7. What we will not do

- Adopt a cross-platform UI toolkit.
- Write the abstraction before the second implementor.
- Inherit macOS Proven for another platform.
- Report a portable NFR-KEY approximation.
- Relax FR-KILL because a signal name differs.
