//! Unit tests for the `Walker` lazy-iterator runtime introduced in Phase 1
//! of the iterators design (`doc/design_iterators.md`).
//!
//! The Map/Filter rows construct `FunctionEvaluator`s by building tiny
//! networks via the text-format editor and capturing closures from `expr`
//! nodes — the same approach used by `function_evaluator_test.rs`.

use rust_lib_flutter_cad::structure_designer::evaluator::function_evaluator::FunctionEvaluator;
use rust_lib_flutter_cad::structure_designer::evaluator::iterator_walker::Walker;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::NetworkEvaluator;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{Closure, NetworkResult};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network;

// ============================================================================
// Helpers
// ============================================================================

fn make_evaluator() -> NetworkEvaluator {
    NetworkEvaluator::new()
}

fn empty_registry() -> NodeTypeRegistry {
    NodeTypeRegistry::new()
}

/// Drain a walker into a `Vec<NetworkResult>`, stopping at the first `None`.
/// Caps at 4096 elements as a runaway-test safety net.
fn drain(
    walker: &mut Walker,
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
) -> Vec<NetworkResult> {
    let mut out = Vec::new();
    let cap = 4096;
    while out.len() < cap {
        match walker.next(evaluator, registry) {
            None => return out,
            Some(v) => out.push(v),
        }
    }
    panic!("drain exceeded cap of {} elements", cap);
}

fn ints(values: &[i32]) -> Vec<NetworkResult> {
    values.iter().map(|&v| NetworkResult::Int(v)).collect()
}

fn assert_int_results(actual: &[NetworkResult], expected: &[i32]) {
    let got: Vec<i32> = actual
        .iter()
        .map(|r| match r {
            NetworkResult::Int(v) => *v,
            other => panic!(
                "expected NetworkResult::Int, got {}",
                other.to_display_string()
            ),
        })
        .collect();
    assert_eq!(got, expected);
}

// ----------------------------------------------------------------------------
// Designer / FunctionEvaluator helpers (used by Map / Filter rows)
// ----------------------------------------------------------------------------

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn edit_designer_network(
    designer: &mut StructureDesigner,
    network_name: &str,
    code: &str,
) -> rust_lib_flutter_cad::structure_designer::text_format::EditResult {
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .unwrap();
    let result = edit_network(&mut network, &designer.node_type_registry, code, true);
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);
    designer.validate_active_network();
    result
}

fn find_node_id(designer: &StructureDesigner, network_name: &str, node_type_name: &str) -> u64 {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    *network
        .nodes
        .iter()
        .find(|(_, n)| n.node_type_name == node_type_name)
        .unwrap_or_else(|| panic!("expected a `{}` node in `{}`", node_type_name, network_name))
        .0
}

/// Build a `FunctionEvaluator` for an `expr` node defined inside a freshly
/// edited network. The expr node is named `f` by convention, takes a single
/// `Int` parameter `x`, and returns whatever `expression` evaluates to.
fn build_expr_fe(
    network_name: &str,
    expression: &str,
    output_type: &str,
) -> (StructureDesigner, FunctionEvaluator) {
    let mut designer = setup_designer_with_network(network_name);
    let code = format!(
        r#"
            f = expr {{
                expression: "{expr}",
                parameters: [{{ name: "x", data_type: Int }}],
                output_type: {out}
            }}
        "#,
        expr = expression,
        out = output_type,
    );
    let result = edit_designer_network(&mut designer, network_name, &code);
    assert!(result.success, "edit_network failed: {:?}", result.errors);
    let f_id = find_node_id(&designer, network_name, "expr");
    let closure = Closure {
        node_network_name: network_name.to_string(),
        node_id: f_id,
        captured_argument_values: vec![NetworkResult::None],
    };
    let fe = FunctionEvaluator::new(closure, &designer.node_type_registry);
    (designer, fe)
}

// ============================================================================
// FromArray
// ============================================================================

#[test]
fn from_array_empty_yields_none() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::from_array(vec![]);
    assert!(w.next(&evaluator, &registry).is_none());
}

#[test]
fn from_array_drain_3_elements_in_order() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::from_array(ints(&[10, 20, 30]));
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[10, 20, 30]);
}

