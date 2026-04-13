use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::crystolecule::crystolecule_constants::DIAMOND_UNIT_CELL_SIZE_ANGSTROM;
use rust_lib_flutter_cad::crystolecule::structure::Structure;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::lattice_vecs::LatticeVecsData;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_designer() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn evaluate(designer: &StructureDesigner, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
}

fn set_node_data<T: rust_lib_flutter_cad::structure_designer::node_data::NodeData + 'static>(
    designer: &mut StructureDesigner,
    node_id: u64,
    data: T,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut("test").unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = Box::new(data);
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        node,
        true,
    );
}

fn extract_structure(result: NetworkResult) -> Structure {
    match result {
        NetworkResult::Structure(s) => s,
        other => panic!(
            "Expected Structure result, got {}",
            other.to_display_string()
        ),
    }
}

#[test]
fn bare_structure_node_outputs_diamond_defaults() {
    let mut designer = setup_designer();
    let id = designer.add_node("structure", DVec2::ZERO);

    let result = evaluate(&designer, id);
    let s = extract_structure(result);
    let diamond = Structure::diamond();

    assert_eq!(s.lattice_vecs.cell_length_a, DIAMOND_UNIT_CELL_SIZE_ANGSTROM);
    assert_eq!(s.lattice_vecs.cell_length_b, DIAMOND_UNIT_CELL_SIZE_ANGSTROM);
    assert_eq!(s.lattice_vecs.cell_length_c, DIAMOND_UNIT_CELL_SIZE_ANGSTROM);
    assert_eq!(s.motif_offset, DVec3::ZERO);
    assert_eq!(s.motif.sites.len(), diamond.motif.sites.len());
}

#[test]
fn lattice_vecs_override_replaces_only_lattice() {
    let mut designer = setup_designer();
    let lv_id = designer.add_node("lattice_vecs", DVec2::ZERO);
    set_node_data(
        &mut designer,
        lv_id,
        LatticeVecsData {
            cell_length_a: 5.0,
            cell_length_b: 5.0,
            cell_length_c: 5.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        },
    );
    designer.validate_active_network();

    let st_id = designer.add_node("structure", DVec2::new(200.0, 0.0));
    // `structure` parameters: 0=structure, 1=lattice_vecs, 2=motif, 3=motif_offset
    designer.connect_nodes(lv_id, 0, st_id, 1);

    let s = extract_structure(evaluate(&designer, st_id));
    assert_eq!(s.lattice_vecs.cell_length_a, 5.0);
    assert_eq!(s.motif_offset, DVec3::ZERO);
    // motif still diamond default
    assert_eq!(s.motif.sites.len(), Structure::diamond().motif.sites.len());
}

#[test]
fn motif_offset_override_replaces_only_offset() {
    let mut designer = setup_designer();
    let off_id = designer.add_node("vec3", DVec2::ZERO);
    set_node_data(
        &mut designer,
        off_id,
        Vec3Data {
            value: DVec3::new(0.25, 0.5, 0.75),
        },
    );
    designer.validate_active_network();

    let st_id = designer.add_node("structure", DVec2::new(200.0, 0.0));
    designer.connect_nodes(off_id, 0, st_id, 3);

    let s = extract_structure(evaluate(&designer, st_id));
    assert_eq!(s.motif_offset, DVec3::new(0.25, 0.5, 0.75));
    // lattice still diamond default
    assert_eq!(s.lattice_vecs.cell_length_a, DIAMOND_UNIT_CELL_SIZE_ANGSTROM);
}

#[test]
fn base_structure_plus_override_passes_through_other_fields() {
    let mut designer = setup_designer();

    // Base: structure with custom lattice
    let lv_id = designer.add_node("lattice_vecs", DVec2::ZERO);
    set_node_data(
        &mut designer,
        lv_id,
        LatticeVecsData {
            cell_length_a: 7.0,
            cell_length_b: 7.0,
            cell_length_c: 7.0,
            cell_angle_alpha: 90.0,
            cell_angle_beta: 90.0,
            cell_angle_gamma: 90.0,
        },
    );
    designer.validate_active_network();

    let base_id = designer.add_node("structure", DVec2::new(200.0, 0.0));
    designer.connect_nodes(lv_id, 0, base_id, 1);

    // Override only the motif_offset on a derived structure.
    let off_id = designer.add_node("vec3", DVec2::new(0.0, 200.0));
    set_node_data(
        &mut designer,
        off_id,
        Vec3Data {
            value: DVec3::new(0.1, 0.2, 0.3),
        },
    );
    designer.validate_active_network();

    let derived_id = designer.add_node("structure", DVec2::new(400.0, 0.0));
    designer.connect_nodes(base_id, 0, derived_id, 0); // base -> structure
    designer.connect_nodes(off_id, 0, derived_id, 3); // override offset

    let s = extract_structure(evaluate(&designer, derived_id));
    // Override applied
    assert_eq!(s.motif_offset, DVec3::new(0.1, 0.2, 0.3));
    // Pass-through from base, NOT reset to diamond default
    assert_eq!(s.lattice_vecs.cell_length_a, 7.0);
}
