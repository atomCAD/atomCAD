//! Phase 3 of `doc/design_mechanosynth_editor.md` — the `mechanosynth_edit`
//! node: evaluation, the cursor, the block-editing commands, the text format
//! and persistence.
//!
//! The placement engine is covered in `atomcad-crystolecule`'s
//! `mechanosynth_place_test.rs`, and the placement *API* — arm, pick, choose —
//! in `rust/tests/structure_designer_api/mechanosynth_edit_api_test.rs`, which
//! needs `CAD_INSTANCE`-free designer methods that this harness cannot reach
//! the other half of.
//!
//! Every evaluation assertion is stated as an **equality against the
//! replayer**, never against hand-written coordinates: the two nodes share one
//! engine, and a test that re-derived the coordinates would pass while they
//! drifted apart. The payload is the P1 methylation fixture, so the ladder of
//! expected structures already exists.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::mechanosynth::{
    Step, compare_structures, describe_mismatches, load_build_script,
};
use atomcad_crystolecule::structure::Structure;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{
    Alignment, CrystalData, MoleculeData, NetworkResult,
};
use atomcad_structure_designer::mechanosynth_edit_ops::StepMetadataField;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::nodes::build_script::BuildScriptData;
use atomcad_structure_designer::nodes::build_step::build_step_record;
use atomcad_structure_designer::nodes::import_xyz::ImportXYZData;
use atomcad_structure_designer::nodes::mechanosynth::{
    MS_ADDED_TAG, MS_CURRENT_TAG, MS_LAYER_TAG, MechanosynthData,
};
use atomcad_structure_designer::nodes::mechanosynth_edit::{AuthoredStep, MechanosynthEditData};
use atomcad_structure_designer::nodes::ops_library::OpsLibraryData;
use atomcad_structure_designer::nodes::value::ValueData;
use atomcad_structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::{edit_network, serialize_network};
use atomcad_test_support::fixture_path_str;
use glam::f64::{DMat3, DVec2, DVec3};
use std::collections::HashMap;
use tempfile::tempdir;

const H: i16 = 1;
const C: i16 = 6;
const NET: &str = "test";

// ============================================================================
// Helpers
// ============================================================================

fn fixture(name: &str) -> String {
    fixture_path_str(&format!("mechanosynth/{name}"))
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NET);
    designer.set_active_node_network_name(Some(NET.to_string()));
    designer
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64, pin_index: i32) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NET).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, pin_index, registry, false, &mut context)
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

fn add_value_node(designer: &mut StructureDesigner, value: NetworkResult) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    network.add_node("value", DVec2::ZERO, 0, Box::new(ValueData { value }))
}

fn molecule_value(atoms: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms,
        geo_tree_root: None,
    })
}

fn crystal_value(atoms: AtomicStructure) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms,
        geo_tree_root: None,
        alignment: Alignment::Aligned,
        alignment_reason: None,
    })
}

fn add_ops_library_node(designer: &mut StructureDesigner, name: &str) -> u64 {
    let node_id = designer.add_node("ops_library", DVec2::new(-400.0, -100.0));
    with_data::<OpsLibraryData, _>(designer, node_id, |data| {
        data.file = Some(fixture(name));
        data.reload_missing(None);
    });
    node_id
}

fn add_build_script_node(designer: &mut StructureDesigner, name: &str) -> u64 {
    let node_id = designer.add_node("build_script", DVec2::new(-400.0, 100.0));
    with_data::<BuildScriptData, _>(designer, node_id, |data| {
        data.file = Some(fixture(name));
        data.reload_missing(None);
    });
    node_id
}

/// The three methylation steps, as the editor stores them. Exact by
/// construction: a step read from a generated file is the author's assertion,
/// which is what a zero residual means.
fn methylate_steps() -> Vec<AuthoredStep> {
    load_build_script(std::path::Path::new(&fixture("methylate_build.json")))
        .expect("the fixture parses")
        .steps
        .into_iter()
        .map(|step| AuthoredStep {
            step,
            residual: 0.0,
            approximate: false,
        })
        .collect()
}

