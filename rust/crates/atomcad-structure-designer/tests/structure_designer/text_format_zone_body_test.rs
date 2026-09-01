//! Phase 1 of `doc/design_hof_body_text_format.md`: the **reading** half.
//!
//! Before this, the text format did not project zone bodies at all — an
//! inline-body `map` serialized as `map1 = map { xs: r }`, so the AI read a
//! network that was not the one on screen. These tests pin the four outward
//! spellings a body can need (`$element`, `^capture`, `^^capture`,
//! `^$element`), the nested `output`, the multi-line block layout, and the
//! `closure` node's newly-gained text properties (D12).
//!
//! Bodies are still built by direct manipulation of the zone-owning node's
//! `NodeNetwork` — Phase 2 is what makes the syntax writable.

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::node_network::{Argument, IncomingWire, NodeNetwork, SourcePin};
use atomcad_structure_designer::node_type_registry::NodeTypeRegistry;
use atomcad_structure_designer::nodes::closure::{ClosureData, ClosureKind};
use atomcad_structure_designer::nodes::expr::{ExprData, ExprParameter};
use atomcad_structure_designer::nodes::filter::FilterData;
use atomcad_structure_designer::nodes::fold::FoldData;
use atomcad_structure_designer::nodes::foreach::ForeachData;
use atomcad_structure_designer::nodes::int::IntData;
use atomcad_structure_designer::nodes::map::MapData;
use atomcad_structure_designer::nodes::range::RangeData;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::{edit_network, serialize_network};
use glam::f64::DVec2;

const NETWORK: &str = "Main";

// ============================================================================
// Helpers
// ============================================================================

fn setup() -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(NETWORK);
    designer.set_active_node_network_name(Some(NETWORK.to_string()));
    designer
}

fn serialize(designer: &StructureDesigner) -> String {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(NETWORK).unwrap();
    serialize_network(network, registry, None)
}

/// Walk down a scope path to the network a body node lives in.
fn scope_network<'a>(
    designer: &'a mut StructureDesigner,
    scope_path: &[u64],
) -> &'a mut NodeNetwork {
    let mut network = designer
        .node_type_registry
        .node_networks
        .get_mut(NETWORK)
        .unwrap();
    for hof_id in scope_path {
        network = network
            .nodes
            .get_mut(hof_id)
            .expect("scope path names a missing node")
            .zone_mut()
            .expect("scope path names a node without a zone");
    }
    network
}

/// Add a node in the named scope and give it a stable, readable name.
fn add(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_type: &str,
    name: &str,
    y: f64,
) -> u64 {
    let id = designer.add_node_scoped(scope_path, node_type, DVec2::new(0.0, y), None);
    assert_ne!(id, 0, "failed to add a `{}` node", node_type);
    scope_network(designer, scope_path)
        .nodes
        .get_mut(&id)
        .unwrap()
        .custom_name = Some(name.to_string());
    id
}

/// Replace a node's data and re-derive its custom node type (which is what
/// turns `MapData::input_type` etc. into the node's zone pins).
fn set_data(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let (built_in_types, record_type_defs, built_in_record_type_defs, node_networks) = (
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        &mut registry.node_networks,
    );
    let mut network = node_networks.get_mut(NETWORK).unwrap();
    for hof_id in scope_path {
        network = network
            .nodes
            .get_mut(hof_id)
            .unwrap()
            .zone_mut()
            .expect("scope path names a node without a zone");
    }
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        built_in_types,
        record_type_defs,
        built_in_record_type_defs,
        node,
        true,
    );
}

/// Add an `expr` node with the given parameter names (which become its input
/// pin names) to the named scope.
fn add_expr(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    name: &str,
    expression: &str,
    params: &[&str],
    y: f64,
) -> u64 {
    let id = add(designer, scope_path, "expr", name, y);
    let mut data = ExprData {
        parameters: params
            .iter()
            .map(|p| ExprParameter {
                id: None,
                name: (*p).to_string(),
                data_type: DataType::Int,
                data_type_str: None,
            })
            .collect(),
        expression: expression.to_string(),
        expr: None,
        error: None,
        output_type: None,
    };
    let _ = data.parse_and_validate(0);
    set_data(designer, scope_path, id, Box::new(data));
    // `add_node_scoped` sized `arguments` from the *default* (parameterless)
    // expr type; the new parameters need matching argument slots.
    let node = scope_network(designer, scope_path)
        .nodes
        .get_mut(&id)
        .unwrap();
    node.arguments.resize_with(params.len(), Argument::new);
    id
}

