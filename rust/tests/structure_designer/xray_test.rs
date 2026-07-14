//! Phase 2 of `doc/design_xray_node.md` — the `xray` region-gated
//! semi-transparency node. Covers: alpha applied to all atoms with the
//! `region` pin disconnected and only in-region atoms when wired; the wired
//! `alpha` pin overriding the stored property; `alpha = 1.0` clearing
//! previously recorded alphas (last-writer-wins composition, including
//! disjoint and overlapping chained regions); out-of-range property clamping;
//! concrete-phase pass-through; and localized errors on bad input types.

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::geo_tree::GeoNode;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    BlueprintData, CrystalData, MoleculeData, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;
use rust_lib_flutter_cad::structure_designer::nodes::xray::XrayData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn add_value_node(
    designer: &mut StructureDesigner,
    network_name: &str,
    pos: DVec2,
    value: NetworkResult,
) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    network.add_node("value", pos, 0, Box::new(ValueData { value }))
}

fn molecule_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Molecule(MoleculeData {
        atoms: structure,
        geo_tree_root: None,
    })
}

fn crystal_value(structure: AtomicStructure) -> NetworkResult {
    NetworkResult::Crystal(CrystalData {
        structure: Structure::diamond(),
        atoms: structure,
        geo_tree_root: None,
        alignment: Default::default(),
        alignment_reason: None,
    })
}

/// Wrap a `GeoNode` SDF as a region Blueprint value (structure is ignored).
fn blueprint_value(geo_tree_root: GeoNode) -> NetworkResult {
    NetworkResult::Blueprint(BlueprintData {
        structure: Structure::diamond(),
        geo_tree_root,
        alignment: Default::default(),
        alignment_reason: None,
    })
}

fn add_xray(designer: &mut StructureDesigner, pos: DVec2) -> u64 {
    designer.add_node("xray", pos)
}

/// Sets the stored `alpha` property on an xray node via text properties.
fn set_alpha_property(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    value: f64,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let mut props = HashMap::new();
    props.insert("alpha".to_string(), TextValue::Float(value));
    node.data.set_text_properties(&props).unwrap();
}

fn evaluate(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn evaluate_to_atomic(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
) -> AtomicStructure {
    match evaluate(designer, network_name, node_id) {
        NetworkResult::Crystal(c) => c.atoms,
        NetworkResult::Molecule(m) => m.atoms,
        NetworkResult::Error(e) => panic!("Expected Atomic result, got Error: {}", e),
        other => panic!("Expected Atomic result, got {:?}", other.infer_data_type()),
    }
}

/// A line of carbons along +x at the given x coordinates.
fn carbons_at(xs: &[f64]) -> AtomicStructure {
    let mut s = AtomicStructure::new();
    for &x in xs {
        s.add_atom(6, DVec3::new(x, 0.0, 0.0));
    }
    s
}

fn alpha_at(s: &AtomicStructure, x: f64) -> f32 {
    s.iter_atoms()
        .find(|(_, a)| (a.position.x - x).abs() < 1e-6)
        .map(|(id, _)| s.get_atom_alpha(*id))
        .unwrap_or_else(|| panic!("no atom near x={}", x))
}

/// Half-space whose in-region (`sdf ≤ margin`) side is `x ≤ 0`.
fn region_x_le_0() -> NetworkResult {
    blueprint_value(GeoNode::half_space(DVec3::X, DVec3::ZERO))
}

// ============================================================================
// Region gating
// ============================================================================

/// Disconnected `region` → every atom gets the (default 0.5) alpha.
#[test]
fn xray_region_disconnected_applies_to_all() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let xray_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, xray_id, 0);

    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, -1.0), 0.5, "default alpha on all atoms");
    assert_eq!(alpha_at(&result, 1.0), 0.5, "default alpha on all atoms");
}

/// With a half-space region, only in-region atoms get the alpha; the rest
/// stay fully opaque.
#[test]
fn xray_region_applies_only_in_region() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let xray_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, xray_id, 0);
    set_alpha_property(&mut designer, net, xray_id, 0.3);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, xray_id, 2);

    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, -1.0), 0.3, "in-region atom ghosted");
    assert_eq!(alpha_at(&result, 1.0), 1.0, "out-of-region atom opaque");
}