#[test]
fn from_array_drain_twice_without_reset_yields_none() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::from_array(ints(&[1, 2, 3]));
    let _ = drain(&mut w, &evaluator, &registry);
    assert!(
        w.next(&evaluator, &registry).is_none(),
        "second drain should immediately yield None"
    );
}

#[test]
fn from_array_reset_replays_full_sequence() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::from_array(ints(&[1, 2, 3]));
    let _ = drain(&mut w, &evaluator, &registry);
    w.reset();
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[1, 2, 3]);
}

#[test]
fn from_array_partial_drain_then_reset_replays_full_sequence() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::from_array(ints(&[1, 2, 3, 4, 5]));
    // Drain 2 of 5 explicitly.
    assert_eq!(
        matches!(w.next(&evaluator, &registry), Some(NetworkResult::Int(1))),
        true
    );
    assert_eq!(
        matches!(w.next(&evaluator, &registry), Some(NetworkResult::Int(2))),
        true
    );
    w.reset();
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[1, 2, 3, 4, 5]);
}

#[test]
fn from_array_clone_shares_arc() {
    let w = Walker::from_array(ints(&[1, 2, 3]));
    assert_eq!(w.from_array_items_strong_count(), Some(1));
    let clone = w.clone();
    assert_eq!(w.from_array_items_strong_count(), Some(2));
    assert_eq!(clone.from_array_items_strong_count(), Some(2));
    drop(clone);
    assert_eq!(w.from_array_items_strong_count(), Some(1));
}

#[test]
fn from_array_clone_advances_independently() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::from_array(ints(&[10, 20, 30, 40]));
    let _ = w.next(&evaluator, &registry); // original advanced past 10
    let mut clone = w.clone();
    // Drain the clone fully.
    let clone_drain = drain(&mut clone, &evaluator, &registry);
    assert_int_results(&clone_drain, &[20, 30, 40]);
    // Original continues from where it was — its idx was 1, so it should still
    // yield 20, 30, 40 in order.
    let original_drain = drain(&mut w, &evaluator, &registry);
    assert_int_results(&original_drain, &[20, 30, 40]);
}

// ============================================================================
// Range
// ============================================================================

#[test]
fn range_basic_drain() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::range(0, 1, 5);
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[0, 1, 2, 3, 4]);
}

#[test]
fn range_negative_step_drain() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::range(10, -2, 3);
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[10, 8, 6]);
}

#[test]
fn range_empty_when_count_is_zero() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::range(0, 1, 0);
    assert!(w.next(&evaluator, &registry).is_none());
}

#[test]
fn range_reset_replays_sequence() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::range(0, 1, 5);
    let first = drain(&mut w, &evaluator, &registry);
    w.reset();
    let second = drain(&mut w, &evaluator, &registry);
    assert_int_results(&first, &[0, 1, 2, 3, 4]);
    assert_int_results(&second, &[0, 1, 2, 3, 4]);
}

// ============================================================================
// Map
// ============================================================================

#[test]
fn map_drain_doubles_elements() {
    let (designer, fe) = build_expr_fe("net_map_double", "x * 2", "Int");
    let evaluator = make_evaluator();
    let mut w = Walker::map(Walker::range(0, 1, 5), fe);
    let out = drain(&mut w, &evaluator, &designer.node_type_registry);
    assert_int_results(&out, &[0, 2, 4, 6, 8]);
}

#[test]
fn map_reset_replays_sequence() {
    let (designer, fe) = build_expr_fe("net_map_reset", "x * 2", "Int");
    let evaluator = make_evaluator();
    let mut w = Walker::map(Walker::range(0, 1, 5), fe);
    let first = drain(&mut w, &evaluator, &designer.node_type_registry);
    w.reset();
    let second = drain(&mut w, &evaluator, &designer.node_type_registry);
    assert_int_results(&first, &[0, 2, 4, 6, 8]);
    assert_int_results(&second, &[0, 2, 4, 6, 8]);
}

