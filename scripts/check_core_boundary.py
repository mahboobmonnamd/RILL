"""SPEC-PLATFORM T-PLAT-CORE: core crates carry no UI/platform dependency.

`check_metadata` is a pure function over a parsed `cargo metadata
--format-version 1` document — it has no side effects, so it is unit-tested
directly against synthetic fixtures (see test_check_core_boundary.py) as well
as against the real workspace graph via check-core-boundary.sh.

ADR 0027 D1: kernel, attach codec, Chip 0 adapter, and the daemon must not
depend on platform-UI crates. The FFI (crates/rill-host) is the only place a
platform toolkit may appear.
"""
import sys
import json

CORE_CRATES = ["rill-kernel", "rill-attach", "rilld", "rill-content"]

# Substrings of a dependency crate name that indicate a platform-UI toolkit.
# Case-insensitive. Not exhaustive by design — this is a regression guard,
# not an allowlist audit.
BANNED_SUBSTRINGS = [
    "cocoa",
    "objc",
    "core-foundation",
    "core-graphics",
    "core-text",
    "appkit",
    "metal",
    "winit",
    "gtk",
    "gdk",
    "glib-",
    "x11",
    "wayland",
    "swift",
]


def _pkg_names_by_id(metadata):
    return {p["id"]: p["name"] for p in metadata["packages"]}


def _dep_graph(metadata):
    """id -> set of dependency ids, from the resolved graph (post feature
    resolution — this is what actually gets compiled, not the declared
    Cargo.toml surface)."""
    nodes = metadata["resolve"]["nodes"]
    return {n["id"]: [d["pkg"] for d in n["deps"]] for n in nodes}


def _transitive_deps(start_id, graph):
    seen = set()
    frontier = [start_id]
    while frontier:
        cur = frontier.pop()
        for dep in graph.get(cur, []):
            if dep not in seen:
                seen.add(dep)
                frontier.append(dep)
    return seen


def check_metadata(metadata):
    """Returns a list of (crate, offending_dependency) violations."""
    names = _pkg_names_by_id(metadata)
    graph = _dep_graph(metadata)
    id_by_name = {v: k for k, v in names.items()}

    violations = []
    for crate in CORE_CRATES:
        start = id_by_name.get(crate)
        if start is None:
            continue
        for dep_id in _transitive_deps(start, graph):
            dep_name = names.get(dep_id, "")
            lowered = dep_name.lower()
            if any(b in lowered for b in BANNED_SUBSTRINGS):
                violations.append((crate, dep_name))
    return violations


def main():
    metadata = json.load(sys.stdin)
    violations = check_metadata(metadata)
    if violations:
        for crate, dep in violations:
            print(
                f"check-core-boundary: FAIL [T-PLAT-CORE] {crate} transitively "
                f"depends on platform-UI crate {dep!r} (ADR 0027 D1)",
                file=sys.stderr,
            )
        return 1
    print("check-core-boundary: T-PLAT-CORE clean — no core crate depends on a platform-UI crate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