/// Push one inbound wire onto a node's argument pin.
fn wire(
    designer: &mut StructureDesigner,
    scope_path: &[u64],
    dest_node_id: u64,
    dest_arg: usize,
    incoming: IncomingWire,
) {
    scope_network(designer, scope_path)
        .nodes
        .get_mut(&dest_node_id)
        .unwrap()
        .arguments[dest_arg]
        .incoming_wires
        .push(incoming);
}

/// Wire a body node into its owner's zone-output (`result` / `keep` / …) pin.
fn wire_zone_output(
    designer: &mut StructureDesigner,
    owner_scope: &[u64],
    owner_id: u64,
    body_node_id: u64,
) {
    let owner = scope_network(designer, owner_scope)
        .nodes
        .get_mut(&owner_id)
        .unwrap();
    if owner.zone_output_arguments.is_empty() {
        owner.zone_output_arguments.push(Argument::new());
    }
    owner.zone_output_arguments[0]
        .incoming_wires
        .push(IncomingWire::node_output(body_node_id, 0));
}

fn zone_input(owner_id: u64, pin_index: usize, depth: u8) -> IncomingWire {
    IncomingWire {
        source_node_id: owner_id,
        source_pin: SourcePin::ZoneInput { pin_index },
        source_scope_depth: depth,
    }
}

fn capture(source_node_id: u64, depth: u8) -> IncomingWire {
    IncomingWire {
        source_node_id,
        source_pin: SourcePin::NodeOutput { pin_index: 0 },
        source_scope_depth: depth,
    }
}

/// The canonical `map` from the design doc: a range, a captured constant, and
/// a two-node body that multiplies the element by the capture.
fn build_map_with_two_node_body() -> StructureDesigner {
    let mut designer = setup();

    let r = add(&mut designer, &[], "range", "r", 0.0);
    set_data(
        &mut designer,
        &[],
        r,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 5,
        }),
    );
    let scale = add(&mut designer, &[], "int", "scale", 100.0);
    set_data(&mut designer, &[], scale, Box::new(IntData { value: 3 }));

    let m1 = add(&mut designer, &[], "map", "m1", 200.0);
    set_data(
        &mut designer,
        &[],
        m1,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    wire(&mut designer, &[], m1, 0, IncomingWire::node_output(r, 0));

    // Body: `d = element * scale`, then `e = d + 1`.
    let d = add_expr(&mut designer, &[m1], "d", "a * b", &["a", "b"], 0.0);
    wire(&mut designer, &[m1], d, 0, zone_input(m1, 0, 1));
    wire(&mut designer, &[m1], d, 1, capture(scale, 1));
    let e = add_expr(&mut designer, &[m1], "e", "a + 1", &["a"], 100.0);
    wire(&mut designer, &[m1], e, 0, IncomingWire::node_output(d, 0));
    wire_zone_output(&mut designer, &[], m1, e);

    designer
        .node_type_registry
        .node_networks
        .get_mut(NETWORK)
        .unwrap()
        .return_node_id = Some(m1);
    designer
}

// ============================================================================
// Reading a body
// ============================================================================

#[test]
fn map_with_two_node_body_serializes_both_nodes_and_the_output() {
    let designer = build_map_with_two_node_body();
    let text = serialize(&designer);

    assert!(
        text.contains("  body {\n"),
        "expected an indented `body {{` block, got:\n{text}"
    );
    // Both body nodes are listed, in dependency order.
    let d_at = text.find("    d = expr").expect("body node `d` missing");
    let e_at = text.find("    e = expr").expect("body node `e` missing");
    assert!(d_at < e_at, "body nodes are not topologically ordered");
    // The body's result, and the network's own return, are both `output` —
    // the indentation is what distinguishes them.
    assert!(
        text.contains("    output e\n"),
        "expected the body's `output e`, got:\n{text}"
    );
    assert!(
        text.contains("\noutput m1\n"),
        "expected the network's `output m1`, got:\n{text}"
    );
    // Body nodes are part of the listing, so they are part of the count.
    assert!(
        text.contains("# 5 nodes"),
        "expected 5 nodes (3 top-level + 2 body), got:\n{text}"
    );
}

#[test]
fn body_node_reading_the_iteration_value_emits_dollar_element() {
    let designer = build_map_with_two_node_body();
    let text = serialize(&designer);
    assert!(
        text.contains("a: $element"),
        "expected `$element` for a depth-1 ZoneInput wire, got:\n{text}"
    );
}

#[test]
fn body_node_wired_to_a_parent_node_emits_a_caret_capture() {
    let designer = build_map_with_two_node_body();
    let text = serialize(&designer);
    assert!(
        text.contains("b: ^scale"),
        "expected `^scale` for a depth-1 NodeOutput capture, got:\n{text}"
    );
}

