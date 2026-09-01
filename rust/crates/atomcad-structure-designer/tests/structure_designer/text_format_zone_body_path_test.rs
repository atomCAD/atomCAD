//! Phase 3 of `doc/design_hof_body_text_format.md`: **path-addressed edits**.
//!
//! Phase 2 made a body writable only as a whole: `body { … }` is total, so
//! changing one node of a thirty-node body meant restating the other
//! twenty-nine. Phase 3 adds the surgical form — `m1/x = …`, `output m1/x`,
//! `delete m1/x` — where **the prefix selects the scope and the last segment
//! names the node in it** (D7).
//!
//! Two properties carry most of these tests. A path statement *merges*: it
//! touches the node it names and leaves its siblings' ids and positions alone,
//! which is the whole reason it exists. And it is *relative*: inside it, bare
//! names resolve in the addressed body, `$…` are that body's zone inputs and
//! `^…` walks outward from it — so `m1/x = …` and the same statement written
//! inside `m1`'s block mean exactly the same thing.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::{IncomingWire, NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type::NodeTypeCategory;
use atomcad_structure_designer::node_type::{NodeType, OutputPinDefinition};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::text_format::{EditResult, Statement, edit_network};
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

/// Every node's position, keyed by its full path.
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

/// The same canonical `map` Phase 2's tests use: a range, a captured constant
/// and a two-node body that multiplies the element by the capture.
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

/// A `map` inside a `map`, for the depth-2 path tests.
const NESTED_MAPS: &str = r#"
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
        p = expr { a: $element, expression: "a", parameters: [{ name: "a", data_type: Int }] }
        output p
      }
    }
    output inner
  }
}
"#;

// ============================================================================
// The parser: `expect_path` in the three statement forms
// ============================================================================

#[test]
fn the_three_statement_forms_accept_a_path() {
    let stmts = atomcad_structure_designer::text_format::Parser::parse(
        "m1/x = int { value: 1 }\noutput outer/inner/p\ndelete m1/e",
    )
    .unwrap();

    match &stmts[0] {
        Statement::Assignment {
            scope_path, name, ..
        } => {
            assert_eq!(scope_path, &["m1".to_string()]);
            assert_eq!(name, "x");
        }
        other => panic!("expected an assignment, got {:?}", other),
    }
    match &stmts[1] {
        Statement::Output {
            scope_path,
            node_name,
        } => {
            assert_eq!(scope_path, &["outer".to_string(), "inner".to_string()]);
            assert_eq!(node_name, "p");
        }
        other => panic!("expected an output, got {:?}", other),
    }
    match &stmts[2] {
        Statement::Delete {
            scope_path,
            node_name,
        } => {
            assert_eq!(scope_path, &["m1".to_string()]);
            assert_eq!(node_name, "e");
        }
        other => panic!("expected a delete, got {:?}", other),
    }
}

/// Paths split on the `/` **token**, so a backtick-quoted identifier that
/// contains a slash is one segment and stays addressable (D7).
#[test]
fn a_backtick_quoted_segment_may_contain_a_slash() {
    let stmts =
        atomcad_structure_designer::text_format::Parser::parse("m1/`a/b` = int { value: 1 }")
            .unwrap();
    match &stmts[0] {
        Statement::Assignment {
            scope_path, name, ..
        } => {
            assert_eq!(scope_path, &["m1".to_string()]);
            assert_eq!(name, "a/b");
        }
        other => panic!("expected an assignment, got {:?}", other),
    }
}

// ============================================================================
// A path statement merges: it touches one node and nothing else
// ============================================================================

#[test]
fn a_path_assignment_updates_one_body_node_and_leaves_its_siblings_alone() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    let before_ids = ids(&network);
    let before_positions = positions(&network);

    // Re-point `d`'s first input from the iteration value to the capture.
    apply(
        &mut network,
        &registry,
        r#"m1/d = expr { a: ^scale, b: ^scale, expression: "a * b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }"#,
    );

    let body = body_of(&network, "m1");
    let d_a = wire(body, "d", 0);
    assert!(matches!(
        d_a.source_pin,
        SourcePin::NodeOutput { pin_index: 0 }
    ));
    assert_eq!(d_a.source_scope_depth, 1, "`^scale` is one scope out");
    assert_eq!(d_a.source_node_id, node_id(&network, "scale"));

    // `e` — and everything else — is untouched, ids and positions included.
    assert_eq!(ids(&network), before_ids, "no node may be recreated");
    assert_eq!(
        positions(&network),
        before_positions,
        "a path statement must not move anything"
    );
    assert_eq!(body_of(&network, "m1").nodes.len(), 2);
    let e_a = wire(body_of(&network, "m1"), "e", 0);
    assert_eq!(e_a.source_node_id, node_id(body_of(&network, "m1"), "d"));
}

