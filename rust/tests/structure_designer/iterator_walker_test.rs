//! Unit tests for the `Walker` lazy-iterator runtime introduced in Phase 1
//! of the iterators design (`doc/design_iterators.md`).
//!
//! Covers the source/structural walker variants (`FromArray`, `Range`,
//! `Product`). The `map`/`filter` walkers are now zone-closure driven
//! (`MapZone` / `FilterZone`) and are exercised end-to-end through the HOF
//! node tests (`map_test`, `filter_test`, `closures_test`) rather than by
//! hand-built walkers here. The legacy FE-driven `Walker::map`/`Walker::filter`
//! constructors were removed in closures Phase 2 (`doc/design_closures.md`).

use rust_lib_flutter_cad::structure_designer::evaluator::iterator_walker::Walker;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

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
///
/// The walker no longer self-supplies its evaluation context — Phase 2 of the
/// node-execution design threads the outer-pass context through `Walker::next`
/// so closures inside zone walkers inherit `execute` and so prints drain back
/// into the per-pass log. Tests that don't care about either flag just
/// construct an empty context here.
fn drain(
    walker: &mut Walker,
    evaluator: &NetworkEvaluator,
    registry: &NodeTypeRegistry,
) -> Vec<NetworkResult> {
    let mut ctx = NetworkEvaluationContext::new();
    let mut out = Vec::new();
    let cap = 4096;
    while out.len() < cap {
        match walker.next(evaluator, registry, &mut ctx) {
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

// ============================================================================
// FromArray
// ============================================================================

#[test]
fn from_array_empty_yields_none() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::from_array(vec![]);
    assert!(
        w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new())
            .is_none()
    );
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
        w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new())
            .is_none(),
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
        matches!(
            w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new()),
            Some(NetworkResult::Int(1))
        ),
        true
    );
    assert_eq!(
        matches!(
            w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new()),
            Some(NetworkResult::Int(2))
        ),
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
    let _ = w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new()); // original advanced past 10
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
    assert!(
        w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new())
            .is_none()
    );
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
    assert!(
        w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new())
            .is_none()
    );
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
        assert!(
            w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new())
                .is_some()
        );
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
    let _ = w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new());
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
// ZipZone (zip_with's walker — `doc/design_zip_with.md` Phase 1)
// ============================================================================

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::zone_closure::ZoneClosure;
use rust_lib_flutter_cad::structure_designer::node_network::{
    IncomingWire, NodeNetwork, SourcePin,
};
use std::collections::HashMap;
use std::sync::Arc;

/// A minimal hand-built closure whose result wire reads the zone-input pin at
/// `pin_index` directly (a lane passthrough). `eval_step`'s `ZoneInput` branch
/// resolves it from the live frame pushed by `run_closure_once`, so no body
/// nodes are needed — enough to exercise the ZipZone pull/frame discipline
/// without standing up a full designer.
#[allow(clippy::arc_with_non_send_sync)]
fn passthrough_closure(pin_index: usize, arity: usize) -> ZoneClosure {
    const OWNER: u64 = 42;
    ZoneClosure {
        body: Arc::new(NodeNetwork::new_empty()),
        captures: Arc::new(HashMap::new()),
        zone_output_wires: Arc::new(vec![IncomingWire {
            source_node_id: OWNER,
            source_pin: SourcePin::ZoneInput { pin_index },
            source_scope_depth: 0,
        }]),
        owner_node_id: OWNER,
        param_types: vec![DataType::Int; arity],
        return_type: DataType::Int,
        pre_supplied_args: Arc::new(Vec::new()),
    }
}

#[test]
fn zip_zone_pulls_all_lanes_elementwise_in_order() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    // Passthrough of lane 0: results mirror the first source.
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(ints(&[1, 2, 3])),
            Walker::from_array(ints(&[10, 20, 30])),
        ],
        passthrough_closure(0, 2),
    );
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[1, 2, 3]);

    // Passthrough of lane 1: the frame really carries every lane's element,
    // in lane order.
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(ints(&[1, 2, 3])),
            Walker::from_array(ints(&[10, 20, 30])),
        ],
        passthrough_closure(1, 2),
    );
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[10, 20, 30]);
}

