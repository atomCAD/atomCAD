//! Phase 4 of `doc/design_proxy_node.md` — the `proxy` panel's kernel seam.
//!
//! The node itself is covered in
//! `crates/atomcad-structure-designer/tests/structure_designer/proxy_node_test.rs`;
//! what is tested here is the three-function API the property panel talks to.
//! Three things are not widget behaviour and so must not hide behind the
//! manual-walkthrough rule (`feedback_manual_test_for_editor_ui`):
//!
//! - every accessor resolves through `scope_path` rather than by bare node id
//!   (a body node and a top-level node routinely share an id);
//! - the setter is a persisted mutation, so it must dirty the project and be
//!   undoable (`feedback_persisted_mutations_must_be_undoable`);
//! - `get_proxy_stats` reads the **selected node's eval cache**, which is the
//!   only path the report can travel — the stats are deliberately not on the
//!   node data (§7.6).

use atomcad_crystolecule::atomic_structure::AtomicStructure;
use atomcad_crystolecule::atomic_structure::inline_bond::BOND_SINGLE;
use atomcad_crystolecule::proxy_cut::{ProxyOptions, proxy_cut};
use atomcad_structure_designer::evaluator::network_result::{MoleculeData, NetworkResult};
use atomcad_structure_designer::nodes::proxy::ProxyData;
use atomcad_structure_designer::nodes::value::ValueData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::{DVec2, DVec3};
use rust_lib_flutter_cad::api::structure_designer::proxy_api::{
    proxy_node_data, proxy_node_stats, set_proxy_node_data,
};
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIProxyData;

// ============================================================================
// Fixture
// ============================================================================

const CC: f64 = 1.54;

/// `C0–C1–C2–C3–C4` along x, three hydrogens on each end carbon so no chain
/// atom has a single bond (an atom with exactly one bond is a *rider*, §4.1,
/// and a bare chain's ends would get no distance at all). `C0` carries the
/// `focus` tag, so the bond distances from it are simply `0, 1, 2, 3, 4`.
///
/// The same fixture the node test uses, for the same reason: no dropped heavy
/// atom here has two kept heavy neighbours, so `fill` never fires and the
/// counts are exactly the plain cut.
fn capped_chain() -> AtomicStructure {
    let mut s = AtomicStructure::new();
    let carbons: Vec<u32> = (0..5)
        .map(|i| s.add_atom(6, DVec3::new(i as f64 * CC, 0.0, 0.0)))
        .collect();
    for pair in carbons.windows(2) {
        s.add_bond(pair[0], pair[1], BOND_SINGLE);
    }
    for (carbon, sign) in [(carbons[0], -1.0f64), (carbons[4], 1.0f64)] {
        let base = s.get_atom(carbon).unwrap().position;
        for (dy, dz) in [(1.0, 0.0), (-0.5, 0.87), (-0.5, -0.87)] {
            let h = s.add_atom(1, base + DVec3::new(sign * 0.4, dy, dz));
            s.add_bond(carbon, h, BOND_SINGLE);
        }
    }
    s.add_atom_tag(carbons[0], "focus").unwrap();
    s
}

// ============================================================================
// Harness helpers
// ============================================================================

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("test");
    designer.set_active_node_network_name(Some("test".to_string()));
    designer
}

fn add_value_node(designer: &mut StructureDesigner, position: DVec2) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut("test")
        .unwrap();
    network.add_node(
        "value",
        position,
        0,
        Box::new(ValueData {
            value: NetworkResult::Molecule(MoleculeData {
                atoms: capped_chain(),
                geo_tree_root: None,
            }),
        }),
    )
}

/// A `proxy` node with the stored properties the panel would have written.
fn add_proxy_node(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    hops: i32,
    rim: i32,
) -> u64 {
    let node_id = if scope_path.is_empty() {
        designer.add_node("proxy", DVec2::new(200.0, 0.0))
    } else {
        designer.add_node_scoped(scope_path, "proxy", DVec2::new(200.0, 0.0), None)
    };
    let data = ProxyData {
        hops,
        rim,
        ..ProxyData::default()
    };
    designer.set_node_network_data_scoped(scope_path, node_id, Box::new(data));
    node_id
}

/// The eight persisted fields, all away from their defaults, so a field the
/// round trip drops shows up as a mismatch rather than as a coincidence.
fn non_default_twin() -> APIProxyData {
    APIProxyData {
        focus: "site".to_string(),
        hops: 4,
        rim: 2,
        rm_single: false,
        passivate: false,
        passiv_elem: 9,
        core: 1,
        fill: false,
        // Deliberately non-empty: the setter must drop it (it is an eval-time
        // snapshot, not a property).
        available_tags: vec!["stale".to_string()],
    }
}

