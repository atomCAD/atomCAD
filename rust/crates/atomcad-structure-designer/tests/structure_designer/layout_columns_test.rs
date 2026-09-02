//! Two Phase 1 fixes to the full Sugiyama reflow
//! (`doc/design_incremental_layout.md`, "Prerequisite: one size function" and
//! D7).
//!
//! Both are corrections to a layout that was *already* wrong, independent of
//! the incremental pass they are prerequisites for:
//!
//! - **Per-layer column width.** Columns were a fixed 210 px pitch against a
//!   fixed 160 px node width. An expanded HOF is 460 px wide, so the reflow
//!   drew it straight through the next column — "the reflow is the cure" (D4)
//!   was false for any network containing one.
//! - **Deterministic component seeds.** `find_connected_components` seeded its
//!   BFS from `HashMap` order and then sorted by size with a *stable* sort, so
//!   two equal-size disconnected components stacked in per-process-random
//!   order. Same file, same button, different drawing.

use atomcad_structure_designer::layout::{LayoutAlgorithm, compute_layout, rendered_node_size};
use atomcad_structure_designer::node_layout::nodes_overlap;
use atomcad_structure_designer::node_network::NodeNetwork;
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::text_format::edit_network;

use super::layout_test_support::empty_network;

fn built(code: &str) -> (NodeNetwork, NodeTypeRegistry) {
    let registry = NodeTypeRegistry::new();
    let mut network = empty_network();
    let result = edit_network(&mut network, &registry, code, false);
    assert!(result.success, "setup edit failed: {:?}", result.errors);
    (network, registry)
}

/// `range -> map(with a body) -> collect`: three layers, the middle one held by
/// a node three times the base width.
const HOF_CHAIN: &str = r#"
r = range { start: 0, step: 1, count: 5 }
m = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = int { value: 7 }
    output d
  }
}
c = collect { iter: m }
"#;

#[test]
fn an_expanded_hof_does_not_overlap_the_next_column() {
    let (network, registry) = built(HOF_CHAIN);
    let positions = compute_layout(&network, &registry, LayoutAlgorithm::Sugiyama);

    let mut ids: Vec<u64> = positions.keys().copied().collect();
    ids.sort_unstable();
    for (i, a) in ids.iter().enumerate() {
        for b in &ids[i + 1..] {
            let (pa, pb) = (positions[a], positions[b]);
            let (sa, sb) = (
                rendered_node_size(&network.nodes[a], &registry),
                rendered_node_size(&network.nodes[b], &registry),
            );
            assert!(
                !nodes_overlap(pa, sa, pb, sb, 0.0),
                "a full reflow must not overlap two nodes: \
                 {} at {pa:?} size {sa:?} vs {} at {pb:?} size {sb:?}",
                network.nodes[a].custom_name.as_deref().unwrap_or("?"),
                network.nodes[b].custom_name.as_deref().unwrap_or("?"),
            );
        }
    }
}

#[test]
fn a_uniform_network_keeps_the_old_column_pitch() {
    // The per-layer width must be a *widening*, not a re-tuning: a network of
    // ordinary 160 px nodes has to land exactly where it did before, or every
    // existing drawing shifts the first time someone reflows it.
    let (network, registry) = built(
        r#"
a = int { value: 1 }
b = expr { x: a, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }
c = expr { x: b, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }
"#,
    );
    let positions = compute_layout(&network, &registry, LayoutAlgorithm::Sugiyama);

    let x_of = |name: &str| {
        let id = network
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some(name))
            .unwrap()
            .id;
        positions[&id].x
    };
    assert_eq!(x_of("b") - x_of("a"), 210.0);
    assert_eq!(x_of("c") - x_of("b"), 210.0);
}

#[test]
fn two_equal_size_components_lay_out_identically_every_time() {
    // Two disconnected two-node chains: equal size, so the size sort cannot
    // separate them and the seed order decides which stacks on top.
    let (network, registry) = built(
        r#"
a1 = int { value: 1 }
a2 = expr { x: a1, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }
b1 = int { value: 2 }
b2 = expr { x: b1, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }
"#,
    );

    let first = compute_layout(&network, &registry, LayoutAlgorithm::Sugiyama);
    for _ in 0..8 {
        let again = compute_layout(&network, &registry, LayoutAlgorithm::Sugiyama);
        assert_eq!(
            first, again,
            "two runs over one network must place every node identically (D7)"
        );
    }

    // And the tie is broken by id, so the lower-id component is on top —
    // a rule, rather than whatever the hasher happened to do this process.
    let id_of = |name: &str| {
        network
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some(name))
            .unwrap()
            .id
    };
    let (a1, b1) = (id_of("a1"), id_of("b1"));
    assert!(a1 < b1, "the setup script creates `a1` first");
    assert!(
        first[&a1].y < first[&b1].y,
        "the component seeded from the lower id is laid out first"
    );
}
