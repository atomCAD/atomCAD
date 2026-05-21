//! Phase 1 of `doc/design_hof_node_collapse.md` — the Rust data model,
//! `CollapseMode` resolution, the `set_collapse_mode` mutation, and
//! serialization round-trips. Flutter rendering/interaction is Phase 2.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::node_network::{
    CollapseMode, IncomingWire, SourcePin, collapsable_type_name, resolve_body_collapsed,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, node_network_to_serializable, serializable_to_node_network,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// --- Helpers ---

fn setup(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Read a node's stored `collapse_mode` at the given scope.
fn mode_of(designer: &StructureDesigner, scope: &[u64], node_id: u64) -> CollapseMode {
    designer
        .get_scope_network(scope)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .collapse_mode
}

/// Resolve `resolve_body_collapsed` for a top-level node in "main".
fn resolve(designer: &StructureDesigner, node_id: u64) -> bool {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("main").unwrap();
    let node = network.nodes.get(&node_id).unwrap();
    let node_type = registry.get_node_type_for_node(node).unwrap();
    resolve_body_collapsed(node, node_type)
}

/// Simulate an `f` pin connection by pushing a wire onto the node's `f`
/// argument. `resolve_body_collapsed`/`function_input_pin_connected` only check
/// that the `f` argument carries at least one wire, so a dummy source suffices —
/// no need to build a real `closure` source for this unit-level check.
fn mark_f_connected(designer: &mut StructureDesigner, node_id: u64) {
    let idx = {
        let registry = &designer.node_type_registry;
        let network = registry.node_networks.get("main").unwrap();
        let node = network.nodes.get(&node_id).unwrap();
        let node_type = registry.get_node_type_for_node(node).unwrap();
        node_type
            .parameters
            .iter()
            .position(|p| p.name == "f")
            .expect("HOF should declare an `f` pin")
    };
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.arguments[idx].incoming_wires.push(IncomingWire {
        source_node_id: 999_999,
        source_pin: SourcePin::NodeOutput { pin_index: 0 },
        source_scope_depth: 0,
    });
}

/// Force a node's `collapse_mode` field directly, bypassing the `set_collapse_mode`
/// collapsable guard. Used to verify `resolve_body_collapsed`'s own guard for
/// non-collapsable nodes.
fn force_mode(designer: &mut StructureDesigner, node_id: u64, mode: CollapseMode) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("main")
        .unwrap();
    network.nodes.get_mut(&node_id).unwrap().collapse_mode = mode;
}

// --- Tests ---

#[test]
fn fresh_hofs_default_to_auto() {
    let mut designer = setup("main");
    for type_name in ["map", "filter", "fold", "foreach"] {
        let id = designer.add_node(type_name, DVec2::ZERO);
        assert_eq!(
            mode_of(&designer, &[], id),
            CollapseMode::Auto,
            "fresh {type_name} should default to Auto"
        );
    }
}

#[test]
fn collapsable_type_name_matches_only_hofs() {
    for name in ["map", "filter", "fold", "foreach"] {
        assert!(collapsable_type_name(name), "{name} should be collapsable");
    }
    for name in ["closure", "apply", "sphere", "int", "expr"] {
        assert!(
            !collapsable_type_name(name),
            "{name} should not be collapsable"
        );
    }
}

#[test]
fn resolve_auto_follows_f_pin() {
    let mut designer = setup("main");
    let map_id = designer.add_node("map", DVec2::ZERO);

    // Auto + f disconnected → expanded (false).
    assert!(!resolve(&designer, map_id));

    // Auto + f connected → collapsed (true).
    mark_f_connected(&mut designer, map_id);
    assert!(resolve(&designer, map_id));
}

#[test]
fn resolve_overrides_force_the_result() {
    let mut designer = setup("main");
    let map_id = designer.add_node("map", DVec2::ZERO);

    // Collapsed forces compact regardless of f.
    force_mode(&mut designer, map_id, CollapseMode::Collapsed);
    assert!(resolve(&designer, map_id));

    // Expanded forces body shown — even with f wired.
    force_mode(&mut designer, map_id, CollapseMode::Expanded);
    assert!(!resolve(&designer, map_id));
    mark_f_connected(&mut designer, map_id);
    assert!(!resolve(&designer, map_id));
}

