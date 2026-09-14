//! The placement flow of `mechanosynth_edit`: the atom-first sweep, previewing
//! a row, choosing one, and cancelling. See `doc/design_mechanosynth_editor.md`
//! — Phase 3 for the flow, and §Revised for why there is no armed mode and no
//! second list of orientations.
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
use atomcad_structure_designer::mechanosynth_edit_ops::StepMetadataField;
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
// Atom-first offers — the only placement flow there is
// ============================================================================
//
// There is no "armed" mode: a viewport click is always the question "what can
// be done at this atom", and a commit returns the tool to Idle rather than
// leaving the last operation loaded for the next click. The library names one
// operation per host *environment*, so the operation just placed is usually
// the wrong one at the next site.

#[test]
fn a_sweep_carries_every_placement_of_every_row() {
    // The popup lists each orientation as its own row, so the sweep has to hand
    // over all of them — with their own ghosts — in one call. There is no
    // second list to open and no second `place` to run.
    let (mut designer, node_id) = setup();
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(B))
        .expect("the atom exists");
    let row = sweep
        .rows
        .iter()
        .find(|row| row.op == "dimerize")
        .expect("dimerize fits this host");

    assert_eq!(row.candidates.len(), 2, "two partners are in reach");
    assert_eq!(row.candidates.len(), row.candidate_count);
    assert!(
        row.candidates.iter().all(|c| !c.ghost.is_empty()),
        "every placement carries the atoms it would draw"
    );
    assert_eq!(
        row.candidates.iter().map(|c| c.index).collect::<Vec<_>>(),
        vec![0, 1],
        "indexed as `choose` and `select_preview` take them"
    );
    assert!(data(&designer, node_id).authored.is_empty());
    assert_eq!(designer.undo_stack.history_len(), 0);

    // Each is placeable by its own index, straight from the offer list.
    designer
        .mechanosynth_edit_choose(&[], node_id, "dimerize", 1)
        .expect("the second placement");
    assert_eq!(data(&designer, node_id).authored.len(), 1);
    assert_eq!(designer.undo_stack.history_len(), 1);
}

#[test]
fn a_near_miss_carries_its_one_rejected_fit_and_is_never_a_choice() {
    // It previews like anything else — that is what makes a library of
    // environment variants learnable — but it is never expanded into an
    // orientation choice, and it still cannot be placed.
    let (mut designer, node_id) = setup();
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let miss = sweep
        .rows
        .iter()
        .find(|row| !row.fits)
        .expect("the wide variant is a near miss on this host");

    assert_eq!(miss.candidates.len(), 1);
    assert!(!miss.candidates[0].ghost.is_empty());
    assert_eq!(miss.candidate_count, 0, "a near miss offers no placement");
    assert!(
        designer
            .mechanosynth_edit_choose(&[], node_id, &miss.op, 0)
            .is_err()
    );
}

// ============================================================================
// The input cache
// ============================================================================
//
// `eval` reuses the node's evaluated inputs so that an interaction touching
// only this node — a hover preview — does not re-evaluate the chain above it.
// The refresh system drops the cache through `NodeData::clear_input_cache`
// whenever upstream may have changed; the danger is reading a stale one, so
// what these pin is that the invalidation reaches it.

#[test]
fn the_input_cache_is_filled_by_an_evaluation_and_dropped_on_demand() {
    use atomcad_structure_designer::node_data::NodeData;

    let (mut designer, node_id) = setup();
    assert!(
        !data(&designer, node_id).has_cached_input(),
        "nothing is cached before the node has been evaluated"
    );

    // Any operation that needs the workpiece evaluates the node.
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    assert!(
        data(&designer, node_id).has_cached_input(),
        "a completed evaluation leaves its inputs behind"
    );

    // The hook the refresh system uses.
    NodeData::clear_input_cache(data(&designer, node_id));
    assert!(!data(&designer, node_id).has_cached_input());
}

#[test]
fn a_cached_evaluation_produces_the_same_workpiece_as_a_cold_one() {
    // The cache holds the *unmutated* inputs and `eval` replays into a clone of
    // them. Getting that wrong would make the second placement of a session fit
    // against a workpiece that already had the first one applied.
    use atomcad_structure_designer::node_data::NodeData;

    let (mut designer, node_id) = setup();
    let cold = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");

    // …and again, now that the inputs are cached.
    let warm = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    assert_eq!(
        cold.rows.iter().map(|r| &r.op).collect::<Vec<_>>(),
        warm.rows.iter().map(|r| &r.op).collect::<Vec<_>>()
    );
    assert_eq!(cold.anchor_position, warm.anchor_position);

    // A placement, then a cold evaluation of the same node: the block is
    // applied exactly once, not twice.
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
        .expect("an applicable row");
    NodeData::clear_input_cache(data(&designer, node_id));
    let after_cold = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let after_warm = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    assert_eq!(
        after_cold.rows.iter().map(|r| &r.op).collect::<Vec<_>>(),
        after_warm.rows.iter().map(|r| &r.op).collect::<Vec<_>>(),
        "a warm evaluation must not replay the authored block twice"
    );
}

#[test]
fn undoing_a_block_edit_drops_the_input_cache() {
    // The undo command reaches into the node's data directly rather than
    // through a refresh, so it owes the cache the invalidation the refresh
    // system would have done.
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
        .expect("an applicable row");
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    assert!(data(&designer, node_id).has_cached_input());

    designer.undo();
    assert!(
        !data(&designer, node_id).has_cached_input(),
        "the restored block evaluates against freshly read inputs"
    );
}

#[test]
fn a_stale_atom_id_is_an_error() {
    let (mut designer, node_id) = setup();
    assert!(
        designer
            .mechanosynth_edit_offers(&[], node_id, 9999)
            .is_err()
    );
}

