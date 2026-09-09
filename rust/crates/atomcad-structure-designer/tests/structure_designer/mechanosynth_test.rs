//! P2 of `design_mechanosynth_node.md` — the `mechanosynth` *node*.
//!
//! The replay engine is covered in `atomcad-crystolecule`'s
//! `mechanosynth_test.rs`; what is tested here is the wrapper: that the result
//! pin carries the workpiece at the asked-for step, that a wired pin beats the
//! stored property, that a load or match failure reaches the pin with the
//! engine's own wording, that the `#[serde(skip)]` caches survive a no-op
//! property write and a `.cnnd` round trip, and that the three properties
//! round-trip through the text format and through undo.
//!
//! The payload throughout is the P1 multi-step fixture (`methylate_ops.json` +
//! `methylate_build.json`): abstract the +z hydrogen of a methane, add a CH2,
//! cap the radical. Expected structures are built in code and compared with
//! `compare_structures`, never by asserting on atom ids — a replay re-assigns
//! them.

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::mechanosynth::{compare_structures, describe_mismatches};
use atomcad_crystolecule::structure::Structure;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::{
    Alignment, CrystalData, MoleculeData, NetworkResult,
};
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::mechanosynth::{MS_CURRENT_TAG, MechanosynthData};
use atomcad_structure_designer::nodes::string::StringData;
use atomcad_structure_designer::nodes::value::ValueData;
use atomcad_structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::{edit_network, serialize_network};
use atomcad_test_support::fixture_path_str;
use glam::f64::{DVec2, DVec3};
use std::collections::HashMap;
use tempfile::tempdir;

const H: i16 = 1;
const C: i16 = 6;

/// Positions are ideal, so structural comparisons can be far tighter than the
/// 0.3 Å match tolerance.
const EXACT: f64 = 1e-9;

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

fn add_string_node(designer: &mut StructureDesigner, value: &str) -> u64 {
    let node_id = designer.add_node("string", DVec2::new(-200.0, 0.0));
    with_data::<StringData, _>(designer, node_id, |data| data.value = value.to_string());
    node_id
}

fn add_int_node(designer: &mut StructureDesigner, value: i32) -> u64 {
    let node_id = designer.add_node("int", DVec2::new(-200.0, 100.0));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(NET)
        .unwrap();
    let mut props = HashMap::new();
    props.insert(
        "value".to_string(),
        atomcad_structure_designer::text_format::TextValue::Int(value),
    );
    network
        .nodes
        .get_mut(&node_id)
        .unwrap()
        .data
        .set_text_properties(&props)
        .expect("int node takes a value property");
    node_id
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

fn node_data(designer: &StructureDesigner, node_id: u64) -> &MechanosynthData {
    designer
        .node_type_registry
        .node_networks
        .get(NET)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()
        .expect("mechanosynth node carries MechanosynthData")
}

/// Node data with both fixture files loaded — the state the API's property
/// setter and the `.cnnd` loader both leave a node in.
fn loaded_data(ops: &str, build: &str, step: i32) -> MechanosynthData {
    let mut data = MechanosynthData {
        ops_file: Some(fixture(ops)),
        build_file: Some(fixture(build)),
        step,
        ..MechanosynthData::new()
    };
    data.reload_missing(None);
    data
}

/// A `mechanosynth` node wired to `base_id`, with both fixture files loaded.
fn add_mechanosynth(designer: &mut StructureDesigner, base_id: u64, data: MechanosynthData) -> u64 {
    let node_id = designer.add_node("mechanosynth", DVec2::new(200.0, 0.0));
    designer.set_node_network_data_scoped(&[], node_id, Box::new(data));
    designer.connect_nodes(base_id, 0, node_id, 0);
    node_id
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

fn assert_same(actual: &AtomicStructure, expected: &AtomicStructure, what: &str) {
    let mismatches = compare_structures(actual, expected, EXACT);
    assert!(
        mismatches.is_empty(),
        "{what} differs from the hand-built structure:\n{}",
        describe_mismatches(&mismatches)
    );
}

/// The only atom within a whisker of `pos`.
fn atom_at(s: &AtomicStructure, pos: DVec3) -> u32 {
    let found = s.get_atoms_in_radius(&pos, 1e-6);
    assert_eq!(found.len(), 1, "expected exactly one atom at {pos:?}");
    found[0]
}

/// Carbon at the origin with a hydrogen along +z and three more completing the
/// tetrahedron — the workpiece the fixture script methylates.
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

/// The expected workpiece after each of the three methylation steps — the same
/// hand-built ladder the engine test uses.
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
fn the_result_pin_carries_the_workpiece_at_every_step() {
    for (stored_step, expected_step) in [(-1, 3), (0, 0), (1, 1), (2, 2), (3, 3)] {
        let mut designer = setup_designer();
        let base_id = add_value_node(&mut designer, molecule_value(methane()));
        let node_id = add_mechanosynth(
            &mut designer,
            base_id,
            loaded_data("methylate_ops.json", "methylate_build.json", stored_step),
        );

        let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
        assert_same(
            &result,
            &methylate_expected(expected_step),
            &format!("step = {stored_step}"),
        );
    }
}

#[test]
fn a_step_beyond_the_script_clamps_rather_than_erroring() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", 99),
    );

    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(&result, &methylate_expected(3), "step = 99");
}