/// An editor wired to a base and an `ops_library`, carrying `authored` at
/// `cursor`. Returns the editor's node id.
fn add_editor(
    designer: &mut StructureDesigner,
    base_id: u64,
    authored: Vec<AuthoredStep>,
    cursor: i32,
) -> u64 {
    let node_id = designer.add_node("mechanosynth_edit", DVec2::new(200.0, 0.0));
    let ops_id = add_ops_library_node(designer, "methylate_ops.json");
    designer.connect_nodes(base_id, 0, node_id, 0);
    designer.connect_nodes(ops_id, 0, node_id, 1);
    with_data::<MechanosynthEditData, _>(designer, node_id, |data| {
        data.authored = authored;
        data.cursor = cursor;
    });
    node_id
}

fn editor_data(designer: &StructureDesigner, node_id: u64) -> &MechanosynthEditData {
    designer
        .mechanosynth_edit_data(&[], node_id)
        .expect("a mechanosynth_edit node")
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

fn expect_error(result: NetworkResult) -> String {
    match result {
        NetworkResult::Error(message) => message,
        other => panic!("expected an error, got {}", other.to_display_string()),
    }
}

fn assert_same(a: &AtomicStructure, b: &AtomicStructure) {
    let mismatches = compare_structures(a, b, 1e-6);
    assert!(
        mismatches.is_empty(),
        "structures differ:\n{}",
        describe_mismatches(&mismatches)
    );
}

fn atom_at(s: &AtomicStructure, pos: DVec3) -> u32 {
    let found = s.get_atoms_in_radius(&pos, 1e-6);
    assert_eq!(found.len(), 1, "expected exactly one atom at {pos:?}");
    found[0]
}

/// Carbon at the origin with four hydrogens — the workpiece the fixture script
/// methylates. Copied from `mechanosynth_test.rs`, whose ladder below matches.
fn methane() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let c = s.add_atom(C, DVec3::ZERO);
    let bond_length = 1.09;
    let z = -bond_length / 3.0;
    let radius = bond_length * (8.0f64 / 9.0).sqrt();
    let mut directions = vec![DVec3::new(0.0, 0.0, bond_length)];
    for i in 0..3 {
        let angle = std::f64::consts::TAU * (i as f64) / 3.0;
        directions.push(DVec3::new(radius * angle.cos(), radius * angle.sin(), z));
    }
    for d in directions {
        let h = s.add_atom(H, d);
        s.add_bond(c, h, 1);
    }
    s
}

fn methylate_expected(step: usize) -> AtomicStructure {
    let mut s = methane();
    if step >= 1 {
        s.delete_atom(atom_at(&s, DVec3::new(0.0, 0.0, 1.09)));
    }
    if step >= 2 {
        let host = atom_at(&s, DVec3::ZERO);
        let carbon = s.add_atom(C, DVec3::new(0.0, 0.0, 1.54));
        let h1 = s.add_atom(H, DVec3::new(0.89, 0.0, 2.17));
        let h2 = s.add_atom(H, DVec3::new(-0.89, 0.0, 2.17));
        s.add_bond(host, carbon, 1);
        s.add_bond(carbon, h1, 1);
        s.add_bond(carbon, h2, 1);
    }
    if step >= 3 {
        let carbon = atom_at(&s, DVec3::new(0.0, 0.0, 1.54));
        let cap = s.add_atom(H, DVec3::new(0.0, 0.0, 2.63));
        s.add_bond(carbon, cap, 1);
    }
    s
}

// ============================================================================
// Evaluation
// ============================================================================

#[test]
fn the_result_pin_carries_the_workpiece_at_the_cursor() {
    for (cursor, expected) in [(0, 0), (1, 1), (2, 2), (3, 3), (-1, 3)] {
        let mut designer = setup_designer();
        let base_id = add_value_node(&mut designer, molecule_value(methane()));
        let node_id = add_editor(&mut designer, base_id, methylate_steps(), cursor);
        let atoms = expect_atoms(evaluate_pin(&designer, node_id, 0));
        assert_same(&atoms, &methylate_expected(expected));
    }
}

