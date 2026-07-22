//! Find Usages backend collection (issue #414, `doc/design_find_usages.md`
//! Phase 1): `StructureDesigner::network_usages` /
//! `network_usage_counts` and the display-label helpers.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::network_usages::{node_label, resolve_scope_labels};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Creates a custom network `name` returning a `sphere`, so it is usable as a
/// node type in other networks. Leaves the active network unchanged on exit
/// only if the caller re-activates; callers do that explicitly.
fn add_helper_network(designer: &mut StructureDesigner, name: &str) {
    designer.add_node_network(name);
    designer.set_active_node_network_name(Some(name.to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer
        .node_type_registry
        .node_networks
        .get_mut(name)
        .unwrap()
        .set_return_node(sphere_id);
    designer.validate_active_network();
}

fn activate(designer: &mut StructureDesigner, name: &str) {
    designer.set_active_node_network_name(Some(name.to_string()));
}

// ---------------------------------------------------------------------------
// Collection
// ---------------------------------------------------------------------------

#[test]
fn top_level_usage_is_reported() {
    let mut designer = setup_designer_with_network("main");
    add_helper_network(&mut designer, "helper");

    activate(&mut designer, "main");
    let instance_id = designer.add_node("helper", DVec2::new(10.0, 20.0));

    let usages = designer.network_usages("helper");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].host_network, "main");
    assert_eq!(usages[0].scope_path, Vec::<u64>::new());
    assert_eq!(usages[0].node_id, instance_id);
}

#[test]
fn usage_in_nested_body_reports_full_scope_path() {
    let mut designer = setup_designer_with_network("main");
    add_helper_network(&mut designer, "helper");

    // main: map { filter { helper } } — the instance sits two bodies deep.
    activate(&mut designer, "main");
    let map_id = designer.add_node("map", DVec2::ZERO);
    let filter_id = designer.add_node_scoped(&[map_id], "filter", DVec2::ZERO, None);
    let instance_id = designer.add_node_scoped(&[map_id, filter_id], "helper", DVec2::ZERO, None);
    assert_ne!(instance_id, 0, "precondition: body instance was added");

    let usages = designer.network_usages("helper");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].host_network, "main");
    assert_eq!(usages[0].scope_path, vec![map_id, filter_id]);
    assert_eq!(usages[0].node_id, instance_id);
}

#[test]
fn unused_network_has_no_usages() {
    let mut designer = setup_designer_with_network("main");
    add_helper_network(&mut designer, "helper");

    activate(&mut designer, "main");
    designer.add_node("sphere", DVec2::ZERO);

    assert!(designer.network_usages("helper").is_empty());
    // A name that isn't a network at all is simply unused, not an error.
    assert!(designer.network_usages("does_not_exist").is_empty());
}

#[test]
fn usages_spread_across_hosts_are_all_reported_and_sorted() {
    let mut designer = setup_designer_with_network("alpha");
    designer.add_node_network("beta");
    add_helper_network(&mut designer, "helper");

    // Two instances in `alpha` (one of them in a body), one in `beta`.
    activate(&mut designer, "alpha");
    let alpha_top = designer.add_node("helper", DVec2::ZERO);
    let map_id = designer.add_node("map", DVec2::new(300.0, 0.0));
    let alpha_body = designer.add_node_scoped(&[map_id], "helper", DVec2::ZERO, None);

    activate(&mut designer, "beta");
    let beta_top = designer.add_node("helper", DVec2::ZERO);

    let usages = designer.network_usages("helper");
    assert_eq!(usages.len(), 3);

    // Sorted by (host_network, scope_path, node_id): `alpha` before `beta`,
    // and within `alpha` the top-level (empty path) entry comes first.
    assert_eq!(usages[0].host_network, "alpha");
    assert_eq!(usages[0].scope_path, Vec::<u64>::new());
    assert_eq!(usages[0].node_id, alpha_top);

    assert_eq!(usages[1].host_network, "alpha");
    assert_eq!(usages[1].scope_path, vec![map_id]);
    assert_eq!(usages[1].node_id, alpha_body);

    assert_eq!(usages[2].host_network, "beta");
    assert_eq!(usages[2].scope_path, Vec::<u64>::new());
    assert_eq!(usages[2].node_id, beta_top);
}

