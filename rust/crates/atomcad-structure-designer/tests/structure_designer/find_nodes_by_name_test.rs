//! Find Node backend (`doc/design_node_names_in_ui.md` Phase 3, D7/D8):
//! `StructureDesigner::resolve_node_path` — the exact lookup the AI History
//! panel jumps through — and `find_nodes_by_name`, the ranked substring search
//! behind the picker.

use atomcad_structure_designer::node_network::CollapseMode;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn activate(designer: &mut StructureDesigner, name: &str) {
    designer.set_active_node_network_name(Some(name.to_string()));
}

/// Adds a node and gives it `name`, the way the property-panel strip does.
fn add_named(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_type: &str,
    name: &str,
) -> u64 {
    let id = if scope_path.is_empty() {
        designer.add_node(node_type, DVec2::ZERO)
    } else {
        designer.add_node_scoped(scope_path, node_type, DVec2::ZERO, None)
    };
    designer
        .rename_node(scope_path, id, name)
        .unwrap_or_else(|e| panic!("could not name {node_type} node {id} '{name}': {e}"));
    id
}

/// The name paths of a search result, in result order.
fn paths(matches: &[atomcad_structure_designer::node_name_search::NodeNameMatch]) -> Vec<String> {
    matches.iter().map(|m| m.name_path.clone()).collect()
}

// ---------------------------------------------------------------------------
// resolve_node_path — the exact lookup
// ---------------------------------------------------------------------------

#[test]
fn resolves_a_root_node() {
    let mut designer = setup_designer_with_network("main");
    let id = add_named(&mut designer, &[], "sphere", "chassis");

    let found = designer.resolve_node_path("main", "chassis").unwrap();
    assert_eq!(found.scope_path, Vec::<u64>::new());
    assert_eq!(found.node_id, id);
}

#[test]
fn resolves_a_body_node_two_levels_down() {
    let mut designer = setup_designer_with_network("main");
    // main: map4 { c1 { e1 } }
    let map_id = add_named(&mut designer, &[], "map", "map4");
    let filter_id = add_named(&mut designer, &[map_id], "filter", "c1");
    let expr_id = add_named(&mut designer, &[map_id, filter_id], "expr", "e1");

    let found = designer.resolve_node_path("main", "map4/c1/e1").unwrap();
    assert_eq!(found.scope_path, vec![map_id, filter_id]);
    assert_eq!(found.node_id, expr_id);
}

#[test]
fn resolves_a_name_the_text_format_would_have_to_quote() {
    let mut designer = setup_designer_with_network("main");
    // `x.shape` is a legal node name that is not an identifier, so the text
    // format prints it backtick-quoted. A *path* is always the bare spelling.
    let id = add_named(&mut designer, &[], "sphere", "x.shape");

    let found = designer.resolve_node_path("main", "x.shape").unwrap();
    assert_eq!(found.node_id, id);
    assert!(designer.resolve_node_path("main", "`x.shape`").is_none());
}

#[test]
fn a_path_ending_at_a_body_owner_resolves_to_the_owner() {
    let mut designer = setup_designer_with_network("main");
    let map_id = add_named(&mut designer, &[], "map", "map4");
    add_named(&mut designer, &[map_id], "expr", "e1");

    let found = designer.resolve_node_path("main", "map4").unwrap();
    assert_eq!(found.scope_path, Vec::<u64>::new());
    assert_eq!(found.node_id, map_id);
}

#[test]
fn resolve_misses_are_none_not_panics() {
    let mut designer = setup_designer_with_network("main");
    let map_id = add_named(&mut designer, &[], "map", "map4");
    add_named(&mut designer, &[map_id], "expr", "e1");
    add_named(&mut designer, &[], "sphere", "leaf");

    // Unknown network.
    assert!(designer.resolve_node_path("nope", "map4").is_none());
    // Unknown segment.
    assert!(designer.resolve_node_path("main", "map9").is_none());
    assert!(designer.resolve_node_path("main", "map4/nope").is_none());
    // Descending into a node that has no body.
    assert!(designer.resolve_node_path("main", "leaf/e1").is_none());
    // Degenerate paths.
    assert!(designer.resolve_node_path("main", "").is_none());
    assert!(designer.resolve_node_path("main", "map4/").is_none());
    // A body node is not addressable from the top level without its owner.
    assert!(designer.resolve_node_path("main", "e1").is_none());
}

// ---------------------------------------------------------------------------
// find_nodes_by_name — the ranked search
// ---------------------------------------------------------------------------

#[test]
fn search_covers_the_active_network_and_its_bodies() {
    let mut designer = setup_designer_with_network("main");
    let map_id = add_named(&mut designer, &[], "map", "map4");
    let filter_id = add_named(&mut designer, &[map_id], "filter", "c1");
    let deep_id = add_named(&mut designer, &[map_id, filter_id], "expr", "e1");

    let matches = designer.find_nodes_by_name("e1", false);
    assert_eq!(paths(&matches), vec!["map4/c1/e1"]);
    assert_eq!(matches[0].network, "main");
    assert_eq!(matches[0].scope_path, vec![map_id, filter_id]);
    assert_eq!(matches[0].node_id, deep_id);
    assert_eq!(matches[0].node_type_name, "expr");
}