// ============================================================================
// Alpha resolution: wired pin > stored property; clamping
// ============================================================================

/// A wired `alpha` pin (pin 1) overrides the stored property.
#[test]
fn xray_wired_alpha_overrides_property() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let xray_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, xray_id, 0);
    set_alpha_property(&mut designer, net, xray_id, 0.9);
    let alpha_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        NetworkResult::Float(0.2),
    );
    designer.connect_nodes(alpha_id, 0, xray_id, 1);

    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert!(
        (alpha_at(&result, 0.0) - 0.2).abs() < 1e-6,
        "wired 0.2 wins over stored 0.9"
    );
}

/// Out-of-range values clamp to [0, 1]: negative → 0.0, above 1.0 → removal
/// (fully opaque).
#[test]
fn xray_out_of_range_values_clamp() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let xray_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, xray_id, 0);

    set_alpha_property(&mut designer, net, xray_id, -0.5);
    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, 0.0), 0.0, "negative clamps to 0.0");

    set_alpha_property(&mut designer, net, xray_id, 1.5);
    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, 0.0), 1.0, "above 1.0 clamps to opaque");
}

// ============================================================================
// Composition — chaining, last-writer-wins, 1.0 clears
// ============================================================================

/// Chain a no-region xray at 0.3 into a region-gated xray at 1.0 → in-region
/// atoms are re-opaqued, out-of-region atoms keep 0.3.
#[test]
fn xray_alpha_one_clears_previous_recording() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0, 1.0])),
    );
    let ghost_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, ghost_id, 0);
    set_alpha_property(&mut designer, net, ghost_id, 0.3);

    let unghost_id = add_xray(&mut designer, DVec2::new(400.0, 0.0));
    designer.connect_nodes(ghost_id, 0, unghost_id, 0);
    set_alpha_property(&mut designer, net, unghost_id, 1.0);
    let region_id = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_id, 0, unghost_id, 2);

    let result = evaluate_to_atomic(&designer, net, unghost_id);
    assert_eq!(alpha_at(&result, -1.0), 1.0, "in-region atom re-opaqued");
    assert_eq!(alpha_at(&result, 1.0), 0.3, "out-of-region keeps ghost");
}

/// Two chained xray nodes with different alphas and disjoint regions leave
/// both values on their respective atoms.
#[test]
fn xray_disjoint_regions_coexist() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-2.0, 0.0, 2.0])),
    );

    // Region A: x ≤ -1 at alpha 0.2.
    let node_a = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, node_a, 0);
    set_alpha_property(&mut designer, net, node_a, 0.2);
    let region_a = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        blueprint_value(GeoNode::half_space(DVec3::X, DVec3::new(-1.0, 0.0, 0.0))),
    );
    designer.connect_nodes(region_a, 0, node_a, 2);

    // Region B: x ≥ 1 at alpha 0.7.
    let node_b = add_xray(&mut designer, DVec2::new(400.0, 0.0));
    designer.connect_nodes(node_a, 0, node_b, 0);
    set_alpha_property(&mut designer, net, node_b, 0.7);
    let region_b = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 400.0),
        blueprint_value(GeoNode::half_space(DVec3::NEG_X, DVec3::new(1.0, 0.0, 0.0))),
    );
    designer.connect_nodes(region_b, 0, node_b, 2);

    let result = evaluate_to_atomic(&designer, net, node_b);
    assert_eq!(alpha_at(&result, -2.0), 0.2, "region A alpha survives");
    assert_eq!(alpha_at(&result, 2.0), 0.7, "region B alpha applied");
    assert_eq!(alpha_at(&result, 0.0), 1.0, "middle atom untouched");
}

/// Overlapping regions → the downstream node's value wins (last-writer-wins).
#[test]
fn xray_overlapping_regions_downstream_wins() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[-1.0])),
    );

    let node_a = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, node_a, 0);
    set_alpha_property(&mut designer, net, node_a, 0.2);
    let region_a = add_value_node(&mut designer, net, DVec2::new(0.0, 200.0), region_x_le_0());
    designer.connect_nodes(region_a, 0, node_a, 2);

    let node_b = add_xray(&mut designer, DVec2::new(400.0, 0.0));
    designer.connect_nodes(node_a, 0, node_b, 0);
    set_alpha_property(&mut designer, net, node_b, 0.6);
    let region_b = add_value_node(&mut designer, net, DVec2::new(0.0, 400.0), region_x_le_0());
    designer.connect_nodes(region_b, 0, node_b, 2);

    let result = evaluate_to_atomic(&designer, net, node_b);
    assert_eq!(alpha_at(&result, -1.0), 0.6, "downstream value wins");
}