#[test]
fn map_clone_advances_independently() {
    let (designer, fe) = build_expr_fe("net_map_clone", "x * 2", "Int");
    let evaluator = make_evaluator();
    let mut w = Walker::map(Walker::range(0, 1, 5), fe);
    // Advance original by 2 (consumes 0 and 1 from underlying range; emits 0, 2).
    assert!(matches!(
        w.next(&evaluator, &designer.node_type_registry),
        Some(NetworkResult::Int(0))
    ));
    assert!(matches!(
        w.next(&evaluator, &designer.node_type_registry),
        Some(NetworkResult::Int(2))
    ));
    let mut clone = w.clone();
    // Advance clone by 2 — it should pick up where the original left off.
    let clone_step1 = clone.next(&evaluator, &designer.node_type_registry);
    let clone_step2 = clone.next(&evaluator, &designer.node_type_registry);
    assert!(matches!(clone_step1, Some(NetworkResult::Int(4))));
    assert!(matches!(clone_step2, Some(NetworkResult::Int(6))));
    // Original is unaffected by clone's advancement — it still yields its
    // own next-in-sequence (which is also 4 because clone is independent).
    assert!(matches!(
        w.next(&evaluator, &designer.node_type_registry),
        Some(NetworkResult::Int(4))
    ));
}

// ============================================================================
// Filter
// ============================================================================

#[test]
fn filter_keeps_even_elements() {
    let (designer, fe) = build_expr_fe("net_filter_even", "x % 2 == 0", "Bool");
    let evaluator = make_evaluator();
    let mut w = Walker::filter(Walker::range(0, 1, 10), fe);
    let out = drain(&mut w, &evaluator, &designer.node_type_registry);
    assert_int_results(&out, &[0, 2, 4, 6, 8]);
}

#[test]
fn filter_all_false_drains_to_none_immediately() {
    let (designer, fe) = build_expr_fe("net_filter_all_false", "false", "Bool");
    let evaluator = make_evaluator();
    let mut w = Walker::filter(Walker::range(0, 1, 5), fe);
    let out = drain(&mut w, &evaluator, &designer.node_type_registry);
    assert!(out.is_empty(), "expected no elements, got {:?}", out.len());
}

#[test]
fn filter_non_bool_predicate_yields_error() {
    // Predicate returns Int — runtime should yield an Error and then None.
    let (designer, fe) = build_expr_fe("net_filter_nonbool", "x", "Int");
    let evaluator = make_evaluator();
    let mut w = Walker::filter(Walker::range(0, 1, 3), fe);
    let first = w.next(&evaluator, &designer.node_type_registry);
    match first {
        Some(NetworkResult::Error(msg)) => {
            assert!(
                msg.contains("non-Bool"),
                "error message should reference non-Bool, got: {}",
                msg
            );
        }
        other => panic!(
            "expected Error from filter with non-Bool predicate, got: {:?}",
            other.map(|r| r.to_display_string())
        ),
    }
    // Outer fuse: subsequent calls return None, not another error.
    assert!(
        w.next(&evaluator, &designer.node_type_registry).is_none(),
        "outer fuse should have tripped"
    );
    assert!(w.is_fused());
}

// ============================================================================
// Map error propagation + outer-fuse stickiness
// ============================================================================

#[test]
fn map_error_propagates_then_fuses() {
    // Expression that errors when x == 3: divide by (3 - x), so element 3
    // triggers "division by zero".
    let (designer, fe) = build_expr_fe("net_map_err", "10 / (3 - x)", "Int");
    let evaluator = make_evaluator();
    let mut w = Walker::map(Walker::range(0, 1, 6), fe);
    // Elements 0, 1, 2 should be 10/3=3, 10/2=5, 10/1=10 — but this depends on
    // Int division semantics in expr. Drain until we see the error.
    let mut got_values = Vec::new();
    let mut got_error = false;
    for _ in 0..10 {
        match w.next(&evaluator, &designer.node_type_registry) {
            None => break,
            Some(NetworkResult::Error(_)) => {
                got_error = true;
                break;
            }
            Some(other) => got_values.push(other),
        }
    }
    assert!(
        got_error,
        "expected an Error mid-stream, got values: {:?}",
        got_values.len()
    );
    // After error: subsequent calls return None (outer fuse).
    assert!(w.next(&evaluator, &designer.node_type_registry).is_none());
    assert!(w.is_fused());
}

// ============================================================================
// Product
// ============================================================================

