//! Comment placement in auto-layout (`doc/design_wire_annotations.md` Phase 5).
//!
//! Comments never take part in layer assignment (D8). An anchored one is placed
//! against the target its `anchors[0]` resolves to, an unanchored one keeps its
//! offset relative to the graph's top-left (D9.1), and neither ever overlaps a
//! node or another comment.

use std::collections::HashMap;

use glam::DVec2;

use atomcad_structure_designer::layout::common::LayoutAlgorithm;
use atomcad_structure_designer::layout::compute_layout;
use atomcad_structure_designer::node_layout;
use atomcad_structure_designer::node_network::{ArgumentKind, NodeNetwork};
use atomcad_structure_designer::nodes::comment::{CommentAnchor, CommentData, WireAnchor};
use atomcad_structure_designer::structure_designer::StructureDesigner;

const BOTH: [LayoutAlgorithm; 2] = [LayoutAlgorithm::TopologicalGrid, LayoutAlgorithm::Sugiyama];

fn setup(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn network<'a>(designer: &'a StructureDesigner, name: &str) -> &'a NodeNetwork {
    designer.node_type_registry.node_networks.get(name).unwrap()
}

/// Add a comment node, sized and positioned as given.
fn add_comment(designer: &mut StructureDesigner, name: &str, position: DVec2, size: DVec2) -> u64 {
    let node_id = designer.add_node("Comment", position);
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    let comment = node
        .data
        .as_any_mut()
        .downcast_mut::<CommentData>()
        .unwrap();
    comment.width = size.x;
    comment.height = size.y;
    node_id
}

fn set_anchors(
    designer: &mut StructureDesigner,
    name: &str,
    comment_id: u64,
    anchors: Vec<CommentAnchor>,
) {
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(name)
        .unwrap();
    let node = network.nodes.get_mut(&comment_id).unwrap();
    let comment = node
        .data
        .as_any_mut()
        .downcast_mut::<CommentData>()
        .unwrap();
    comment.anchors = anchors;
}

fn comment_size(designer: &StructureDesigner, name: &str, comment_id: u64) -> DVec2 {
    let node = network(designer, name).nodes.get(&comment_id).unwrap();
    let comment = node
        .data
        .as_any_ref()
        .downcast_ref::<CommentData>()
        .unwrap();
    DVec2::new(comment.width, comment.height)
}

/// The box a *graph* node occupies, as the layout algorithms size them.
fn graph_node_box(designer: &StructureDesigner, name: &str, node_id: u64) -> (DVec2, DVec2) {
    let network = network(designer, name);
    let node = network.nodes.get(&node_id).unwrap();
    let node_type = designer
        .node_type_registry
        .get_node_type(&node.node_type_name);
    let params = node_type.map(|nt| nt.parameters.len()).unwrap_or(0);
    let outputs = node_type.map(|nt| nt.output_pin_count()).unwrap_or(1);
    (
        node.position,
        DVec2::new(
            node_layout::NODE_WIDTH,
            node_layout::estimate_node_height(params, outputs, true),
        ),
    )
}

fn boxes_overlap(a_pos: DVec2, a_size: DVec2, b_pos: DVec2, b_size: DVec2) -> bool {
    node_layout::nodes_overlap(a_pos, a_size, b_pos, b_size, 0.0)
}

/// Distance between the edges of two boxes, 0 when they touch or overlap.
fn box_gap(a_pos: DVec2, a_size: DVec2, b_pos: DVec2, b_size: DVec2) -> f64 {
    let dx = (b_pos.x - (a_pos.x + a_size.x)).max(a_pos.x - (b_pos.x + b_size.x));
    let dy = (b_pos.y - (a_pos.y + a_size.y)).max(a_pos.y - (b_pos.y + b_size.y));
    dx.max(0.0).hypot(dy.max(0.0))
}

// =============================================================================
// Tier 1: the natural spot
// =============================================================================

#[test]
fn anchored_comment_lands_beside_its_node_in_a_sparse_graph() {
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(2000.0, 2000.0),
            DVec2::new(200.0, 100.0),
        );
        set_anchors(
            &mut designer,
            "test_network",
            comment_id,
            vec![CommentAnchor::Node(sphere_id)],
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        let comment_pos = positions[&comment_id];
        let comment_size = comment_size(&designer, "test_network", comment_id);
        let sphere_pos = positions[&sphere_id];
        let (_, sphere_size) = graph_node_box(&designer, "test_network", sphere_id);

        // It is adjacent to its anchor, not parked 2000 px away where it started.
        let gap = box_gap(comment_pos, comment_size, sphere_pos, sphere_size);
        assert!(
            gap < 40.0,
            "{algorithm:?}: comment at {comment_pos:?} is {gap} px from its anchor at {sphere_pos:?}"
        );
        assert!(!boxes_overlap(
            comment_pos,
            comment_size,
            sphere_pos,
            sphere_size
        ));
    }
}

