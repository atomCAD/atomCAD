//! Phase 1 of `doc/design_zero_ary_closure_body_display.md` — **scene re-keying
//! by `NodeRef`**.
//!
//! `StructureDesignerScene.node_data` and the invisible-node LRU cache used to
//! be keyed by a bare `u64` node id. Because every zone body carries its own
//! `next_node_id` counter, a body node and a top-level node routinely share a
//! numeric id, so displaying body nodes (Phase 2) requires a scope-aware key.
//! Phase 1 performs that re-keying with no user-visible change.
//!
//! Production code only ever constructs `NodeRef::top(..)` keys until Phase 2,
//! so the collision behavior these tests pin down cannot be exercised through
//! the refresh pipeline yet. They therefore drive the scene container
//! **directly**, which is exactly what makes them the Phase 1 acceptance gate:
//! they assert the property the whole refactor exists to buy (colliding ids stay
//! distinct) before any producer depends on it.
//!
//! The end-to-end behavior-preservation gate lives in `refresh_pipeline_test.rs`
//! (Phase 0), which is driven through `StructureDesigner::refresh`.

use glam::f64::DVec3;
use rust_lib_flutter_cad::crystolecule::atomic_structure::AtomicStructure;
use rust_lib_flutter_cad::structure_designer::node_network::NodeRef;
use rust_lib_flutter_cad::structure_designer::structure_designer_scene::{
    NodeOutput, NodeSceneData, StructureDesignerScene,
};
use std::collections::HashSet;

// ============================================================================
// Helpers
// ============================================================================

/// A scene entry whose atom count identifies it — the cheapest way to tell two
/// entries apart after a round-trip through the map or the LRU cache.
fn entry_with_atoms(count: usize) -> NodeSceneData {
    let mut structure = AtomicStructure::new();
    for i in 0..count {
        structure.add_atom(6, DVec3::new(i as f64, 0.0, 0.0));
    }
    NodeSceneData::new(NodeOutput::Atomic(structure, None))
}

fn atom_count(scene: &StructureDesignerScene, node_ref: &NodeRef) -> usize {
    match &scene
        .node_data
        .get(node_ref)
        .expect("expected a live scene entry")
        .output
    {
        NodeOutput::Atomic(structure, _) => structure.get_num_of_atoms(),
        _ => panic!("expected an Atomic scene output"),
    }
}

// ============================================================================
// Live map: colliding ids stay distinct
// ============================================================================

#[test]
fn colliding_node_ids_in_different_scopes_get_separate_scene_entries() {
    let top = NodeRef::top(7);
    let body = NodeRef::scoped(&[3], 7);
    let nested = NodeRef::scoped(&[3, 5], 7);

    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(top.clone(), entry_with_atoms(1));
    scene.node_data.insert(body.clone(), entry_with_atoms(2));
    scene.node_data.insert(nested.clone(), entry_with_atoms(3));

    assert_eq!(
        scene.node_data.len(),
        3,
        "three refs sharing node id 7 must occupy three distinct slots"
    );
    assert_eq!(atom_count(&scene, &top), 1);
    assert_eq!(atom_count(&scene, &body), 2);
    assert_eq!(atom_count(&scene, &nested), 3);
}

#[test]
fn scope_path_order_is_part_of_the_key() {
    // `[3, 5]` and `[5, 3]` are different chains of body owners, so they must
    // not alias — a `HashSet`-like or order-insensitive key would break this.
    let a = NodeRef::scoped(&[3, 5], 7);
    let b = NodeRef::scoped(&[5, 3], 7);

    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(a.clone(), entry_with_atoms(1));
    scene.node_data.insert(b.clone(), entry_with_atoms(2));

    assert_eq!(scene.node_data.len(), 2);
    assert_eq!(atom_count(&scene, &a), 1);
    assert_eq!(atom_count(&scene, &b), 2);
}

// ============================================================================
// Invisible cache: move / restore round-trip
// ============================================================================

#[test]
fn move_to_cache_and_restore_from_cache_round_trip_a_scoped_ref() {
    let body = NodeRef::scoped(&[3], 7);

    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(body.clone(), entry_with_atoms(4));

    assert!(scene.move_to_cache(&body), "the live entry must be found");
    assert!(
        !scene.node_data.contains_key(&body),
        "a cached entry leaves the live map"
    );
    assert_eq!(scene.cached_node_count(), 1);

    assert!(
        scene.restore_from_cache(&body),
        "the cached entry must be found under the same scoped key"
    );
    assert_eq!(scene.cached_node_count(), 0);
    assert_eq!(
        atom_count(&scene, &body),
        4,
        "the restored entry must be the one that was cached"
    );
}

