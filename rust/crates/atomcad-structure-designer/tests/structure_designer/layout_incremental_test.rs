//! Step 2 of the incremental layout pass — repair grown nodes
//! (`doc/design_incremental_layout.md` §Algorithm).
//!
//! The primitives themselves are covered in `layout_motion_test.rs`; what is
//! specific here is that Step 2 drives them from an `EditDelta`, in ascending
//! id order, re-reading positions between entries.

use std::collections::{HashMap, HashSet};

use glam::DVec2;

use atomcad_structure_designer::layout::delta::EditDelta;
use atomcad_structure_designer::layout::incremental::repair_grown;
use atomcad_structure_designer::layout::motion::Rect;
use atomcad_structure_designer::node_data::NoData;
use atomcad_structure_designer::node_network::NodeNetwork;

/// A network under test plus the `sizes` map the pass reads it through.
struct Scene {
    network: NodeNetwork,
    sizes: HashMap<u64, DVec2>,
}

impl Scene {
    fn new() -> Self {
        Self {
            network: NodeNetwork::new_empty(),
            sizes: HashMap::new(),
        }
    }

    fn node(&mut self, pos: DVec2, size: DVec2) -> u64 {
        let id = self.network.add_node("union", pos, 2, Box::new(NoData {}));
        self.sizes.insert(id, size);
        id
    }

    fn plain(&mut self, x: f64, y: f64) -> u64 {
        self.node(DVec2::new(x, y), DVec2::new(160.0, 83.0))
    }

    fn pos(&self, id: u64) -> DVec2 {
        self.network.nodes[&id].position
    }

    fn rect(&self, id: u64) -> Rect {
        Rect::new(self.pos(id), self.sizes[&id])
    }

    fn no_overlaps(&self) {
        let mut all: Vec<u64> = self.sizes.keys().copied().collect();
        all.sort_unstable();
        for (i, &a) in all.iter().enumerate() {
            for &b in &all[i + 1..] {
                assert!(
                    !self.rect(a).overlaps(&self.rect(b), 0.0),
                    "nodes {a} {:?} and {b} {:?} overlap",
                    self.rect(a),
                    self.rect(b)
                );
            }
        }
    }
}

fn delta_with_grown(grown: Vec<(u64, DVec2, DVec2)>) -> EditDelta {
    EditDelta {
        grown,
        ..EditDelta::default()
    }
}

#[test]
fn an_empty_delta_moves_nothing() {
    let mut scene = Scene::new();
    scene.plain(0.0, 0.0);
    scene.plain(300.0, 0.0);
    let before: HashMap<u64, DVec2> = scene
        .network
        .nodes
        .iter()
        .map(|(&id, n)| (id, n.position))
        .collect();

    let moves = repair_grown(&mut scene.network, &scene.sizes, &EditDelta::default());

    assert!(moves.is_empty());
    for (id, position) in before {
        assert_eq!(scene.pos(id), position);
    }
}

#[test]
fn one_grown_node_pushes_its_right_neighbour_and_nothing_else() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(500.0, 300.0));
    let right = scene.plain(600.0, 0.0);
    let left = scene.plain(-400.0, 0.0);
    let below = scene.plain(0.0, 600.0);

    let moves = repair_grown(
        &mut scene.network,
        &scene.sizes,
        &delta_with_grown(vec![(
            grown,
            DVec2::new(400.0, 300.0),
            DVec2::new(500.0, 300.0),
        )]),
    );

    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0].0, right);
    assert_eq!(moves[0].1, DVec2::new(600.0, 0.0));
    assert_eq!(moves[0].2, DVec2::new(700.0, 0.0));
    assert_eq!(scene.pos(left), DVec2::new(-400.0, 0.0));
    assert_eq!(scene.pos(below), DVec2::new(0.0, 600.0));
    scene.no_overlaps();
}

/// Two grown nodes in one scope compose rather than fight: the second one's
/// shift line sees where the first one left things, and a node moved by both
/// is reported once, with its net displacement.
#[test]
fn two_grown_nodes_compose_and_a_doubly_moved_node_is_reported_once() {
    let mut scene = Scene::new();
    let first = scene.node(DVec2::ZERO, DVec2::new(300.0, 200.0));
    let second = scene.node(DVec2::new(400.0, 0.0), DVec2::new(300.0, 200.0));
    let far_right = scene.plain(900.0, 0.0);

    let moves = repair_grown(
        &mut scene.network,
        &scene.sizes,
        &delta_with_grown(vec![
            (first, DVec2::new(200.0, 200.0), DVec2::new(300.0, 200.0)),
            (second, DVec2::new(200.0, 200.0), DVec2::new(300.0, 200.0)),
        ]),
    );

    // `far_right` is right of both lines, so it took both shifts — and appears
    // once, from its original position to its final one.
    let reported: HashSet<u64> = moves.iter().map(|m| m.0).collect();
    assert!(reported.contains(&far_right));
    let entry = moves.iter().find(|m| m.0 == far_right).unwrap();
    assert_eq!(entry.1, DVec2::new(900.0, 0.0));
    assert_eq!(entry.2.x - entry.1.x, 200.0, "both shifts, counted once");
    assert_eq!(moves.iter().filter(|m| m.0 == far_right).count(), 1);
    scene.no_overlaps();
}

#[test]
fn a_grown_node_never_moves_itself() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::new(120.0, 60.0), DVec2::new(500.0, 400.0));
    scene.plain(700.0, 60.0);
    scene.plain(120.0, 500.0);

    let moves = repair_grown(
        &mut scene.network,
        &scene.sizes,
        &delta_with_grown(vec![(
            grown,
            DVec2::new(400.0, 300.0),
            DVec2::new(500.0, 400.0),
        )]),
    );

    assert_eq!(scene.pos(grown), DVec2::new(120.0, 60.0));
    assert!(!moves.iter().any(|m| m.0 == grown));
    scene.no_overlaps();
}

#[test]
fn moves_are_reported_in_ascending_id_order() {
    let mut scene = Scene::new();
    let grown = scene.node(DVec2::ZERO, DVec2::new(500.0, 300.0));
    for i in 0..5 {
        scene.plain(600.0, i as f64 * 200.0);
    }

    let moves = repair_grown(
        &mut scene.network,
        &scene.sizes,
        &delta_with_grown(vec![(
            grown,
            DVec2::new(400.0, 300.0),
            DVec2::new(500.0, 300.0),
        )]),
    );

    let reported: Vec<u64> = moves.iter().map(|m| m.0).collect();
    let mut sorted = reported.clone();
    sorted.sort_unstable();
    assert_eq!(reported, sorted);
    assert_eq!(reported.len(), 5);
}