#[test]
fn a_cursor_past_the_end_clamps_as_the_replayers_slider_does() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_editor(&mut designer, base_id, methylate_steps(), 99);
    let atoms = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(&atoms, &methylate_expected(3));
}

#[test]
fn the_editor_equals_the_replayer_fed_the_same_steps() {
    // The shared "same result, both nodes" assertion. The replayer is driven
    // from the fixture file, the editor from the stored block, and the two must
    // agree atom for atom at every cursor.
    for cursor in [0, 1, 2, 3] {
        let mut designer = setup_designer();
        let base_id = add_value_node(&mut designer, molecule_value(methane()));

        let editor_id = add_editor(&mut designer, base_id, methylate_steps(), cursor);

        let replayer_id = designer.add_node("mechanosynth", DVec2::new(200.0, 300.0));
        let ops_id = add_ops_library_node(&mut designer, "methylate_ops.json");
        let steps_id = add_build_script_node(&mut designer, "methylate_build.json");
        designer.connect_nodes(base_id, 0, replayer_id, 0);
        designer.connect_nodes(ops_id, 0, replayer_id, 1);
        designer.connect_nodes(steps_id, 0, replayer_id, 2);
        with_data::<MechanosynthData, _>(&mut designer, replayer_id, |data| {
            data.step = cursor;
        });

        let from_editor = expect_atoms(evaluate_pin(&designer, editor_id, 0));
        let from_replayer = expect_atoms(evaluate_pin(&designer, replayer_id, 0));
        assert_same(&from_editor, &from_replayer);
    }
}

#[test]
fn the_authored_block_is_applied_after_the_wired_prefix() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let mut steps = methylate_steps();
    let tail = steps.split_off(1);

    // The first step arrives on the wire; the other two are authored.
    let prefix = add_value_node(
        &mut designer,
        NetworkResult::Array(steps.iter().map(|s| build_step_record(&s.step)).collect()),
    );
    let node_id = add_editor(&mut designer, base_id, tail, -1);
    designer.connect_nodes(prefix, 0, node_id, 2);

    assert_same(
        &expect_atoms(evaluate_pin(&designer, node_id, 0)),
        &methylate_expected(3),
    );

    // …and the cursor counts the *authored* steps only, so cursor 0 is the
    // state the prefix left behind.
    with_data::<MechanosynthEditData, _>(&mut designer, node_id, |data| data.cursor = 0);
    assert_same(
        &expect_atoms(evaluate_pin(&designer, node_id, 0)),
        &methylate_expected(1),
    );
}

#[test]
fn the_steps_output_is_the_prefix_and_the_whole_block_whatever_the_cursor() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let mut steps = methylate_steps();
    let tail = steps.split_off(1);
    let prefix = add_value_node(
        &mut designer,
        NetworkResult::Array(steps.iter().map(|s| build_step_record(&s.step)).collect()),
    );
    let node_id = add_editor(&mut designer, base_id, tail, 0);
    designer.connect_nodes(prefix, 0, node_id, 2);

    let NetworkResult::Array(emitted) = evaluate_pin(&designer, node_id, 1) else {
        panic!("the steps pin carries an array");
    };
    assert_eq!(emitted.len(), 3, "the cursor must not truncate the output");
    let ops: Vec<String> = emitted
        .iter()
        .map(|record| match record.extract_record_field("op") {
            Some(NetworkResult::String(op)) => op.clone(),
            _ => panic!("every element of the steps pin is a BuildStep record"),
        })
        .collect();
    assert_eq!(ops, vec!["habst", "gm_methylate", "hdon"]);
}

#[test]
fn the_result_keeps_the_inputs_concrete_type() {
    for (make, expect_crystal) in [
        (
            molecule_value as fn(AtomicStructure) -> NetworkResult,
            false,
        ),
        (crystal_value as fn(AtomicStructure) -> NetworkResult, true),
    ] {
        let mut designer = setup_designer();
        let base_id = add_value_node(&mut designer, make(methane()));
        let node_id = add_editor(&mut designer, base_id, methylate_steps(), -1);
        let result = evaluate_pin(&designer, node_id, 0);
        assert_eq!(
            matches!(result, NetworkResult::Crystal(_)),
            expect_crystal,
            "the concrete phase must flow through"
        );
    }
}

