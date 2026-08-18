//! Container-tree gates: kernel-plane half of T-NAV-TOPOLOGY, T-NAV-CLOSE,
//! T-NAV-REPARENT (ADR 0020, SPEC-NAV).
//!
//! These are the kernel-only sub-properties: the data structure exists, is
//! addressed by kernel-allocated ids, close terminates by explicit call and
//! never touches a sibling subtree, and reparent never disturbs identity.
//! They do not close the full SPEC-NAV gates, which also require chrome to
//! render from this snapshot on a packaged app (ADR 0020 D7, ADR 0002 D8) —
//! that half is unwritten and stays Red. This file's assertions are
//! everything that can be proven without a window.

use rill_kernel::{Kernel, NodeChild, NodeKind, Winsize};

fn spawn_probe(kernel: &mut Kernel) -> rill_kernel::SessionId {
    kernel
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 60"], Winsize::default())
        .expect("spawn probe leaf")
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

// ---------------------------------------------------------- T-NAV-TOPOLOGY

/// The container snapshot must actually carry what was created — a tab
/// nested under a workspace, holding the leaf that was attached to it — not
/// a shape the test's own inputs would satisfy trivially (ADR 0002 D4): the
/// oracle reads `container_snapshot()`, which the kernel builds fresh from
/// its own map, not from a value this test handed back to itself.
///
/// Required mutation: `RILL_MUTATE=omit_node_children`.
#[test]
fn t_nav_topology_snapshot_reflects_created_nodes_and_leaves() {
    let mut kernel = Kernel::new();
    let workspace = kernel
        .create_node(NodeKind::Workspace, None)
        .expect("create workspace");
    let tab = kernel
        .create_node(NodeKind::Tab, Some(workspace))
        .expect("create tab under workspace");
    let leaf = spawn_probe(&mut kernel);
    kernel.attach_leaf(tab, leaf).expect("attach leaf to tab");

    let snapshot = kernel.container_snapshot();

    let ws_row = snapshot
        .iter()
        .find(|n| n.id == workspace)
        .expect("workspace node missing from snapshot");
    assert_eq!(ws_row.kind, NodeKind::Workspace);
    assert!(ws_row.parent.is_none());
    assert!(
        ws_row.children.contains(&NodeChild::Node(tab)),
        "workspace snapshot does not list the tab created under it"
    );

    let tab_row = snapshot
        .iter()
        .find(|n| n.id == tab)
        .expect("tab node missing from snapshot");
    assert_eq!(tab_row.parent, Some(workspace));
    assert!(
        tab_row.children.contains(&NodeChild::Leaf(leaf)),
        "tab snapshot does not list the leaf attached to it — a chrome \
         reading this would render an empty pane over a live child (SPEC-NAV §1)"
    );

    kernel.close_node(workspace).ok();
}

// -------------------------------------------------------------- T-NAV-CLOSE

/// Closing one subtree terminates only the leaves it owns. A sibling
/// workspace's leaf must stay alive and its process must actually still
/// answer `kill(pid, 0)` — not merely "not recorded as terminated" but a
/// live PID, downstream of the real `posix_spawn` (ADR 0002 D4).
///
/// Required mutation: `RILL_MUTATE=close_node_terminates_all_leaves`.
#[test]
fn t_nav_close_terminates_only_the_closed_subtrees_leaves() {
    let mut kernel = Kernel::new();

    let ws_a = kernel.create_node(NodeKind::Workspace, None).expect("ws a");
    let tab_a = kernel
        .create_node(NodeKind::Tab, Some(ws_a))
        .expect("tab a");
    let leaf_a = spawn_probe(&mut kernel);
    kernel.attach_leaf(tab_a, leaf_a).expect("attach a");
    let pid_a = kernel.session(leaf_a).expect("session a").child_pid();

    let ws_b = kernel.create_node(NodeKind::Workspace, None).expect("ws b");
    let tab_b = kernel
        .create_node(NodeKind::Tab, Some(ws_b))
        .expect("tab b");
    let leaf_b = spawn_probe(&mut kernel);
    kernel.attach_leaf(tab_b, leaf_b).expect("attach b");
    let pid_b = kernel.session(leaf_b).expect("session b").child_pid();

    assert!(alive(pid_a) && alive(pid_b), "both probes must start alive");

    kernel.close_node(ws_a).expect("close workspace a");

    assert!(
        !alive(pid_a),
        "closing workspace A must terminate leaf A's child (ADR 0020 D3)"
    );
    assert!(
        alive(pid_b),
        "closing workspace A terminated workspace B's leaf — a sibling was \
         disturbed, which ADR 0020 D3 forbids"
    );

    let snapshot = kernel.container_snapshot();
    assert!(
        !snapshot.iter().any(|n| n.id == ws_a || n.id == tab_a),
        "closed subtree is still present in the snapshot"
    );
    assert!(
        snapshot.iter().any(|n| n.id == ws_b),
        "unrelated workspace B was removed by closing A"
    );

    kernel.close_node(ws_b).ok();
}

// ---------------------------------------------------------- T-NAV-REPARENT

/// Reparenting a node (drag, zoom, template restore) must not change the
/// node's own id, must not touch the leaf's `SessionId`, and must not
/// disturb the child pid — the same process, still answering `kill(pid, 0)`,
/// after the move (ADR 0020 D4).
///
/// Required mutation: `RILL_MUTATE=reparent_recreates_node`.
#[test]
fn t_nav_reparent_preserves_session_identity() {
    let mut kernel = Kernel::new();

    let ws_a = kernel.create_node(NodeKind::Workspace, None).expect("ws a");
    let ws_b = kernel.create_node(NodeKind::Workspace, None).expect("ws b");
    let tab = kernel
        .create_node(NodeKind::Tab, Some(ws_a))
        .expect("tab under a");
    let leaf = spawn_probe(&mut kernel);
    kernel.attach_leaf(tab, leaf).expect("attach leaf");
    let pid_before = kernel.session(leaf).expect("session").child_pid();

    kernel
        .reparent_node(tab, Some(ws_b))
        .expect("reparent tab from a to b");

    let snapshot = kernel.container_snapshot();
    let moved = snapshot.iter().find(|n| n.id == tab).unwrap_or_else(|| {
        panic!(
            "reparented node's original NodeId no longer exists in the \
             snapshot — identity was not preserved (ADR 0020 D4)"
        )
    });
    assert_eq!(
        moved.parent,
        Some(ws_b),
        "node did not actually move to the new parent"
    );
    assert!(
        moved.children.contains(&NodeChild::Leaf(leaf)),
        "the leaf's SessionId changed across a reparent"
    );

    let ws_a_row = snapshot.iter().find(|n| n.id == ws_a).expect("ws a row");
    assert!(
        !ws_a_row.children.contains(&NodeChild::Node(tab)),
        "old parent still lists the node after it was reparented away"
    );

    let pid_after = kernel
        .session(leaf)
        .expect("session still there")
        .child_pid();
    assert_eq!(
        pid_before, pid_after,
        "reparenting changed the child pid — the leaf was respawned, not moved"
    );
    assert!(alive(pid_after), "leaf did not survive the reparent");

    kernel.close_node(ws_a).ok();
    kernel.close_node(ws_b).ok();
}