#[test]
fn anchored_comment_is_placed_at_its_real_size_not_the_pin_estimate() {
    // A resized note (400 x 300) must be placed as 400 x 300, never as the
    // 83 px `estimate_node_height` a comment's (0 params, 1 output) would give.
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(0.0, 0.0),
            DVec2::new(400.0, 300.0),
        );
        set_anchors(
            &mut designer,
            "test_network",
            comment_id,
            vec![CommentAnchor::Node(sphere_id)],
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        let comment_pos = positions[&comment_id];
        let comment_size = DVec2::new(400.0, 300.0);
        for &graph_id in &[float_id, sphere_id] {
            let (_, size) = graph_node_box(&designer, "test_network", graph_id);
            assert!(
                !boxes_overlap(comment_pos, comment_size, positions[&graph_id], size),
                "{algorithm:?}: 400x300 comment at {comment_pos:?} overlaps node {graph_id}"
            );
        }
    }
}

#[test]
fn unanchored_comment_in_clear_space_does_not_move() {
    // The graph is authored at the layout origin already, so the origin delta is
    // zero and the note's translated position *is* its current position.
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(100.0, 100.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(310.0, 100.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(100.0, 600.0),
            DVec2::new(200.0, 100.0),
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        assert_eq!(
            positions[&comment_id],
            DVec2::new(100.0, 600.0),
            "{algorithm:?}: an unanchored note in clear space must not move"
        );
    }
}

#[test]
fn anchored_comment_is_placed_against_the_wire_it_documents() {
    // The channel between two adjacent columns is 50 px wide, so a note never
    // fits *beside* an ordinary wire - it goes to the gutter, which preserves
    // the wire midpoint's centre x. That is the designed outcome, not a
    // degraded one: annotations in the margin with a leader pointing into the
    // drawing is the drafting idiom the whole pass aims at.
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(3000.0, 3000.0),
            DVec2::new(200.0, 100.0),
        );
        set_anchors(
            &mut designer,
            "test_network",
            comment_id,
            vec![CommentAnchor::Wire(WireAnchor {
                destination_node_id: sphere_id,
                destination_argument_kind: ArgumentKind::External,
                destination_argument_index: 0,
                destination_param_id: None,
                source_node_id: float_id,
            })],
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        let midpoint = (node_layout::output_pin_position(positions[&float_id], 0)
            + node_layout::input_pin_position(positions[&sphere_id], 0))
            * 0.5;
        let comment_pos = positions[&comment_id];
        let comment_size = comment_size(&designer, "test_network", comment_id);

        assert!(
            (comment_pos.x + comment_size.x / 2.0 - midpoint.x).abs() < 1.0,
            "{algorithm:?}: comment centre x {} should track the wire midpoint {midpoint:?}",
            comment_pos.x + comment_size.x / 2.0
        );
        // ...and it is genuinely placed against the wire, not left where it was.
        assert!(
            box_gap(comment_pos, comment_size, midpoint, DVec2::ZERO) < 200.0,
            "{algorithm:?}: comment at {comment_pos:?} is far from the wire {midpoint:?}"
        );
        for graph_id in [float_id, sphere_id] {
            let (_, size) = graph_node_box(&designer, "test_network", graph_id);
            assert!(!boxes_overlap(
                comment_pos,
                comment_size,
                positions[&graph_id],
                size
            ));
        }
    }
}

// =============================================================================
// Tier 2: the gutter
// =============================================================================

/// Three tall columns. Interior gaps are 50 px horizontally (`COLUMN_WIDTH` -
/// `NODE_WIDTH`) and 30 px vertically (`VERTICAL_GAP`), both far below any
/// comment's size, so a note anchored to a node in the *middle* column is
/// boxed in on all four sides and tier 1 cannot succeed.
///
/// Returns the middle column's node ids, top to bottom by construction order.
fn dense_three_columns(designer: &mut StructureDesigner, rows: usize) -> Vec<u64> {
    let mut middle = Vec::new();
    for row in 0..rows {
        let y = row as f64 * 30.0;
        let source = designer.add_node("float", DVec2::new(0.0, y));
        let sphere = designer.add_node("sphere", DVec2::new(30.0, y));
        let sink = designer.add_node("union", DVec2::new(60.0, y));
        designer.connect_nodes(source, 0, sphere, 0);
        designer.connect_nodes(sphere, 0, sink, 0);
        middle.push(sphere);
    }
    middle
}

