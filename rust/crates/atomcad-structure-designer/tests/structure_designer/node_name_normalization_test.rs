//! Load-time healing of stored `custom_name`s (`doc/design_node_names_in_ui.md`
//! Phase 0 / D1).
//!
//! Names are unique within a scope and slash-free from now on, but older files
//! predate both rules: parameter minting stored the parameter name verbatim, so
//! a network can hold several nodes called `to_degrees`, and `/` used to be a
//! legal name character even though it is the node-*path* joiner. The loader
//! rewrites both, in ascending id order, to exactly the spellings
//! `text_format::unique_node_names` used to produce on the way out — which is
//! why the migration is invisible in the text format.

use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use atomcad_structure_designer::text_format::unique_node_names;
use atomcad_test_support::fixture_path_str;

const FIXTURE_DIR: &str = "duplicate_node_names";

fn fixture_file(name: &str) -> String {
    fixture_path_str(&format!("{FIXTURE_DIR}/{name}"))
}

fn load(fixture: &str) -> NodeTypeRegistry {
    let mut registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry, &fixture_file(fixture))
        .unwrap_or_else(|e| panic!("fixture {fixture} failed to load: {e}"));
    registry
}

/// Names by node id, so the ascending-id assignment order is visible.
fn names(registry: &NodeTypeRegistry, network: &str) -> Vec<(u64, String)> {
    let net = registry.node_networks.get(network).unwrap();
    let mut out: Vec<(u64, String)> = net
        .nodes
        .iter()
        .map(|(id, node)| (*id, node.custom_name.clone().unwrap()))
        .collect();
    out.sort_by_key(|(id, _)| *id);
    out
}

#[test]
fn duplicate_stored_names_are_suffixed_on_load_in_ascending_id_order() {
    let registry = load("legacy_names.cnnd");

    assert_eq!(
        names(&registry, "legacy"),
        vec![
            (1, "to_degrees".to_string()),
            (2, "to_degrees_2".to_string()),
            (3, "to_degrees_3".to_string()),
            (4, "a_b".to_string()),
        ]
    );
}

#[test]
fn a_slash_in_a_stored_name_becomes_an_underscore_on_load() {
    // `/` joins the segments of a node path (`map4/e1`), so a name holding one
    // makes a path ambiguous. The fixture's `a/b` loads as `a_b`.
    let registry = load("legacy_names.cnnd");
    let net = registry.node_networks.get("legacy").unwrap();
    assert_eq!(net.nodes[&4].custom_name, Some("a_b".to_string()));
    assert!(
        net.nodes
            .values()
            .all(|n| !n.custom_name.as_deref().unwrap().contains('/'))
    );
}

#[test]
fn the_healed_names_are_what_gets_saved() {
    let mut registry = load("legacy_names.cnnd");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resaved.cnnd");
    save_node_networks_to_file(
        &mut registry,
        &path,
        true,
        &std::collections::HashMap::new(),
    )
    .unwrap();

    let mut reloaded = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut reloaded, path.to_str().unwrap()).unwrap();

    assert_eq!(names(&reloaded, "legacy"), names(&registry, "legacy"));
    // The second load renames nothing: the healing is idempotent.
    assert_eq!(
        names(&reloaded, "legacy"),
        vec![
            (1, "to_degrees".to_string()),
            (2, "to_degrees_2".to_string()),
            (3, "to_degrees_3".to_string()),
            (4, "a_b".to_string()),
        ]
    );
}

#[test]
fn unique_node_names_is_a_no_op_after_a_load() {
    // The invariant D1 buys: `custom_name` *is* the name the text format
    // prints, so the serializer's uniquifier has nothing left to do.
    let registry = load("legacy_names.cnnd");
    let net = registry.node_networks.get("legacy").unwrap();
    for (id, name) in unique_node_names(net) {
        assert_eq!(
            Some(&name),
            net.nodes[&id].custom_name.as_ref(),
            "node {id} was renamed by the serializer"
        );
    }
}
