//! `layout::diff_scope` — the incremental layout pass's only input
//! (`doc/design_incremental_layout.md` D1, D13).
//!
//! Nothing consumes an `EditDelta` yet; Phase 3 does. These tests pin the
//! classification, and in particular the three things a naive diff gets wrong:
//!
//! - **it is name-keyed, not id-keyed.** A `--replace` mints a fresh id for
//!   every node, so an id-keyed diff reports the entire network as `added` and
//!   hands the whole drawing to the repair passes — the exact opposite of what
//!   this design is for.
//! - **a value change is not a layout event.** Only a *footprint* change is
//!   (D13), which is what makes "did this node grow?" one comparison instead of
//!   a growing list of per-trigger detectors.
//! - **shrinking is not an event either.** Deletion leaves holes and nothing
//!   compacts (D4), so a node that got smaller is simply not in `grown`.

use std::collections::HashSet;

use atomcad_structure_designer::layout::{
    EditDelta, WireEnd, WireKey, collect_all_wires, diff_scope, scopes_inside_out,
};
use atomcad_structure_designer::node_network::{CollapseMode, NodeNetwork};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::text_format::{
    NamePath, PositionSnapshot, edit_network, snapshot_node_positions,
};

use super::layout_test_support::{empty_network, id_by_path, node_by_path};

// ============================================================================
// Helpers
// ============================================================================

struct Fixture {
    network: NodeNetwork,
    registry: NodeTypeRegistry,
    snapshot: PositionSnapshot,
    wires: HashSet<WireKey>,
}

impl Fixture {
    /// Build a network from `code`, then take the "before" pair.
    fn new(code: &str) -> Self {
        let registry = NodeTypeRegistry::new();
        let mut network = empty_network();
        let result = edit_network(&mut network, &registry, code, false);
        assert!(result.success, "setup edit failed: {:?}", result.errors);
        let snapshot = snapshot_node_positions(&network, &registry);
        let wires = collect_all_wires(&network);
        Self {
            network,
            registry,
            snapshot,
            wires,
        }
    }

    /// Apply an edit **without** refreshing the "before" pair — the shape every
    /// test here needs.
    fn edit(&mut self, code: &str, replace: bool) {
        let result = edit_network(&mut self.network, &self.registry, code, replace);
        assert!(result.success, "edit failed: {:?}", result.errors);
    }

    /// The delta for one scope, named by the chain of HOF names down to it.
    fn delta(&self, scope: &[&str]) -> EditDelta {
        let ids: Vec<u64> = (1..=scope.len())
            .map(|n| id_by_path(&self.network, &scope[..n]))
            .collect();
        let names: Vec<String> = scope.iter().map(|s| s.to_string()).collect();
        diff_scope(
            &self.network,
            &self.registry,
            &self.snapshot,
            &self.wires,
            &ids,
            &names,
        )
        .expect("scope should resolve")
    }

    fn id(&self, path: &[&str]) -> u64 {
        id_by_path(&self.network, path)
    }
}

fn path(segments: &[&str]) -> NamePath {
    segments.iter().map(|s| s.to_string()).collect()
}

const CHAIN: &str = r#"
a = int { value: 1 }
b = expr { x: a, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }
"#;

// ============================================================================
// Nodes
// ============================================================================

#[test]
fn a_value_only_change_is_not_a_layout_event() {
    let mut fixture = Fixture::new(CHAIN);
    // The subtitle is derived from the value, but `int`'s is one line either
    // way, so the footprint is unchanged and the delta must be empty.
    fixture.edit("a = int { value: 987654 }\n", false);

    assert!(
        fixture.delta(&[]).is_empty(),
        "changing a literal moved nothing: {:?}",
        fixture.delta(&[])
    );
}