#[test]
fn ms_current_marks_the_cursor_steps_atoms_and_nothing_else() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_editor(&mut designer, base_id, methylate_steps(), 0);

    // Cursor 0: no step has been applied, so nothing is current.
    let atoms = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert!(atoms.atoms_with_tag(MS_CURRENT_TAG).is_empty());

    // Cursor 2 added the CH2, so its three atoms plus the host are current.
    with_data::<MechanosynthEditData, _>(&mut designer, node_id, |data| data.cursor = 2);
    let atoms = expect_atoms(evaluate_pin(&designer, node_id, 0));
    let current = atoms.atoms_with_tag(MS_CURRENT_TAG);
    assert!(!current.is_empty());
    for atom_id in &current {
        let position = atoms.get_atom(*atom_id).expect("atom exists").position;
        assert!(
            position.z >= -1e-9,
            "only the methylated end is current, not the far hydrogens: {position:?}"
        );
    }

    // The replayer's two build-wide tags are not this node's business.
    assert!(atoms.atoms_with_tag(MS_ADDED_TAG).is_empty());
    assert!(atoms.atoms_with_tag(MS_LAYER_TAG).is_empty());
}

#[test]
fn a_step_whose_pattern_no_longer_matches_is_an_error_that_names_it() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    // The cap comes before the methylation that creates the radical it caps.
    let mut steps = methylate_steps();
    steps.swap(1, 2);
    let node_id = add_editor(&mut designer, base_id, steps, -1);

    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(message.contains("mechanosynth_edit"), "{message}");
    assert!(message.contains("step 2"), "{message}");
    assert!(
        editor_data(&designer, node_id)
            .last_error()
            .is_some_and(|error| error.contains("step 2")),
        "the transient last-error field carries it for the panel"
    );
}

#[test]
fn an_unknown_op_in_the_authored_block_names_the_step() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let mut steps = methylate_steps();
    steps[1].step.op = "no_such_op".to_string();
    let node_id = add_editor(&mut designer, base_id, steps, -1);

    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(message.contains("no_such_op"), "{message}");
    assert!(message.contains("step 2"), "{message}");
}

#[test]
fn an_unwired_ops_pin_is_an_error_naming_the_pin() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = designer.add_node("mechanosynth_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(base_id, 0, node_id, 0);

    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(message.contains("ops pin"), "{message}");
}

#[test]
fn the_error_reaches_both_output_pins() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = designer.add_node("mechanosynth_edit", DVec2::new(200.0, 0.0));
    designer.connect_nodes(base_id, 0, node_id, 0);
    expect_error(evaluate_pin(&designer, node_id, 0));
    expect_error(evaluate_pin(&designer, node_id, 1));
}

// ============================================================================
// The block-editing commands
// ============================================================================

/// The shared *undo restores the tuple* assertion: run `edit`, then undo and
/// redo, comparing `(authored, cursor)` structurally each time.
fn assert_undo_restores_the_tuple(
    designer: &mut StructureDesigner,
    node_id: u64,
    edit: impl FnOnce(&mut StructureDesigner),
) {
    let before = {
        let data = editor_data(designer, node_id);
        (data.authored.clone(), data.cursor)
    };
    let entries = designer.undo_stack.history_len();

    edit(designer);

    let after = {
        let data = editor_data(designer, node_id);
        (data.authored.clone(), data.cursor)
    };
    assert_ne!(before, after, "the edit must have changed something");
    assert_eq!(
        designer.undo_stack.history_len(),
        entries + 1,
        "exactly one undo entry"
    );

    assert!(designer.undo());
    let data = editor_data(designer, node_id);
    assert_eq!((data.authored.clone(), data.cursor), before);

    assert!(designer.redo());
    let data = editor_data(designer, node_id);
    assert_eq!((data.authored.clone(), data.cursor), after);
}