#[test]
fn closure_is_never_collapsed_regardless_of_mode() {
    let mut designer = setup("main");
    let closure_id = designer.add_node("closure", DVec2::ZERO);
    for mode in [
        CollapseMode::Auto,
        CollapseMode::Collapsed,
        CollapseMode::Expanded,
    ] {
        force_mode(&mut designer, closure_id, mode);
        assert!(
            !resolve(&designer, closure_id),
            "closure must never resolve to collapsed (mode {mode:?})"
        );
    }
}

#[test]
fn set_collapse_mode_flips_mode_and_ignores_non_collapsable() {
    let mut designer = setup("main");

    let map_id = designer.add_node("map", DVec2::ZERO);
    designer.set_collapse_mode(&[], map_id, CollapseMode::Collapsed);
    assert_eq!(mode_of(&designer, &[], map_id), CollapseMode::Collapsed);
    designer.set_collapse_mode(&[], map_id, CollapseMode::Expanded);
    assert_eq!(mode_of(&designer, &[], map_id), CollapseMode::Expanded);

    // `closure` is not collapsable — the mutation is a no-op (stays Auto).
    let closure_id = designer.add_node("closure", DVec2::ZERO);
    designer.set_collapse_mode(&[], closure_id, CollapseMode::Collapsed);
    assert_eq!(mode_of(&designer, &[], closure_id), CollapseMode::Auto);
}

#[test]
fn serialization_preserves_explicit_mode() {
    let mut designer = setup("main");
    let map_id = designer.add_node("map", DVec2::ZERO);
    designer.set_collapse_mode(&[], map_id, CollapseMode::Expanded);

    let registry = &mut designer.node_type_registry;
    let (built_in, networks) = (&registry.built_in_node_types, &mut registry.node_networks);
    let network = networks.get_mut("main").unwrap();

    // Round-trip through JSON to exercise the serde Serialize/Deserialize path.
    let serializable = node_network_to_serializable(network, built_in, None).unwrap();
    let value = serde_json::to_value(&serializable).unwrap();
    let restored_ser: SerializableNodeNetwork = serde_json::from_value(value).unwrap();
    let restored = serializable_to_node_network(&restored_ser, built_in, None).unwrap();

    assert_eq!(
        restored.nodes.get(&map_id).unwrap().collapse_mode,
        CollapseMode::Expanded
    );
}

#[test]
fn deserialize_without_field_yields_auto() {
    let mut designer = setup("main");
    let map_id = designer.add_node("map", DVec2::ZERO);
    // Explicitly set to a non-default so we know Auto comes from the default,
    // not from a carried-over value.
    designer.set_collapse_mode(&[], map_id, CollapseMode::Collapsed);

    let registry = &mut designer.node_type_registry;
    let (built_in, networks) = (&registry.built_in_node_types, &mut registry.node_networks);
    let network = networks.get_mut("main").unwrap();

    let serializable = node_network_to_serializable(network, built_in, None).unwrap();
    let mut value = serde_json::to_value(&serializable).unwrap();

    // Strip `collapse_mode` from every top-level node object, simulating an
    // older `.cnnd` file written before the field existed.
    if let Some(nodes) = value.get_mut("nodes").and_then(|v| v.as_array_mut()) {
        for node in nodes {
            node.as_object_mut().unwrap().remove("collapse_mode");
        }
    }

    let restored_ser: SerializableNodeNetwork = serde_json::from_value(value).unwrap();
    let restored = serializable_to_node_network(&restored_ser, built_in, None).unwrap();

    assert_eq!(
        restored.nodes.get(&map_id).unwrap().collapse_mode,
        CollapseMode::Auto,
        "a node without the field should load as Auto"
    );
}
