//! Tests for the `with_structure` node (Blueprint + Structure -> Blueprint).

use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    Alignment, NetworkResult,
};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
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

fn set_node_data<T: NodeData + 'static>(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: T,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(data);
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        node,
        true,
    );
}

#[test]
fn with_structure_node_type_signature() {
    let registry = NodeTypeRegistry::new();
    let node_type = registry.get_node_type("with_structure").unwrap();
    assert_eq!(node_type.parameters.len(), 2);
    assert_eq!(node_type.parameters[0].name, "shape");
    assert_eq!(node_type.parameters[0].data_type, DataType::Blueprint);
    assert_eq!(node_type.parameters[1].name, "structure");
    assert_eq!(node_type.parameters[1].data_type, DataType::Structure);
    assert_eq!(node_type.output_pins.len(), 1);
    assert_eq!(
        node_type.output_pins[0].fixed_type(),
        Some(&DataType::Blueprint),
    );
}

/// Blueprint in + Structure differing in motif_offset -> Blueprint out carries
/// the new Structure, `geo_tree_root` preserved bit-for-bit, alignment degraded
/// to `LatticeUnaligned` (see doc/design_blueprint_alignment.md §10).
#[test]
fn with_structure_replaces_structure_and_preserves_geo_tree_root() {
    let network_name = "test_with_structure_ok";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    // shape input: a cuboid Blueprint.
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);

    // structure input: a `structure` node with a non-zero motif_offset override
    // (produced by wiring a vec3 literal into the offset pin). This gives us a
    // Structure that differs from the Blueprint's diamond default on
    // motif_offset, so we can detect substitution and alignment degradation.
    let off_id = designer.add_node("vec3", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        network_name,
        off_id,
        Vec3Data {
            value: DVec3::new(0.25, 0.5, 0.75),
        },
    );
    let st_id = designer.add_node("structure", DVec2::new(200.0, 200.0));
    designer.connect_nodes(off_id, 0, st_id, 3); // pin 3 = motif_offset

    // with_structure: shape=cuboid, structure=st.
    let ws_id = designer.add_node("with_structure", DVec2::new(400.0, 100.0));
    designer.connect_nodes(cuboid_id, 0, ws_id, 0);
    designer.connect_nodes(st_id, 0, ws_id, 1);
    designer.validate_active_network();

    // Capture the cuboid Blueprint for geo_tree_root comparison.
    let cuboid_bp = match evaluate_raw(&designer, network_name, cuboid_id) {
        NetworkResult::Blueprint(bp) => bp,
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    };
    let cuboid_geo_hash = *cuboid_bp.geo_tree_root.hash();

    // Evaluate with_structure.
    let result = evaluate_raw(&designer, network_name, ws_id);
    let out_bp = match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("with_structure returned error: {}", e),
        other => panic!(
            "with_structure expected Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    // Structure field was replaced: motif_offset now reflects the override.
    assert_eq!(
        out_bp.structure.motif_offset,
        DVec3::new(0.25, 0.5, 0.75),
        "with_structure should replace the Blueprint's Structure",
    );

    // geo_tree_root is preserved bit-for-bit (blake3 hash equality).
    assert_eq!(
        *out_bp.geo_tree_root.hash(),
        cuboid_geo_hash,
        "with_structure must preserve geo_tree_root unchanged",
    );

    // motif_offset differs from the input Blueprint's diamond default (zero),
    // so alignment must degrade to LatticeUnaligned.
    assert_eq!(out_bp.alignment, Alignment::LatticeUnaligned);
    assert!(
        out_bp
            .alignment_reason
            .as_deref()
            .is_some_and(|r| r.contains("with_structure")),
        "alignment_reason should cite with_structure, got {:?}",
        out_bp.alignment_reason
    );
}

/// When the replacement Structure is approximately equal to the Blueprint's
/// existing Structure, alignment is a pure pass-through — no degradation.
#[test]
fn with_structure_identical_structure_preserves_alignment() {
    let network_name = "test_with_structure_identity";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    // shape input: cuboid -> Blueprint with diamond Structure, Aligned.
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);

    // structure input: bare `structure` node -> also diamond.
    let st_id = designer.add_node("structure", DVec2::new(0.0, 200.0));

    let ws_id = designer.add_node("with_structure", DVec2::new(400.0, 100.0));
    designer.connect_nodes(cuboid_id, 0, ws_id, 0);
    designer.connect_nodes(st_id, 0, ws_id, 1);
    designer.validate_active_network();

    let cuboid_bp = match evaluate_raw(&designer, network_name, cuboid_id) {
        NetworkResult::Blueprint(bp) => bp,
        other => panic!("expected Blueprint, got {:?}", other.infer_data_type()),
    };

    let result = evaluate_raw(&designer, network_name, ws_id);
    let out_bp = match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("with_structure returned error: {}", e),
        other => panic!(
            "with_structure expected Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    // Identical Structure -> no alignment change, no reason emitted.
    assert_eq!(out_bp.alignment, cuboid_bp.alignment);
    assert_eq!(out_bp.alignment_reason, cuboid_bp.alignment_reason);
    assert_eq!(out_bp.alignment, Alignment::Aligned);
}