#[test]
fn intra_body_wire_stays_unqualified() {
    let designer = build_map_with_two_node_body();
    let text = serialize(&designer);
    // `e` reads `d`, both in the same body: no sigil at all.
    assert!(
        text.contains("e = expr { a: d,"),
        "expected a bare local reference `a: d`, got:\n{text}"
    );
}

#[test]
fn body_statement_is_laid_out_as_a_multi_line_block() {
    let designer = build_map_with_two_node_body();
    let text = serialize(&designer);
    // The whole `m1` statement, brace to brace. `xs:` is comma-terminated
    // like any property, and `body` is the last item.
    let start = text.find("m1 = map {\n").expect("multi-line `m1` missing");
    let block = &text[start..];
    let end = block.find("\n}\n").expect("`m1` block is not closed") + 3;
    // Properties are comma-separated and `body` is the last of them; the
    // statements *inside* the block are statements, not properties, so they
    // carry no commas.
    assert_eq!(
        &block[..end],
        "m1 = map {\n  \
           xs: r,\n  \
           input_type: Int,\n  \
           output_type: Int,\n  \
           visible: true,\n  \
           body {\n    \
             d = expr { a: $element, b: ^scale, expression: \"a * b\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Int }], visible: true }\n    \
             e = expr { a: d, expression: \"a + 1\", parameters: [{ name: \"a\", data_type: Int }], visible: true }\n    \
             output e\n  \
           }\n\
         }\n"
    );
}

// ============================================================================
// Nesting: `^^` and `^$`
// ============================================================================