#[test]
fn anchored_comment_in_a_dense_graph_lands_in_a_gutter_and_never_overlaps() {
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let middle = dense_three_columns(&mut designer, 6);
        let anchor_target = middle[3];

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(0.0, 0.0),
            DVec2::new(400.0, 300.0),
        );
        set_anchors(
            &mut designer,
            "test_network",
            comment_id,
            vec![CommentAnchor::Node(anchor_target)],
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        let comment_pos = positions[&comment_id];
        let comment_size = DVec2::new(400.0, 300.0);

        // Never on top of a node.
        for (&node_id, &pos) in &positions {
            if node_id == comment_id {
                continue;
            }
            let (_, size) = graph_node_box(&designer, "test_network", node_id);
            assert!(
                !boxes_overlap(comment_pos, comment_size, pos, size),
                "{algorithm:?}: comment at {comment_pos:?} overlaps node {node_id} at {pos:?}"
            );
        }

        // Outside the graph's bounding box, i.e. in a gutter.
        let graph_bbox = graph_bounding_box(&designer, "test_network", &positions, comment_id);
        let outside = comment_pos.y + comment_size.y <= graph_bbox.0.y
            || comment_pos.y >= graph_bbox.1.y
            || comment_pos.x + comment_size.x <= graph_bbox.0.x
            || comment_pos.x >= graph_bbox.1.x;
        assert!(
            outside,
            "{algorithm:?}: comment at {comment_pos:?} should be in a gutter, bbox {graph_bbox:?}"
        );

        // A horizontal gutter is preferred, and the note stays aligned to its
        // anchor's centre x rather than being flung to a corner.
        assert!(
            comment_pos.y + comment_size.y <= graph_bbox.0.y || comment_pos.y >= graph_bbox.1.y,
            "{algorithm:?}: expected the top or bottom gutter, got {comment_pos:?}"
        );
        let target_center_x = positions[&anchor_target].x + node_layout::NODE_WIDTH / 2.0;
        assert!(
            (comment_pos.x + comment_size.x / 2.0 - target_center_x).abs() < 1.0,
            "{algorithm:?}: comment centre x {} should track its anchor's {target_center_x}",
            comment_pos.x + comment_size.x / 2.0
        );
    }
}

/// The bounding box of every laid-out node except `exclude`.
fn graph_bounding_box(
    designer: &StructureDesigner,
    name: &str,
    positions: &HashMap<u64, DVec2>,
    exclude: u64,
) -> (DVec2, DVec2) {
    let mut min = DVec2::splat(f64::MAX);
    let mut max = DVec2::splat(f64::MIN);
    for (&node_id, &pos) in positions {
        if node_id == exclude {
            continue;
        }
        let (_, size) = graph_node_box(designer, name, node_id);
        min = min.min(pos);
        max = max.max(pos + size);
    }
    (min, max)
}

#[test]
fn two_comments_anchored_to_the_same_node_do_not_overlap_each_other() {
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let first = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(0.0, 0.0),
            DVec2::new(200.0, 100.0),
        );
        let second = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(0.0, 0.0),
            DVec2::new(200.0, 100.0),
        );
        for id in [first, second] {
            set_anchors(
                &mut designer,
                "test_network",
                id,
                vec![CommentAnchor::Node(sphere_id)],
            );
        }

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        assert!(
            !boxes_overlap(
                positions[&first],
                DVec2::new(200.0, 100.0),
                positions[&second],
                DVec2::new(200.0, 100.0),
            ),
            "{algorithm:?}: {:?} and {:?} overlap",
            positions[&first],
            positions[&second]
        );
    }
}