#[test]
fn product_2x2_rightmost_varies_fastest() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::product(
        vec![
            Walker::from_array(ints(&[10, 20])),
            Walker::from_array(ints(&[1, 2])),
        ],
        vec!["a".to_string(), "b".to_string()],
    );
    let out = drain(&mut w, &evaluator, &registry);
    assert_eq!(out.len(), 4);
    let pairs: Vec<(i32, i32)> = out
        .iter()
        .map(|r| {
            let a = r.extract_record_field("a").unwrap();
            let b = r.extract_record_field("b").unwrap();
            let av = match a {
                NetworkResult::Int(v) => *v,
                _ => panic!("a not Int"),
            };
            let bv = match b {
                NetworkResult::Int(v) => *v,
                _ => panic!("b not Int"),
            };
            (av, bv)
        })
        .collect();
    // Rightmost (b) varies fastest.
    assert_eq!(pairs, vec![(10, 1), (10, 2), (20, 1), (20, 2)]);
}

#[test]
fn product_with_empty_axis_yields_none_immediately() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::product(
        vec![
            Walker::from_array(ints(&[1, 2])),
            Walker::from_array(vec![]), // empty
            Walker::from_array(ints(&[3, 4])),
        ],
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
    );
    assert!(w.next(&evaluator, &registry).is_none());
}

#[test]
fn product_single_axis_yields_each_value() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::product(
        vec![Walker::from_array(ints(&[7, 8, 9]))],
        vec!["x".to_string()],
    );
    let out = drain(&mut w, &evaluator, &registry);
    assert_eq!(out.len(), 3);
    let xs: Vec<i32> = out
        .iter()
        .map(|r| match r.extract_record_field("x").unwrap() {
            NetworkResult::Int(v) => *v,
            _ => panic!("x not Int"),
        })
        .collect();
    assert_eq!(xs, vec![7, 8, 9]);
}

#[test]
fn product_reset_replays_sequence() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::product(
        vec![
            Walker::from_array(ints(&[10, 20])),
            Walker::from_array(ints(&[1, 2])),
        ],
        vec!["a".to_string(), "b".to_string()],
    );
    let first = drain(&mut w, &evaluator, &registry);
    w.reset();
    let second = drain(&mut w, &evaluator, &registry);
    assert_eq!(first.len(), 4);
    assert_eq!(second.len(), 4);
    // Compare displayed strings as a structural check.
    let s1: Vec<String> = first.iter().map(|r| r.to_display_string()).collect();
    let s2: Vec<String> = second.iter().map(|r| r.to_display_string()).collect();
    assert_eq!(s1, s2);
}

#[test]
fn product_partial_drain_reset_replays_full_sequence() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let make = || {
        Walker::product(
            vec![
                Walker::from_array(ints(&[10, 20])),
                Walker::from_array(ints(&[1, 2, 3])),
            ],
            vec!["a".to_string(), "b".to_string()],
        )
    };
    let mut w = make();
    // Drain 3 records mid-odometer.
    for _ in 0..3 {
        assert!(w.next(&evaluator, &registry).is_some());
    }
    w.reset();
    let after_reset = drain(&mut w, &evaluator, &registry);
    // Compare against a fresh full drain from a brand-new walker.
    let mut fresh = make();
    let fresh_drain = drain(&mut fresh, &evaluator, &registry);
    assert_eq!(after_reset.len(), fresh_drain.len());
    let after_strs: Vec<String> = after_reset.iter().map(|r| r.to_display_string()).collect();
    let fresh_strs: Vec<String> = fresh_drain.iter().map(|r| r.to_display_string()).collect();
    assert_eq!(after_strs, fresh_strs);
}

#[test]
fn product_3_axes_mixed_radix_carry() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::product(
        vec![
            Walker::from_array(ints(&[100, 200])),
            Walker::from_array(ints(&[10, 20])),
            Walker::from_array(ints(&[1, 2])),
        ],
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
    );
    let out = drain(&mut w, &evaluator, &registry);
    assert_eq!(out.len(), 8);
    // Rightmost (c) varies fastest, then b, then a.
    let triples: Vec<(i32, i32, i32)> = out
        .iter()
        .map(|r| {
            let a = match r.extract_record_field("a").unwrap() {
                NetworkResult::Int(v) => *v,
                _ => panic!(),
            };
            let b = match r.extract_record_field("b").unwrap() {
                NetworkResult::Int(v) => *v,
                _ => panic!(),
            };
            let c = match r.extract_record_field("c").unwrap() {
                NetworkResult::Int(v) => *v,
                _ => panic!(),
            };
            (a, b, c)
        })
        .collect();
    assert_eq!(
        triples,
        vec![
            (100, 10, 1),
            (100, 10, 2),
            (100, 20, 1),
            (100, 20, 2),
            (200, 10, 1),
            (200, 10, 2),
            (200, 20, 1),
            (200, 20, 2),
        ]
    );
}

