//! `import_cube` node tests (`doc/design_scalar_fields.md` P3).
//!
//! The loader itself is covered in `atomcad-crystolecule`'s
//! `io/cube_loader_test.rs`. What is tested here is the *node*: that both
//! output pins carry what they should, that a wired `file_name` overrides the
//! stored property, that the path relativizes and the `#[serde(skip)]` payload
//! reloads across a `.cnnd` round-trip, and that the units plausibility
//! warning arrives as a **non-blocking** node-data error without costing the
//! user a usable field.

use atomcad_crystolecule::field::ScalarField;
use atomcad_structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use atomcad_structure_designer::evaluator::network_result::NetworkResult;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::import_cube::ImportCubeData;
use atomcad_structure_designer::nodes::string::StringData;
use atomcad_structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_test_support::fixture_path_str;
use glam::{DVec2, DVec3};
use std::collections::HashMap;
use tempfile::tempdir;

fn cube_fixture(name: &str) -> String {
    fixture_path_str(&format!("cube/{}", name))
}

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn evaluate_pin(designer: &StructureDesigner, node_id: u64, pin_index: i32) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let stack = vec![NetworkStackElement::root(network)];
    evaluator.evaluate(&stack, node_id, pin_index, registry, false, &mut context)
}

/// Add an `import_cube` node with `file_name` stored *and* the payload loaded,
/// which is the state the `import_cube` API action leaves the node in.
fn add_loaded_import_cube_node(designer: &mut StructureDesigner, file_path: &str) -> u64 {
    let node_id = add_import_cube_node(designer, file_path);
    load_node_payload(designer, node_id, file_path);
    node_id
}

/// Add an `import_cube` node with only the stored `file_name` — no payload,
/// which is the state a freshly deserialized node has before its loader runs.
fn add_import_cube_node(designer: &mut StructureDesigner, file_path: &str) -> u64 {
    let node_id = designer.add_node("import_cube", DVec2::new(0.0, 0.0));
    with_node_data(designer, node_id, |data| {
        data.file_name = Some(file_path.to_string());
    });
    node_id
}

fn load_node_payload(designer: &mut StructureDesigner, node_id: u64, file_path: &str) {
    let cube = atomcad_crystolecule::io::cube_loader::load_cube(file_path, true)
        .expect("fixture should parse");
    let loaded = atomcad_structure_designer::nodes::import_cube::LoadedCube::from_cube_file(cube)
        .expect("fixture should carry a field");
    with_node_data(designer, node_id, |data| {
        data.loaded = Some(loaded);
    });
}

fn with_node_data<F: FnOnce(&mut ImportCubeData)>(
    designer: &mut StructureDesigner,
    node_id: u64,
    f: F,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<ImportCubeData>()
        .expect("import_cube node should carry ImportCubeData");
    f(data);
}

// ============================================================================
// Both output pins
// ============================================================================

#[test]
fn import_cube_field_pin_carries_the_fixtures_value_range() {
    let mut designer = setup_designer();
    let node_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("ramp_3x4x5.cube"));

    match evaluate_pin(&designer, node_id, 0) {
        NetworkResult::ScalarField(field) => {
            // The ramp is value(i,j,k) = 100i + 10j + k on a 3x4x5 grid, so the
            // extremes are literal and checkable by eye: 0 and 100*2+10*3+4.
            assert_eq!(field.value_range(), Some((0.0, 234.0)));
            let grid = field.native_grid().expect("a sampled field has a grid");
            assert_eq!(grid.dims, [3, 4, 5]);
            // Spot-check one interior sample through the node's own value, so
            // this test would catch a field swapped for a different one.
            assert!((field.sample(DVec3::new(1.0, 2.0, 3.0)) - 123.0).abs() < 1e-4);
        }
        other => panic!(
            "pin 0 should be a ScalarField, got {:?}",
            other.to_display_string()
        ),
    }
}

#[test]
fn import_cube_molecule_pin_matches_the_files_atom_block() {
    let mut designer = setup_designer();
    let node_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("water_bohr.cube"));

    match evaluate_pin(&designer, node_id, 1) {
        NetworkResult::Molecule(molecule) => {
            assert_eq!(molecule.atoms.get_num_of_atoms(), 3);
            // Bonds mean the Bohr→Ångström conversion landed: read as Ångström
            // the geometry would be 1.89x too small and auto-bonding would
            // produce a different picture entirely.
            assert_eq!(molecule.atoms.get_num_of_bonds(), 2);
        }
        other => panic!(
            "pin 1 should be a Molecule, got {:?}",
            other.to_display_string()
        ),
    }
}

#[test]
fn import_cube_without_a_file_errors_on_both_pins() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("import_cube", DVec2::new(0.0, 0.0));

    // Positional outputs: an error on pin 0 alone would leave pin 1 reading
    // `None` rather than the error.
    for pin in 0..2 {
        assert!(
            evaluate_pin(&designer, node_id, pin).is_error(),
            "pin {pin} should carry the error"
        );
    }
}

// ============================================================================
// The wired `file_name` input pin
// ============================================================================

