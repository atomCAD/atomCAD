//! Phase 3 of `doc/design_mechanosynth_editor.md` — the placement flow of
//! `mechanosynth_edit`: arm, pick, choose, cancel, and the atom-first offers.
//!
//! These are the `StructureDesigner` methods the api layer wraps. They live
//! down here rather than in `rust/tests/structure_designer_api/` because the
//! orchestration does, and that is what makes them reachable from a plain
//! `StructureDesigner::new()` (`rust/AGENTS.md` §Testing).
//!
//! The payload is the placement fixture pair (`place_ops.json` +
//! `place_workpiece.xyz`): one operation per shape the engine distinguishes,
//! and one exact host per operation, in sub-clusters far enough apart that
//! nothing cross-matches.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_structure_designer::mechanosynth_edit_ops::{PickOutcome, StepMetadataField};
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::mechanosynth_edit::{MechanosynthEditData, ToolState};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::{DVec2, DVec3};

const NET: &str = "test";

/// Sub-cluster origins of `place_workpiece.xyz`.
const A: DVec3 = DVec3::new(0.0, 0.0, 0.0); // 3-coordinate Si with three Si frames
const B: DVec3 = DVec3::new(20.0, 0.0, 0.0); // Si with two partners at 3.5 Å

fn fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn with_data<T: 'static, F: FnOnce(&mut T)>(designer: &mut StructureDesigner, node_id: u64, f: F) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    let data = network
        .nodes
        .get_mut(&node_id)
        .unwrap()
        .data
        .as_any_mut()
        .downcast_mut::<T>()
        .expect("node carries the expected data type");
    f(data);
}

/// A designer holding one editor wired to the placement fixtures. Returns the
/// editor's node id; the undo stack starts empty.
fn setup() -> (StructureDesigner, u64) {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));

    let base_id = designer.add_node("import_xyz", DVec2::new(-600.0, 0.0));
    with_data::<ImportXYZData, _>(&mut designer, base_id, |data| {
        data.file_name = Some(fixture("place_workpiece.xyz"));
        data.atomic_structure = load_xyz(&fixture("place_workpiece.xyz"), true).ok();
    });
    let ops_id = designer.add_node("ops_library", DVec2::new(-600.0, 200.0));
    with_data::<OpsLibraryData, _>(&mut designer, ops_id, |data| {
        data.file = Some(fixture("place_ops.json"));
        data.reload_missing(None);
    });

    let node_id = designer.add_node("mechanosynth_edit", DVec2::new(0.0, 0.0));
    designer.connect_nodes(base_id, 0, node_id, 0);
    designer.connect_nodes(ops_id, 0, node_id, 1);
    designer.undo_stack.clear();
    (designer, node_id)
}

fn workpiece() -> AtomicStructure {
    load_xyz(&fixture("place_workpiece.xyz"), true).expect("the fixture loads")
}

/// The id of the only atom at `pos`, in the *same numbering* the editor sees:
/// the node re-evaluates the same loader, so ids agree.
fn at(pos: DVec3) -> u32 {
    let s = workpiece();
    let found = s.get_atoms_in_radius(&pos, 1e-4);
    assert_eq!(found.len(), 1, "expected exactly one atom at {pos:?}");
    found[0]
}

fn data(designer: &StructureDesigner, node_id: u64) -> &MechanosynthEditData {
    designer
        .mechanosynth_edit_data(&[], node_id)
        .expect("a mechanosynth_edit node")
}

fn tool_state(designer: &StructureDesigner, node_id: u64) -> ToolState {
    data(designer, node_id).placement.state()
}

// ============================================================================
// Arm → pick → commit
// ============================================================================

#[test]
fn arming_and_picking_a_single_candidate_commits_it_and_leaves_one_undo_entry() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_arm(&[], node_id, "hdon_frame")
        .expect("the op is in the wired library");
    assert_eq!(tool_state(&designer, node_id), ToolState::Armed);

    let outcome = designer
        .mechanosynth_edit_pick(&[], node_id, at(A))
        .expect("the host fits");
    assert_eq!(outcome, PickOutcome::Committed { index: 0 });

    let stored = data(&designer, node_id);
    assert_eq!(stored.authored.len(), 1);
    assert_eq!(stored.authored[0].step.op, "hdon_frame");
    assert!(stored.authored[0].is_exact());
    assert_eq!(stored.cursor, 1, "the cursor lands on the new step");
    assert_eq!(designer.undo_stack.history_len(), 1);

    // …and the tool stays armed with the same operation, so a run of identical
    // placements is one click each.
    assert_eq!(tool_state(&designer, node_id), ToolState::Armed);
    assert_eq!(
        data(&designer, node_id).placement.armed.as_deref(),
        Some("hdon_frame")
    );
}