#[test]
fn instance_consumed_as_a_function_value_is_still_a_usage() {
    let mut designer = setup_designer_with_network("main");

    // `helper` takes one parameter, so its instance exposes a 1-ary function
    // on the `-1` pin rather than being coerced to a plain value.
    designer.add_node_network("helper");
    activate(&mut designer, "helper");
    let param_id = designer.add_node("parameter", DVec2::ZERO);
    designer
        .node_type_registry
        .node_networks
        .get_mut("helper")
        .unwrap()
        .set_return_node(param_id);
    designer.validate_active_network();

    activate(&mut designer, "main");
    let instance_id = designer.add_node("helper", DVec2::ZERO);
    let apply_id = designer.add_node("apply", DVec2::new(300.0, 0.0));
    // `-1` = the title-bar function pin; `apply.f` is input pin 0.
    designer.connect_nodes(instance_id, -1, apply_id, 0);

    assert!(
        designer
            .node_type_registry
            .node_networks
            .get("main")
            .unwrap()
            .function_pin_consumed(instance_id),
        "precondition: the instance's -1 pin is consumed by apply"
    );

    let usages = designer.network_usages("helper");
    assert_eq!(
        usages.len(),
        1,
        "an instance used as a function value is still an instance node"
    );
    assert_eq!(usages[0].node_id, instance_id);
    assert_eq!(usages[0].scope_path, Vec::<u64>::new());
}

// ---------------------------------------------------------------------------
// Batched counts
// ---------------------------------------------------------------------------

#[test]
fn usage_counts_match_per_network_queries() {
    let mut designer = setup_designer_with_network("main");
    add_helper_network(&mut designer, "helper");
    add_helper_network(&mut designer, "other");

    activate(&mut designer, "main");
    designer.add_node("helper", DVec2::ZERO);
    let map_id = designer.add_node("map", DVec2::new(300.0, 0.0));
    designer.add_node_scoped(&[map_id], "helper", DVec2::ZERO, None);
    designer.add_node_scoped(&[map_id], "other", DVec2::new(100.0, 0.0), None);

    let counts = designer.network_usage_counts();
    for name in ["helper", "other", "map", "sphere"] {
        assert_eq!(
            counts.get(name).copied().unwrap_or(0) as usize,
            designer.network_usages(name).len(),
            "batched count for '{name}' must match the per-network query"
        );
    }
    assert_eq!(counts.get("helper"), Some(&2));
    assert_eq!(counts.get("other"), Some(&1));
    // Unreferenced names are absent rather than zero.
    assert_eq!(counts.get("nonexistent_type"), None);
}

// ---------------------------------------------------------------------------
// Display-string helpers (feed `APINetworkUsage`'s resolved fields)
// ---------------------------------------------------------------------------

#[test]
fn scope_labels_name_the_enclosing_hof_chain() {
    let mut designer = setup_designer_with_network("main");
    add_helper_network(&mut designer, "helper");

    activate(&mut designer, "main");
    let map_id = designer.add_node("map", DVec2::ZERO);
    let filter_id = designer.add_node_scoped(&[map_id], "filter", DVec2::ZERO, None);
    designer.add_node_scoped(&[map_id, filter_id], "helper", DVec2::ZERO, None);

    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();

    // Labels are the nodes' names — `add_node` auto-assigns `map1` / `filter1`
    // — not the bare type names.
    assert!(resolve_scope_labels(main, &[]).is_empty());
    assert_eq!(resolve_scope_labels(main, &[map_id]), vec!["map1"]);
    assert_eq!(
        resolve_scope_labels(main, &[map_id, filter_id]),
        vec!["map1", "filter1"]
    );
    // A path that doesn't resolve degrades to what it could resolve.
    assert_eq!(resolve_scope_labels(main, &[map_id, 9999]), vec!["map1"]);
}

#[test]
fn node_label_prefers_the_node_name_over_the_type_name() {
    let mut designer = setup_designer_with_network("main");
    add_helper_network(&mut designer, "helper");

    activate(&mut designer, "main");
    // `add_node` auto-assigns a per-network-unique name, so two instances of
    // the same type are distinguishable in a usage picker.
    let auto_named = designer.add_node("helper", DVec2::ZERO);
    let renamed = designer.add_node("helper", DVec2::new(300.0, 0.0));
    let nameless = designer.add_node("helper", DVec2::new(600.0, 0.0));
    {
        let main = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap();
        main.nodes.get_mut(&renamed).unwrap().custom_name = Some("left_wheel".to_string());
        // Hand-authored / legacy nodes can carry no name at all.
        main.nodes.get_mut(&nameless).unwrap().custom_name = None;
    }

    let main = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap();
    assert_eq!(node_label(&main.nodes[&auto_named]), "helper1");
    assert_eq!(node_label(&main.nodes[&renamed]), "left_wheel");
    assert_eq!(node_label(&main.nodes[&nameless]), "helper");
}