#[test]
fn zip_zone_truncates_to_shortest_input() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(ints(&[1, 2, 3])),
            Walker::from_array(ints(&[10, 20, 30, 40, 50])),
        ],
        passthrough_closure(1, 2),
    );
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[10, 20, 30]);
}

#[test]
fn zip_zone_empty_source_yields_empty() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(vec![]),
            Walker::from_array(ints(&[10, 20])),
        ],
        passthrough_closure(0, 2),
    );
    assert!(
        w.next(&evaluator, &registry, &mut NetworkEvaluationContext::new())
            .is_none()
    );
}

#[test]
fn zip_zone_error_mid_stream_fuses() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(vec![
                NetworkResult::Int(1),
                NetworkResult::Error("boom".to_string()),
                NetworkResult::Int(3),
            ]),
            Walker::from_array(ints(&[10, 20, 30])),
        ],
        passthrough_closure(0, 2),
    );
    let mut ctx = NetworkEvaluationContext::new();
    assert!(matches!(
        w.next(&evaluator, &registry, &mut ctx),
        Some(NetworkResult::Int(1))
    ));
    assert!(matches!(
        w.next(&evaluator, &registry, &mut ctx),
        Some(NetworkResult::Error(_))
    ));
    assert!(w.is_fused());
    assert!(w.next(&evaluator, &registry, &mut ctx).is_none());
}

#[test]
fn zip_zone_clone_advances_independently() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(ints(&[1, 2, 3])),
            Walker::from_array(ints(&[10, 20, 30])),
        ],
        passthrough_closure(1, 2),
    );
    let mut ctx = NetworkEvaluationContext::new();
    // Advance the original past the first element, then clone mid-stream.
    let _ = w.next(&evaluator, &registry, &mut ctx);
    let mut clone = w.clone();
    let clone_out = drain(&mut clone, &evaluator, &registry);
    assert_int_results(&clone_out, &[20, 30]);
    // The original is unaffected by the clone's advancement.
    let original_out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&original_out, &[20, 30]);
}

#[test]
fn zip_zone_reset_rewinds_all_sources() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(ints(&[1, 2])),
            Walker::from_array(ints(&[10, 20])),
        ],
        passthrough_closure(1, 2),
    );
    let _ = drain(&mut w, &evaluator, &registry);
    w.reset();
    let out = drain(&mut w, &evaluator, &registry);
    assert_int_results(&out, &[10, 20]);
}

#[test]
fn zip_zone_undersized_closure_yields_error_not_panic() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    // A 1-parameter closure over 2 lanes is unreachable through type-checking
    // but reachable via a hand-authored file: must yield an Error element.
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(ints(&[1, 2])),
            Walker::from_array(ints(&[10, 20])),
        ],
        passthrough_closure(0, 1),
    );
    let mut ctx = NetworkEvaluationContext::new();
    match w.next(&evaluator, &registry, &mut ctx) {
        Some(NetworkResult::Error(msg)) => {
            assert!(msg.contains("zip_with"), "unexpected message: {}", msg)
        }
        other => panic!("expected Error element, got {:?}", other.is_none()),
    }
    // The outer fuse terminates the stream after the error.
    assert!(w.next(&evaluator, &registry, &mut ctx).is_none());
}

#[test]
fn zip_zone_excess_arity_yields_partial_functions() {
    let evaluator = make_evaluator();
    let registry = empty_registry();
    // A 3-parameter closure over 2 lanes: each element binds the two lane
    // values into `pre_supplied_args` and yields a 1-parameter Function value
    // (currying auto-partialization, mirroring MapZone).
    let mut w = Walker::zip_zone(
        vec![
            Walker::from_array(ints(&[1, 2])),
            Walker::from_array(ints(&[10, 20])),
        ],
        passthrough_closure(0, 3),
    );
    let out = drain(&mut w, &evaluator, &registry);
    assert_eq!(out.len(), 2);
    for elem in &out {
        match elem {
            NetworkResult::Function(zc) => {
                assert_eq!(zc.param_types.len(), 1);
                assert_eq!(zc.pre_supplied_args.len(), 2);
            }
            other => panic!(
                "expected Function element, got {}",
                other.to_display_string()
            ),
        }
    }
}
