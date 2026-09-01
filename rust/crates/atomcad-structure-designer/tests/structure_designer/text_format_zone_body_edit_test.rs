//! Phase 2 of `doc/design_hof_body_text_format.md`: the **writing** half.
//!
//! Phase 1 made `query` project zone bodies; the editor stayed single-scope, so
//! its own output was not valid `edit` input. These tests pin the scope-aware
//! editor: `body { … }` blocks parse and apply, the four outward spellings
//! (`$element`, `^capture`, `^^capture`, `^$element`) become the wire shapes
//! they encode, whole-body assign is total over both the body and the parent's
//! zone-output wire, and name-keyed identity carries node ids and positions
//! across an edit.
//!
//! The test this whole design exists for is
//! [`query_output_round_trips_through_edit_replace`]: before Phase 2 that
//! round-trip destroyed every body node and reported `success: true`.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::network_validator::validate_network;
use atomcad_structure_designer::node_network::{IncomingWire, NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type::NodeTypeCategory;
use atomcad_structure_designer::node_type::{NodeType, OutputPinDefinition};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::closure::{ClosureData, ClosureKind};
use atomcad_structure_designer::scoped_validation_errors::{
    collect_scoped_validation_errors, error_node_path,
};
use atomcad_structure_designer::text_format::{EditResult, edit_network, serialize_network};
use glam::DVec2;
use std::collections::HashMap;

// ============================================================================
// Helpers
// ============================================================================

fn registry() -> NodeTypeRegistry {
    NodeTypeRegistry::new()
}

fn empty_network() -> NodeNetwork {
    let node_type = NodeType {
        name: "test".to_string(),
        description: String::new(),
        summary: None,
        category: NodeTypeCategory::Custom,
        parameters: vec![],
        output_pins: OutputPinDefinition::single(DataType::Blueprint),
        zone_input_pins: vec![],
        zone_output_pins: vec![],
        public: true,
        node_data_creator: || Box::new(atomcad_structure_designer::node_data::NoData {}),
        node_data_saver: atomcad_structure_designer::node_type::no_data_saver,
        node_data_loader: atomcad_structure_designer::node_type::no_data_loader,
    };
    NodeNetwork::new(node_type)
}

fn apply(network: &mut NodeNetwork, registry: &NodeTypeRegistry, code: &str) -> EditResult {
    let result = edit_network(network, registry, code, false);
    assert!(
        result.success,
        "edit should have succeeded, errors: {:?}",
        result.errors
    );
    result
}

fn node_id(network: &NodeNetwork, name: &str) -> u64 {
    network
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("no node named '{}' in this scope", name))
        .id
}

/// The body network of the named zone-owning node.
fn body_of<'a>(network: &'a NodeNetwork, owner: &str) -> &'a NodeNetwork {
    network
        .nodes
        .get(&node_id(network, owner))
        .unwrap()
        .zone
        .as_deref()
        .unwrap_or_else(|| panic!("'{}' has no body", owner))
}

/// The inbound wires on one argument pin of a named node.
fn wires(network: &NodeNetwork, name: &str, arg_index: usize) -> Vec<IncomingWire> {
    network
        .nodes
        .get(&node_id(network, name))
        .unwrap()
        .arguments
        .get(arg_index)
        .map(|a| a.incoming_wires.clone())
        .unwrap_or_default()
}

/// The single inbound wire on one argument pin (panics if there isn't exactly one).
fn wire(network: &NodeNetwork, name: &str, arg_index: usize) -> IncomingWire {
    let mut found = wires(network, name, arg_index);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one wire on {}[{}]",
        name,
        arg_index
    );
    found.pop().unwrap()
}

/// The zone-output wires of a named zone-owning node.
fn zone_output_wires(network: &NodeNetwork, owner: &str) -> Vec<IncomingWire> {
    network
        .nodes
        .get(&node_id(network, owner))
        .unwrap()
        .zone_output_arguments
        .first()
        .map(|a| a.incoming_wires.clone())
        .unwrap_or_default()
}