fn editor_with_block(designer: &mut StructureDesigner) -> u64 {
    let base_id = add_value_node(designer, molecule_value(methane()));
    let node_id = add_editor(designer, base_id, methylate_steps(), -1);
    // The setup writes through `with_data`, which records nothing; start the
    // command assertions from an empty stack.
    designer.undo_stack.clear();
    node_id
}

#[test]
fn inserting_a_step_moves_the_cursor_onto_it_and_undo_restores_both() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);
    let copy = editor_data(&designer, node_id).authored[0].clone();
    assert_undo_restores_the_tuple(&mut designer, node_id, |designer| {
        designer
            .mechanosynth_edit_insert_step(&[], node_id, 1, copy)
            .expect("insert at a valid index");
    });
    let data = editor_data(&designer, node_id);
    assert_eq!(data.authored.len(), 4);
    assert_eq!(data.cursor, 2, "the cursor lands on the new step");
}

#[test]
fn deleting_the_cursor_step_steps_the_cursor_back() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);
    designer
        .set_mechanosynth_edit_cursor(&[], node_id, 3)
        .expect("a valid cursor");
    assert_undo_restores_the_tuple(&mut designer, node_id, |designer| {
        designer
            .mechanosynth_edit_delete_step(&[], node_id, 2)
            .expect("delete a real step");
    });
    let data = editor_data(&designer, node_id);
    assert_eq!(data.authored.len(), 2);
    assert_eq!(data.cursor, 2);
}

#[test]
fn moving_a_step_carries_its_residual_and_flag() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);
    designer
        .mechanosynth_edit_insert_step(
            &[],
            node_id,
            0,
            AuthoredStep {
                step: Step::new("habst", DVec3::new(0.0, 0.0, 1.09)),
                residual: 0.0213,
                approximate: true,
            },
        )
        .expect("insert");

    assert_undo_restores_the_tuple(&mut designer, node_id, |designer| {
        designer
            .mechanosynth_edit_move_step(&[], node_id, 0, 2)
            .expect("move");
    });
    let moved = &editor_data(&designer, node_id).authored[2];
    assert_eq!(moved.residual, 0.0213);
    assert!(moved.approximate);
}

#[test]
fn a_metadata_edit_is_one_undo_entry_and_undo_restores_the_tuple() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);
    assert_undo_restores_the_tuple(&mut designer, node_id, |designer| {
        designer
            .set_mechanosynth_edit_step_metadata(
                &[],
                node_id,
                1,
                StepMetadataField::Phase,
                "layer1",
                0,
            )
            .expect("a real step");
    });
    assert_eq!(
        editor_data(&designer, node_id).authored[1].step.phase,
        "layer1"
    );
}

#[test]
fn consecutive_edits_to_one_field_coalesce_and_a_different_field_does_not() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);

    for text in ["l", "la", "lay", "layer1"] {
        designer
            .set_mechanosynth_edit_step_metadata(&[], node_id, 1, StepMetadataField::Phase, text, 0)
            .expect("a real step");
    }
    assert_eq!(
        designer.undo_stack.history_len(),
        1,
        "typing into one chip is one undo entry"
    );

    designer
        .set_mechanosynth_edit_step_metadata(&[], node_id, 1, StepMetadataField::Method, "probe", 0)
        .expect("a real step");
    assert_eq!(
        designer.undo_stack.history_len(),
        2,
        "a different field starts a new entry"
    );

    // And one undo takes the whole typed word back, not its last letter.
    assert!(designer.undo());
    assert!(designer.undo());
    let data = editor_data(&designer, node_id);
    assert_eq!(data.authored[1].step.phase, "");
    assert_eq!(data.authored[1].step.method, "");
}

#[test]
fn cursor_changes_are_not_undo_entries() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);
    for cursor in [0, 1, 2, 3, 2, 1, 0, 3, -1, 2] {
        designer
            .set_mechanosynth_edit_cursor(&[], node_id, cursor)
            .expect("a cursor");
    }
    assert_eq!(designer.undo_stack.history_len(), 0);
    assert_eq!(editor_data(&designer, node_id).cursor, 2);
}