#[test]
fn a_path_assignment_appends_a_node_to_a_body() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    let before_ids = ids(&network);

    let result = apply(
        &mut network,
        &registry,
        r#"m1/new1 = expr { a: d, b: $element, expression: "a - b", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }] }"#,
    );

    let body = body_of(&network, "m1");
    assert_eq!(body.nodes.len(), 3, "`new1` joins `d` and `e`");

    // A bare name inside a path statement resolves in the addressed body...
    let new_a = wire(body, "new1", 0);
    assert_eq!(new_a.source_node_id, node_id(body, "d"));
    assert_eq!(new_a.source_scope_depth, 0);
    // ...and `$element` is that body's own zone input.
    let new_b = wire(body, "new1", 1);
    assert_eq!(new_b.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(new_b.source_scope_depth, 1);
    assert_eq!(new_b.source_node_id, node_id(&network, "m1"));

    // The pre-existing nodes keep their ids.
    for (path, id) in &before_ids {
        assert_eq!(
            ids(&network).get(path),
            Some(id),
            "'{}' was recreated",
            path
        );
    }
    assert!(
        result.nodes_created.contains(&"m1/new1".to_string()),
        "a path-created node is reported by its full path (D10), got: {:?}",
        result.nodes_created
    );
}

#[test]
fn delete_removes_one_body_node_and_nothing_else() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    let d_id = node_id(body_of(&network, "m1"), "d");

    let result = apply(&mut network, &registry, "delete m1/d");

    let body = body_of(&network, "m1");
    assert_eq!(body.nodes.len(), 1, "only `e` is left");
    assert!(
        wires(body, "e", 0).is_empty(),
        "`e`'s wire to the deleted node goes with it"
    );
    assert_eq!(body.nodes.len(), 1);
    assert!(network.nodes.contains_key(&node_id(&network, "m1")));
    assert!(
        result.nodes_deleted.contains(&"m1/d".to_string()),
        "got: {:?}",
        result.nodes_deleted
    );
    assert_ne!(d_id, node_id(body_of(&network, "m1"), "e"));
}

// ============================================================================
// The zone-output wire
// ============================================================================

#[test]
fn output_with_a_path_re_points_the_zone_output_wire() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    assert_eq!(
        zone_output_wires(&network, "m1")[0].source_node_id,
        node_id(body_of(&network, "m1"), "e")
    );

    apply(&mut network, &registry, "output m1/d");

    let body = body_of(&network, "m1");
    let zone_out = zone_output_wires(&network, "m1");
    assert_eq!(zone_out.len(), 1);
    assert_eq!(zone_out[0].source_node_id, node_id(body, "d"));
    assert_eq!(
        body.nodes.len(),
        2,
        "a path `output` re-points a wire; it does not touch the body"
    );
    assert_eq!(
        network.return_node_id,
        Some(node_id(&network, "m1")),
        "the network's own return node is a different statement"
    );
}

/// The merge table: `delete m1/x` clears the zone-output wire **iff** `x` was
/// its source — never left dangling.
#[test]
fn deleting_the_zone_output_source_clears_that_wire() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);
    assert_eq!(zone_output_wires(&network, "m1").len(), 1);

    apply(&mut network, &registry, "delete m1/e");

    assert!(
        zone_output_wires(&network, "m1").is_empty(),
        "the wire pointed at the deleted node, so it must go"
    );
    assert_eq!(body_of(&network, "m1").nodes.len(), 1, "`d` survives");
}

#[test]
fn deleting_a_body_node_that_is_not_the_output_source_leaves_the_wire() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    apply(&mut network, &registry, "delete m1/d");

    let body = body_of(&network, "m1");
    let zone_out = zone_output_wires(&network, "m1");
    assert_eq!(zone_out.len(), 1);
    assert_eq!(zone_out[0].source_node_id, node_id(body, "e"));
}

// ============================================================================
// Depth 2
// ============================================================================

#[test]
fn a_two_segment_path_reaches_a_nested_body_and_its_outward_forms_still_work() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, NESTED_MAPS);

    apply(
        &mut network,
        &registry,
        r#"outer/inner/n2 = expr { a: $element, b: ^$element, c: ^^base, expression: "a * b + c", parameters: [{ name: "a", data_type: Int }, { name: "b", data_type: Int }, { name: "c", data_type: Int }] }"#,
    );

    let outer_id = node_id(&network, "outer");
    let base_id = node_id(&network, "base");
    let outer_body = body_of(&network, "outer");
    let inner_id = node_id(outer_body, "inner");
    let inner_body = body_of(outer_body, "inner");
    assert_eq!(inner_body.nodes.len(), 2, "`n2` joins `p`");

    // `$element` — the inner map's own iteration value, relative to the
    // *addressed* scope rather than to the top level.
    let a = wire(inner_body, "n2", 0);
    assert_eq!(a.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(a.source_scope_depth, 1);
    assert_eq!(a.source_node_id, inner_id);

    // `^$element` — the outer map's, at depth 2 and naming the outer HOF node.
    let b = wire(inner_body, "n2", 1);
    assert_eq!(b.source_pin, SourcePin::ZoneInput { pin_index: 0 });
    assert_eq!(b.source_scope_depth, 2);
    assert_eq!(b.source_node_id, outer_id);

    // `^^base` — a node two scopes out, i.e. the top level.
    let c = wire(inner_body, "n2", 2);
    assert!(matches!(
        c.source_pin,
        SourcePin::NodeOutput { pin_index: 0 }
    ));
    assert_eq!(c.source_scope_depth, 2);
    assert_eq!(c.source_node_id, base_id);
}