// ============================================================================
// The getter / setter round trip
// ============================================================================

#[test]
fn the_setter_and_getter_round_trip_all_eight_persisted_fields() {
    let mut designer = setup();
    let node_id = add_proxy_node(&mut designer, &[], 6, 3);

    set_proxy_node_data(&mut designer, &[], node_id, &non_default_twin());
    let read = proxy_node_data(&designer, &[], node_id).expect("the node is a proxy");

    assert_eq!(read.focus, "site");
    assert_eq!(read.hops, 4);
    assert_eq!(read.rim, 2);
    assert!(!read.rm_single);
    assert!(!read.passivate);
    assert_eq!(read.passiv_elem, 9);
    assert_eq!(read.core, 1);
    assert!(!read.fill);
}

#[test]
fn the_setter_drops_the_available_tags_snapshot_rather_than_storing_it() {
    let mut designer = setup();
    let node_id = add_proxy_node(&mut designer, &[], 6, 3);

    set_proxy_node_data(&mut designer, &[], node_id, &non_default_twin());

    let read = proxy_node_data(&designer, &[], node_id).expect("the node is a proxy");
    assert!(
        read.available_tags.is_empty(),
        "the panel's copy of the snapshot must not be written back as data: {:?}",
        read.available_tags
    );
}

#[test]
fn the_getter_surfaces_the_eval_time_tag_snapshot() {
    let mut designer = setup();
    let node_id = add_proxy_node(&mut designer, &[], 6, 3);

    // The node writes this in `eval`; here it is planted directly, because
    // what is under test is that the getter reads it at all.
    {
        let network = designer
            .node_type_registry
            .node_networks
            .get_mut("test")
            .unwrap();
        let data = network.nodes.get_mut(&node_id).unwrap();
        let proxy = data.data.as_any_ref().downcast_ref::<ProxyData>().unwrap();
        *proxy.available_tags.borrow_mut() = vec!["focus".to_string(), "high".to_string()];
    }

    let read = proxy_node_data(&designer, &[], node_id).expect("the node is a proxy");
    assert_eq!(
        read.available_tags,
        vec!["focus".to_string(), "high".to_string()]
    );
}

#[test]
fn the_getter_returns_none_for_a_node_of_another_type() {
    let mut designer = setup();
    let other = designer.add_node("float", DVec2::ZERO);
    assert!(proxy_node_data(&designer, &[], other).is_none());
}

#[test]
fn the_setter_leaves_a_node_of_another_type_alone() {
    let mut designer = setup();
    let other = designer.add_node("float", DVec2::ZERO);

    set_proxy_node_data(&mut designer, &[], other, &non_default_twin());

    // Still a float node, still readable as one — not silently retyped.
    assert!(proxy_node_data(&designer, &[], other).is_none());
    assert_eq!(
        designer
            .node_type_registry
            .node_networks
            .get("test")
            .unwrap()
            .nodes
            .get(&other)
            .unwrap()
            .node_type_name,
        "float"
    );
}

// ============================================================================
// `scope_path`
// ============================================================================

#[test]
fn a_node_inside_a_closure_body_is_reachable_and_a_colliding_root_id_is_not_confused() {
    let mut designer = setup();
    let closure_id = designer.add_node("closure", DVec2::ZERO);
    let body_id = add_proxy_node(&mut designer, &[closure_id], 4, 1);
    let root_id = add_proxy_node(&mut designer, &[], 7, 5);

    // Per-body `next_node_id` counters make a collision the normal case rather
    // than a coincidence; if it ever stops colliding the test below is merely
    // less interesting, not wrong.
    assert_eq!(
        proxy_node_data(&designer, &[closure_id], body_id)
            .expect("body node")
            .hops,
        4
    );
    assert_eq!(
        proxy_node_data(&designer, &[], root_id)
            .expect("root node")
            .hops,
        7
    );

    // The setter is scoped the same way: writing the body node must leave the
    // root node alone.
    let mut edited = non_default_twin();
    edited.hops = 2;
    set_proxy_node_data(&mut designer, &[closure_id], body_id, &edited);

    assert_eq!(
        proxy_node_data(&designer, &[closure_id], body_id)
            .unwrap()
            .hops,
        2
    );
    assert_eq!(proxy_node_data(&designer, &[], root_id).unwrap().hops, 7);
}

// ============================================================================
// The persisted-mutation rule
// ============================================================================