#[test]
fn the_output_carries_ms_current_on_the_current_steps_atoms() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", 2),
    );

    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));

    // Step 2 (`gm_methylate`) keeps the host carbon and adds three atoms.
    let tagged = result.atoms_with_tag(MS_CURRENT_TAG);
    assert_eq!(
        tagged.len(),
        4,
        "the host carbon plus the three atoms step 2 added should carry the tag"
    );
    for position in [
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 1.54),
        DVec3::new(0.89, 0.0, 2.17),
        DVec3::new(-0.89, 0.0, 2.17),
    ] {
        assert!(
            tagged.contains(&atom_at(&result, position)),
            "the atom at {position:?} belongs to step 2 and should be tagged"
        );
    }
}

#[test]
fn step_zero_clears_a_highlight_the_base_already_carried() {
    // An upstream `mechanosynth` leaves `ms_current` on the base; a downstream
    // one at step 0 must hand back a clean structure rather than two overlapping
    // highlights.
    let mut base = methane();
    let carbon = atom_at(&base, DVec3::ZERO);
    base.add_atom_tag(carbon, MS_CURRENT_TAG).ok();
    assert_eq!(base.atoms_with_tag(MS_CURRENT_TAG).len(), 1);

    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(base));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", 0),
    );

    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert!(result.atoms_with_tag(MS_CURRENT_TAG).is_empty());
}

#[test]
fn the_output_keeps_the_inputs_variant() {
    let mut designer = setup_designer();
    let molecule_base = add_value_node(&mut designer, molecule_value(methane()));
    let crystal_base = add_value_node(&mut designer, crystal_value(methane()));
    let from_molecule = add_mechanosynth(
        &mut designer,
        molecule_base,
        loaded_data("methylate_ops.json", "methylate_build.json", -1),
    );
    let from_crystal = add_mechanosynth(
        &mut designer,
        crystal_base,
        loaded_data("methylate_ops.json", "methylate_build.json", -1),
    );

    assert!(matches!(
        evaluate_pin(&designer, from_molecule, 0),
        NetworkResult::Molecule(_)
    ));
    match evaluate_pin(&designer, from_crystal, 0) {
        NetworkResult::Crystal(crystal) => {
            // The node only edits atoms; the geometry shell is the input's.
            assert!(crystal.geo_tree_root.is_none());
            assert_eq!(
                crystal.structure.motif_offset,
                Structure::diamond().motif_offset
            );
            assert_same(&crystal.atoms, &methylate_expected(3), "crystal result");
        }
        other => panic!(
            "a Crystal base should stay a Crystal, got {}",
            other.to_display_string()
        ),
    }
}

#[test]
fn a_non_atomic_base_is_rejected_by_name() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, NetworkResult::Int(3));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", -1),
    );

    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(
        message.starts_with("mechanosynth:"),
        "the node should own its error: {message}"
    );
}