#[test]
fn anchored_comment_follows_its_target_when_the_target_changes_column() {
    // The whole point of a stored anchor: the note tracks what it documents
    // across a relayout, instead of being left where proximity used to put it.
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(0.0, 0.0),
            DVec2::new(200.0, 100.0),
        );
        set_anchors(
            &mut designer,
            "test_network",
            comment_id,
            vec![CommentAnchor::Node(sphere_id)],
        );

        // `sphere` starts as a source node, i.e. in column 0.
        let before = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );
        let before_x = before[&sphere_id].x;

        // Wiring `float` into it pushes it to column 1.
        designer.connect_nodes(float_id, 0, sphere_id, 0);
        let after = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );
        let after_x = after[&sphere_id].x;
        assert!(
            after_x > before_x,
            "{algorithm:?}: the anchor target should have moved right a column"
        );

        // The note moved with it: still adjacent, and by roughly the same shift.
        let (_, sphere_size) = graph_node_box(&designer, "test_network", sphere_id);
        let comment_size = DVec2::new(200.0, 100.0);
        assert!(
            box_gap(
                after[&comment_id],
                comment_size,
                after[&sphere_id],
                sphere_size
            ) < 40.0,
            "{algorithm:?}: comment at {:?} left behind by its anchor at {:?}",
            after[&comment_id],
            after[&sphere_id]
        );
        assert!(
            (after[&comment_id].x - before[&comment_id].x - (after_x - before_x)).abs() < 1.0,
            "{algorithm:?}: the note should have shifted with its anchor"
        );
    }
}

// =============================================================================
// D9.1: the origin delta
// =============================================================================

#[test]
fn unanchored_comment_keeps_its_offset_relative_to_the_graph_origin() {
    // The graph is authored far from the layout origin. Layout canonicalizes it
    // back to (100, 100); the note must travel with it rather than be left
    // 3000 px away from its own drawing.
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(3000.0, 2000.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(3210.0, 2000.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(3000.0, 2500.0),
            DVec2::new(200.0, 100.0),
        );

        let before_origin = DVec2::new(3000.0, 2000.0);
        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        let after_origin = DVec2::new(
            positions.values().map(|p| p.x).fold(f64::MAX, f64::min),
            positions
                .iter()
                .filter(|&(&id, _)| id != comment_id)
                .map(|(_, p)| p.y)
                .fold(f64::MAX, f64::min),
        );
        let expected = DVec2::new(3000.0, 2500.0) + (after_origin - before_origin);
        assert_eq!(
            positions[&comment_id], expected,
            "{algorithm:?}: the note should translate with the drawing"
        );
    }
}

// =============================================================================
// anchors[0], determinism, and D8
// =============================================================================

#[test]
fn placement_uses_the_first_anchor() {
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
        let cuboid_id = designer.add_node("cuboid", DVec2::new(300.0, 400.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);
        designer.connect_nodes(sphere_id, 0, cuboid_id, 0);

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(0.0, 0.0),
            DVec2::new(200.0, 100.0),
        );
        set_anchors(
            &mut designer,
            "test_network",
            comment_id,
            vec![
                CommentAnchor::Node(float_id),
                CommentAnchor::Node(cuboid_id),
            ],
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );

        let comment_pos = positions[&comment_id];
        let comment_size = DVec2::new(200.0, 100.0);
        let (_, float_size) = graph_node_box(&designer, "test_network", float_id);
        let (_, cuboid_size) = graph_node_box(&designer, "test_network", cuboid_id);
        let to_first = box_gap(comment_pos, comment_size, positions[&float_id], float_size);
        let to_second = box_gap(
            comment_pos,
            comment_size,
            positions[&cuboid_id],
            cuboid_size,
        );
        assert!(
            to_first < to_second,
            "{algorithm:?}: placed against anchors[1] ({to_second}) rather than anchors[0] ({to_first})"
        );
    }
}

#[test]
fn comments_do_not_influence_the_graphs_own_column_assignment() {
    // D8: the graph must lay out identically with and without comments in it.
    for algorithm in BOTH {
        let mut bare = setup("test_network");
        let float_id = bare.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = bare.add_node("sphere", DVec2::new(300.0, 0.0));
        let cuboid_id = bare.add_node("cuboid", DVec2::new(300.0, 400.0));
        bare.connect_nodes(float_id, 0, sphere_id, 0);
        bare.connect_nodes(sphere_id, 0, cuboid_id, 0);
        let bare_positions = compute_layout(
            network(&bare, "test_network"),
            &bare.node_type_registry,
            algorithm,
        );

        let mut annotated = setup("test_network");
        let float_id = annotated.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = annotated.add_node("sphere", DVec2::new(300.0, 0.0));
        let cuboid_id = annotated.add_node("cuboid", DVec2::new(300.0, 400.0));
        annotated.connect_nodes(float_id, 0, sphere_id, 0);
        annotated.connect_nodes(sphere_id, 0, cuboid_id, 0);
        let anchored = add_comment(
            &mut annotated,
            "test_network",
            DVec2::new(-500.0, -500.0),
            DVec2::new(400.0, 300.0),
        );
        set_anchors(
            &mut annotated,
            "test_network",
            anchored,
            vec![CommentAnchor::Node(sphere_id)],
        );
        add_comment(
            &mut annotated,
            "test_network",
            DVec2::new(900.0, 900.0),
            DVec2::new(200.0, 100.0),
        );
        let annotated_positions = compute_layout(
            network(&annotated, "test_network"),
            &annotated.node_type_registry,
            algorithm,
        );

        for graph_id in [float_id, sphere_id, cuboid_id] {
            assert_eq!(
                bare_positions[&graph_id], annotated_positions[&graph_id],
                "{algorithm:?}: node {graph_id} moved because comments were present"
            );
        }
    }
}