#[test]
fn the_setter_dirties_the_project_and_is_undoable() {
    let mut designer = setup();
    let node_id = add_proxy_node(&mut designer, &[], 6, 3);
    designer.set_dirty(false);

    set_proxy_node_data(&mut designer, &[], node_id, &non_default_twin());
    assert!(designer.is_dirty, "a property edit dirties the project");
    assert_eq!(proxy_node_data(&designer, &[], node_id).unwrap().hops, 4);

    assert!(designer.undo(), "a property edit is undoable");
    let restored = proxy_node_data(&designer, &[], node_id).expect("the node is still a proxy");
    assert_eq!(restored.focus, "focus");
    assert_eq!(restored.hops, 6);
    assert_eq!(restored.rim, 3);
    assert!(restored.rm_single, "the default the node was created with");
    assert!(restored.passivate);
    assert_eq!(restored.passiv_elem, 1);
    assert_eq!(restored.core, -1);
    assert!(restored.fill);
}

// ============================================================================
// The report
// ============================================================================

/// Drives a root evaluation of the selected, displayed `proxy` node — the one
/// path that fills `selected_node_eval_cache`.
fn evaluate_with_selection(designer: &mut StructureDesigner, node_id: u64) {
    designer.select_node(node_id);
    designer.set_node_display(node_id, true);
    designer.mark_full_refresh();
    let changes = designer.get_pending_changes();
    designer.refresh(&changes);
}

#[test]
fn the_stats_getter_reports_the_crates_own_figures() {
    let mut designer = setup();
    let value_id = add_value_node(&mut designer, DVec2::ZERO);
    let proxy_id = add_proxy_node(&mut designer, &[], 2, 1);
    designer.connect_nodes(value_id, 0, proxy_id, 0);
    // Deliberately no `validate_active_network()`: the `value` node's declared
    // output type is `None`, so the validator would mark the wire bad and
    // poison the node before it ever evaluates.
    evaluate_with_selection(&mut designer, proxy_id);

    let stats = proxy_node_stats(&designer).expect("the selected proxy evaluated as a root node");

    let mut direct = capped_chain();
    let expected = proxy_cut(
        &mut direct,
        "focus",
        &ProxyOptions {
            hops: 2,
            rim: 1,
            ..Default::default()
        },
    )
    .expect("the direct cut succeeds");

    assert_eq!(stats.formula, expected.formula);
    assert_eq!(stats.heavy, expected.heavy);
    assert_eq!(stats.riders, expected.riders);
    assert_eq!(stats.caps, expected.caps);
    assert_eq!(stats.free, expected.free);
    assert_eq!(stats.frozen, expected.frozen);
    assert_eq!(stats.filled, expected.filled);
    assert_eq!(stats.fill_rounds, expected.fill_rounds);
    assert_eq!(stats.farthest_hop, expected.farthest_hop);
    assert_eq!(stats.free_hops, expected.free_hops);
    assert_eq!(stats.open_valences, expected.open_valences);
    assert_eq!(stats.min_cap_pair, expected.min_cap_pair);
    assert_eq!(stats.nearest_dropped, expected.nearest_dropped);

    // The `Option` fields cross the bridge as options rather than as a
    // sentinel: this cut's two caps are far apart, so the panel prints "—".
    assert_eq!(stats.min_cap_pair, None);
}

#[test]
fn the_stats_getter_returns_none_when_another_node_type_is_selected() {
    let mut designer = setup();
    let value_id = add_value_node(&mut designer, DVec2::ZERO);
    let proxy_id = add_proxy_node(&mut designer, &[], 2, 1);
    designer.connect_nodes(value_id, 0, proxy_id, 0);
    evaluate_with_selection(&mut designer, proxy_id);
    assert!(
        proxy_node_stats(&designer).is_some(),
        "CONTROL: the proxy node's own report is there"
    );

    let relax_id = designer.add_node("relax", DVec2::new(400.0, 0.0));
    designer.connect_nodes(proxy_id, 0, relax_id, 0);
    evaluate_with_selection(&mut designer, relax_id);

    assert!(
        proxy_node_stats(&designer).is_none(),
        "a `relax` node's cache must not be read as a proxy report"
    );
}

#[test]
fn the_stats_getter_returns_none_before_the_node_has_been_evaluated_as_a_root() {
    let mut designer = setup();
    let proxy_id = add_proxy_node(&mut designer, &[], 2, 1);
    designer.select_node(proxy_id);
    assert!(proxy_node_stats(&designer).is_none());
}
