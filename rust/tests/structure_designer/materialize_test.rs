//! Tests for Phase 7b: `atom_fill` was renamed to `materialize` and its
//! `motif` / `m_offset` input pins were removed. Motif and motif offset now
//! flow through the upstream Blueprint's `Structure` value.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn evaluate_raw(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
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

/// `materialize` with a Blueprint input produces `NetworkResult::Crystal`
/// whose `structure` matches the Blueprint's `structure` and whose carved
/// atoms are non-empty.
#[test]
fn materialize_outputs_crystal_with_blueprint_structure() {
    let network_name = "test_materialize_crystal";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let fill_id = designer.add_node("materialize", DVec2::new(300.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, fill_id, 0);

    let blueprint_result = evaluate_raw(&designer, network_name, cuboid_id);
    let blueprint = match blueprint_result {
        NetworkResult::Blueprint(bp) => bp,
        other => panic!(
            "Expected cuboid to produce Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    let fill_result = evaluate_raw(&designer, network_name, fill_id);
    let crystal = match fill_result {
        NetworkResult::Crystal(c) => c,
        NetworkResult::Error(e) => panic!("materialize returned error: {}", e),
        other => panic!(
            "Expected materialize to produce Crystal, got {:?}",
            other.infer_data_type()
        ),
    };

    assert!(
        crystal
            .structure
            .lattice_vecs
            .is_approximately_equal(&blueprint.structure.lattice_vecs),
        "Crystal lattice_vecs should match the Blueprint's lattice_vecs"
    );
    assert!(
        crystal.geo_tree_root.is_some(),
        "Crystal should carry the Blueprint's geo_tree_root"
    );
    assert!(
        crystal.atoms.get_num_of_atoms() > 0,
        "materialize over a default cuboid should carve at least one atom"
    );
}

/// `materialize`'s output pin is declared as `Crystal`.
#[test]
fn materialize_node_type_output_is_crystal() {
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("materialize").unwrap();
    assert_eq!(node_type.output_pins.len(), 1);
    assert_eq!(
        node_type.output_pins[0].fixed_type(),
        Some(&DataType::Crystal),
        "materialize output pin should be Fixed(Crystal)"
    );
}

/// `materialize` exposes only `shape`, `passivate`, `rm_single`,
/// `surf_recon`, `invert_phase` — no `motif` or `m_offset` pins.
#[test]
fn materialize_has_no_motif_or_offset_pins() {
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("materialize").unwrap();
    let pin_names: Vec<&str> = node_type
        .parameters
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        pin_names,
        vec![
            "shape",
            "passivate",
            "rm_single",
            "surf_recon",
            "invert_phase"
        ],
        "materialize parameter pins"
    );
}