/// Every node's position, keyed by its full path — the same key the editor
/// matches identity on.
fn positions(network: &NodeNetwork) -> HashMap<String, DVec2> {
    fn walk(network: &NodeNetwork, prefix: &str, out: &mut HashMap<String, DVec2>) {
        for node in network.nodes.values() {
            let Some(name) = node.custom_name.as_deref() else {
                continue;
            };
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{}/{}", prefix, name)
            };
            out.insert(path.clone(), node.position);
            if let Some(body) = node.zone.as_deref() {
                walk(body, &path, out);
            }
        }
    }
    let mut out = HashMap::new();
    walk(network, "", &mut out);
    out
}

/// Every node's id, keyed by its full path.
fn ids(network: &NodeNetwork) -> HashMap<String, u64> {
    fn walk(network: &NodeNetwork, prefix: &str, out: &mut HashMap<String, u64>) {
        for node in network.nodes.values() {
            let Some(name) = node.custom_name.as_deref() else {
                continue;
            };
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{}/{}", prefix, name)
            };
            out.insert(path.clone(), node.id);
            if let Some(body) = node.zone.as_deref() {
                walk(body, &path, out);
            }
        }
    }
    let mut out = HashMap::new();
    walk(network, "", &mut out);
    out
}

/// The design doc's canonical `map`: a range, a captured constant and a
/// two-node body that multiplies the element by the capture.
const MAP_WITH_BODY: &str = r#"
r = range { start: 0, step: 1, count: 5 }
scale = int { value: 3 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    e = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output e
  }
}
output m1
"#;

// ============================================================================
// A body block creates, updates and empties a body
// ============================================================================

#[test]
fn a_body_block_creates_a_body_from_nothing() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let body = body_of(&network, "m1");
    assert_eq!(body.nodes.len(), 2, "expected `d` and `e` in the body");
    // Intra-body wire: `e` reads `d`, same scope, no sigil.
    let e_a = wire(body, "e", 0);
    assert_eq!(e_a.source_node_id, node_id(body, "d"));
    assert_eq!(e_a.source_scope_depth, 0);
    assert!(matches!(
        e_a.source_pin,
        SourcePin::NodeOutput { pin_index: 0 }
    ));
}

#[test]
fn re_applying_the_same_block_changes_no_id_and_moves_no_node() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let before_ids = ids(&network);
    let before_positions = positions(&network);

    apply(&mut network, &registry, MAP_WITH_BODY);

    assert_eq!(ids(&network), before_ids, "node ids must be stable");
    assert_eq!(
        positions(&network),
        before_positions,
        "no node may move on a no-op re-apply"
    );
}

#[test]
fn renaming_one_body_node_leaves_its_siblings_untouched() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let d_id = node_id(body_of(&network, "m1"), "d");
    let d_position = body_of(&network, "m1").nodes.get(&d_id).unwrap().position;

    // `e` becomes `f`; `d` is written exactly as before.
    apply(
        &mut network,
        &registry,
        r#"
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    f = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output f
  }
}
"#,
    );

    let body = body_of(&network, "m1");
    assert_eq!(body.nodes.len(), 2, "the renamed node replaced `e`");
    assert!(
        body.nodes
            .values()
            .all(|n| n.custom_name.as_deref() != Some("e")),
        "`e` was not mentioned by the block, so it is gone"
    );
    assert_eq!(node_id(body, "d"), d_id, "`d` keeps its id");
    assert_eq!(
        body.nodes.get(&d_id).unwrap().position,
        d_position,
        "`d` keeps its position"
    );
}

#[test]
fn an_empty_block_empties_the_body() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    assert_eq!(body_of(&network, "m1").nodes.len(), 2);

    apply(
        &mut network,
        &registry,
        "m1 = map { input_type: Int, output_type: Int, body { } }",
    );

    assert!(
        body_of(&network, "m1").nodes.is_empty(),
        "`body {{ }}` empties the body"
    );
    assert!(
        zone_output_wires(&network, "m1").is_empty(),
        "and clears the zone-output wire with it"
    );
}

