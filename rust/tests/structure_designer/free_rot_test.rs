//! Phase 2 tests for issue #384: `free_rot`'s angle input switches from
//! radians to degrees (`angle` → `angle_degrees`).
//!
//! See `doc/design_degree_angle_inputs.md`.

use glam::f64::{DVec2, DVec3};
use std::collections::{HashMap, HashSet};
use std::f64::consts::FRAC_PI_2;

use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_network_gadget::NodeNetworkGadget;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::expr::{ExprData, ExprParameter};
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::nodes::free_rot::{FreeRotData, FreeRotGadget};
use rust_lib_flutter_cad::structure_designer::nodes::import_xyz::ImportXYZData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;

fn setup_designer(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn evaluate_raw(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn molecule(result: NetworkResult) -> MoleculeData {
    match result {
        NetworkResult::Molecule(m) => m,
        NetworkResult::Error(e) => panic!("expected Molecule, got Error: {}", e),
        other => panic!("expected Molecule, got {:?}", other.infer_data_type()),
    }
}

/// Add an `import_xyz` node preloaded with a single carbon atom at `position`.
/// This is the simplest way to inject a Molecule with a *known* atom position
/// into the network without touching the filesystem.
fn add_single_atom_source(
    designer: &mut StructureDesigner,
    network_name: &str,
    position: DVec3,
) -> u64 {
    let id = designer.add_node("import_xyz", DVec2::ZERO);
    let mut atoms = AtomicStructure::new();
    atoms.add_atom(6, position);

    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&id).unwrap();
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<ImportXYZData>()
        .unwrap();
    data.file_name = Some("test.xyz".to_string());
    data.atomic_structure = Some(atoms);
    id
}

fn single_atom_position(result: NetworkResult) -> DVec3 {
    let mol = molecule(result);
    let (_, atom) = mol
        .atoms
        .iter_atoms()
        .next()
        .expect("molecule should have one atom");
    atom.position
}

fn set_free_rot(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    angle_degrees: f64,
    rot_axis: DVec3,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let data = node
        .data
        .as_any_mut()
        .downcast_mut::<FreeRotData>()
        .unwrap();
    data.angle_degrees = angle_degrees;
    data.rot_axis = rot_axis;
    data.pivot_point = DVec3::ZERO;
}

/// Add a top-level `expr` node computing `degrees(x)` (one `Float` parameter
/// `x`). Mirrors the node the v5→v6 migration synthesizes on a wired angle pin.
fn add_degrees_expr(designer: &mut StructureDesigner, network_name: &str) -> u64 {
    let id = designer.add_node("expr", DVec2::new(150.0, 100.0));
    let mut expr_data = ExprData {
        parameters: vec![ExprParameter {
            id: None,
            name: "x".to_string(),
            data_type: DataType::Float,
            data_type_str: None,
        }],
        expression: "degrees(x)".to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = expr_data.parse_and_validate(0);

    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&id).unwrap();
    node.data = Box::new(expr_data);
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
    id
}

// ---- Eval semantics: the load-bearing assertion of the whole change ----

/// The golden equivalence proof for the v5→v6 migration: a radian value that a
/// pre-v6 file fed straight into `free_rot.angle` still produces the SAME
/// rotation once the migration splices a `degrees(x)` node onto the wire. Builds
/// the exact post-migration topology (`float(radians) → degrees(x) → free_rot`)
/// and checks a known atom rotates by the original radian amount.
#[test]
fn synthesized_degrees_node_preserves_radian_era_rotation() {
    let mut designer = setup_designer("t");
    let src = add_single_atom_source(&mut designer, "t", DVec3::new(1.0, 0.0, 0.0));
    let rot = designer.add_node("free_rot", DVec2::new(400.0, 0.0));
    // Stored angle is irrelevant — the wire drives it.
    set_free_rot(&mut designer, "t", rot, 0.0, DVec3::new(0.0, 0.0, 1.0));
    designer.connect_nodes(src, 0, rot, 0);

    // A pre-v6 file carried PI/2 *radians* on this wire.
    let float_id = designer.add_node("float", DVec2::new(0.0, 150.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let node = network.nodes.get_mut(&float_id).unwrap();
        let data = node.data.as_any_mut().downcast_mut::<FloatData>().unwrap();
        data.value = FRAC_PI_2;
    }

    // The synthesized degrees(x) node converts radians → degrees on the wire.
    let expr_id = add_degrees_expr(&mut designer, "t");
    designer.connect_nodes(float_id, 0, expr_id, 0);
    designer.connect_nodes(expr_id, 0, rot, 1);

    // PI/2 radians → degrees → a quarter turn about Z: (1,0,0) → (0,1,0).
    let pos = single_atom_position(evaluate_raw(&designer, "t", rot));
    assert!(
        (pos - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9,
        "radian-era PI/2 through the synthesized degrees(x) node must still be a \
         quarter turn: expected (0,1,0), got {:?}",
        pos
    );
}

#[test]
fn free_rot_stored_angle_degrees_90_rotates_quarter_turn() {
    let mut designer = setup_designer("t");
    // Atom at (1, 0, 0); a 90° rotation about +Z should land it at (0, 1, 0).
    // (If the value were still interpreted as radians, 90 rad would land it
    // somewhere else entirely — this is exactly what the switch must prevent.)
    let src = add_single_atom_source(&mut designer, "t", DVec3::new(1.0, 0.0, 0.0));
    let rot = designer.add_node("free_rot", DVec2::new(300.0, 0.0));
    set_free_rot(&mut designer, "t", rot, 90.0, DVec3::new(0.0, 0.0, 1.0));
    designer.connect_nodes(src, 0, rot, 0);

    let pos = single_atom_position(evaluate_raw(&designer, "t", rot));
    assert!(
        (pos - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9,
        "expected (0, 1, 0) after a 90° rotation about Z, got {:?}",
        pos
    );
}

#[test]
fn free_rot_wired_angle_matches_stored_value() {
    // A wired `float(90)` into the `angle` pin must produce the same result as
    // storing `angle_degrees = 90`, and must override a divergent stored value.
    let mut designer = setup_designer("t");
    let src = add_single_atom_source(&mut designer, "t", DVec3::new(1.0, 0.0, 0.0));
    let rot = designer.add_node("free_rot", DVec2::new(300.0, 0.0));
    // Stored value deliberately wrong (0°) to prove the pin wins.
    set_free_rot(&mut designer, "t", rot, 0.0, DVec3::new(0.0, 0.0, 1.0));
    designer.connect_nodes(src, 0, rot, 0);

    let float_id = designer.add_node("float", DVec2::new(150.0, 100.0));
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("t")
            .unwrap();
        let node = network.nodes.get_mut(&float_id).unwrap();
        let data = node.data.as_any_mut().downcast_mut::<FloatData>().unwrap();
        data.value = 90.0;
    }
    designer.connect_nodes(float_id, 0, rot, 1);

    let pos = single_atom_position(evaluate_raw(&designer, "t", rot));
    assert!(
        (pos - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9,
        "wired float(90)° should rotate (1,0,0) to (0,1,0), got {:?}",
        pos
    );
}

// ---- Text format ----

#[test]
fn free_rot_text_properties_roundtrip_angle_degrees() {
    let mut data = FreeRotData {
        angle_degrees: 0.0,
        rot_axis: DVec3::new(0.0, 0.0, 1.0),
        pivot_point: DVec3::ZERO,
    };

    let mut props = HashMap::new();
    props.insert("angle_degrees".to_string(), TextValue::Float(90.0));
    data.set_text_properties(&props).unwrap();
    assert_eq!(data.angle_degrees, 90.0);

    // And it serializes back under the same key.
    let serialized = data.get_text_properties();
    let (_, value) = serialized
        .iter()
        .find(|(name, _)| name == "angle_degrees")
        .expect("get_text_properties should emit angle_degrees");
    match value {
        TextValue::Float(f) => assert_eq!(*f, 90.0),
        other => panic!("expected Float, got {:?}", other),
    }
    // The radian-era key must no longer be emitted.
    assert!(
        !serialized.iter().any(|(name, _)| name == "angle"),
        "get_text_properties must not emit the old `angle` key"
    );
}

#[test]
fn free_rot_set_text_properties_rejects_old_angle_key() {
    let mut data = FreeRotData {
        angle_degrees: 0.0,
        rot_axis: DVec3::new(0.0, 0.0, 1.0),
        pivot_point: DVec3::ZERO,
    };

    let mut props = HashMap::new();
    props.insert("angle".to_string(), TextValue::Float(90.0));
    let err = data
        .set_text_properties(&props)
        .expect_err("old `angle` key must be rejected, not silently ignored");
    assert!(
        err.contains("angle_degrees") && err.contains("degrees"),
        "error should point the user at angle_degrees, got: {}",
        err
    );
    // The stored value must be untouched by the rejected write.
    assert_eq!(data.angle_degrees, 0.0);
}

// ---- Subtitle ----

#[test]
fn free_rot_subtitle_renders_stored_degrees_directly() {
    let data = FreeRotData {
        angle_degrees: 90.0,
        rot_axis: DVec3::new(0.0, 0.0, 1.0),
        pivot_point: DVec3::ZERO,
    };
    let subtitle = data.get_subtitle(&HashSet::new()).unwrap();
    assert!(
        subtitle.contains("90.0°"),
        "subtitle should show 90.0° (no double conversion), got: {}",
        subtitle
    );
}

// ---- Gadget boundary ----

#[test]
fn free_rot_gadget_sync_writes_degrees() {
    // The gadget's internal `angle` is radians; sync_data must convert to
    // degrees when writing back to the node data.
    let gadget = FreeRotGadget::new(
        std::f64::consts::FRAC_PI_2, // 90° in radians
        DVec3::new(0.0, 0.0, 1.0),
        DVec3::ZERO,
    );
    let mut data = FreeRotData {
        angle_degrees: 0.0,
        rot_axis: DVec3::new(0.0, 0.0, 1.0),
        pivot_point: DVec3::ZERO,
    };
    gadget.sync_data(&mut data);
    assert!(
        (data.angle_degrees - 90.0).abs() < 1e-9,
        "sync_data should write 90 degrees for a PI/2 gadget angle, got {}",
        data.angle_degrees
    );
}