#[test]
fn a_new_node_is_added_and_its_wire_with_it() {
    let mut fixture = Fixture::new(CHAIN);
    fixture.edit(
        r#"c = expr { x: b, expression: "x * 2", parameters: [{ name: "x", data_type: Int }] }"#,
        false,
    );

    let delta = fixture.delta(&[]);
    assert_eq!(delta.added, vec![fixture.id(&["c"])]);
    assert!(delta.grown.is_empty(), "{:?}", delta.grown);
    assert!(delta.removed.is_empty());
    assert_eq!(
        delta.added_wires,
        vec![WireKey {
            source: WireEnd::NodeOutput {
                path: path(&["b"]),
                pin_index: 0
            },
            destination: path(&["c"]),
            slot: atomcad_structure_designer::layout::WireSlot {
                kind: atomcad_structure_designer::node_network::ArgumentKind::External,
                index: 0
            },
        }]
    );
}

#[test]
fn a_renamed_node_reads_as_added_and_removed() {
    let mut fixture = Fixture::new(CHAIN);
    fixture.edit("delete b\n", false);
    fixture.edit(
        r#"b2 = expr { x: a, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }"#,
        false,
    );

    let delta = fixture.delta(&[]);
    assert_eq!(delta.added, vec![fixture.id(&["b2"])]);
    assert_eq!(delta.removed, vec![path(&["b"])]);
}

#[test]
fn a_node_that_gained_a_pin_is_grown() {
    let mut fixture = Fixture::new(CHAIN);
    let before = fixture.snapshot[&path(&["b"])].footprint;
    fixture.edit(
        r#"b = expr { x: a, y: a, expression: "x + y", parameters: [{ name: "x", data_type: Int }, { name: "y", data_type: Int }] }"#,
        false,
    );

    let delta = fixture.delta(&[]);
    let after = atomcad_structure_designer::layout::rendered_node_size(
        node_by_path(&fixture.network, &["b"]),
        &fixture.registry,
    );
    assert_eq!(delta.grown, vec![(fixture.id(&["b"]), before, after)]);
    assert!(after.y > before.y, "a new pin makes the node taller");
    assert!(delta.added.is_empty());
}

#[test]
fn a_node_that_shrank_is_not_reported() {
    // The inverse of the previous test: two pins down to one. Nothing compacts
    // (D4), so a shrink is not a layout event at all.
    let mut fixture = Fixture::new(
        r#"
a = int { value: 1 }
b = expr { x: a, y: a, expression: "x + y", parameters: [{ name: "x", data_type: Int }, { name: "y", data_type: Int }] }
"#,
    );
    fixture.edit(
        r#"b = expr { x: a, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }"#,
        false,
    );

    let delta = fixture.delta(&[]);
    assert!(delta.grown.is_empty(), "a shrink is not growth: {delta:?}");
}

// ============================================================================
// Wires
// ============================================================================

#[test]
fn shrinking_an_array_pin_yields_a_removed_wire_and_no_growth() {
    // A mentioned property assigns that pin's *whole* inbound wire set
    // (`text_format/AGENTS.md`), which is what lets an array pin shrink. Wire
    // removal is layout-inert — it can neither create an overlap nor flip a
    // wire — so it must be reported and must not drag anything into `grown`.
    let mut fixture = Fixture::new(
        r#"
s1 = sphere { radius: 1 }
s2 = sphere { radius: 2 }
u = union { shapes: [s1, s2] }
"#,
    );
    fixture.edit("u = union { shapes: [s1] }\n", false);

    let delta = fixture.delta(&[]);
    assert_eq!(delta.removed_wires.len(), 1, "{:?}", delta.removed_wires);
    assert_eq!(
        delta.removed_wires[0].source,
        WireEnd::NodeOutput {
            path: path(&["s2"]),
            pin_index: 0
        }
    );
    assert!(delta.added_wires.is_empty());
    assert!(delta.grown.is_empty(), "{:?}", delta.grown);
}