#[test]
fn an_omitted_block_leaves_the_body_alone() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    // No `body` mentioned at all: the property rule applies to bodies exactly
    // as it does to properties — what is not mentioned is untouched.
    apply(&mut network, &registry, "m1 = map { output_type: Int }");

    assert_eq!(body_of(&network, "m1").nodes.len(), 2);
    assert_eq!(zone_output_wires(&network, "m1").len(), 1);
}

// ============================================================================
// The four outward spellings
// ============================================================================

#[test]
fn dollar_element_produces_a_zone_input_wire_at_depth_1() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let m1 = node_id(&network, "m1");
    let d_a = wire(body_of(&network, "m1"), "d", 0);
    assert_eq!(
        d_a.source_pin,
        SourcePin::ZoneInput { pin_index: 0 },
        "`$element` is map's zone-input pin 0"
    );
    assert_eq!(d_a.source_scope_depth, 1, "`k` carets + `$` → depth k + 1");
    assert_eq!(
        d_a.source_node_id, m1,
        "a ZoneInput wire names the owning HOF node, never a body node"
    );
}

#[test]
fn a_caret_capture_produces_a_node_output_wire_at_depth_1() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let scale = node_id(&network, "scale");
    let d_b = wire(body_of(&network, "m1"), "d", 1);
    assert_eq!(d_b.source_node_id, scale);
    assert_eq!(d_b.source_scope_depth, 1, "`k` carets → NodeOutput depth k");
    assert!(matches!(
        d_b.source_pin,
        SourcePin::NodeOutput { pin_index: 0 }
    ));
}

/// `^^` for a node two scopes out, and `^$element` for the **outer** HOF's
/// iteration value — the depth-2 `ZoneInput` that has no other spelling.
#[test]
fn nested_bodies_reach_outward_with_double_caret_and_caret_dollar() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
rows = range { start: 0, step: 1, count: 3 }
base = int { value: 7 }
outer = map {
  xs: rows,
  input_type: Int,
  output_type: Int,
  body {
    inner = map {
      xs: $element,
      input_type: Int,
      output_type: Int,
      body {
        p = expr { a: $element, b: ^$element, c: ^^base, expression: "a * b + c", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }, { name: "c", data_type: Int }] }
        output p
      }
    }
    output inner
  }
}
"#,
    );

    let outer_id = node_id(&network, "outer");
    let base_id = node_id(&network, "base");
    let outer_body = body_of(&network, "outer");
    let inner_id = node_id(outer_body, "inner");
    let inner_body = body_of(outer_body, "inner");

    // `$element` — the inner map's own iteration value.
    let p_a = wire(inner_body, "p", 0);
    assert_eq!(p_a.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(p_a.source_scope_depth, 1);
    assert_eq!(p_a.source_node_id, inner_id);

    // `^$element` — the outer map's. Depth 2, and the source node id is the
    // **outer** HOF's, not the inner one's and not a body node's.
    let p_b = wire(inner_body, "p", 1);
    assert_eq!(p_b.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(p_b.source_scope_depth, 2);
    assert_eq!(p_b.source_node_id, outer_id);

    // `^^base` — a node two scopes out.
    let p_c = wire(inner_body, "p", 2);
    assert!(matches!(
        p_c.source_pin,
        SourcePin::NodeOutput { pin_index: 0 }
    ));
    assert_eq!(p_c.source_scope_depth, 2);
    assert_eq!(p_c.source_node_id, base_id);

    // The inner map iterates the outer element.
    let inner_xs = wire(outer_body, "inner", 0);
    assert_eq!(inner_xs.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(inner_xs.source_scope_depth, 1);
}

#[test]
fn an_unqualified_name_resolves_to_the_body_one_when_both_scopes_have_it() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
r = range { start: 0, step: 1, count: 3 }
x = int { value: 1 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    x = int { value: 2 }
    d = expr { a: x, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output d
  }
}
"#,
    );

    let body = body_of(&network, "m1");
    let d_a = wire(body, "d", 0);
    assert_eq!(
        d_a.source_scope_depth, 0,
        "a bare name resolves this body first; inner shadows outer"
    );
    assert_eq!(d_a.source_node_id, node_id(body, "x"));
}