#[test]
fn wired_file_name_overrides_the_stored_property() {
    let mut designer = setup_designer();
    // Stored property points at the ramp; the wire points at water. The wire
    // must win.
    let node_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("ramp_3x4x5.cube"));

    let string_id = designer.add_node("string", DVec2::new(-200.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let node = network.nodes.get_mut(&string_id).unwrap();
        if let Some(data) = node.data.as_any_mut().downcast_mut::<StringData>() {
            data.value = cube_fixture("water_bohr.cube");
        }
    }
    designer.connect_nodes(string_id, 0, node_id, 0);

    match evaluate_pin(&designer, node_id, 1) {
        NetworkResult::Molecule(molecule) => {
            assert_eq!(
                molecule.atoms.get_num_of_atoms(),
                3,
                "the wired water file should have won over the stored ramp"
            );
        }
        other => panic!(
            "pin 1 should be a Molecule, got {:?}",
            other.to_display_string()
        ),
    }

    match evaluate_pin(&designer, node_id, 0) {
        NetworkResult::ScalarField(field) => {
            let grid = field.native_grid().expect("a sampled field has a grid");
            assert_eq!(
                grid.dims,
                [5, 5, 5],
                "the field should come from the wired file too, not the ramp"
            );
        }
        other => panic!(
            "pin 0 should be a ScalarField, got {:?}",
            other.to_display_string()
        ),
    }
}

#[test]
fn a_wired_file_name_that_cannot_be_loaded_errors_on_both_pins() {
    let mut designer = setup_designer();
    let node_id = designer.add_node("import_cube", DVec2::new(0.0, 0.0));

    let string_id = designer.add_node("string", DVec2::new(-200.0, 0.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let node = network.nodes.get_mut(&string_id).unwrap();
        if let Some(data) = node.data.as_any_mut().downcast_mut::<StringData>() {
            data.value = cube_fixture("truncated.cube");
        }
    }
    designer.connect_nodes(string_id, 0, node_id, 0);

    for pin in 0..2 {
        assert!(
            evaluate_pin(&designer, node_id, pin).is_error(),
            "pin {pin} should carry the load error"
        );
    }
}

// ============================================================================
// .cnnd round-trip: relativization + payload reload
// ============================================================================

#[test]
fn cnnd_roundtrip_relativizes_the_path_and_reloads_the_payload() {
    let tmp = tempdir().expect("tempdir");
    // Put the cube file beside the project file so `try_make_relative` has
    // something to shorten.
    let cube_path = tmp.path().join("water_bohr.cube");
    std::fs::copy(cube_fixture("water_bohr.cube"), &cube_path).expect("copy fixture");
    let project_path = tmp.path().join("project.cnnd");

    let mut designer = setup_designer();
    let node_id =
        add_loaded_import_cube_node(&mut designer, cube_path.to_str().expect("utf-8 path"));
    designer.validate_active_network();

    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &project_path,
        false,
        &HashMap::new(),
    )
    .expect("save should succeed");

    // The saved path is relative, so the project survives being moved.
    let saved_name = designer
        .node_type_registry
        .node_networks
        .get("test")
        .and_then(|net| net.nodes.get(&node_id))
        .and_then(|node| node.data.as_any_ref().downcast_ref::<ImportCubeData>())
        .and_then(|data| data.file_name.clone())
        .expect("the node should still store a file name");
    assert_eq!(
        saved_name, "water_bohr.cube",
        "saving should relativize the path against the project directory"
    );

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, project_path.to_str().unwrap())
        .expect("load should succeed");

    let node = registry2
        .node_networks
        .get("test")
        .and_then(|net| net.nodes.get(&node_id))
        .expect("the import_cube node should survive the roundtrip");
    let data = node
        .data
        .as_any_ref()
        .downcast_ref::<ImportCubeData>()
        .expect("import_cube node should carry ImportCubeData");

    assert_eq!(data.file_name.as_deref(), Some("water_bohr.cube"));
    let loaded = data
        .loaded
        .as_ref()
        .expect("the loader should have repopulated the #[serde(skip)] payload");
    assert_eq!(loaded.atoms.get_num_of_atoms(), 3);
    assert!(loaded.field.value_range().is_some());
}

// ============================================================================
// The units plausibility warning
// ============================================================================

#[test]
fn an_implausible_atom_block_warns_without_costing_the_field() {
    let mut designer = setup_designer();
    let node_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("water_angstrom.cube"));

    let error = {
        let network = designer
            .node_type_registry
            .node_networks
            .get("test")
            .unwrap();
        network.nodes.get(&node_id).unwrap().data.get_data_error()
    }
    .expect("an Ångström-scaled file should produce a units warning");

    assert!(
        !error.blocking,
        "the node still produces a usable field, so the warning must not block"
    );
    assert!(
        error.message.contains("0.52"),
        "the warning should name the observed ratio: {}",
        error.message
    );

    // Non-blocking means exactly this: both pins still carry values.
    match evaluate_pin(&designer, node_id, 0) {
        NetworkResult::ScalarField(field) => {
            assert!(field.value_range().is_some());
        }
        other => panic!(
            "pin 0 should still be a ScalarField, got {:?}",
            other.to_display_string()
        ),
    }
    match evaluate_pin(&designer, node_id, 1) {
        NetworkResult::Molecule(molecule) => {
            assert_eq!(molecule.atoms.get_num_of_atoms(), 3);
        }
        other => panic!(
            "pin 1 should still be a Molecule, got {:?}",
            other.to_display_string()
        ),
    }
}

#[test]
fn a_plausible_atom_block_produces_no_data_error() {
    let mut designer = setup_designer();
    let node_id = add_loaded_import_cube_node(&mut designer, &cube_fixture("water_bohr.cube"));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let error = network.nodes.get(&node_id).unwrap().data.get_data_error();
    assert!(
        error.is_none(),
        "a correctly-written Bohr file should be silent, got {:?}",
        error.map(|e| e.message)
    );
}

#[test]
fn an_unloaded_node_reports_no_data_error() {
    // A node whose file failed to load has no payload, hence no warning — the
    // load failure surfaces through `eval`, not through the data-error channel.
    let mut designer = setup_designer();
    let node_id = add_import_cube_node(&mut designer, &cube_fixture("water_angstrom.cube"));

    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    assert!(
        network
            .nodes
            .get(&node_id)
            .unwrap()
            .data
            .get_data_error()
            .is_none()
    );
}