#[test]
fn an_out_of_range_index_is_an_error_that_changes_nothing() {
    let mut designer = setup_designer();
    let node_id = editor_with_block(&mut designer);
    assert!(
        designer
            .mechanosynth_edit_delete_step(&[], node_id, 9)
            .is_err()
    );
    assert!(
        designer
            .mechanosynth_edit_move_step(&[], node_id, 0, 9)
            .is_err()
    );
    assert_eq!(editor_data(&designer, node_id).authored.len(), 3);
    assert_eq!(designer.undo_stack.history_len(), 0);
}

// ============================================================================
// Text format
// ============================================================================

fn author_and_serialize(designer: &mut StructureDesigner, code: &str) -> String {
    let registry = &mut designer.node_type_registry;
    let mut network = registry.node_networks.remove(NET).expect("test network");
    let result = edit_network(&mut network, registry, code, true);
    assert!(
        result.success,
        "authoring must succeed: {:?}",
        result.errors
    );
    let text = serialize_network(&network, registry, None);
    registry.node_networks.insert(NET.to_string(), network);
    text
}

#[test]
fn an_editor_node_round_trips_through_the_text_format() {
    let mut designer = setup_designer();
    let code = r#"lib = ops_library { file: "ops.json" }
edit = mechanosynth_edit { ops: lib, cursor: 2, authored: [{ op: "habst", t: (12.71, 9.53, 8.02), method: "probe", phase: "layer1", layer: 1, site: 0 }, { op: "dimerize", t: (14.27, 9.53, 8.02), r: ((0.0, 1.0, 0.0), (-1.0, 0.0, 0.0), (0.0, 0.0, 1.0)), residual: 0.0213, approximate: true }] }
output edit
"#;
    let first = author_and_serialize(&mut designer, code);
    assert!(first.contains("cursor: 2"), "{first}");
    assert!(first.contains("residual: 0.0213"), "{first}");
    assert!(first.contains("approximate: true"), "{first}");

    let second = author_and_serialize(&mut designer, &first);
    assert_eq!(first, second, "a re-author must be a no-op");
}

