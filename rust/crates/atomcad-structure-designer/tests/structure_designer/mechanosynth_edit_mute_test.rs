//! Muting operations in `mechanosynth_edit` — the kernel half.
//! See `doc/design_mechanosynth_op_muting.md`.
//!
//! The feature is one sentence with one dangerous way to get it wrong: **mute
//! filters the offer sweep and nothing else**. So the tests below are mostly
//! about what muting must *not* do — a muted operation still replays, still
//! reaches the `steps` pin, and is still placeable once a *show all here* sweep
//! has put it back in the offer list. Only the first two are about hiding.
//!
//! Same fixture pair as `mechanosynth_edit_placement_test.rs`
//! (`place_ops.json` + `place_workpiece.xyz`): fourteen operations, one exact
//! host per operation, sub-clusters far enough apart that nothing
//! cross-matches.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::io::xyz_loader::load_xyz;
use atomcad_crystolecule::mechanosynth::compare_structures;
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::mechanosynth_edit::{
    MechanosynthEditData, STEPS_OUTPUT_PIN,
};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::f64::{DVec2, DVec3};

const NET: &str = "test";

/// Sub-cluster origin of `place_workpiece.xyz`: a 3-coordinate Si with three
/// Si frames. Several operations have something to say here, which is what a
/// mute test needs.
const A: DVec3 = DVec3::new(0.0, 0.0, 0.0);

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

fn at(pos: DVec3) -> u32 {
    let structure = load_xyz(&fixture("place_workpiece.xyz"), true).expect("the fixture loads");
    let found = structure.get_atoms_in_radius(&pos, 1e-4);
    assert_eq!(found.len(), 1, "expected exactly one atom at {pos:?}");
    found[0]
}

fn data(designer: &StructureDesigner, node_id: u64) -> &MechanosynthEditData {
    designer
        .mechanosynth_edit_data(&[], node_id)
        .expect("a mechanosynth_edit node")
}

fn offered(designer: &mut StructureDesigner, node_id: u64, atom_id: u32) -> Vec<String> {
    designer
        .mechanosynth_edit_offers(&[], node_id, atom_id)
        .expect("the atom exists")
        .rows
        .iter()
        .map(|row| row.op.clone())
        .collect()
}

fn mute(designer: &mut StructureDesigner, node_id: u64, ops: &[&str]) {
    let names: Vec<String> = ops.iter().map(|op| op.to_string()).collect();
    designer
        .set_mechanosynth_edit_muted(&[], node_id, &names, true)
        .expect("a mechanosynth_edit node");
}