#[test]
fn restoring_one_of_two_colliding_ids_restores_the_right_entry() {
    let top = NodeRef::top(7);
    let body = NodeRef::scoped(&[3], 7);

    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(top.clone(), entry_with_atoms(1));
    scene.node_data.insert(body.clone(), entry_with_atoms(2));

    // Both go to the cache, then only the body one comes back.
    assert!(scene.move_to_cache(&top));
    assert!(scene.move_to_cache(&body));
    assert_eq!(scene.cached_node_count(), 2);

    assert!(scene.restore_from_cache(&body));
    assert_eq!(
        scene.node_data.len(),
        1,
        "restoring one ref must not drag its colliding twin along"
    );
    assert_eq!(atom_count(&scene, &body), 2);
    assert!(
        !scene.node_data.contains_key(&top),
        "the top-level twin must still be cached, not live"
    );
    assert_eq!(scene.cached_node_count(), 1);

    // …and the twin is still intact and independently restorable.
    assert!(scene.restore_from_cache(&top));
    assert_eq!(atom_count(&scene, &top), 1);
}

#[test]
fn restore_from_cache_misses_when_the_scope_path_differs() {
    // The silent-cache-miss failure mode in reverse: a key built with the wrong
    // scope must NOT find another scope's entry.
    let body = NodeRef::scoped(&[3], 7);

    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(body.clone(), entry_with_atoms(4));
    assert!(scene.move_to_cache(&body));

    assert!(
        !scene.restore_from_cache(&NodeRef::top(7)),
        "a top-level key must not restore a body entry"
    );
    assert!(
        !scene.restore_from_cache(&NodeRef::scoped(&[9], 7)),
        "a different body's key must not restore this body's entry"
    );
    assert_eq!(scene.cached_node_count(), 1, "the entry stays cached");
    assert!(scene.restore_from_cache(&body), "the right key still works");
}

// ============================================================================
// Invisible cache: targeted invalidation
// ============================================================================

#[test]
fn invalidate_cached_nodes_with_a_scoped_ref_leaves_the_colliding_twin_intact() {
    let top = NodeRef::top(7);
    let body = NodeRef::scoped(&[3], 7);

    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(top.clone(), entry_with_atoms(1));
    scene.node_data.insert(body.clone(), entry_with_atoms(2));
    assert!(scene.move_to_cache(&top));
    assert!(scene.move_to_cache(&body));

    scene.invalidate_cached_nodes(&HashSet::from([body.clone()]));

    assert_eq!(
        scene.cached_node_count(),
        1,
        "only the body entry is evicted"
    );
    assert!(
        !scene.restore_from_cache(&body),
        "the invalidated entry is gone, so it must be re-evaluated"
    );
    assert!(
        scene.restore_from_cache(&top),
        "the colliding top-level entry must survive the eviction"
    );
    assert_eq!(atom_count(&scene, &top), 1);
}

// ============================================================================
// Invisible cache: pin-set updates
// ============================================================================

#[test]
fn update_cached_displayed_pins_targets_exactly_one_scope() {
    let top = NodeRef::top(7);
    let body = NodeRef::scoped(&[3], 7);

    let mut scene = StructureDesignerScene::new();
    scene.node_data.insert(top.clone(), entry_with_atoms(1));
    scene.node_data.insert(body.clone(), entry_with_atoms(2));
    assert!(scene.move_to_cache(&top));
    assert!(scene.move_to_cache(&body));

    scene.update_cached_displayed_pins(&body, HashSet::from([1]));

    assert!(scene.restore_from_cache(&body));
    assert!(scene.restore_from_cache(&top));
    assert_eq!(
        scene.node_data.get(&body).unwrap().displayed_pins,
        HashSet::from([1]),
        "the targeted entry's pin set is updated"
    );
    assert_eq!(
        scene.node_data.get(&top).unwrap().displayed_pins,
        HashSet::from([0]),
        "the colliding twin keeps the default pin set"
    );
}