#[test]
fn a_commit_returns_the_tool_to_idle_rather_than_arming_the_operation() {
    // The invariant the removed "armed" mode broke. A click after a placement
    // must be another question, not a silent repeat of the last answer.
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
        .expect("an applicable row");

    assert_eq!(data(&designer, node_id).authored.len(), 1);
    assert_eq!(tool_state(&designer, node_id), ToolState::Idle);
    let placement = &data(&designer, node_id).placement;
    assert!(placement.anchor.is_none());
    assert!(placement.offers.is_empty());
    assert!(
        placement.preview_ghosts.is_empty(),
        "a commit drops the preview with the list"
    );
}

// ============================================================================
// Preview selection
// ============================================================================
//
// Selecting a row puts its ghost atoms into the transient state; the node's
// `eval(decorate)` hands them to the decorator and the tessellator draws them
// with the workpiece. Taken on a click, never on hover — it costs an
// evaluation.

#[test]
fn selecting_an_offer_row_stages_its_ghosts() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    assert!(
        data(&designer, node_id).placement.preview_ghosts.is_empty(),
        "opening the list previews nothing"
    );

    designer
        .mechanosynth_edit_select_preview(&[], node_id, "hdon_frame", 0)
        .expect("an applicable row");
    let placement = &data(&designer, node_id).placement;
    assert!(!placement.preview_ghosts.is_empty());
    assert!(!placement.preview_near_miss);
    assert!(
        data(&designer, node_id).authored.is_empty(),
        "previewing places nothing"
    );
    assert_eq!(designer.undo_stack.history_len(), 0);
}

#[test]
fn selecting_a_near_miss_row_previews_it_and_flags_it() {
    // A near miss keeps its one rejected fit outside the placeable list, so
    // previewing it has to look there — and it must be flagged, because it is
    // drawn in a warning colour and can never be placed.
    let (mut designer, node_id) = setup();
    let sweep = designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let near_miss = sweep
        .rows
        .iter()
        .find(|row| !row.fits)
        .expect("this fixture has a near miss")
        .op
        .clone();

    designer
        .mechanosynth_edit_select_preview(&[], node_id, &near_miss, 0)
        .expect("a near miss previews");
    let placement = &data(&designer, node_id).placement;
    assert!(!placement.preview_ghosts.is_empty());
    assert!(placement.preview_near_miss);

    // …and still cannot be placed.
    assert!(
        designer
            .mechanosynth_edit_choose(&[], node_id, &near_miss, 0)
            .is_err()
    );
}

#[test]
fn clearing_the_preview_leaves_the_list_open() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_select_preview(&[], node_id, "hdon_frame", 0)
        .expect("an applicable row");
    designer
        .mechanosynth_edit_clear_preview(&[], node_id)
        .expect("the node exists");

    assert!(data(&designer, node_id).placement.preview_ghosts.is_empty());
    assert_eq!(
        tool_state(&designer, node_id),
        ToolState::Offers,
        "dropping the preview must not close the list"
    );
}

#[test]
fn selecting_an_operation_the_list_does_not_hold_is_an_error() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let error = designer
        .mechanosynth_edit_select_preview(&[], node_id, "no_such_op", 0)
        .expect_err("the library has no such operation");
    assert!(error.contains("no_such_op"), "{error}");
}

#[test]
fn the_offers_to_choose_sequence_inserts_the_step_the_fit_found() {
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

    let step = &data(&designer, node_id).authored[0];
    assert_eq!(step.step.op, "hdon_frame");
    assert!(step.is_exact());
    assert_eq!(data(&designer, node_id).cursor, 1, "the cursor lands on it");
    assert_eq!(designer.undo_stack.history_len(), 1);
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
        .mechanosynth_edit_offers(&[], node_id, at(B))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_select_preview(&[], node_id, "dimerize", 1)
        .expect("the second placement previews");
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
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
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
        .mechanosynth_edit_offers(&[], node_id, at(DVec3::new(0.0, 0.0, -20.0)))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_uniq", 0)
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
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    let index = designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_bare", 0)
        .expect("one free direction, so one candidate");
    assert_eq!(index, 0);
    let step = &data(&designer, node_id).authored[0];
    assert!(step.approximate);
    assert_eq!(data(&designer, node_id).inexact_counts().1, 1);
}

#[test]
fn a_commit_is_undoable_and_the_undo_empties_the_block() {
    let (mut designer, node_id) = setup();
    designer
        .mechanosynth_edit_offers(&[], node_id, at(A))
        .expect("the atom exists");
    designer
        .mechanosynth_edit_choose(&[], node_id, "hdon_frame", 0)
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
    // The three terminators of sub-cluster E, abstracted one after another —
    // each one a fresh question, since a commit no longer arms anything.
    let e = DVec3::new(0.0, 0.0, 20.0);
    for offset in [
        DVec3::new(0.854478, 0.854478, 0.854478),
        DVec3::new(0.854478, -0.854478, -0.854478),
        DVec3::new(-0.854478, 0.854478, -0.854478),
    ] {
        designer
            .mechanosynth_edit_offers(&[], node_id, at(e + offset))
            .expect("the atom exists");
        designer
            .mechanosynth_edit_choose(&[], node_id, "habst", 0)
            .expect("each terminator is abstractable");
    }
    let stored = data(&designer, node_id);
    assert_eq!(stored.authored.len(), 3);
    assert_eq!(stored.cursor, 3);
    assert!(stored.authored.iter().all(|step| step.step.op == "habst"));
    assert_eq!(designer.undo_stack.history_len(), 3);
}