#[test]
fn rewiring_a_consumer_is_one_removal_and_one_addition() {
    let mut fixture = Fixture::new(
        r#"
a = int { value: 1 }
a2 = int { value: 2 }
b = expr { x: a, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }
"#,
    );
    fixture.edit(
        r#"b = expr { x: a2, expression: "x + 1", parameters: [{ name: "x", data_type: Int }] }"#,
        false,
    );

    let delta = fixture.delta(&[]);
    assert_eq!(
        delta
            .removed_wires
            .iter()
            .map(|w| w.source.path().clone())
            .collect::<Vec<_>>(),
        vec![path(&["a"])]
    );
    assert_eq!(
        delta
            .added_wires
            .iter()
            .map(|w| w.source.path().clone())
            .collect::<Vec<_>>(),
        vec![path(&["a2"])]
    );
    assert!(delta.added.is_empty());
    assert!(delta.grown.is_empty(), "{:?}", delta.grown);
}

#[test]
fn a_replace_of_an_unchanged_script_classifies_nothing_in_any_scope() {
    // The round-trip that motivated D14: a body-bearing network serialized and
    // fed straight back through `--replace`. Every node is re-created with a
    // fresh id, so an id-keyed diff would report the whole drawing as new.
    let mut fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output d
  }
}
"#,
    );
    let text = atomcad_structure_designer::text_format::serialize_network(
        &fixture.network,
        &fixture.registry,
        None,
    );
    fixture.edit(&text, true);

    for scope in [vec![], vec!["m"]] {
        let delta = fixture.delta(&scope);
        assert!(
            delta.is_empty(),
            "a --replace round-trip is a no-op for layout, but scope {scope:?} got {delta:?}"
        );
    }
}

// ============================================================================
// D13: the footprint comparison subsumes the per-trigger detectors
// ============================================================================

#[test]
fn unwiring_f_on_an_auto_mode_hof_classifies_it_grown() {
    // `map` in `Auto` mode renders compact while its `f` pin is wired, and
    // re-expands when the wire goes. That is a 300-px footprint change with no
    // node added, no pin added and no property touched — and D13 catches it as
    // an ordinary growth rather than needing a rule of its own.
    let mut fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
inc = closure { kind: "custom", params: ["x"], type_args: [Int, Int], body { p = int { value: 1 } output p } }
m = map { xs: r, f: inc, input_type: Int, output_type: Int }
"#,
    );
    let before = fixture.snapshot[&path(&["m"])].footprint;

    // Unwiring a function pin has no text-format spelling — a wire-only pin
    // rejects a literal — so this is the GUI action, done directly. What is
    // being pinned is the *classification*, not how the wire went away.
    let m = fixture.id(&["m"]);
    let f_index = fixture
        .registry
        .get_node_type("map")
        .unwrap()
        .parameters
        .iter()
        .position(|p| p.name == "f")
        .expect("`map` has an `f` pin");
    fixture.network.nodes.get_mut(&m).unwrap().arguments[f_index]
        .incoming_wires
        .clear();

    let delta = fixture.delta(&[]);
    let grown_ids: Vec<u64> = delta.grown.iter().map(|(id, _, _)| *id).collect();
    assert!(
        grown_ids.contains(&m),
        "unwiring `f` re-expands the body, which is growth: {delta:?}"
    );
    let after = atomcad_structure_designer::layout::rendered_node_size(
        node_by_path(&fixture.network, &["m"]),
        &fixture.registry,
    );
    assert!(after.x > before.x);
}

#[test]
fn a_body_statement_grows_the_owning_hof_in_the_parent() {
    let mut fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = int { value: 1 }
    output d
  }
}
"#,
    );

    // A node far enough right to push the body past its stored width.
    fixture.edit("m/e = int { value: 2 }\n", false);
    let e = id_by_path(&fixture.network, &["m", "e"]);
    let m = fixture.id(&["m"]);
    let body = fixture
        .network
        .nodes
        .get_mut(&m)
        .unwrap()
        .zone_mut()
        .unwrap();
    body.nodes.get_mut(&e).unwrap().position = glam::DVec2::new(600.0, 10.0);

    let body_delta = fixture.delta(&["m"]);
    assert_eq!(body_delta.added, vec![e], "the body sees a new node");

    let parent_delta = fixture.delta(&[]);
    let grown_ids: Vec<u64> = parent_delta.grown.iter().map(|(id, _, _)| *id).collect();
    assert!(
        grown_ids.contains(&m),
        "the HOF's footprint grew with its body, so the parent must repair it: {parent_delta:?}"
    );
}