#[test]
fn placement_is_deterministic_across_runs() {
    // Several same-sized comments anchored to the same node tie on everything
    // but their node id; without the id tiebreak the result would follow
    // `HashMap` iteration order, which std randomizes per process.
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let mut comment_ids = Vec::new();
        for _ in 0..5 {
            let id = add_comment(
                &mut designer,
                "test_network",
                DVec2::new(0.0, 0.0),
                DVec2::new(200.0, 100.0),
            );
            set_anchors(
                &mut designer,
                "test_network",
                id,
                vec![CommentAnchor::Node(sphere_id)],
            );
            comment_ids.push(id);
        }

        let first = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );
        for _ in 0..8 {
            let again = compute_layout(
                network(&designer, "test_network"),
                &designer.node_type_registry,
                algorithm,
            );
            for &id in &comment_ids {
                assert_eq!(
                    first[&id], again[&id],
                    "{algorithm:?}: placement of {id} varies"
                );
            }
        }
    }
}

#[test]
fn a_dangling_anchor_falls_back_to_the_unanchored_rule() {
    // `place_comments` must not panic or misplace when an anchor points at a
    // node that is gone; the note is simply translated like any unanchored one.
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let float_id = designer.add_node("float", DVec2::new(100.0, 100.0));
        let sphere_id = designer.add_node("sphere", DVec2::new(310.0, 100.0));
        designer.connect_nodes(float_id, 0, sphere_id, 0);

        let comment_id = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(100.0, 600.0),
            DVec2::new(200.0, 100.0),
        );
        set_anchors(
            &mut designer,
            "test_network",
            comment_id,
            vec![CommentAnchor::Node(9999)],
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );
        assert_eq!(positions[&comment_id], DVec2::new(100.0, 600.0));
    }
}

#[test]
fn a_comment_only_network_lays_out_at_the_origin() {
    for algorithm in BOTH {
        let mut designer = setup("test_network");
        let first = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(2000.0, 2000.0),
            DVec2::new(200.0, 100.0),
        );
        let second = add_comment(
            &mut designer,
            "test_network",
            DVec2::new(2000.0, 2400.0),
            DVec2::new(200.0, 100.0),
        );

        let positions = compute_layout(
            network(&designer, "test_network"),
            &designer.node_type_registry,
            algorithm,
        );
        assert_eq!(positions[&first], DVec2::new(100.0, 100.0));
        assert_eq!(positions[&second], DVec2::new(100.0, 500.0));
    }
}

// =============================================================================
// The whole reflow is one undo step
// =============================================================================

#[test]
fn laying_out_an_annotated_network_is_a_single_undo_step() {
    let mut designer = setup("test_network");
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    let sphere_id = designer.add_node("sphere", DVec2::new(300.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    let comment_id = add_comment(
        &mut designer,
        "test_network",
        DVec2::new(2000.0, 2000.0),
        DVec2::new(200.0, 100.0),
    );
    set_anchors(
        &mut designer,
        "test_network",
        comment_id,
        vec![CommentAnchor::Node(sphere_id)],
    );

    let before: HashMap<u64, DVec2> = network(&designer, "test_network")
        .nodes
        .iter()
        .map(|(&id, node)| (id, node.position))
        .collect();

    assert!(designer.layout_active_network());
    let moved = network(&designer, "test_network")
        .nodes
        .get(&comment_id)
        .unwrap()
        .position;
    assert_ne!(moved, before[&comment_id], "the note should have moved");

    assert!(designer.undo());
    for (&node_id, &position) in &before {
        assert_eq!(
            network(&designer, "test_network")
                .nodes
                .get(&node_id)
                .unwrap()
                .position,
            position,
            "one undo must restore node {node_id}"
        );
    }
}