#[test]
fn absent_step_fields_are_omitted_and_defaulted() {
    let mut designer = setup_designer();
    let text = author_and_serialize(
        &mut designer,
        "edit = mechanosynth_edit { authored: [{ op: \"habst\", t: (1.0, 2.0, 3.0) }] }\noutput edit\n",
    );
    assert!(
        text.contains(r#"authored: [{ op: "habst", t: (1.0, 2.0, 3.0) }]"#),
        "an identity r, empty metadata, an exact residual and approximate: false are all \
         omitted:\n{text}"
    );

    let node_id = *designer
        .node_type_registry
        .node_networks
        .get(NET)
        .unwrap()
        .nodes
        .keys()
        .next()
        .expect("one node");
    let step = &editor_data(&designer, node_id).authored[0];
    assert_eq!(step.step.r, DMat3::IDENTITY);
    assert_eq!(step.step.layer, -1);
    assert_eq!(step.step.site, -1);
    assert_eq!(step.residual, 0.0);
    assert!(!step.approximate);
    assert!(step.is_exact(), "a hand-typed step counts as exact");
}

#[test]
fn an_empty_block_round_trips() {
    let mut designer = setup_designer();
    let text = author_and_serialize(
        &mut designer,
        "edit = mechanosynth_edit { authored: [] }\noutput edit\n",
    );
    assert!(text.contains("authored: []"), "{text}");
    assert_eq!(author_and_serialize(&mut designer, &text), text);
}

#[test]
fn an_all_integer_transform_is_accepted_the_way_a_wire_would_coerce_it() {
    let mut designer = setup_designer();
    author_and_serialize(
        &mut designer,
        "edit = mechanosynth_edit { authored: [{ op: \"habst\", t: (0, 0, 1), r: ((0, 1, 0), (-1, 0, 0), (0, 0, 1)) }] }\noutput edit\n",
    );
    let node_id = *designer
        .node_type_registry
        .node_networks
        .get(NET)
        .unwrap()
        .nodes
        .keys()
        .next()
        .expect("one node");
    let step = &editor_data(&designer, node_id).authored[0].step;
    assert_eq!(step.t, DVec3::new(0.0, 0.0, 1.0));
    assert_ne!(step.r, DMat3::IDENTITY);
}

#[test]
fn a_step_literal_with_an_unknown_field_is_a_parse_error_naming_it() {
    let mut data = MechanosynthEditData::new();
    let mut props = HashMap::new();
    props.insert(
        "authored".to_string(),
        atomcad_structure_designer::text_format::TextValue::Array(vec![
            atomcad_structure_designer::text_format::TextValue::Object(vec![
                (
                    "op".to_string(),
                    atomcad_structure_designer::text_format::TextValue::String("habst".to_string()),
                ),
                (
                    "t".to_string(),
                    atomcad_structure_designer::text_format::TextValue::Vec3(DVec3::ZERO),
                ),
                (
                    "methdo".to_string(),
                    atomcad_structure_designer::text_format::TextValue::String("probe".to_string()),
                ),
            ]),
        ]),
    );
    let error = data
        .set_text_properties(&props)
        .expect_err("an unknown field is refused");
    assert!(error.contains("methdo"), "{error}");
}

// ============================================================================
// Persistence
// ============================================================================

#[test]
fn the_block_and_the_cursor_survive_a_cnnd_round_trip() {
    let mut designer = setup_designer();
    // An `import_xyz` base rather than a `value` node: a `value` carries a
    // runtime `NetworkResult`, which does not persist, so the reloaded network
    // would have nothing to replay onto.
    let base_id = designer.add_node("import_xyz", DVec2::new(-600.0, 0.0));
    with_data::<ImportXYZData, _>(&mut designer, base_id, |data| {
        data.file_name = Some(fixture("methane.xyz"));
        data.atomic_structure =
            atomcad_crystolecule::io::xyz_loader::load_xyz(&fixture("methane.xyz"), true).ok();
    });
    let mut steps = methylate_steps();
    steps[2].residual = 0.0213;
    steps[2].approximate = true;
    let node_id = add_editor(&mut designer, base_id, steps, 2);

    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("editor.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");

    let mut reloaded = StructureDesigner::new();
    load_node_networks_from_file(&mut reloaded.node_type_registry, path.to_str().unwrap())
        .expect("load");
    reloaded.set_active_node_network_name(Some(NET.to_string()));

    let data = reloaded
        .mechanosynth_edit_data(&[], node_id)
        .expect("the editor node survives");
    assert_eq!(data.cursor, 2);
    assert_eq!(data.authored.len(), 3);
    assert_eq!(data.authored[2].residual, 0.0213);
    assert!(data.authored[2].approximate);
    assert_eq!(data.inexact_counts(), (1, 1));

    // …and it still evaluates to the same thing. The comparison runs at 1e-5
    // because the `.xyz` base rounds its coordinates to six decimals, which is
    // the file's precision and not the engine's.
    let atoms = expect_atoms(evaluate_pin(&reloaded, node_id, 0));
    let mismatches = compare_structures(&atoms, &methylate_expected(2), 1e-5);
    assert!(
        mismatches.is_empty(),
        "structures differ after a reload:
{}",
        describe_mismatches(&mismatches)
    );
}

#[test]
fn the_placement_state_is_never_serialized() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_editor(&mut designer, base_id, methylate_steps(), -1);
    with_data::<MechanosynthEditData, _>(&mut designer, node_id, |data| {
        data.placement.armed = Some("habst".to_string());
        data.placement.anchor = Some(7);
    });

    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("editor.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");
    let saved = std::fs::read_to_string(&path).expect("read back");
    assert!(!saved.contains("habst\",\"anchor"), "{saved}");
    assert!(!saved.contains("\"armed\""), "the tool state is transient");
}