#[test]
fn a_node_added_to_a_body_with_slack_leaves_the_parent_delta_empty() {
    let mut fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = int { value: 1 }
    output d
  }
}
"#,
    );

    fixture.edit("m/e = int { value: 2 }\n", false);
    let e = id_by_path(&fixture.network, &["m", "e"]);
    let m = fixture.id(&["m"]);
    let body = fixture
        .network
        .nodes
        .get_mut(&m)
        .unwrap()
        .zone_mut()
        .unwrap();
    // Well inside the stored 320x180 body.
    body.nodes.get_mut(&e).unwrap().position = glam::DVec2::new(10.0, 90.0);

    assert_eq!(fixture.delta(&["m"]).added, vec![e]);
    assert!(
        fixture.delta(&[]).grown.is_empty(),
        "growth inside the stored body size never reaches the parent (D4/D12)"
    );
}

// ============================================================================
// Cross-scope wires
// ============================================================================

#[test]
fn a_zone_input_wire_is_reported_and_marked_cross_scope() {
    let fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output d
  }
}
"#,
    );

    let wires = collect_all_wires(&fixture.network);
    let zone_input = wires
        .iter()
        .find(|w| matches!(w.source, WireEnd::ZoneInput { .. }))
        .expect("`$element` is a zone-input wire");
    assert_eq!(zone_input.destination, path(&["m", "d"]));
    assert_eq!(zone_input.source.path(), &path(&["m"]));
    assert!(zone_input.is_cross_scope());

    // The body's `output` terminates on the HOF itself, one scope up.
    let body_output = wires
        .iter()
        .find(|w| {
            w.destination == path(&["m"])
                && w.slot.kind == atomcad_structure_designer::node_network::ArgumentKind::ZoneOutput
        })
        .expect("the body's `output` is a wire on the HOF's zone-output pin");
    assert!(body_output.is_cross_scope());
}

// ============================================================================
// The scope walk (D12)
// ============================================================================

#[test]
fn scopes_are_enumerated_deepest_first() {
    let fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = int { value: 1 }
    output d
  }
}
c = closure {
  kind: "custom",
  params: [],
  type_args: [Int],
  body {
    p = int { value: 2 }
    output p
  }
}
"#,
    );

    let scopes = scopes_inside_out(&fixture.network);
    let names: Vec<Vec<String>> = scopes.iter().map(|(_, names)| names.clone()).collect();
    assert_eq!(
        names,
        vec![path(&["c"]), path(&["m"]), Vec::<String>::new()],
        "bodies settle before the parent that has to repair their owners' footprints, \
         and equal depths come in name order so the walk is deterministic"
    );
}

#[test]
fn a_collapsed_hof_survives_a_replace_round_trip() {
    // D14, the concrete motivation: `collapse_mode` and the stored body size
    // have no text-format spelling, so a `--replace` used to reset every HOF to
    // 320x180 and to `Auto`, re-expanding a body the user had collapsed.
    let mut fixture = Fixture::new(
        r#"
r = range { start: 0, step: 1, count: 3 }
m = map { xs: r, input_type: Int, output_type: Int }
"#,
    );
    let m = fixture.id(&["m"]);
    {
        let node = fixture.network.nodes.get_mut(&m).unwrap();
        node.collapse_mode = CollapseMode::Collapsed;
        node.body_width = 512.0;
        node.body_height = 256.0;
    }

    let text = atomcad_structure_designer::text_format::serialize_network(
        &fixture.network,
        &fixture.registry,
        None,
    );
    fixture.edit(&text, true);

    let node = node_by_path(&fixture.network, &["m"]);
    assert_eq!(node.collapse_mode, CollapseMode::Collapsed);
    assert_eq!((node.body_width, node.body_height), (512.0, 256.0));
}
