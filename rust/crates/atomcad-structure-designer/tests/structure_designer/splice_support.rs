//! The shared oracles of `doc/design_insert_node_on_wire.md` (Testing), used
//! by every splice operation's test file: *no mutation* for plans and
//! refusals, *round trip* and *invariants* for commits.

use crate::library_links_support::check_invariants;
use crate::undo_test::snapshot_all_networks;
use atomcad_structure_designer::structure_designer::StructureDesigner;

/// What a plan or a refusal must leave exactly as it was: every network's
/// serialized form, the undo stack's depth, and the dirty flag. The
/// serialized form alone would miss a stray undo push.
#[derive(Debug, PartialEq)]
pub struct Untouched {
    networks: serde_json::Value,
    pushes: u64,
    dirty: bool,
}

pub fn untouched(d: &mut StructureDesigner) -> Untouched {
    Untouched {
        networks: snapshot_all_networks(&mut d.node_type_registry),
        pushes: d.undo_stack.push_count(),
        dirty: d.is_dirty(),
    }
}

/// Runs `action` and asserts it changed nothing (*no mutation*).
#[track_caller]
pub fn assert_no_mutation<R>(
    d: &mut StructureDesigner,
    action: impl FnOnce(&mut StructureDesigner) -> R,
) -> R {
    let before = untouched(d);
    let result = action(d);
    assert!(before == untouched(d), "the call changed the design");
    result
}

/// Runs a successful commit and asserts it is exactly one undo step whose
/// undo restores the networks as they were and whose redo restores them as
/// the commit left them (*round trip*), with no fatal invariant violation
/// after the commit, the undo or the redo (*invariants*). Leaves the design
/// in the committed state and returns the commit's result.
#[track_caller]
pub fn assert_one_step_round_trip<R>(
    d: &mut StructureDesigner,
    commit: impl FnOnce(&mut StructureDesigner) -> R,
) -> R {
    let before = snapshot_all_networks(&mut d.node_type_registry);
    let pushes = d.undo_stack.push_count();
    let result = commit(d);
    assert_eq!(pushes + 1, d.undo_stack.push_count(), "one undo step");
    check_invariants(d).unwrap();
    let after = snapshot_all_networks(&mut d.node_type_registry);

    assert!(d.undo());
    assert_eq!(
        before,
        snapshot_all_networks(&mut d.node_type_registry),
        "undo restores the design"
    );
    check_invariants(d).unwrap();
    assert!(d.redo());
    assert_eq!(
        after,
        snapshot_all_networks(&mut d.node_type_registry),
        "redo restores the commit"
    );
    check_invariants(d).unwrap();
    result
}
