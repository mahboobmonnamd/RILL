# ADR 0027: One Rust core, native UI per OS

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-226 [#237](https://github.com/mahboobmonnamd/RILL/issues/237), F-227
  [#238](https://github.com/mahboobmonnamd/RILL/issues/238), F-228
  [#239](https://github.com/mahboobmonnamd/RILL/issues/239), F-231
  [#242](https://github.com/mahboobmonnamd/RILL/issues/242).
- **Requires:** [ADR 0001](0001-session-operating-system.md),
  [ADR 0003](0003-display-pipeline.md),
  [ADR 0009](0009-direct-to-display-echo.md) (presenter, T-NFR closer),
  [ADR 0011](0011-session-graph.md)
- **Amends:** nothing. macOS remains v0.1 (PRD §4).
- **Does not authorize:** a cross-platform UI toolkit, a Linux or Windows UI
  implementation in M2, a second presenter on the measured path, weakening
  NFR-KEY to a portable number, an abstraction layer over AppKit written before
  a second platform exists.

## Context

Four rows: macOS native windowing (F-226), Linux UI (F-227), Windows UI (F-228),
multi-OS shared core (F-231).

The core is already portable by construction. `crates/rill-kernel`,
`rill-attach`, `rill-chip0` and `rilld` are Rust with no AppKit. The macOS
surface is `host/macos/` plus `crates/rill-host`'s FFI. PRD §4 puts Linux and
Windows UI out of scope until after macOS is stable.

The decision worth recording now is not *when* to port. It is the shape that
keeps porting possible without paying for it early — and specifically, the
refusal to write a platform abstraction layer speculatively. An abstraction
designed against one implementation is a guess; it will be wrong, and it will
have already cost the macOS path its directness.

## Decision

### D1 — The core is platform-neutral Rust and the boundary is the FFI

Kernel, attach codec, Chip 0 adapter and the daemon MUST NOT contain
platform-specific code beyond what the OS genuinely differs on (PTY creation,
process reaping, socket details), and those MUST be behind narrow, named
`cfg` boundaries inside the crate that owns them.

The UI boundary is the existing FFI (`crates/rill-host/ffi.rs`,
`host/macos/rill_ffi.h`). A second platform adds a second consumer of that FFI.
It MUST NOT add a second core.

Named test `t_core_crates_have_no_appkit_dependency`, enforced as a build-level
check on the dependency graph, not a source grep (same standard NFR-SPAWN sets).
Mutation `leak_platform_type_into_kernel` MUST turn T-PLAT-CORE red.

### D2 — Each platform's UI is native, and the presenter is not abstracted

F-226, F-227, F-228. macOS is AppKit + Metal + `CAMetalDisplayLink` (ADR 0005,
ADR 0008). A future Linux UI is native to Linux; Windows is native to Windows.

There MUST NOT be a cross-platform UI toolkit between the core and the screen.
The presenter is the thing NFR-KEY measures (ADR 0009); an abstraction over it
is an abstraction over the product's only hard promise.

Concretely: no portable-presenter trait is written until a second presenter
exists and is measured. Until then the macOS path stays direct.

### D3 — Every platform re-runs the gates; a port does not inherit Proven

A platform is supported when **its own** packaged build has demonstrated the
named gates red-then-green (ADR 0002 D2), including NFR-KEY on that platform's
hardware, on battery where the concept applies.

Proven on macOS is Proven on macOS. It MUST NOT be cited for Linux or Windows.
A platform that cannot meet NFR-KEY is not shipped as a platform; per PRD §7,
stop that surface rather than restate the number.

Each platform declares its own NFR-KEY instrument, because "one display refresh
interval" and `presentedTime` mean different things off macOS. The *definition*
is portable; the measurement is not, and MUST NOT be faked with a portable
approximation.

### D4 — FR requirements are platform obligations, not macOS details

Every functional requirement in PRD §5 (FR-PTY, FR-ATTACH, FR-SOLE, FR-CHIP0,
FR-HISTORY, FR-RESYNC, FR-EXIT, FR-RESIZE, FR-ONE, FR-KILL) MUST hold on every
supported platform.

In particular the persist wedge — GUI `SIGKILL`, same child accepts input on
reopen (FR-KILL) — MUST be re-proven per platform. Windows has no `SIGKILL` and
no PTY master fd in the POSIX sense; a Windows port MUST demonstrate the
equivalent property on ConPTY or it does not ship. Restating the requirement in
Windows terms is allowed; dropping it is not.

NFR-SPAWN (no GUI spawn of the user shell) is likewise per platform and stays a
link-level gate.

### D5 — No speculative portability work

Until a second platform is actually being built, this tree MUST NOT:

- add a UI abstraction layer,
- widen the FFI "for portability",
- introduce a windowing or rendering trait with one implementor,
- move macOS-specific behaviour behind a `cfg` that has no other arm.

This is a decision, not an omission. Speculative generality here would trade the
measured macOS path for an unmeasured portable one.

The only portability work authorized now is D1's negative constraint: keep
platform types out of the core, and keep the check green.

### D6 — Oracle

| ID | Closes |
|---|---|
| T-PLAT-CORE | D1 — core crates carry no UI/platform dependency |
| T-PLAT-FFI | D1 — the FFI is the only boundary; no second core |
| T-PLAT-GATES | D3 — per-platform gate ledger; macOS Proven not inherited |

T-NFR, T-KILL, T-SPAWN stay the macOS closers and are not re-cut by this ADR.

## Consequences

- [SPEC-PLATFORM](../spec/SPEC-PLATFORM.md) records the boundary, the
  per-platform obligations, and the ledger rule.
- F-226 is effectively already met by the shipped macOS host and is tracked to
  the a11y and menu obligations in ADR 0026 D8.
- F-227 and F-228 ship nothing in M2 by decision; their contract is fixed so a
  later port cannot quietly lower the bar.

## Rejected alternatives

- **A cross-platform UI toolkit now.** Rejected: D2. The prototype in PRD §2
  died of a UI layer observing the data plane; a portable one is the same
  mistake with more surface.
- **Write the abstraction first, port later.** Rejected: D5. One implementor is
  a guess.
- **Inherit macOS Proven for other platforms.** Rejected: D3, ADR 0002 D8.
- **A portable NFR-KEY approximation so all platforms report one number.**
  Rejected: D3. That is re-cutting the instrument to flatter it (PRD §7).
- **Relax FR-KILL on Windows because `SIGKILL` does not exist.** Rejected: D4.
  The requirement is the property, not the signal name.