/// `outer = map { body { inner = map { body { p = $element * ^$element } } } }`
/// plus a top-level `base` constant captured from two scopes down.
fn build_nested_maps() -> StructureDesigner {
    let mut designer = setup();

    let rows = add(&mut designer, &[], "range", "rows", 0.0);
    set_data(
        &mut designer,
        &[],
        rows,
        Box::new(RangeData {
            start: 0,
            step: 1,
            count: 3,
        }),
    );
    let base = add(&mut designer, &[], "int", "base", 100.0);
    set_data(&mut designer, &[], base, Box::new(IntData { value: 7 }));

    let outer = add(&mut designer, &[], "map", "outer", 200.0);
    set_data(
        &mut designer,
        &[],
        outer,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    wire(
        &mut designer,
        &[],
        outer,
        0,
        IncomingWire::node_output(rows, 0),
    );

    let inner = add(&mut designer, &[outer], "map", "inner", 0.0);
    set_data(
        &mut designer,
        &[outer],
        inner,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    // The inner map iterates the outer element.
    wire(&mut designer, &[outer], inner, 0, zone_input(outer, 0, 1));

    // Inner body: `p = $element * ^$element + ^^base`.
    let p = add_expr(
        &mut designer,
        &[outer, inner],
        "p",
        "a * b + c",
        &["a", "b", "c"],
        0.0,
    );
    wire(
        &mut designer,
        &[outer, inner],
        p,
        0,
        zone_input(inner, 0, 1),
    );
    wire(
        &mut designer,
        &[outer, inner],
        p,
        1,
        zone_input(outer, 0, 2),
    );
    wire(&mut designer, &[outer, inner], p, 2, capture(base, 2));
    wire_zone_output(&mut designer, &[outer], inner, p);
    wire_zone_output(&mut designer, &[], outer, inner);

    designer
}

#[test]
fn nested_body_reaches_a_two_scopes_out_node_with_a_double_caret() {
    let designer = build_nested_maps();
    let text = serialize(&designer);
    assert!(
        text.contains("c: ^^base"),
        "expected `^^base` for a depth-2 NodeOutput capture, got:\n{text}"
    );
}

/// The `ZoneInput`-at-depth-2 case: the only spelling that reaches an outer
/// HOF's iteration value from a nested body, and the one that has no
/// alternative encoding at all.
#[test]
fn nested_body_reaches_the_outer_iteration_value_with_caret_dollar() {
    let designer = build_nested_maps();
    let text = serialize(&designer);
    assert!(
        text.contains("a: $element, b: ^$element"),
        "expected `$element` (inner) and `^$element` (outer) side by side, got:\n{text}"
    );
}

#[test]
fn nested_bodies_nest_their_blocks_and_outputs() {
    let designer = build_nested_maps();
    let text = serialize(&designer);
    assert!(
        text.contains("    inner = map {\n"),
        "expected the inner map indented inside the outer body, got:\n{text}"
    );
    assert!(
        text.contains("      body {\n"),
        "expected a doubly-indented inner body, got:\n{text}"
    );
    assert!(
        text.contains("        output p\n"),
        "expected the inner body's `output p`, got:\n{text}"
    );
    assert!(
        text.contains("    output inner\n"),
        "expected the outer body's `output inner`, got:\n{text}"
    );
}

// ============================================================================
// Zone-pin names are not uniform
// ============================================================================

#[test]
fn fold_emits_its_own_acc_and_element_names() {
    let mut designer = setup();
    let xs = add(&mut designer, &[], "range", "xs", 0.0);
    set_data(
        &mut designer,
        &[],
        xs,
        Box::new(RangeData {
            start: 1,
            step: 1,
            count: 4,
        }),
    );
    let zero = add(&mut designer, &[], "int", "zero", 100.0);
    set_data(&mut designer, &[], zero, Box::new(IntData { value: 0 }));

    let total = add(&mut designer, &[], "fold", "total", 200.0);
    set_data(
        &mut designer,
        &[],
        total,
        Box::new(FoldData {
            element_type: DataType::Int,
            accumulator_type: DataType::Int,
        }),
    );
    wire(
        &mut designer,
        &[],
        total,
        0,
        IncomingWire::node_output(xs, 0),
    );
    wire(
        &mut designer,
        &[],
        total,
        1,
        IncomingWire::node_output(zero, 0),
    );

    let s = add_expr(&mut designer, &[total], "s", "a + b", &["a", "b"], 0.0);
    wire(&mut designer, &[total], s, 0, zone_input(total, 0, 1));
    wire(&mut designer, &[total], s, 1, zone_input(total, 1, 1));
    wire_zone_output(&mut designer, &[], total, s);

    let text = serialize(&designer);
    assert!(
        text.contains("a: $acc, b: $element"),
        "expected fold's `$acc` / `$element`, got:\n{text}"
    );
}

#[test]
fn zip_with_emits_per_lane_element_names() {
    let mut designer = setup();
    let z = add(&mut designer, &[], "zip_with", "z", 0.0);
    // The default `zip_with` already declares two `Float` lanes.
    let p = add_expr(&mut designer, &[z], "p", "a + b", &["a", "b"], 0.0);
    wire(&mut designer, &[z], p, 0, zone_input(z, 0, 1));
    wire(&mut designer, &[z], p, 1, zone_input(z, 1, 1));
    wire_zone_output(&mut designer, &[], z, p);

    let text = serialize(&designer);
    assert!(
        text.contains("a: $element1, b: $element2"),
        "expected zip_with's `$element1` / `$element2`, got:\n{text}"
    );
}

/// `filter` and `foreach` name their zone-output pin `keep` and `out`, not
/// `result` — a hand-written name table gets this wrong. The `output`
/// statement is spelled the same way regardless, which is the point: it is
/// read off `zone_output_arguments`, never off a name.
#[test]
fn filter_and_foreach_bodies_emit_their_output_statement() {
    for (node_type, data) in [
        (
            "filter",
            Box::new(FilterData {
                element_type: DataType::Int,
            }) as Box<dyn NodeData>,
        ),
        (
            "foreach",
            Box::new(ForeachData {
                input_type: DataType::Int,
            }) as Box<dyn NodeData>,
        ),
    ] {
        let mut designer = setup();
        let h = add(&mut designer, &[], node_type, "h", 0.0);
        set_data(&mut designer, &[], h, data);

        let body_node = add_expr(&mut designer, &[h], "k", "a > 2", &["a"], 0.0);
        wire(&mut designer, &[h], body_node, 0, zone_input(h, 0, 1));
        wire_zone_output(&mut designer, &[], h, body_node);

        let text = serialize(&designer);
        assert!(
            text.contains("    k = expr { a: $element,"),
            "{node_type}: expected `$element`, got:\n{text}"
        );
        assert!(
            text.contains("    output k\n"),
            "{node_type}: expected `output k` inside the body, got:\n{text}"
        );
    }
}

// ============================================================================
// `closure` gains text properties (D12)
// ============================================================================

#[test]
fn closure_emits_its_kind_params_and_type_args() {
    let mut designer = setup();
    let f1 = add(&mut designer, &[], "closure", "f1", 0.0);
    set_data(
        &mut designer,
        &[],
        f1,
        Box::new(ClosureData {
            kind: ClosureKind::Custom,
            type_args: vec![DataType::Int, DataType::Int, DataType::Int],
            param_names: vec!["x".to_string(), "y".to_string()],
            custom_label: None,
        }),
    );

    let p = add_expr(&mut designer, &[f1], "p", "a * b", &["a", "b"], 0.0);
    wire(&mut designer, &[f1], p, 0, zone_input(f1, 0, 1));
    wire(&mut designer, &[f1], p, 1, zone_input(f1, 1, 1));
    wire_zone_output(&mut designer, &[], f1, p);

    let text = serialize(&designer);
    assert!(
        text.contains("kind: \"custom\""),
        "expected the closure's kind, got:\n{text}"
    );
    assert!(
        text.contains("params: [\"x\", \"y\"]"),
        "expected the closure's parameter names, got:\n{text}"
    );
    assert!(
        text.contains("type_args: [Int, Int, Int]"),
        "expected the closure's type arguments, got:\n{text}"
    );
    // A closure's zone inputs are named by its own parameters, which is why
    // the names have to come from the node data rather than a static table.
    assert!(
        text.contains("a: $x, b: $y"),
        "expected the closure's own parameter names as zone inputs, got:\n{text}"
    );
}

#[test]
fn preset_closure_kinds_round_trip_their_spelling() {
    for (kind, spelling) in [
        (ClosureKind::Map, "map"),
        (ClosureKind::Filter, "filter"),
        (ClosureKind::Fold, "fold"),
        (ClosureKind::Foreach, "foreach"),
        (ClosureKind::Custom, "custom"),
    ] {
        assert_eq!(kind.text_name(), spelling);
        assert_eq!(ClosureKind::from_text_name(spelling), Some(kind));
    }
    assert_eq!(ClosureKind::from_text_name("nonsense"), None);
}

// ============================================================================
// No regression for the 94% of nodes with no body
// ============================================================================

#[test]
fn a_body_less_network_is_unchanged() {
    let mut designer = setup();
    let a = add(&mut designer, &[], "int", "a", 0.0);
    set_data(&mut designer, &[], a, Box::new(IntData { value: 2 }));
    let b = add(&mut designer, &[], "int", "b", 100.0);
    set_data(&mut designer, &[], b, Box::new(IntData { value: 40 }));
    let sum = add_expr(&mut designer, &[], "sum", "x + y", &["x", "y"], 200.0);
    wire(&mut designer, &[], sum, 0, IncomingWire::node_output(a, 0));
    wire(&mut designer, &[], sum, 1, IncomingWire::node_output(b, 0));
    designer
        .node_type_registry
        .node_networks
        .get_mut(NETWORK)
        .unwrap()
        .return_node_id = Some(sum);

    assert_eq!(
        serialize(&designer),
        "a = int { value: 2, visible: true }\n\
         b = int { value: 40, visible: true }\n\
         sum = expr { x: a, y: b, expression: \"x + y\", parameters: [{ name: \"x\", data_type: Int }, { name: \"y\", data_type: Int }], visible: true }\n\
         output sum\n\
         \n# 3 nodes\n"
    );
}

/// An HOF driven through its `f:` pin has an empty body, and an empty body
/// writes nothing at all — an omitted `body` leaves it untouched, which for
/// an already-empty body is the same state (D6).
#[test]
fn an_empty_body_emits_no_block() {
    let mut designer = setup();
    let m1 = add(&mut designer, &[], "map", "m1", 0.0);
    set_data(
        &mut designer,
        &[],
        m1,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );
    let text = serialize(&designer);
    assert!(
        !text.contains("body"),
        "an empty body should emit no block, got:\n{text}"
    );
    assert!(
        text.contains("m1 = map { input_type: Int, output_type: Int, visible: true }"),
        "expected the single-line form, got:\n{text}"
    );
}

// ============================================================================
// Phase 1 is read-only, deliberately
// ============================================================================

/// The editor is still single-scope, so `query` output for a body-bearing
/// network is not yet valid `edit --replace` input — the parser has no `body`
/// block and no `$` / `^` sigils. Phase 2 is what closes that.
///
/// Note what the failure replaces: before this change the round trip
/// *succeeded* and silently deleted every body node. A refused edit is the
/// better of the two states, but it is not the end state.
#[test]
fn query_output_is_not_yet_valid_edit_input() {
    let designer = build_map_with_two_node_body();
    let text = serialize(&designer);

    let mut network = designer
        .node_type_registry
        .node_networks
        .get(NETWORK)
        .unwrap()
        .clone();
    let result = edit_network(&mut network, &designer.node_type_registry, &text, true);

    assert!(
        !result.success,
        "Phase 1 does not parse `body` blocks; expected a refused edit, got:\n{:?}",
        result.errors
    );
}
