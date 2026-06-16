//! Phase 1 tests for the built-in `Patch` record def (surface-reconstruction
//! patches — see `doc/design_surface_patches.md` §2 "Schema").
//!
//! The `Patch` record is carried by the (not-yet-implemented) `patch_build` /
//! `patch_latticefill` nodes. As a built-in record it needs no serialization,
//! FFI, or validation plumbing of its own; these tests lock in that it resolves
//! through the registry and behaves like any other record def.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::nodes::record_destructure::RecordDestructureData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

// ============================================================================
// 8. The built-in def resolves with the three expected fields and types.
// ============================================================================

#[test]
fn patch_def_resolves_with_expected_fields() {
    let registry = NodeTypeRegistry::new();
    let def = registry
        .lookup_record_type_def("Patch")
        .expect("Patch should resolve via built_in_record_type_defs");
    assert_eq!(def.name, "Patch");
    assert_eq!(
        def.fields,
        vec![
            ("tile".to_string(), DataType::Molecule),
            (
                "tiling_vectors".to_string(),
                DataType::Array(Box::new(DataType::IVec3))
            ),
            ("cut_volume".to_string(), DataType::Blueprint),
        ]
    );
}

#[test]
fn patch_is_a_built_in_record_def() {
    let registry = NodeTypeRegistry::new();
    assert!(registry.is_built_in_record_type_def("Patch"));
    assert!(registry.name_is_taken("Patch"));
}

// ============================================================================
// 9. record_construct / record_destructure derive their pin layout from the
//    built-in `Patch` def in authored order.
// ============================================================================

#[test]
fn record_construct_pins_match_patch_schema() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    let id = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordConstructData {
            schema: "Patch".to_string(),
            ..Default::default()
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    // Pins follow the authored field order of the built-in def.
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["tile", "tiling_vectors", "cut_volume"]);
    assert_eq!(nt.parameters[0].data_type, DataType::Molecule);
    assert_eq!(
        nt.parameters[1].data_type,
        DataType::Array(Box::new(DataType::IVec3))
    );
    assert_eq!(nt.parameters[2].data_type, DataType::Blueprint);
    assert_eq!(
        nt.output_pins[0].fixed_type(),
        Some(&DataType::Record(RecordType::Named("Patch".to_string())))
    );
}

#[test]
fn record_destructure_pins_match_patch_schema() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    let id = designer.add_node("record_destructure", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordDestructureData {
            schema: "Patch".to_string(),
            ..Default::default()
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    assert_eq!(
        nt.parameters[0].data_type,
        DataType::Record(RecordType::Named("Patch".to_string()))
    );
    let pin_names: Vec<&str> = nt.output_pins.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(pin_names, vec!["tile", "tiling_vectors", "cut_volume"]);
}

// ============================================================================
// 10. A user record def referencing `Patch` is NOT flagged dangling.
// ============================================================================

#[test]
fn user_def_referencing_patch_is_not_dangling() {
    let mut registry = NodeTypeRegistry::new();
    // A user def whose field is a `Patch` must resolve cleanly because the
    // built-in `Patch` def is reachable through `lookup_record_type_def`.
    registry
        .add_record_type_def(RecordTypeDef {
            name: "PatchHolder".to_string(),
            fields: vec![(
                "patch".to_string(),
                DataType::Record(RecordType::Named("Patch".to_string())),
            )],
        })
        .expect("a def referencing the built-in Patch must be accepted (not dangling)");

    // And the reference resolves.
    let ty = RecordType::Named("Patch".to_string());
    assert!(
        ty.resolve_fields(&registry).is_some(),
        "Named(Patch) must resolve through the registry"
    );
}