#[test]
fn two_bodies_may_both_contain_a_node_named_a() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
r = range { start: 0, step: 1, count: 3 }
m1 = map { xs: r, input_type: Int, output_type: Int, body { a = int { value: 1 } output a } }
m2 = map { xs: r, input_type: Int, output_type: Int, body { a = int { value: 2 } output a } }
"#,
    );

    // Names are unique per scope, not globally.
    assert_eq!(body_of(&network, "m1").nodes.len(), 1);
    assert_eq!(body_of(&network, "m2").nodes.len(), 1);
    let m1_a = node_id(body_of(&network, "m1"), "a");
    let m2_a = node_id(body_of(&network, "m2"), "a");
    assert_eq!(zone_output_wires(&network, "m1")[0].source_node_id, m1_a);
    assert_eq!(zone_output_wires(&network, "m2")[0].source_node_id, m2_a);
}

// ============================================================================
// `output` inside a block
// ============================================================================

#[test]
fn output_inside_a_block_writes_the_parents_zone_output_not_the_bodys_return() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let body = body_of(&network, "m1");
    let e_id = node_id(body, "e");
    assert_eq!(
        body.return_node_id, None,
        "a body has no interface; `output` inside it is not a return node"
    );
    let zone_out = zone_output_wires(&network, "m1");
    assert_eq!(zone_out.len(), 1);
    assert_eq!(zone_out[0].source_node_id, e_id);
    assert_eq!(zone_out[0].source_scope_depth, 0);

    // The network-level `output m1` is unaffected — same keyword, different
    // scope, and the position is what disambiguates them.
    assert_eq!(network.return_node_id, Some(node_id(&network, "m1")));
}

#[test]
fn a_block_with_no_output_clears_the_zone_output_wire() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    assert_eq!(zone_output_wires(&network, "m1").len(), 1);

    apply(
        &mut network,
        &registry,
        r#"
m1 = map {
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, expression: "a * 2", parameters: [{ name: "a", data_type: Int }] }
  }
}
"#,
    );

    assert_eq!(body_of(&network, "m1").nodes.len(), 1);
    assert!(
        zone_output_wires(&network, "m1").is_empty(),
        "the block is total over the parent's zone-output wire too (D5)"
    );
}

// ============================================================================
// D10: EditResult reports full paths
// ============================================================================

#[test]
fn edit_result_reports_body_nodes_by_full_path() {
    let registry = registry();
    let mut network = empty_network();
    let result = apply(&mut network, &registry, MAP_WITH_BODY);

    assert!(
        result.nodes_created.contains(&"m1/d".to_string())
            && result.nodes_created.contains(&"m1/e".to_string()),
        "body nodes are reported as `m1/d` / `m1/e`, got: {:?}",
        result.nodes_created
    );
    assert!(
        !result.nodes_created.contains(&"d".to_string()),
        "a bare name cannot say which body it is in"
    );
    // Top-level nodes are unchanged: a path with no scopes is just the name.
    assert!(result.nodes_created.contains(&"m1".to_string()));
}

// ============================================================================
// D13: properties are applied before the body block
// ============================================================================