#[test]
fn a_pick_with_several_candidates_waits_for_a_choice() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_arm(&[], node_id, "dimerize")
        .expect("armed");
    let outcome = designer
        .mechanosynth_edit_pick(&[], node_id, at(B))
        .expect("two partners are in reach");
    let PickOutcome::Candidates(rows) = outcome else {
        panic!("two partners must yield two candidates, not a commit");
    };
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter().all(|row| !row.ghost.is_empty()),
        "every candidate carries the atoms it would draw"
    );
    assert!(data(&designer, node_id).authored.is_empty());
    assert_eq!(designer.undo_stack.history_len(), 0);
    assert_eq!(tool_state(&designer, node_id), ToolState::Candidates);

    designer
        .mechanosynth_edit_choose(&[], node_id, "dimerize", 1)
        .expect("the second candidate");
    assert_eq!(data(&designer, node_id).authored.len(), 1);
    assert_eq!(designer.undo_stack.history_len(), 1);
}

#[test]
fn a_pick_while_idle_is_an_error() {
    let (mut designer, node_id) = setup();
    let error = designer
        .mechanosynth_edit_pick(&[], node_id, at(A))
        .expect_err("nothing is armed");
    assert!(error.contains("armed"), "{error}");
}

#[test]
fn arming_an_operation_the_library_does_not_have_is_an_error_naming_it() {
    let (mut designer, node_id) = setup();
    let error = designer
        .mechanosynth_edit_arm(&[], node_id, "no_such_op")
        .expect_err("the library has no such operation");
    assert!(error.contains("no_such_op"), "{error}");
}

#[test]
fn a_failed_pick_reports_the_reason_and_the_offers_for_the_same_atom() {
    let (mut designer, node_id) = setup();
    // `habst` acts on hydrogen; this is a silicon.
    designer
        .mechanosynth_edit_arm(&[], node_id, "habst")
        .expect("armed");
    let outcome = designer
        .mechanosynth_edit_pick(&[], node_id, at(A))
        .expect("a failed pick is an outcome, not an error");
    let PickOutcome::NoFit { message, offers } = outcome else {
        panic!("habst cannot act on a silicon");
    };
    assert!(message.contains("habst"), "{message}");
    assert!(
        offers.rows.iter().any(|row| row.op == "hdon_frame"),
        "the popup opens on what does fit: {:?}",
        offers.rows.iter().map(|row| &row.op).collect::<Vec<_>>()
    );
    assert_eq!(offers.anchor_atom_id, at(A));
    assert_eq!(offers.anchor_atomic_number, 14);
    assert!(data(&designer, node_id).authored.is_empty());
}

#[test]
fn a_stale_atom_id_is_an_error_from_both_entry_points() {
    let (mut designer, node_id) = setup();
    assert!(
        designer
            .mechanosynth_edit_offers(&[], node_id, 9999)
            .is_err()
    );
    designer
        .mechanosynth_edit_arm(&[], node_id, "hdon_frame")
        .expect("armed");
    assert!(designer.mechanosynth_edit_pick(&[], node_id, 9999).is_err());
}

// ============================================================================
// Atom-first offers
// ============================================================================

#[test]
fn the_offers_to_choose_sequence_inserts_the_same_step_as_arm_to_pick() {
    let (mut designer, node_id) = setup();
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    assert_eq!(tool_state(&designer, node_id), ToolState::Offers);
    assert!(
        sweep.rows.iter().all(|row| !row.ghost.is_empty()),
        "every row previews itself without a second call"
    );
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
        .expect("an applicable row");

    let from_offers = data(&designer, node_id).authored[0].clone();
    assert_eq!(designer.undo_stack.history_len(), 1);

    let (mut other, other_id) = setup();
    other
        .mechanosynth_edit_arm(&[], other_id, "hdon_frame")
        .expect("armed");
    other
        .mechanosynth_edit_pick(&[], other_id, at(A))
        .expect("fits");
    assert_eq!(from_offers, data(&other, other_id).authored[0]);
}

#[test]
fn choosing_a_near_miss_is_refused_with_its_residual() {
    let (mut designer, node_id) = setup();
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let miss = sweep
        .rows
        .iter()
        .find(|row| !row.fits)
        .expect("the wide variant is a near miss on this host");
    assert_eq!(miss.op, "hdon_frame_wide");

    let error = designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame_wide", 0)
        .expect_err("a near miss cannot be committed");
    assert!(error.contains("hdon_frame_wide"), "{error}");
    assert!(error.contains("off here"), "{error}");
    assert!(data(&designer, node_id).authored.is_empty());
    assert_eq!(designer.undo_stack.history_len(), 0);
}

#[test]
fn choosing_an_operation_the_offer_list_does_not_hold_is_an_error() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let error = designer
        .mechanosynth_edit_choose(&[], node_id, "dimerize", 0)
        .expect_err("dimerize has nothing to say about this atom");
    assert!(error.contains("dimerize"), "{error}");
    assert!(data(&designer, node_id).authored.is_empty());
}