// ============================================================================
// Pins override properties
// ============================================================================

#[test]
fn a_wired_step_overrides_the_stored_one() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", 3),
    );
    let step_id = add_int_node(&mut designer, 1);
    designer.connect_nodes(step_id, 0, node_id, 3);

    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(&result, &methylate_expected(1), "wired step = 1");
}

#[test]
fn wired_file_names_override_the_stored_ones_without_being_cached() {
    // Both properties are empty, so only the wires can supply the files.
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        MechanosynthData {
            step: 2,
            ..MechanosynthData::new()
        },
    );
    let ops_id = add_string_node(&mut designer, &fixture("methylate_ops.json"));
    let build_id = add_string_node(&mut designer, &fixture("methylate_build.json"));
    designer.connect_nodes(ops_id, 0, node_id, 1);
    designer.connect_nodes(build_id, 0, node_id, 2);

    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(&result, &methylate_expected(2), "wired files, step = 2");

    // A wired file is parsed at evaluation and never enters the node data,
    // exactly as `import_xyz` does — the property panel must keep showing the
    // (empty) stored names.
    let data = node_data(&designer, node_id);
    assert!(data.ops_file.is_none() && data.build_file.is_none());
    assert!(data.library.is_none() && data.script.is_none());
}

#[test]
fn a_wired_file_that_cannot_be_loaded_errors_with_the_file_name() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", -1),
    );
    let missing = fixture("no_such_build.json");
    let build_id = add_string_node(&mut designer, &missing);
    designer.connect_nodes(build_id, 0, node_id, 2);

    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(
        message.contains("no_such_build.json"),
        "the failure should name the file: {message}"
    );
}

// ============================================================================
// Failure surfacing
// ============================================================================

#[test]
fn a_step_that_cannot_match_surfaces_the_engines_message() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "unmatched_build.json", -1),
    );

    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(
        message.starts_with("mechanosynth: step 2 (habst @ (0.000, 0.000, 1.090))"),
        "the engine's wording should reach the pin behind one prefix: {message}"
    );
    assert!(
        message.contains("not found within 0.30") && message.contains("nearest atom is C"),
        "the whole diagnostic should survive: {message}"
    );

    // The partial state is reachable by asking for one step fewer.
    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| data.step = 1);
    let partial = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(
        &partial,
        &methylate_expected(1),
        "step = 1 of a failing build",
    );
}

#[test]
fn a_library_with_a_validation_error_errors_at_eval_and_sits_in_load_error() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let data = loaded_data("bad_ops.json", "methylate_build.json", -1);

    let load_error = data
        .load_error
        .clone()
        .expect("an invalid library should record a load error");
    assert!(data.library.is_none(), "a rejected library is not cached");
    assert!(
        data.script.is_some(),
        "the other file parsed fine and must keep its cache"
    );

    let node_id = add_mechanosynth(&mut designer, base_id, data);
    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(
        message.contains(&load_error),
        "the eval error should carry the load failure verbatim:\n  eval: {message}\n  load: {load_error}"
    );
    assert!(
        message.contains("add_star"),
        "the message should name the offending operation: {message}"
    );
}

#[test]
fn a_missing_file_property_names_the_property() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));

    let mut ops_only = MechanosynthData::new();
    ops_only.ops_file = Some(fixture("methylate_ops.json"));
    ops_only.reload_missing(None);
    let node_id = add_mechanosynth(&mut designer, base_id, ops_only);
    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(
        message.contains("build_file"),
        "the missing property should be named: {message}"
    );

    let mut build_only = MechanosynthData::new();
    build_only.build_file = Some(fixture("methylate_build.json"));
    build_only.reload_missing(None);
    let node_id = add_mechanosynth(&mut designer, base_id, build_only);
    let message = expect_error(evaluate_pin(&designer, node_id, 0));
    assert!(
        message.contains("ops_file"),
        "the missing property should be named: {message}"
    );
}

// ============================================================================
// Payload cache (`project_import_node_payload_wipe`)
// ============================================================================

