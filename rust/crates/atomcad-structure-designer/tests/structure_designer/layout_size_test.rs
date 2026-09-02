//! `layout::rendered_node_size` — the one node-size function, and its parity
//! with the Flutter rule that actually renders.
//!
//! `doc/design_incremental_layout.md`, "Prerequisite: one size function".
//! Before this, four functions disagreed; the two disagreements that mattered
//! were comments (sized by pin count in two of them, against a real 400×300
//! note) and expanded HOF bodies (ignored entirely by the Sugiyama helper,
//! which drew a 460 px `map` straight through the next column).
//!
//! # The parity pipeline
//!
//! Sizes are not hand-copied numbers here. Two JSON files under
//! `rust/tests/fixtures/` carry a contract with Flutter:
//!
//! | File | Written by | Read by |
//! |---|---|---|
//! | `layout_size_parity_input.json` | this test | `test/layout_size_parity_test.dart` |
//! | `layout_size_parity.json` | that Dart test | this test |
//!
//! [`fixture_network`] builds a network with the five cases the design names —
//! a comment, an expanded HOF, a collapsed HOF, a closure and a two-level
//! nested HOF — and [`parity_input_is_current`] projects it into the *view*
//! terms Flutter's `ScopeResolver` consumes. The Dart test rebuilds the views
//! from that projection, runs `effectiveNodeSizeLogical`, and writes the sizes
//! back. [`rendered_node_size_matches_flutter`] then asserts equality to the
//! pixel.
//!
//! **When this test fails saying the fixture is stale**, the refresh is:
//!
//! ```text
//! cargo test -p atomcad-structure-designer --test structure_designer layout_size   # rewrites the input
//! flutter test test/layout_size_parity_test.dart                                   # rewrites the sizes
//! cargo test -p atomcad-structure-designer --test structure_designer layout_size   # green
//! ```
//!
//! That a Flutter-side size change fails a *Rust* test is the point: the two
//! rules have drifted before, and the drift was invisible until a node
//! overlapped something on screen.

use std::collections::BTreeMap;
use std::path::PathBuf;

use atomcad_structure_designer::layout::{rendered_body_size, rendered_node_size};
use atomcad_structure_designer::node_layout;
use atomcad_structure_designer::node_network::{
    CollapseMode, DEFAULT_BODY_HEIGHT, DEFAULT_BODY_WIDTH, Node, NodeNetwork,
    resolve_body_collapsed,
};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::text_format::edit_network;
use glam::DVec2;
use serde_json::{Value, json};

use super::layout_test_support::{empty_network, node_by_path};

// ============================================================================
// The fixture network
// ============================================================================

/// The five cases the design names, in one network.
///
/// | Name | Case |
/// |---|---|
/// | `note` | a comment, resized well past the pin estimate |
/// | `m_expanded` | an expanded HOF with a body node |
/// | `m_collapsed` | the same HOF, collapsed |
/// | `c1` | a `closure` (trimmed gutters, no external input column) |
/// | `outer` / `outer/inner` | a two-level nested HOF |
///
/// Positions are irrelevant to every size except the two body-derived ones, and
/// `int1` inside `outer/inner` is placed deliberately far right so `outer`'s
/// footprint is driven by content rather than by the stored default — the
/// recursive case that a non-recursive body measurement gets wrong.
fn fixture_network() -> (NodeNetwork, NodeTypeRegistry) {
    let registry = NodeTypeRegistry::new();
    let mut network = empty_network();

    let result = edit_network(
        &mut network,
        &registry,
        r#"
note = Comment { label: "Parity", text: "a resized note", width: 260, height: 140 }
r = range { start: 0, step: 1, count: 5 }
m_expanded = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = int { value: 7 }
    output d
  }
}
m_collapsed = map { xs: r, input_type: Int, output_type: Int }
c1 = closure {
  kind: "custom",
  params: ["x"],
  type_args: [Int, Int],
  body {
    p = int { value: 1 }
    output p
  }
}
outer = closure {
  kind: "custom",
  params: [],
  type_args: [Int],
  body {
    rr = range { start: 0, step: 1, count: 2 }
    inner = map {
      xs: rr,
      input_type: Int,
      output_type: Int,
      body {
        q = int { value: 2 }
        output q
      }
    }
  }
}
"#,
        false,
    );
    assert!(result.success, "fixture edit failed: {:?}", result.errors);

    // `collapse_mode` has no text-format spelling (it is exactly the state D14
    // exists to carry across a `--replace`), so the collapsed case is set here.
    set_collapse_mode(&mut network, "m_collapsed", CollapseMode::Collapsed);

    // Every body node is placed explicitly. Creation-time placement is a
    // heuristic over the *whole* network, so leaving it to decide would make
    // this fixture — and therefore the checked-in Flutter sizes — move whenever
    // that heuristic is tuned.
    move_node(&mut network, &["m_expanded", "d"], DVec2::new(10.0, 10.0));
    move_node(&mut network, &["c1", "p"], DVec2::new(10.0, 10.0));
    move_node(&mut network, &["outer", "rr"], DVec2::new(10.0, 10.0));
    // Far enough right that `outer`'s footprint is driven by its content
    // rather than by `DEFAULT_BODY_WIDTH` — the recursive case.
    move_node(&mut network, &["outer", "inner"], DVec2::new(260.0, 40.0));
    move_node(
        &mut network,
        &["outer", "inner", "q"],
        DVec2::new(10.0, 10.0),
    );

    (network, registry)
}