#[test]
fn choosing_an_out_of_range_index_is_an_error_that_inserts_nothing() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let error = designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 7)
        .expect_err("there is no seventh candidate");
    assert!(error.contains("out of range"), "{error}");
    assert!(data(&designer, node_id).authored.is_empty());
}

#[test]
fn cancel_inserts_nothing_and_returns_to_idle() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_arm(&[], node_id, "dimerize")
        .expect("armed");
    designer
        .mechanosynth_edit_pick(&[], node_id, at(B))
        .expect("two candidates");
    designer
        .mechanosynth_edit_cancel(&[], node_id)
        .expect("a real node");
    assert_eq!(tool_state(&designer, node_id), ToolState::Idle);
    assert!(data(&designer, node_id).authored.is_empty());
    assert_eq!(designer.undo_stack.history_len(), 0);
}

// ============================================================================
// What a commit carries
// ============================================================================

#[test]
fn a_commit_inherits_the_previous_steps_metadata_but_not_its_note() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_arm(&[], node_id, "hdon_frame")
        .expect("armed");
    designer
        .mechanosynth_edit_pick(&[], node_id, at(A))
        .expect("fits");

    for (field, text, number) in [
        (StepMetadataField::Method, "probe", 0),
        (StepMetadataField::Phase, "layer1", 0),
        (StepMetadataField::Note, "the first donation", 0),
        (StepMetadataField::Layer, "", 1),
        (StepMetadataField::Site, "", 4),
    ] {
        designer
            .set_mechanosynth_edit_step_metadata(&[], node_id, 0, field, text, number)
            .expect("step 0 exists");
    }

    // A second placement, on the cluster-G host this time, so it is a real
    // second reaction rather than a repeat of the first.
    designer
        .mechanosynth_edit_arm(&[], node_id, "hdon_uniq")
        .expect("armed");
    designer
        .mechanosynth_edit_pick(&[], node_id, at(DVec3::new(0.0, 0.0, -20.0)))
        .expect("the rotated host fits");

    let stored = data(&designer, node_id);
    assert_eq!(stored.authored.len(), 2);
    let second = &stored.authored[1].step;
    assert_eq!(second.method, "probe");
    assert_eq!(second.phase, "layer1");
    assert_eq!(second.layer, 1);
    assert_eq!(second.site, 4);
    assert_eq!(
        second.note, None,
        "a note is about one step and is never inherited"
    );
}

#[test]
fn an_approximate_placement_is_flagged_in_the_stored_block() {
    let (mut designer, node_id) = setup();
    // `hdon_bare` names no frame atoms, so its orientation comes from the
    // host's bonds — coordinates from the application, not from the library.
    designer
        .mechanosynth_edit_arm(&[], node_id, "hdon_bare")
        .expect("armed");
    let outcome = designer
        .mechanosynth_edit_pick(&[], node_id, at(A))
        .expect("one free direction, so one candidate");
    assert_eq!(outcome, PickOutcome::Committed { index: 0 });
    let step = &data(&designer, node_id).authored[0];
    assert!(step.approximate);
    assert_eq!(data(&designer, node_id).inexact_counts().1, 1);
}

#[test]
fn a_commit_is_undoable_and_the_undo_empties_the_block() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_arm(&[], node_id, "hdon_frame")
        .expect("armed");
    designer
        .mechanosynth_edit_pick(&[], node_id, at(A))
        .expect("fits");
    assert!(designer.undo());
    let stored = data(&designer, node_id);
    assert!(stored.authored.is_empty());
    assert_eq!(stored.cursor, -1, "the cursor comes back with the block");
    assert!(designer.redo());
    assert_eq!(data(&designer, node_id).authored.len(), 1);
}

#[test]
fn a_run_of_placements_appends_in_order() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_arm(&[], node_id, "habst")
        .expect("armed");
    // The three terminators of sub-cluster E, abstracted one after another.
    let e = DVec3::new(0.0, 0.0, 20.0);
    for offset in [
        DVec3::new(0.854478, 0.854478, 0.854478),
        DVec3::new(0.854478, -0.854478, -0.854478),
        DVec3::new(-0.854478, 0.854478, -0.854478),
    ] {
        let outcome = designer
            .mechanosynth_edit_pick(&[], node_id, at(e + offset))
            .expect("each terminator is abstractable");
        assert!(matches!(outcome, PickOutcome::Committed { .. }));
    }
    let stored = data(&designer, node_id);
    assert_eq!(stored.authored.len(), 3);
    assert_eq!(stored.cursor, 3);
    assert!(stored.authored.iter().all(|step| step.step.op == "habst"));
    assert_eq!(designer.undo_stack.history_len(), 3);
}