#[test]
fn a_two_segment_output_and_delete_reach_a_nested_body() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, NESTED_MAPS);
    apply(
        &mut network,
        &registry,
        r#"outer/inner/n2 = expr { a: $element, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }"#,
    );

    apply(&mut network, &registry, "output outer/inner/n2");
    {
        let inner_body_owner = body_of(&network, "outer");
        let inner_body = body_of(inner_body_owner, "inner");
        let n2_id = node_id(inner_body, "n2");
        assert_eq!(
            zone_output_wires(inner_body_owner, "inner")[0].source_node_id,
            n2_id
        );
    }

    apply(&mut network, &registry, "delete outer/inner/p");
    let inner_body_owner = body_of(&network, "outer");
    let inner_body = body_of(inner_body_owner, "inner");
    assert_eq!(inner_body.nodes.len(), 1);
    assert!(
        inner_body
            .nodes
            .values()
            .all(|n| n.custom_name.as_deref() == Some("n2"))
    );
}

// ============================================================================
// Ordering: a block wipes, a later path statement merges into the result
// ============================================================================

#[test]
fn a_block_and_a_path_statement_for_the_same_node_apply_in_order() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    apply(
        &mut network,
        &registry,
        r#"
m1 = map {
  input_type: Int,
  output_type: Int,
  body {
    d = expr { a: $element, expression: "a * 2", parameters: [{ name: "a", data_type: Int }] }
    output d
  }
}
m1/e2 = expr { a: d, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
output m1/e2
"#,
    );

    let body = body_of(&network, "m1");
    assert_eq!(
        body.nodes.len(),
        2,
        "the block wiped `e`, then the path statement added `e2`"
    );
    assert!(
        body.nodes
            .values()
            .all(|n| n.custom_name.as_deref() != Some("e")),
        "`e` was not mentioned by the block, so it is gone"
    );
    let e2_id = node_id(body, "e2");
    assert_eq!(
        zone_output_wires(&network, "m1")[0].source_node_id,
        e2_id,
        "the later `output m1/e2` wins over the block's `output d`"
    );
}

/// A path statement *inside* a block reaches into a nested body — and mentions
/// its owner, so the block that contains it does not delete the very node the
/// path addresses.
#[test]
fn a_path_statement_inside_a_block_keeps_its_owner_alive() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, NESTED_MAPS);
    let inner_id = node_id(body_of(&network, "outer"), "inner");

    apply(
        &mut network,
        &registry,
        r#"
outer = map {
  input_type: Int,
  output_type: Int,
  body {
    inner/n2 = expr { a: $element, expression: "a + 1", parameters: [{ name: "a", data_type: Int }] }
    output inner
  }
}
"#,
    );

    let outer_body = body_of(&network, "outer");
    assert_eq!(
        node_id(outer_body, "inner"),
        inner_id,
        "`inner` is mentioned by the path statement, so it keeps its id"
    );
    let inner_body = body_of(outer_body, "inner");
    assert_eq!(inner_body.nodes.len(), 2, "`n2` joined `p`");
}

// ============================================================================
// Bad paths are errors, not silent no-ops
// ============================================================================

#[test]
fn a_path_naming_a_non_existent_scope_is_an_error() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let result = edit_network(&mut network, &registry, "nope/x = int { value: 1 }", false);

    assert!(!result.success, "expected a refused edit");
    assert!(
        result.errors.iter().any(|e| e.contains("nope")),
        "the error must name the missing scope, got: {:?}",
        result.errors
    );
    assert!(
        result.nodes_created.is_empty(),
        "nothing may be created on a bad path"
    );
}

#[test]
fn a_path_into_a_node_with_no_zone_is_rejected() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let result = edit_network(&mut network, &registry, "scale/x = int { value: 1 }", false);

    assert!(!result.success, "expected a refused edit");
    assert!(
        result.errors.iter().any(|e| e.contains("has no body")),
        "expected a 'has no body' error, got: {:?}",
        result.errors
    );
}

#[test]
fn a_path_output_naming_a_node_outside_the_body_is_an_error() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let result = edit_network(&mut network, &registry, "output m1/nope", false);

    assert!(!result.success, "expected a refused edit");
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.contains("no such node in its body")),
        "got: {:?}",
        result.errors
    );
    assert_eq!(
        zone_output_wires(&network, "m1").len(),
        1,
        "a failed `output` must not clear the existing wire"
    );
}

#[test]
fn a_path_delete_of_a_missing_body_node_is_an_error() {
    let registry = registry();
    let mut network = empty_network();
    apply(&mut network, &registry, MAP_WITH_BODY);

    let result = edit_network(&mut network, &registry, "delete m1/nope", false);

    assert!(!result.success, "expected a refused edit");
    assert!(
        result.errors.iter().any(|e| e.contains("m1/nope")),
        "the error must carry the full path (D10), got: {:?}",
        result.errors
    );
}
