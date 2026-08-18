"""T-PLAT-CORE gate. Demonstrates check_core_boundary.check_metadata()
red-then-green against synthetic fixtures (ADR 0002 D2/D3), then checks it
against this workspace's real, current dependency graph.

Run: python3 scripts/test_check_core_boundary.py
"""
import subprocess
import sys
import os

sys.path.insert(0, os.path.dirname(__file__))
from check_core_boundary import check_metadata, CORE_CRATES  # noqa: E402


def _synthetic_metadata(kernel_deps):
    """A minimal fake `cargo metadata` document: rill-kernel depending on
    whatever package ids `kernel_deps` names, plus libc for realism."""
    packages = [
        {"id": "rill-kernel 0.1.0", "name": "rill-kernel"},
        {"id": "libc 0.2.0", "name": "libc"},
    ]
    deps = [{"pkg": "libc 0.2.0"}]
    for name in kernel_deps:
        pid = f"{name} 0.1.0"
        packages.append({"id": pid, "name": name})
        deps.append({"pkg": pid})
    return {
        "packages": packages,
        "resolve": {
            "nodes": [
                {"id": "rill-kernel 0.1.0", "deps": deps},
                {"id": "libc 0.2.0", "deps": []},
            ]
            + [{"id": f"{n} 0.1.0", "deps": []} for n in kernel_deps]
        },
    }


def test_clean_graph_has_no_violations():
    """Green: rill-kernel depending only on libc (today's real shape)."""
    violations = check_metadata(_synthetic_metadata(["libc"]))
    assert violations == [], f"expected no violations, got {violations}"


def test_platform_ui_dependency_is_caught():
    """Red: rill-kernel transitively depending on a Cocoa binding crate is
    exactly the ADR 0027 D1 regression this check exists to catch.

    Required mutation: this test *is* the mutation — a synthetic dependency
    graph standing in for the real one gaining a banned crate, since
    injecting a real one would require modifying Cargo.lock.
    """
    violations = check_metadata(_synthetic_metadata(["cocoa-foundation"]))
    assert violations == [
        ("rill-kernel", "cocoa-foundation")
    ], f"checker did not flag a Cocoa dependency on rill-kernel: {violations}"


def test_real_workspace_graph_is_clean():
    """The actual close: this workspace's current, real dependency graph has
    no core crate depending on a platform-UI crate."""
    proc = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    import json

    metadata = json.loads(proc.stdout)
    violations = check_metadata(metadata)
    assert violations == [], f"real workspace graph has violations: {violations}"
    checked = [c for c in CORE_CRATES if any(p["name"] == c for p in metadata["packages"])]
    assert checked, "no core crates found in metadata — the check ran over nothing"


def main():
    tests = [
        test_clean_graph_has_no_violations,
        test_platform_ui_dependency_is_caught,
        test_real_workspace_graph_is_clean,
    ]
    failed = 0
    for t in tests:
        try:
            t()
            print(f"ok   {t.__name__}")
        except AssertionError as e:
            failed += 1
            print(f"FAIL {t.__name__}: {e}")
    if failed:
        print(f"{failed} of {len(tests)} failed", file=sys.stderr)
        return 1
    print(f"all {len(tests)} passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