/// A closure's zone inputs are named by its own `params`, so `$x` cannot bind
/// until they are set. The parser keeps the body out of `properties`, so this
/// holds even when the author writes the block *first*.
#[test]
fn a_closure_body_written_before_its_params_still_binds_dollar_x() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
f1 = closure {
  body {
    p = expr { a: $x, b: $y, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    output p
  },
  kind: "custom",
  params: ["x", "y"],
  type_args: [Int, Int, Int]
}
"#,
    );

    let f1 = node_id(&network, "f1");
    let body = body_of(&network, "f1");
    let p_a = wire(body, "p", 0);
    let p_b = wire(body, "p", 1);
    assert_eq!(
        p_a.source_pin,
        SourcePin::ZoneInput { pin_index: 0 },
        "`$x` is parameter 0"
    );
    assert_eq!(
        p_b.source_pin,
        SourcePin::ZoneInput { pin_index: 1 },
        "`$y` is parameter 1"
    );
    assert_eq!(p_a.source_node_id, f1);
    assert_eq!(p_b.source_node_id, f1);
}

/// D12's write half: without it a closure round-tripped as `c1 = closure { }`,
/// losing the definition that its own body's `$x` binds to.
#[test]
fn closure_text_properties_round_trip() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"f1 = closure { kind: "fold", params: [], type_args: [Int, Float] }"#,
    );

    let data = network
        .nodes
        .get(&node_id(&network, "f1"))
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ClosureData>()
        .expect("closure data");
    assert_eq!(data.kind, ClosureKind::Fold);
    assert_eq!(data.type_args, vec![DataType::Int, DataType::Float]);
    assert!(data.param_names.is_empty());
}

#[test]
fn an_unknown_closure_kind_is_an_error_not_a_silent_default() {
    let registry = registry();
    let mut network = empty_network();
    let result = edit_network(
        &mut network,
        &registry,
        r#"f1 = closure { kind: "nonsense" }"#,
        false,
    );
    assert!(!result.success, "expected a refused edit");
}

// ============================================================================
// A body on a node that has none
// ============================================================================

#[test]
fn a_body_on_a_node_without_a_zone_is_refused() {
    let registry = registry();
    let mut network = empty_network();
    let result = edit_network(
        &mut network,
        &registry,
        "i = int { value: 1, body { a = int { value: 2 } } }",
        false,
    );
    assert!(!result.success, "expected a refused edit");
    assert!(
        result.errors.iter().any(|e| e.contains("has no body")),
        "expected a 'has no body' error, got: {:?}",
        result.errors
    );
}

#[test]
fn a_dollar_reference_outside_any_body_is_reported() {
    let registry = registry();
    let mut network = empty_network();
    let result = edit_network(
        &mut network,
        &registry,
        r#"d = expr { a: $element, expression: "a", parameters: [{ name: "a", data_type: Int }] }"#,
        false,
    );
    // The reference cannot resolve, so the pin is simply left unwired and the
    // edit reports it — the same shape as any other unresolvable source.
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("reaches past the outermost enclosing body")),
        "expected a scope warning, got: {:?}",
        result.warnings
    );
}

// ============================================================================
// D8: positions survive `--replace`
// ============================================================================

#[test]
fn replace_of_an_unchanged_script_leaves_every_position_bit_identical() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    let before = positions(&network);
    assert!(before.contains_key("m1/d") && before.contains_key("m1/e"));

    let result = edit_network(&mut network, &registry, MAP_WITH_BODY, true);
    assert!(result.success, "errors: {:?}", result.errors);

    assert_eq!(
        positions(&network),
        before,
        "a --replace of an unchanged script must not move anything (D8)"
    );
}

// ============================================================================
// The regression test that motivates the whole design
// ============================================================================

/// `query` → `edit --replace` → the same network.
///
/// Before Phase 2 this round trip deleted every body node — `clear_network`
/// dropped them and the rebuilt HOF got a fresh empty body — and reported
/// `success: true` while doing it.
#[test]
fn query_output_round_trips_through_edit_replace() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let before_text = serialize_network(&network, &registry, None);
    let before_positions = positions(&network);

    let result = edit_network(&mut network, &registry, &before_text, true);
    assert!(
        result.success,
        "query output must be valid edit input, errors: {:?}",
        result.errors
    );

    // Node count, wiring and positions all survive.
    let body = body_of(&network, "m1");
    assert_eq!(body.nodes.len(), 2, "both body nodes survived");
    assert_eq!(positions(&network), before_positions);
    assert_eq!(
        serialize_network(&network, &registry, None),
        before_text,
        "the round trip is exact"
    );
}

