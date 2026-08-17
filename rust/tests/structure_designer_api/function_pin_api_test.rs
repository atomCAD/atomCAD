//! The api-level half of `function_pin_test.rs` (issue #408, Phase 3).
//!
//! `build_function_pin_role_views` and the two `API…` enums it returns live in
//! the root crate, and a member crate cannot depend on the root — so this one
//! test cannot travel with the rest of the file into
//! `atomcad-structure-designer`. D5.1a of `doc/design_rust_crate_split.md`
//! anticipates exactly this split; the domain half stays at
//! `crates/atomcad-structure-designer/tests/structure_designer/function_pin_test.rs`.
//!
//! The handful of builder helpers below are copies of that file's, trimmed to
//! what this test needs — the two harnesses are separate binaries in separate
//! packages and cannot share private helpers.

use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::FunctionPinRole;
use atomcad_structure_designer::node_network::function_pin_dispositions;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::cuboid::CuboidData;
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::nodes::structure_move::StructureMoveData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use glam::i32::IVec3;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api::build_function_pin_role_views;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::{
    APIFunctionPinDisposition, APIFunctionPinRole,
};

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

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

fn add_int(designer: &mut StructureDesigner, network: &str, value: i32, y: f64) -> u64 {
    let id = designer.add_node("int", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(IntData { value }));
    id
}

fn add_cuboid(designer: &mut StructureDesigner, network: &str, extent: i32, y: f64) -> u64 {
    let id = designer.add_node("cuboid", DVec2::new(0.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(CuboidData {
            min_corner: IVec3::ZERO,
            extent: IVec3::splat(extent),
            subdivision: 1,
        }),
    );
    id
}

/// `materialize(cuboid(extent))` - a self-contained `Crystal` source.
fn add_crystal_source(designer: &mut StructureDesigner, network: &str, extent: i32, y: f64) -> u64 {
    let cuboid_id = add_cuboid(designer, network, extent, y);
    let mat_id = designer.add_node("materialize", DVec2::new(150.0, y));
    designer.connect_nodes(cuboid_id, 0, mat_id, 0); // shape
    mat_id
}

/// A `structure_move` node with the given stored translation. Pins: 0 `input`
/// (HasStructure, required), 1 `translation` (IVec3), 2 `subdivision` (Int).
fn add_structure_move(
    designer: &mut StructureDesigner,
    network: &str,
    translation: IVec3,
    y: f64,
) -> u64 {
    let id = designer.add_node("structure_move", DVec2::new(300.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(StructureMoveData {
            translation,
            lattice_subdivision: IVec3::ONE,
        }),
    );
    id
}

// --- API surface (Phase 3) ---------------------------------------------------

/// The sidebar renders `APIFunctionPinRoleView::effective` verbatim, so it must
/// be the shared partition — not a UI-side re-derivation that could silently
/// disagree with the resolver and the closure synthesizer. Check the whole row
/// set against `function_pin_dispositions` for a node mixing all three roles ×
/// wired/unwired.
#[test]
fn api_function_pin_role_views_match_the_shared_partition() {
    let mut designer = setup_designer_with_network("main");
    let crystal_id = add_crystal_source(&mut designer, "main", 2, 0.0);
    let int_id = add_int(&mut designer, "main", 3, 200.0);
    let mv_id = add_structure_move(&mut designer, "main", IVec3::new(1, 0, 0), -120.0);

    // `input`: Delayed + wired (preview). `translation`: Supplied + unwired
    // (stored/gizmo). `subdivision`: Supplied + wired (frozen capture). That
    // covers a parameter, a capture-stored, and a capture-wire in one node.
    designer.connect_nodes(crystal_id, 0, mv_id, 0);
    designer.connect_nodes(int_id, 0, mv_id, 2);
    designer.set_function_pin_role(&[], mv_id, 0, FunctionPinRole::Delayed);
    designer.set_function_pin_role(&[], mv_id, 1, FunctionPinRole::Supplied);
    designer.set_function_pin_role(&[], mv_id, 2, FunctionPinRole::Supplied);

    let registry = &designer.node_type_registry;
    let node = registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&mv_id)
        .unwrap();
    let node_type = registry.get_node_type_for_node(node).unwrap();
    let views = build_function_pin_role_views(node, node_type);

    // One row per declared input pin, in pin order, named after the pin.
    assert_eq!(
        views
            .iter()
            .map(|v| v.pin_name.as_str())
            .collect::<Vec<_>>(),
        node_type
            .parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
    );

    // `effective` agrees with the shared helper, row for row.
    let expected: Vec<APIFunctionPinDisposition> = function_pin_dispositions(node, node_type)
        .into_iter()
        .map(Into::into)
        .collect();
    assert_eq!(
        views.iter().map(|v| v.effective).collect::<Vec<_>>(),
        expected
    );
    // ...and is the table from the design doc, spelled out. The trailing
    // `subdiv_xyz` pin is untouched (Auto + unwired → parameter).
    assert_eq!(
        expected,
        vec![
            APIFunctionPinDisposition::Parameter,
            APIFunctionPinDisposition::CaptureStored,
            APIFunctionPinDisposition::CaptureWire,
            APIFunctionPinDisposition::Parameter,
        ]
    );

    // The stored roles and the wiring flags round-trip faithfully.
    assert_eq!(
        views.iter().map(|v| v.role).collect::<Vec<_>>(),
        vec![
            APIFunctionPinRole::Delayed,
            APIFunctionPinRole::Supplied,
            APIFunctionPinRole::Supplied,
            APIFunctionPinRole::Auto,
        ]
    );
    assert_eq!(
        views.iter().map(|v| v.wired).collect::<Vec<_>>(),
        vec![true, false, true, false]
    );

    // An `Auto` pin (no stored entry) reports `Auto` explicitly — the API
    // always names a role even though absence is the storage form.
    designer.set_function_pin_role(&[], mv_id, 1, FunctionPinRole::Auto);
    let registry = &designer.node_type_registry;
    let node = registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&mv_id)
        .unwrap();
    let views = build_function_pin_role_views(node, registry.get_node_type_for_node(node).unwrap());
    assert_eq!(views[1].role, APIFunctionPinRole::Auto);
    assert_eq!(views[1].effective, APIFunctionPinDisposition::Parameter);
}