fn set_collapse_mode(network: &mut NodeNetwork, name: &str, mode: CollapseMode) {
    let id = network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named {name}"))
        .id;
    network.nodes.get_mut(&id).unwrap().collapse_mode = mode;
}

fn move_node(network: &mut NodeNetwork, path: &[&str], position: DVec2) {
    let (last, prefix) = path.split_last().unwrap();
    let mut scope: &mut NodeNetwork = network;
    for name in prefix {
        let id = scope
            .nodes
            .values()
            .find(|n| n.custom_name.as_deref() == Some(*name))
            .unwrap_or_else(|| panic!("no node named {name}"))
            .id;
        scope = scope.nodes.get_mut(&id).unwrap().zone_mut().unwrap();
    }
    let id = scope
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(*last))
        .unwrap_or_else(|| panic!("no node named {last}"))
        .id;
    scope.nodes.get_mut(&id).unwrap().position = position;
}

// ============================================================================
// The rule itself
// ============================================================================

#[test]
fn a_comment_is_its_own_size_not_a_pin_estimate() {
    let (network, registry) = fixture_network();
    let note = node_by_path(&network, &["note"]);
    assert_eq!(
        rendered_node_size(note, &registry),
        DVec2::new(260.0, 140.0),
        "a comment reports its authored dimensions"
    );
}

#[test]
fn an_expanded_hof_is_sized_by_its_body_a_collapsed_one_is_not() {
    let (network, registry) = fixture_network();
    let expanded = rendered_node_size(node_by_path(&network, &["m_expanded"]), &registry);
    let collapsed = rendered_node_size(node_by_path(&network, &["m_collapsed"]), &registry);

    assert_eq!(
        collapsed.x,
        node_layout::NODE_WIDTH,
        "a collapsed HOF renders as an ordinary node"
    );
    assert!(
        expanded.x >= DEFAULT_BODY_WIDTH && expanded.x > collapsed.x + 200.0,
        "an expanded HOF gains its whole body region plus both gutters: \
         {expanded:?} vs {collapsed:?}"
    );
    assert!(expanded.y > collapsed.y);
}

#[test]
fn a_body_is_never_smaller_than_its_stored_size() {
    let (network, registry) = fixture_network();
    let (width, height) = rendered_body_size(node_by_path(&network, &["m_expanded"]), &registry);
    assert_eq!(
        (width, height),
        (DEFAULT_BODY_WIDTH, DEFAULT_BODY_HEIGHT),
        "one small node leaves the stored size as the floor"
    );
}

#[test]
fn a_nested_body_cascades_into_the_outer_footprint() {
    let (network, registry) = fixture_network();
    let outer = node_by_path(&network, &["outer"]);
    let inner = node_by_path(&network, &["outer", "inner"]);

    let inner_size = rendered_node_size(inner, &registry);
    let (outer_body_width, _) = rendered_body_size(outer, &registry);

    assert!(
        outer_body_width >= inner.position.x + inner_size.x,
        "outer's body must reach past the inner HOF it contains: \
         body {outer_body_width} vs inner right edge {}",
        inner.position.x + inner_size.x
    );
    assert!(
        outer_body_width > DEFAULT_BODY_WIDTH,
        "and it must have grown past the stored default to do so"
    );
}

#[test]
fn a_subtitle_is_computed_not_assumed() {
    let registry = NodeTypeRegistry::new();
    let mut network = empty_network();
    // `int` renders its value as a subtitle; `map` has none. Both were sized as
    // if they had one before the unification, so `map` was 20 px too tall.
    let result = edit_network(
        &mut network,
        &registry,
        "i = int { value: 3 }\nm = map { input_type: Int, output_type: Int }\n",
        false,
    );
    assert!(result.success, "{:?}", result.errors);

    let i = node_by_path(&network, &["i"]);
    let subtitled = rendered_node_size(i, &registry).y;
    let unsubtitled = node_layout::estimate_node_height(i.arguments.len(), 1, false);
    assert_eq!(
        subtitled - unsubtitled,
        20.0,
        "a node with a subtitle is exactly one subtitle row taller"
    );
}

// ============================================================================
// Flutter parity
// ============================================================================

fn fixtures_dir() -> PathBuf {
    atomcad_test_support::fixtures_root()
}

fn input_path() -> PathBuf {
    fixtures_dir().join("layout_size_parity_input.json")
}

fn sizes_path() -> PathBuf {
    fixtures_dir().join("layout_size_parity.json")
}