#[test]
fn nested_query_output_round_trips_through_edit_replace() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
rows = range { start: 0, step: 1, count: 3 }
base = int { value: 7 }
outer = map {
  xs: rows,
  input_type: Int,
  output_type: Int,
  body {
    inner = map {
      xs: $element,
      input_type: Int,
      output_type: Int,
      body {
        p = expr { a: $element, b: ^$element, c: ^^base, expression: "a * b + c", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }, { name: "c", data_type: Int }] }
        output p
      }
    }
    output inner
  }
}
output outer
"#,
    );

    let before_text = serialize_network(&network, &registry, None);
    let result = edit_network(&mut network, &registry, &before_text, true);
    assert!(result.success, "errors: {:?}", result.errors);
    assert_eq!(serialize_network(&network, &registry, None), before_text);
}

#[test]
fn closure_query_output_round_trips_through_edit_replace() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
f1 = closure {
  kind: "custom",
  params: ["x", "y"],
  type_args: [Int, Int, Int],
  body {
    p = expr { a: $x, b: $y, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    output p
  }
}
"#,
    );

    let before_text = serialize_network(&network, &registry, None);
    let result = edit_network(&mut network, &registry, &before_text, true);
    assert!(result.success, "errors: {:?}", result.errors);
    assert_eq!(serialize_network(&network, &registry, None), before_text);
}

// ============================================================================
// D15: a body error is reported with the offending node's full path
// ============================================================================

/// The path half of D15. A body error's `node_id` is only meaningful inside
/// its own body, so the AI needs the scope chain spelled out — `m1/bad`, the
/// same address `edit` accepts.
#[test]
fn a_body_validation_error_carries_the_offending_nodes_full_path() {
    let mut registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
r = range { start: 0, step: 1, count: 3 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    bad = parameter { name: "p", data_type: Int }
    output bad
  }
}
"#,
    );

    validate_network(&mut network, &mut registry, None);
    let errors = collect_scoped_validation_errors(&network);
    let located: Vec<String> = errors
        .iter()
        .filter_map(|e| error_node_path(&network, &e.scope_path, e.node_id))
        .collect();
    assert!(
        located.contains(&"m1/bad".to_string()),
        "expected the body node's full path, got: {:?} from {:?}",
        located,
        errors.iter().map(|e| &e.error_text).collect::<Vec<_>>()
    );
}

// ============================================================================
// Comment anchors inside a body
// ============================================================================

/// A comment lives in the body it was written in, and its anchors resolve
/// there. The wire form additionally needs a scope on its *source* side,
/// because a body wire may be sourced from a capture — the serializer emits
/// `^scale -> d.b` for one, so the parser and the editor have to accept it.
#[test]
fn body_comment_anchors_resolve_within_the_body() {
    let registry = registry();
    let mut network = empty_network();
    apply(
        &mut network,
        &registry,
        r#"
r = range { start: 0, step: 1, count: 5 }
scale = int { value: 3 }
m1 = map {
  xs: r,
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }
    note = Comment { label: "why", text: "scaled by the captured factor", width: 200, height: 80, on: [d, ^scale -> d.b] }
    output d
  }
}
output m1
"#,
    );

    let body = body_of(&network, "m1");
    let comment = body
        .nodes
        .get(&node_id(body, "note"))
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<atomcad_structure_designer::nodes::comment::CommentData>()
        .expect("comment data");
    assert_eq!(
        comment.anchors.len(),
        2,
        "both the node anchor and the capture-wire anchor resolved"
    );

    // And the whole thing round-trips.
    let text = serialize_network(&network, &registry, None);
    let result = edit_network(&mut network, &registry, &text, true);
    assert!(result.success, "errors: {:?}", result.errors);
    assert_eq!(serialize_network(&network, &registry, None), text);
}