fn expect_atoms(result: NetworkResult) -> AtomicStructure {
    match result {
        NetworkResult::Crystal(crystal) => crystal.atoms,
        NetworkResult::Molecule(molecule) => molecule.atoms,
        other => panic!(
            "expected an atomic result, got {}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Hiding
// ============================================================================

#[test]
fn a_muted_operation_is_not_swept_and_the_count_says_so() {
    let (mut designer, node_id) = setup();
    let before = offered(&mut designer, node_id, at(A));
    let victim = before.first().cloned().expect("something applies at A");

    mute(&mut designer, node_id, &[&victim]);
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let after: Vec<String> = sweep.rows.iter().map(|row| row.op.clone()).collect();

    assert!(
        !after.contains(&victim),
        "{victim} was muted, so the sweep did not look at it"
    );
    assert_eq!(
        after.len(),
        before.len() - 1,
        "muting hides one row and disturbs nothing else"
    );
    // The count is the popup's honesty line: it reports what was *skipped*,
    // never how many of the skipped would have fitted — knowing that costs the
    // sweep the mute avoids.
    assert_eq!(sweep.skipped_muted, 1);
}

#[test]
fn show_all_here_sweeps_the_whole_library_again() {
    let (mut designer, node_id) = setup();
    let before = offered(&mut designer, node_id, at(A));
    let victim = before.first().cloned().expect("something applies at A");
    mute(&mut designer, node_id, &[&victim]);

    let sweep = designer
        .mechanosynth_edit_offers_including_muted(&[], node_id, at(A))
        .expect("the atom exists");
    let after: Vec<String> = sweep.rows.iter().map(|row| row.op.clone()).collect();

    assert_eq!(
        after, before,
        "the escape hatch answers as if nothing were muted"
    );
    assert_eq!(
        sweep.skipped_muted, 0,
        "nothing was skipped, whatever the stored set says"
    );
}

#[test]
fn a_muted_name_the_library_does_not_define_is_inert() {
    // The `ops` pin can be rewired and a generator rewrites its library file
    // constantly, so an unknown name is kept and ignored rather than rejected —
    // rewire back and the mute comes back with it.
    let (mut designer, node_id) = setup();
    let before = offered(&mut designer, node_id, at(A));

    mute(&mut designer, node_id, &["no_such_operation"]);
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");

    assert_eq!(
        sweep
            .rows
            .iter()
            .map(|row| row.op.clone())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(
        sweep.skipped_muted, 0,
        "the count is against the wired library, so an unknown name overstates nothing"
    );
    assert!(
        data(&designer, node_id).is_muted("no_such_operation"),
        "and it is still stored"
    );
}

// ============================================================================
// What muting must not do
// ============================================================================

#[test]
fn a_block_containing_a_muted_operation_replays_unchanged() {
    // The invariant the whole feature hangs on. If this ever fails, opening a
    // saved project after someone muted an operation silently changes its
    // geometry.
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
        .expect("fits this host");

    let atoms_before = expect_atoms(designer.evaluate_node_output(&[], node_id, 0));
    let steps_before = designer.evaluate_node_output(&[], node_id, STEPS_OUTPUT_PIN as i32);

    mute(&mut designer, node_id, &["hdon_frame"]);

    let atoms_after = expect_atoms(designer.evaluate_node_output(&[], node_id, 0));
    let steps_after = designer.evaluate_node_output(&[], node_id, STEPS_OUTPUT_PIN as i32);

    assert!(
        compare_structures(&atoms_before, &atoms_after, 1e-9).is_empty(),
        "the workpiece is the same atom for atom"
    );
    assert_eq!(
        steps_before.to_display_string(),
        steps_after.to_display_string(),
        "and the steps pin carries the same block"
    );
    assert_eq!(data(&designer, node_id).authored.len(), 1);
}

#[test]
fn choose_gains_no_refusal_of_its_own() {
    // Mute filters the sweep; it is not a second gate beside the near-miss and
    // tool-readiness ones. A row a *show all here* sweep put back is therefore
    // fully placeable.
    let (mut designer, node_id) = setup();
    mute(&mut designer, node_id, &["hdon_frame"]);
    designer
        .mechanosynth_edit_offers_including_muted(&[], node_id, at(A))
        .expect("the atom exists");

    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
        .expect("a muted operation in the open list commits like any other");
    assert_eq!(data(&designer, node_id).authored.len(), 1);
}

// ============================================================================
// Undo
// ============================================================================

#[test]
fn a_bulk_mute_is_one_undo_entry() {
    // A group chip in the panel mutes a dozen operations in one user action,
    // and one Ctrl+Z has to put all twelve back.
    let (mut designer, node_id) = setup();
    let ops = ["habst", "recolor", "dimerize", "bridge"];
    mute(&mut designer, node_id, &ops);

    assert_eq!(designer.undo_stack.history_len(), 1);
    assert_eq!(data(&designer, node_id).muted.len(), 4);

    assert!(designer.undo());
    assert!(
        data(&designer, node_id).muted.is_empty(),
        "one undo restores the whole set"
    );
    assert!(designer.redo());
    assert_eq!(data(&designer, node_id).muted.len(), 4);
}

#[test]
fn unmuting_is_its_own_entry_and_a_no_op_mute_is_none() {
    let (mut designer, node_id) = setup();
    mute(&mut designer, node_id, &["habst"]);
    // Muting what is already muted changes nothing, so it must not push an
    // entry a user would then have to press Ctrl+Z through.
    mute(&mut designer, node_id, &["habst"]);
    assert_eq!(designer.undo_stack.history_len(), 1);

    designer
        .set_mechanosynth_edit_muted(&[], node_id, &["habst".to_string()], false)
        .expect("a mechanosynth_edit node");
    assert_eq!(designer.undo_stack.history_len(), 2);
    assert!(data(&designer, node_id).muted.is_empty());

    assert!(designer.undo());
    assert!(data(&designer, node_id).is_muted("habst"));
}

#[test]
fn muting_leaves_an_open_offer_list_alone() {
    // Muting changes no fit, so the rows are exactly as valid afterwards as
    // before — and the popup hides the muted one in place rather than paying
    // for a second sweep. Resetting here would be visible as a bug rather than
    // as tidiness: the viewport closes the popup whenever the kernel stops
    // saying `offers`, so muting a row *from* the popup would shut the list the
    // user is reading.
    let (mut designer, node_id) = setup();
    let before = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists")
        .rows
        .len();
    let anchor = data(&designer, node_id).placement.anchor;

    mute(&mut designer, node_id, &["habst"]);

    assert_eq!(data(&designer, node_id).placement.offers.len(), before);
    assert_eq!(data(&designer, node_id).placement.anchor, anchor);

    // And undo does not drop it either, for the same reason.
    assert!(designer.undo());
    assert_eq!(data(&designer, node_id).placement.offers.len(), before);
}

#[test]
fn the_sweep_reports_the_wired_librarys_size() {
    // So the popup's line can say "4 of 19 operations muted" — a proportion is
    // what makes the count mean anything at a glance.
    let (mut designer, node_id) = setup();
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    assert_eq!(sweep.library_count, 14);
}