/// Project one node into the *view* terms Flutter's size rule consumes.
fn view_of(node: &Node, registry: &NodeTypeRegistry, path: &[String]) -> Value {
    let node_type = registry.get_node_type_for_node(node);
    let (inputs, outputs) = node_type
        .map(|nt| (nt.parameters.len(), nt.output_pin_count()))
        .unwrap_or((0, 1));

    let comment = node
        .data
        .as_any_ref()
        .downcast_ref::<atomcad_structure_designer::nodes::comment::CommentData>()
        .map(|c| json!([c.width, c.height]));

    let zone = match (node_type, node.zone.as_deref()) {
        (Some(nt), Some(body)) if nt.has_zone() => {
            let mut children: Vec<Value> = Vec::new();
            let mut ids: Vec<u64> = body.nodes.keys().copied().collect();
            ids.sort_unstable();
            for id in ids {
                let child = &body.nodes[&id];
                let Some(name) = child.custom_name.clone() else {
                    continue;
                };
                let mut child_path = path.to_vec();
                child_path.push(name);
                children.push(view_of(child, registry, &child_path));
            }
            Some(json!({
                "zoneInputPins": nt.zone_input_pins.len(),
                "zoneOutputPins": nt.zone_output_pins.len(),
                "storedWidth": node.body_width,
                "storedHeight": node.body_height,
                "collapsed": resolve_body_collapsed(node, nt),
                "nodes": children,
            }))
        }
        _ => None,
    };

    json!({
        "path": path.join("/"),
        "nodeTypeName": node.node_type_name,
        "position": [node.position.x, node.position.y],
        "inputPins": inputs,
        "outputPins": outputs,
        "hasSubtitle": atomcad_structure_designer::layout::size::node_has_subtitle(node, registry),
        "comment": comment,
        "zone": zone,
    })
}

fn parity_input(network: &NodeNetwork, registry: &NodeTypeRegistry) -> Value {
    let mut nodes: Vec<Value> = Vec::new();
    let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let node = &network.nodes[&id];
        let Some(name) = node.custom_name.clone() else {
            continue;
        };
        nodes.push(view_of(node, registry, &[name]));
    }
    json!({
        "_comment": "Generated by rust/crates/atomcad-structure-designer/tests/structure_designer/layout_size_test.rs. \
                     Consumed by test/layout_size_parity_test.dart, which writes layout_size_parity.json back.",
        "nodes": nodes,
    })
}

/// Every node's rendered size, keyed by name path — the Rust side of the
/// comparison, in the same shape the Dart test writes.
fn rust_sizes(network: &NodeNetwork, registry: &NodeTypeRegistry) -> BTreeMap<String, Vec<f64>> {
    fn walk(
        network: &NodeNetwork,
        registry: &NodeTypeRegistry,
        prefix: &mut Vec<String>,
        out: &mut BTreeMap<String, Vec<f64>>,
    ) {
        let mut ids: Vec<u64> = network.nodes.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let node = &network.nodes[&id];
            let Some(name) = node.custom_name.clone() else {
                continue;
            };
            prefix.push(name);
            let size = rendered_node_size(node, registry);
            out.insert(prefix.join("/"), vec![size.x, size.y]);
            if let Some(body) = node.zone.as_deref() {
                walk(body, registry, prefix, out);
            }
            prefix.pop();
        }
    }
    let mut out = BTreeMap::new();
    walk(network, registry, &mut Vec::new(), &mut out);
    out
}

/// Keep `layout_size_parity_input.json` in step with the fixture network.
///
/// Deliberately a *write*, not an assertion: the input file is derived, and a
/// Rust-side change (a new pin on `map`) should refresh it rather than fail
/// here. What fails is [`rendered_node_size_matches_flutter`], on the next
/// line, because the checked-in sizes no longer describe this network — which
/// is the signal to re-run the Dart test.
#[test]
fn parity_input_is_current() {
    let (network, registry) = fixture_network();
    let rendered = serde_json::to_string_pretty(&parity_input(&network, &registry)).unwrap() + "\n";
    let path = input_path();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current.replace("\r\n", "\n") != rendered {
        std::fs::write(&path, &rendered).expect("write parity input fixture");
    }
}

#[test]
fn rendered_node_size_matches_flutter() {
    let (network, registry) = fixture_network();
    let ours = rust_sizes(&network, &registry);

    let path = sizes_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e}. Run `flutter test test/layout_size_parity_test.dart` to generate it.",
            path.display()
        )
    });
    let theirs: BTreeMap<String, Vec<f64>> =
        serde_json::from_str(&text).expect("layout_size_parity.json is not the expected shape");

    assert_eq!(
        ours.keys().collect::<Vec<_>>(),
        theirs.keys().collect::<Vec<_>>(),
        "the Dart fixture describes a different set of nodes than the Rust fixture network — \
         regenerate it (see this file's module docs)"
    );
    for (path, ours) in &ours {
        let theirs = &theirs[path];
        assert_eq!(
            ours, theirs,
            "size of `{path}` disagrees with Flutter: Rust {ours:?}, Flutter {theirs:?}"
        );
    }
}