#[test]
fn a_collapsed_body_is_still_searched() {
    let mut designer = setup_designer_with_network("main");
    let map_id = add_named(&mut designer, &[], "map", "map4");
    add_named(&mut designer, &[map_id], "expr", "hidden1");
    // Collapsing is a canvas display state, not a scope the search knows about.
    designer.set_collapse_mode(&[], map_id, CollapseMode::Collapsed);

    assert_eq!(
        paths(&designer.find_nodes_by_name("hidden", false)),
        vec!["map4/hidden1"]
    );
}

#[test]
fn ranking_is_exact_then_prefix_then_substring_then_path() {
    let mut designer = setup_designer_with_network("main");
    // Deliberately created in an order that does not match the expected one.
    let map_id = add_named(&mut designer, &[], "map", "map4");
    add_named(&mut designer, &[map_id], "expr", "e1");
    add_named(&mut designer, &[], "expr", "e1_extra");
    add_named(&mut designer, &[], "expr", "e1");
    add_named(&mut designer, &[], "expr", "abc_e1_def");

    assert_eq!(
        paths(&designer.find_nodes_by_name("e1", false)),
        vec!["e1", "e1_extra", "abc_e1_def", "map4/e1"]
    );
}

#[test]
fn matching_is_case_insensitive_on_the_path() {
    let mut designer = setup_designer_with_network("main");
    let map_id = add_named(&mut designer, &[], "map", "Map4");
    add_named(&mut designer, &[map_id], "expr", "E1");

    assert_eq!(
        paths(&designer.find_nodes_by_name("map4/e1", false)),
        vec!["Map4/E1"]
    );
    assert_eq!(
        paths(&designer.find_nodes_by_name("MAP4/", false)),
        vec!["Map4/E1"]
    );
}

#[test]
fn suffixed_names_match_on_their_stored_spelling() {
    let mut designer = setup_designer_with_network("main");
    // The D1 suffix rule stores `x_2`; the search reads exactly that.
    add_named(&mut designer, &[], "expr", "x");
    let copy_id = designer.add_node("expr", DVec2::ZERO);
    designer.rename_node(&[], copy_id, "x_2").unwrap();

    assert_eq!(
        paths(&designer.find_nodes_by_name("x_2", false)),
        vec!["x_2"]
    );
}

#[test]
fn an_empty_query_lists_every_node_of_the_active_network() {
    let mut designer = setup_designer_with_network("main");
    let map_id = add_named(&mut designer, &[], "map", "map4");
    add_named(&mut designer, &[map_id], "expr", "e1");
    add_named(&mut designer, &[], "sphere", "chassis");

    assert_eq!(
        paths(&designer.find_nodes_by_name("", false)),
        vec!["chassis", "map4", "map4/e1"]
    );
}

#[test]
fn all_networks_widens_the_search_and_keeps_the_active_one_first() {
    let mut designer = setup_designer_with_network("main");
    add_named(&mut designer, &[], "expr", "shared");

    designer.add_node_network("other");
    activate(&mut designer, "other");
    add_named(&mut designer, &[], "expr", "shared");
    add_named(&mut designer, &[], "expr", "aaa_shared");
    activate(&mut designer, "main");

    // Active network only: just the one node.
    let active_only = designer.find_nodes_by_name("shared", false);
    assert_eq!(active_only.len(), 1);
    assert_eq!(active_only[0].network, "main");

    // Widened: `main`'s match still comes first, even though `other` holds an
    // exact match too and `aaa_shared` sorts earlier by path.
    let all = designer.find_nodes_by_name("shared", true);
    assert_eq!(
        all.iter()
            .map(|m| (m.network.as_str(), m.name_path.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("main", "shared"),
            ("other", "shared"),
            ("other", "aaa_shared"),
        ]
    );
}

#[test]
fn a_search_that_matches_nothing_returns_nothing() {
    let mut designer = setup_designer_with_network("main");
    add_named(&mut designer, &[], "expr", "chassis");

    assert!(designer.find_nodes_by_name("nonesuch", false).is_empty());
    assert!(designer.find_nodes_by_name("nonesuch", true).is_empty());
}

#[test]
fn every_match_resolves_back_through_resolve_node_path() {
    // The two entry points share a walk; this is what keeps the picker's rows
    // and the exact resolver on the same spelling.
    let mut designer = setup_designer_with_network("main");
    let map_id = add_named(&mut designer, &[], "map", "map4");
    let filter_id = add_named(&mut designer, &[map_id], "filter", "c1");
    add_named(&mut designer, &[map_id, filter_id], "expr", "e1");
    add_named(&mut designer, &[], "sphere", "x.shape");

    for m in designer.find_nodes_by_name("", true) {
        let resolved = designer
            .resolve_node_path(&m.network, &m.name_path)
            .unwrap_or_else(|| panic!("'{}' did not resolve in '{}'", m.name_path, m.network));
        assert_eq!(resolved.scope_path, m.scope_path, "path {}", m.name_path);
        assert_eq!(resolved.node_id, m.node_id, "path {}", m.name_path);
    }
}