#[test]
fn product_clone_advances_independently() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let make = || {
        Walker::product(
            vec![
                Walker::from_array(ints(&[10, 20])),
                Walker::from_array(ints(&[1, 2])),
            ],
            vec!["a".to_string(), "b".to_string()],
        )
    };
    let mut w = make();
    // Advance the original by 1 (yielding (10, 1)).
    let _ = w.next(&evaluator, &registry);
    let mut clone = w.clone();
    // Drain the clone fully — should yield 3 more records.
    let clone_drain = drain(&mut clone, &evaluator, &registry);
    assert_eq!(clone_drain.len(), 3);
    // Original is unaffected by clone's advancement: still yields its own
    // remaining 3 records.
    let original_drain = drain(&mut w, &evaluator, &registry);
    assert_eq!(original_drain.len(), 3);
}

// ============================================================================
// Display-cap boundary subtitles (Phase 7 of design_iterators.md)
// ============================================================================
//
// `NetworkEvaluator::drain_walker_for_display` is what `generate_scene`
// calls to produce the subtitle decoration on a node whose displayed pin
// output is `Iter[T]`. The display drain caps at `ITER_DISPLAY_CAP` (256)
// elements; the subtitle wording branches on whether the walker exhausted
// before the cap or hit it.

#[test]
fn display_cap_below_boundary_reports_exhausted_count() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut walker = Walker::range(0, 1, 255);
    let (items, subtitle) = evaluator.drain_walker_for_display(&mut walker, "Iter[Int]", &registry);
    assert_eq!(items.len(), 255);
    assert_eq!(subtitle, "Iter[Int] (255 elements)");
}

#[test]
fn display_cap_at_boundary_reports_first_n() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut walker = Walker::range(0, 1, 256);
    let (items, subtitle) = evaluator.drain_walker_for_display(&mut walker, "Iter[Int]", &registry);
    assert_eq!(items.len(), 256);
    // At exactly the cap the walker has yielded ≥ ITER_DISPLAY_CAP elements,
    // so the subtitle reads "(showing first 256)" — see the design doc's
    // Display section for the wording rule.
    assert_eq!(subtitle, "Iter[Int] (showing first 256)");
}

#[test]
fn display_cap_above_boundary_reports_first_n() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut walker = Walker::range(0, 1, 257);
    let (items, subtitle) = evaluator.drain_walker_for_display(&mut walker, "Iter[Int]", &registry);
    // The drain stops at the cap regardless of how many elements remain.
    assert_eq!(items.len(), 256);
    assert_eq!(subtitle, "Iter[Int] (showing first 256)");
}

#[test]
fn display_cap_empty_walker_reports_zero_elements() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut walker = Walker::from_array(Vec::new());
    let (items, subtitle) =
        evaluator.drain_walker_for_display(&mut walker, "Iter[Float]", &registry);
    assert_eq!(items.len(), 0);
    assert_eq!(subtitle, "Iter[Float] (0 elements)");
}

// ============================================================================
// Composite: Map { Filter { Range } }
// ============================================================================

#[test]
fn nested_map_filter_range() {
    let (designer_filter, fe_filter) = build_expr_fe("net_nested_filter", "x % 2 == 0", "Bool");
    // Need a separate FE for the outer map since each FE owns its own network.
    let (designer_map, fe_map) = build_expr_fe("net_nested_map", "x * x", "Int");
    let evaluator = make_evaluator();
    // Build a registry that contains both networks so both FEs resolve.
    // The walker calls FE.evaluate(evaluator, registry); each FE has its own
    // internal network and looks up only built-in node types from the registry,
    // so either designer's registry works for both FEs as long as it has the
    // built-ins (the default registry always does).
    let _ = designer_filter; // keep alive
    let mut w = Walker::map(Walker::filter(Walker::range(0, 1, 10), fe_filter), fe_map);
    let out = drain(&mut w, &evaluator, &designer_map.node_type_registry);
    assert_int_results(&out, &[0, 4, 16, 36, 64]);
}