#[test]
fn an_unchanged_file_name_keeps_the_parsed_cache() {
    let data = loaded_data("methylate_ops.json", "methylate_build.json", -1);

    let rewritten = data
        .with_ops_file(data.ops_file.clone())
        .with_build_file(data.build_file.clone());
    assert!(
        rewritten.library.is_some() && rewritten.script.is_some(),
        "a write that changes nothing must not re-import the files"
    );
}

#[test]
fn a_changed_file_name_drops_only_its_own_cache_and_reloads() {
    let data = loaded_data("methylate_ops.json", "methylate_build.json", -1);

    let mut swapped = data.with_ops_file(Some(fixture("valid_ops.json")));
    assert!(
        swapped.library.is_none(),
        "a new name has not been parsed yet"
    );
    assert!(
        swapped.script.is_some(),
        "the untouched file must keep its cache"
    );

    swapped.reload_missing(None);
    assert!(swapped.load_error.is_none());
    let library = swapped.library.as_ref().expect("the new library parses");
    assert!(
        library.get("keep_only").is_some(),
        "the reload should have read the *new* file"
    );
}

#[test]
fn re_setting_a_node_with_its_own_file_names_still_evaluates() {
    // The defect this guards: the property setter fires on every focus loss of
    // a path field, so a no-op write must not leave the node reporting a
    // missing library for a file it loaded seconds ago.
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", -1),
    );

    let rewritten = {
        let current = node_data(&designer, node_id);
        current
            .with_ops_file(current.ops_file.clone())
            .with_build_file(current.build_file.clone())
    };
    designer.set_node_network_data_scoped(&[], node_id, Box::new(rewritten));

    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(&result, &methylate_expected(3), "after a no-op rewrite");
}

// ============================================================================
// .cnnd round-trip: relativization + cache reload
// ============================================================================

#[test]
fn cnnd_roundtrip_relativizes_both_paths_and_reloads_the_caches() {
    let tmp = tempdir().expect("tempdir");
    for name in ["methylate_ops.json", "methylate_build.json"] {
        std::fs::copy(fixture(name), tmp.path().join(name)).expect("copy fixture");
    }
    let project_path = tmp.path().join("project.cnnd");

    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let mut data = MechanosynthData {
        ops_file: Some(tmp.path().join("methylate_ops.json").display().to_string()),
        build_file: Some(
            tmp.path()
                .join("methylate_build.json")
                .display()
                .to_string(),
        ),
        step: 2,
        ..MechanosynthData::new()
    };
    data.reload_missing(None);
    let node_id = add_mechanosynth(&mut designer, base_id, data);
    designer.validate_active_network();

    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &project_path,
        false,
        &HashMap::new(),
    )
    .expect("save should succeed");

    let saved = node_data(&designer, node_id);
    assert_eq!(saved.ops_file.as_deref(), Some("methylate_ops.json"));
    assert_eq!(saved.build_file.as_deref(), Some("methylate_build.json"));

    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, project_path.to_str().unwrap())
        .expect("load should succeed");

    let reloaded = registry
        .node_networks
        .get(NET)
        .and_then(|net| net.nodes.get(&node_id))
        .expect("the mechanosynth node should survive the roundtrip")
        .data
        .as_any_ref()
        .downcast_ref::<MechanosynthData>()
        .expect("mechanosynth node carries MechanosynthData");

    assert_eq!(reloaded.step, 2);
    assert!(
        reloaded.library.is_some() && reloaded.script.is_some(),
        "the loader should have repopulated both #[serde(skip)] caches"
    );
    assert_eq!(reloaded.step_counts(), Some((2, 3)));

    // And the reloaded node data evaluates identically. It is transplanted into
    // a fresh network rather than evaluated in place: the `value` base node
    // holds a live `NetworkResult`, which is test scaffolding and does not
    // survive serialization, so the reloaded wire has nothing to carry.
    let reloaded = reloaded.clone();
    let mut designer2 = setup_designer();
    let base2 = add_value_node(&mut designer2, molecule_value(methane()));
    let transplanted = add_mechanosynth(&mut designer2, base2, reloaded);
    let result = expect_atoms(evaluate_pin(&designer2, transplanted, 0));
    assert_same(&result, &methylate_expected(2), "after the roundtrip");
}