/// When only the motif differs (lattice_vecs and motif_offset equal),
/// alignment degrades to `MotifUnaligned` — the lattice is preserved but the
/// motif no longer maps to itself.
#[test]
fn with_structure_motif_only_difference_is_motif_unaligned() {
    use rust_lib_flutter_cad::crystolecule::motif::Motif;
    use rust_lib_flutter_cad::crystolecule::structure::Structure;
    use rust_lib_flutter_cad::structure_designer::evaluator::network_result::BlueprintData;
    use rust_lib_flutter_cad::structure_designer::nodes::value::ValueData;

    let network_name = "test_with_structure_motif_only";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);

    // Build a Structure that shares the Blueprint's lattice_vecs and
    // motif_offset but has an empty motif (differs from the default
    // zincblende motif carried by the cuboid's Blueprint). Fed via a `value`
    // node carrying the literal NetworkResult::Structure.
    let diamond = Structure::diamond();
    let mutant = Structure {
        lattice_vecs: diamond.lattice_vecs.clone(),
        motif: Motif {
            parameters: vec![],
            sites: vec![],
            bonds: vec![],
            bonds_by_site1_index: vec![],
            bonds_by_site2_index: vec![],
        },
        motif_offset: diamond.motif_offset,
    };
    let value_id = designer.add_node("value", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        network_name,
        value_id,
        ValueData {
            value: NetworkResult::Structure(mutant),
        },
    );

    let ws_id = designer.add_node("with_structure", DVec2::new(400.0, 100.0));
    designer.connect_nodes(cuboid_id, 0, ws_id, 0);

    // `value` node's output_type is `None`; bypass the static validator to
    // plumb the literal Structure into the `with_structure` pin 1.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.connect_nodes(value_id, 0, ws_id, 1, false);
    }

    let result = evaluate_raw(&designer, network_name, ws_id);
    let out_bp: BlueprintData = match result {
        NetworkResult::Blueprint(bp) => bp,
        NetworkResult::Error(e) => panic!("with_structure returned error: {}", e),
        other => panic!(
            "with_structure expected Blueprint, got {:?}",
            other.infer_data_type()
        ),
    };

    assert_eq!(
        out_bp.alignment,
        Alignment::MotifUnaligned,
        "motif-only difference must degrade to MotifUnaligned, not LatticeUnaligned",
    );
    assert!(
        out_bp
            .alignment_reason
            .as_deref()
            .is_some_and(|r| r.contains("motif")),
        "alignment_reason should mention the motif change, got {:?}",
        out_bp.alignment_reason
    );
}

/// The static validator must reject Crystal on the `shape` pin: Blueprint is
/// concrete and has no conversion from Crystal (abstract upcast only goes
/// Blueprint/Crystal -> HasStructure, not Crystal -> Blueprint).
#[test]
fn with_structure_validator_rejects_crystal_shape_source() {
    let network_name = "test_with_structure_reject_crystal";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);

    let ws_id = designer.add_node("with_structure", DVec2::new(400.0, 0.0));

    assert!(
        !designer.can_connect_nodes(mat_id, 0, ws_id, 0),
        "Crystal output must not be connectable to with_structure's Blueprint shape pin",
    );
}

/// The static validator must reject non-Structure sources on the `structure`
/// pin (Structure is concrete; e.g. a Motif must not be accepted).
#[test]
fn with_structure_validator_rejects_motif_structure_source() {
    let network_name = "test_with_structure_reject_motif";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let motif_id = designer.add_node("motif", DVec2::ZERO);
    let ws_id = designer.add_node("with_structure", DVec2::new(300.0, 0.0));

    assert!(
        !designer.can_connect_nodes(motif_id, 0, ws_id, 1),
        "Motif output must not be connectable to with_structure's Structure pin",
    );
}

/// Defense-in-depth: feed a Crystal (non-Blueprint) `NetworkResult` into the
/// `shape` pin by bypassing the static validator via raw
/// `NodeNetwork::connect_nodes`, and assert eval returns a runtime type error
/// on input 0.
#[test]
fn with_structure_eval_rejects_crystal_shape_input() {
    let network_name = "test_with_structure_eval_reject_crystal";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    // Build a Crystal source: cuboid -> materialize.
    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let mat_id = designer.add_node("materialize", DVec2::new(200.0, 0.0));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0);

    // Build a valid Structure source.
    let st_id = designer.add_node("structure", DVec2::new(0.0, 200.0));

    let ws_id = designer.add_node("with_structure", DVec2::new(400.0, 100.0));

    // Bypass the static validator for the Crystal -> Blueprint connection.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.connect_nodes(mat_id, 0, ws_id, 0, false);
    }
    // Legitimate Structure wire on pin 1.
    designer.connect_nodes(st_id, 0, ws_id, 1);

    let result = evaluate_raw(&designer, network_name, ws_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("runtime type error") && msg.contains("0 indexed"),
                "expected runtime type error on input 0, got: {msg}",
            );
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}

/// Defense-in-depth: feed a non-Structure NetworkResult (here, an `int`) into
/// the `structure` pin by bypassing the static validator, and assert eval
/// returns a runtime type error on input 1.
#[test]
fn with_structure_eval_rejects_non_structure_input() {
    let network_name = "test_with_structure_eval_reject_structure";
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));

    let cuboid_id = designer.add_node("cuboid", DVec2::ZERO);
    let int_id = designer.add_node("int", DVec2::new(0.0, 200.0));
    let ws_id = designer.add_node("with_structure", DVec2::new(400.0, 100.0));

    // Valid Blueprint wire on pin 0.
    designer.connect_nodes(cuboid_id, 0, ws_id, 0);
    // Bypass the static validator for the Int -> Structure connection.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut(network_name)
            .unwrap();
        network.connect_nodes(int_id, 0, ws_id, 1, false);
    }

    let result = evaluate_raw(&designer, network_name, ws_id);
    match result {
        NetworkResult::Error(msg) => {
            assert!(
                msg.contains("runtime type error") && msg.contains("1 indexed"),
                "expected runtime type error on input 1, got: {msg}",
            );
        }
        other => panic!("expected Error, got {:?}", other.infer_data_type()),
    }
}