// ============================================================================
// Phase pass-through and error localization
// ============================================================================

/// Crystal in → Crystal out; Molecule in → Molecule out. Alphas recorded in
/// both phases.
#[test]
fn xray_concrete_phase_passes_through() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);

    let crystal_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        crystal_value(carbons_at(&[0.0])),
    );
    let xray_c = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(crystal_id, 0, xray_c, 0);
    let result = evaluate(&designer, net, xray_c);
    match &result {
        NetworkResult::Crystal(c) => {
            assert_eq!(c.atoms.get_atom_alpha(1), 0.5, "alpha recorded on crystal");
        }
        other => panic!(
            "Crystal in must come out Crystal, got {:?}",
            other.infer_data_type()
        ),
    }

    let molecule_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        molecule_value(carbons_at(&[0.0])),
    );
    let xray_m = add_xray(&mut designer, DVec2::new(200.0, 200.0));
    designer.connect_nodes(molecule_id, 0, xray_m, 0);
    let result = evaluate(&designer, net, xray_m);
    match &result {
        NetworkResult::Molecule(m) => {
            assert_eq!(m.atoms.get_atom_alpha(1), 0.5, "alpha recorded on molecule");
        }
        other => panic!(
            "Molecule in must come out Molecule, got {:?}",
            other.infer_data_type()
        ),
    }
}

/// Non-atomic input on pin 0 → localized `NetworkResult::Error`.
#[test]
fn xray_non_atomic_input_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(&mut designer, net, DVec2::ZERO, NetworkResult::Float(1.0));
    let xray_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, xray_id, 0);

    let result = evaluate(&designer, net, xray_id);
    assert!(
        matches!(result, NetworkResult::Error(_)),
        "expected Error, got {:?}",
        result.infer_data_type()
    );
}

// ============================================================================
// Phase 6 — API-level setter is undoable/redoable
// ============================================================================

/// Setting `alpha` through the `StructureDesigner`-level node-data setter
/// re-evaluates with the new value and is undoable/redoable — the same shared
/// `SetNodeDataCommand` path the FRB `set_xray_data` wrapper uses.
#[test]
fn xray_set_data_is_undoable() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let xray_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, xray_id, 0);

    // Default stored alpha is 0.5.
    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, 0.0), 0.5, "default stored alpha");

    // Edit through the StructureDesigner-level setter (what the FRB API wraps).
    designer.set_node_network_data_scoped(&[], xray_id, Box::new(XrayData { alpha: 0.25 }));
    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, 0.0), 0.25, "setter applies new alpha");

    // Undo restores the previous alpha.
    assert!(designer.undo(), "undo should report a change");
    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, 0.0), 0.5, "undo restores previous alpha");

    // Redo re-applies the edit.
    assert!(designer.redo(), "redo should report a change");
    let result = evaluate_to_atomic(&designer, net, xray_id);
    assert_eq!(alpha_at(&result, 0.0), 0.25, "redo re-applies alpha");
}

/// Non-Blueprint on the `region` pin → localized `NetworkResult::Error`.
#[test]
fn xray_non_blueprint_region_errors() {
    let net = "test";
    let mut designer = setup_designer_with_network(net);
    let value_id = add_value_node(
        &mut designer,
        net,
        DVec2::ZERO,
        molecule_value(carbons_at(&[0.0])),
    );
    let xray_id = add_xray(&mut designer, DVec2::new(200.0, 0.0));
    designer.connect_nodes(value_id, 0, xray_id, 0);
    let region_id = add_value_node(
        &mut designer,
        net,
        DVec2::new(0.0, 200.0),
        molecule_value(carbons_at(&[5.0])),
    );
    designer.connect_nodes(region_id, 0, xray_id, 2);

    let result = evaluate(&designer, net, xray_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("xray.region"),
                "error should be localized to xray.region, got: {}",
                msg
            );
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}