// ============================================================================
// Text format
// ============================================================================

/// Authors `code` into the active network and returns the serialization.
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
fn the_three_properties_round_trip_through_the_text_format() {
    let mut designer = setup_designer();
    let text = author_and_serialize(
        &mut designer,
        r#"
        m = mechanosynth { ops_file: "ops.json", build_file: "build.json", step: 2 }
        "#,
    );

    assert!(text.contains("ops_file: \"ops.json\""), "{text}");
    assert!(text.contains("build_file: \"build.json\""), "{text}");
    assert!(text.contains("step: 2"), "{text}");

    // serialize → parse → serialize must be a fixed point, which is what the
    // corpus test requires of every node (`project_text_format_roundtrip`).
    let again = author_and_serialize(&mut designer, &text);
    assert_eq!(again, text);
}

#[test]
fn a_property_the_text_omits_keeps_its_stored_value() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", 2),
    );

    let mut props = HashMap::new();
    props.insert(
        "step".to_string(),
        atomcad_structure_designer::text_format::TextValue::Int(1),
    );
    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| {
        data.set_text_properties(&props).expect("step is an int");
    });

    let data = node_data(&designer, node_id);
    assert_eq!(data.step, 1);
    assert!(
        data.library.is_some() && data.script.is_some(),
        "editing `step` alone must not disturb the parsed files"
    );
    assert_eq!(data.ops_file, Some(fixture("methylate_ops.json")));
}

#[test]
fn a_text_edit_that_changes_a_file_name_drops_that_cache_and_eval_re_reads() {
    // The text path has no design directory to reload from, so `eval` is what
    // re-reads a file whose cache the edit dropped. Without that fallback a
    // `query` / `--replace` round trip would leave the node unevaluatable.
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "unmatched_build.json", 1),
    );

    let mut props = HashMap::new();
    props.insert(
        "build_file".to_string(),
        atomcad_structure_designer::text_format::TextValue::String(fixture("methylate_build.json")),
    );
    with_data::<MechanosynthData, _>(&mut designer, node_id, |data| {
        data.set_text_properties(&props)
            .expect("build_file is a string");
    });

    assert!(
        node_data(&designer, node_id).script.is_none(),
        "a changed name must drop its stale cache"
    );
    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(&result, &methylate_expected(1), "after a text-format swap");
}

// ============================================================================
// Undo
// ============================================================================

#[test]
fn property_edits_are_undoable_and_undo_restores_the_caches() {
    let mut designer = setup_designer();
    let base_id = add_value_node(&mut designer, molecule_value(methane()));
    let node_id = add_mechanosynth(
        &mut designer,
        base_id,
        loaded_data("methylate_ops.json", "methylate_build.json", 3),
    );

    // Edit `step` through the same whole-data replacement the API uses.
    let stepped = {
        let current = node_data(&designer, node_id);
        let mut next = current.clone();
        next.step = 1;
        next
    };
    designer.set_node_network_data_scoped(&[], node_id, Box::new(stepped));
    assert_eq!(node_data(&designer, node_id).step, 1);

    designer.undo();
    assert_eq!(node_data(&designer, node_id).step, 3);
    designer.redo();
    assert_eq!(node_data(&designer, node_id).step, 1);

    // Now drop the build file, then undo it. The caches are `#[serde(skip)]`,
    // so the restore depends on the undo command running the node's loader.
    let cleared = node_data(&designer, node_id).with_build_file(None);
    designer.set_node_network_data_scoped(&[], node_id, Box::new(cleared));
    {
        let data = node_data(&designer, node_id);
        assert!(data.build_file.is_none() && data.script.is_none());
    }

    designer.undo();
    let restored = node_data(&designer, node_id);
    assert_eq!(restored.build_file, Some(fixture("methylate_build.json")));
    assert!(
        restored.script.is_some(),
        "undo must re-read the file, not just the name"
    );

    let result = expect_atoms(evaluate_pin(&designer, node_id, 0));
    assert_same(&result, &methylate_expected(1), "after undo");
}
