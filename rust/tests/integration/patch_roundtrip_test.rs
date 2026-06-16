//! Phase 3 round-trip tests for the surface-patch nodes (see
//! `doc/design_surface_patches.md` §9 Phase 3, tests 11–13).
//!
//! `Patch` is a built-in record and both `patch_build` / `patch_latticefill` are
//! ordinary node types, so serialization needs **no new plumbing** — these tests
//! lock that in: a network containing both nodes round-trips through `.cnnd` and
//! through the text format, and the durable patch-ghost atom flag (bit 6)
//! survives the atom serialization format.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::patch_build::PatchBuildData;
use rust_lib_flutter_cad::structure_designer::nodes::patch_latticefill::PatchLatticeFillData;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;
use tempfile::tempdir;

// ============================================================================
// 11. A network with patch_build + patch_latticefill round-trips through .cnnd
//     with identical structure (node types + stored properties preserved).
// ============================================================================

#[test]
fn patch_nodes_cnnd_roundtrip() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));

    let build_id = designer.add_node("patch_build", DVec2::new(0.0, 0.0));
    designer.set_node_network_data(build_id, Box::new(PatchBuildData { epsilon: 0.25 }));

    let fill_id = designer.add_node("patch_latticefill", DVec2::new(100.0, 0.0));
    designer.set_node_network_data(
        fill_id,
        Box::new(PatchLatticeFillData {
            passivate: false,
            tolerance: 0.05,
            ..Default::default()
        }),
    );

    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("patch.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .expect("save");

    let mut reloaded = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut reloaded, path.to_str().unwrap()).expect("load");

    let network = reloaded
        .node_networks
        .get("test")
        .expect("the network reloads");
    assert_eq!(network.nodes.len(), 2, "both nodes survive the round-trip");

    let build = network.nodes.get(&build_id).expect("patch_build reloads");
    assert_eq!(build.node_type_name, "patch_build");
    let epsilon = build
        .data
        .get_text_properties()
        .into_iter()
        .find(|(k, _)| k == "epsilon")
        .and_then(|(_, v)| v.as_float())
        .expect("epsilon property");
    assert_eq!(epsilon, 0.25, "patch_build.epsilon preserved");

    let fill = network
        .nodes
        .get(&fill_id)
        .expect("patch_latticefill reloads");
    assert_eq!(fill.node_type_name, "patch_latticefill");
    let props: HashMap<String, _> = fill.data.get_text_properties().into_iter().collect();
    assert_eq!(
        props.get("passivate").and_then(|v| v.as_bool()),
        Some(false),
        "patch_latticefill.passivate preserved"
    );
    assert_eq!(
        props.get("tolerance").and_then(|v| v.as_float()),
        Some(0.05),
        "patch_latticefill.tolerance preserved"
    );
}

// ============================================================================
// 12. Text-format serialize → edit_network round-trip is stable.
// ============================================================================

#[test]
fn patch_nodes_text_format_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::text_format::{edit_network, serialize_network};

    let registry = NodeTypeRegistry::new();

    // A standalone custom network (not registered) to author into. Reuse a
    // built-in single-output custom-network shape via the registry's default.
    let mut network = make_empty_network();

    let source = r#"
        pb = patch_build { epsilon: 0.2 }
        pf = patch_latticefill { passivate: false, tolerance: 0.05 }
    "#;
    let result = edit_network(&mut network, &registry, source, true);
    assert!(result.success, "initial edit succeeds: {:?}", result.errors);
    assert_eq!(network.nodes.len(), 2);

    let serialized = serialize_network(&network, &registry, Some("test"));

    // Re-author a fresh network from the serialized text; it must reproduce the
    // same node set and re-serialize identically.
    let mut network2 = make_empty_network();
    let result2 = edit_network(&mut network2, &registry, &serialized, true);
    assert!(
        result2.success,
        "round-trip edit succeeds: {:?}",
        result2.errors
    );
    assert_eq!(network2.nodes.len(), 2);

    let reserialized = serialize_network(&network2, &registry, Some("test"));
    assert_eq!(serialized, reserialized, "text round-trip is stable");
}

/// Builds an empty custom network to author test nodes into.
fn make_empty_network() -> rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork {
    use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::NodeTypeCategory;
    use rust_lib_flutter_cad::structure_designer::data_type::DataType;
    use rust_lib_flutter_cad::structure_designer::node_network::NodeNetwork;
    use rust_lib_flutter_cad::structure_designer::node_type::{NodeType, OutputPinDefinition};

    let node_type = NodeType {
        name: "test".to_string(),
        description: "Test network".to_string(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Crystal),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || {
            Box::new(rust_lib_flutter_cad::structure_designer::node_data::NoData {})
        },
        node_data_saver: rust_lib_flutter_cad::structure_designer::node_type::no_data_saver,
        node_data_loader: rust_lib_flutter_cad::structure_designer::node_type::no_data_loader,
    };
    NodeNetwork::new(node_type)
}

// ============================================================================
// 13. A tile atom with the patch-ghost flag (bit 6) round-trips with the flag
//     intact through the atom serialization format.
// ============================================================================

#[test]
fn patch_ghost_flag_survives_atom_serialization() {
    use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
    use rust_lib_flutter_cad::crystolecule::atomic_structure::atom::ATOM_FLAG_PATCH_GHOST;
    use rust_lib_flutter_cad::structure_designer::serialization::atom_edit_data_serialization::SerializableAtom;

    // A tile atom flagged as a patch-ghost (as patch_build emits).
    let mut tile = AtomicStructure::new();
    let id = tile.add_atom(6, glam::f64::DVec3::new(1.0, 2.0, 3.0));
    tile.set_atom_patch_ghost(id, true);
    let atom = tile.get_atom(id).unwrap();
    assert!(atom.is_patch_ghost());

    // Map to the serializable atom (selection bit stripped, like real saves).
    let serializable = SerializableAtom {
        id: atom.id,
        atomic_number: atom.atomic_number,
        position: atom.position,
        flags: atom.flags & !0x1,
    };

    let json = serde_json::to_string(&serializable).expect("serialize");
    let restored: SerializableAtom = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(
        restored.flags & ATOM_FLAG_PATCH_GHOST,
        ATOM_FLAG_PATCH_GHOST,
        "the patch-ghost flag survives JSON (not stripped like the selection bit)"
    );

    // And it reconstructs into a real patch-ghost atom.
    let mut reloaded = AtomicStructure::new();
    let new_id = reloaded.add_atom(restored.atomic_number, restored.position);
    reloaded.set_atom_flags(new_id, restored.flags);
    assert!(
        reloaded.get_atom(new_id).unwrap().is_patch_ghost(),
        "bit 6 reconstructs the patch-ghost atom"
    );
}
