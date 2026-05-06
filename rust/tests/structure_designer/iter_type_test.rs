//! Type-system + wire-validation tests for `Iter[T]` (Phase 1 of
//! `doc/design_iterators.md`).
//!
//! Coverage:
//! - `DataType::can_be_converted_to` for every documented `Iter[T]` rule
//!   (and every documented rejection).
//! - Closure-capture restriction: a function pin whose source has an
//!   `Iter[T]` value pin is rejected by the network validator.
//! - Top-level parameter rejection at the CLI binding layer.

use rust_lib_flutter_cad::structure_designer::data_type::{
    DataType, FunctionType, RecordType, contains_iterator,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;

// ============================================================================
// Wire-time conversion rules
// ============================================================================

#[test]
fn array_to_iter_same_element_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Array(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn array_to_iter_with_element_widening_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Array(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn scalar_to_iter_is_allowed_via_broadcast() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Int;
    let dst = DataType::Iterator(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn scalar_to_iter_with_widening_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Int;
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn iter_identity_is_allowed() {
    let registry = NodeTypeRegistry::new();
    let t = DataType::Iterator(Box::new(DataType::Int));
    assert!(DataType::can_be_converted_to(&t, &t, &registry));
}

#[test]
fn iter_to_iter_with_different_element_types_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Iterator(Box::new(DataType::Float));
    assert!(
        !DataType::can_be_converted_to(&src, &dst, &registry),
        "Iter[Int] → Iter[Float] is reserved for a follow-up; not implicit in v1"
    );
}

#[test]
fn iter_to_array_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Array(Box::new(DataType::Int));
    assert!(
        !DataType::can_be_converted_to(&src, &dst, &registry),
        "Iter[T] → [T] requires an explicit `collect` node"
    );
}

#[test]
fn iter_to_scalar_is_rejected() {
    let registry = NodeTypeRegistry::new();
    let src = DataType::Iterator(Box::new(DataType::Int));
    let dst = DataType::Int;
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

#[test]
fn iter_inside_array_disallowed_widening_to_array_only_iter() {
    // `[Iter[Int]] → [Int]` would require unwrapping the iterator at every
    // element, which is not an implicit conversion.
    let registry = NodeTypeRegistry::new();
    let src = DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int))));
    let dst = DataType::Array(Box::new(DataType::Int));
    assert!(!DataType::can_be_converted_to(&src, &dst, &registry));
}

// ============================================================================
// `Iter[T]` parsing roundtrip
// ============================================================================

#[test]
fn iter_int_parses_and_displays() {
    let parsed = DataType::from_string("Iter[Int]").unwrap();
    assert_eq!(parsed, DataType::Iterator(Box::new(DataType::Int)));
    assert_eq!(parsed.to_string(), "Iter[Int]");
}

#[test]
fn nested_iter_parses() {
    let parsed = DataType::from_string("Iter[Iter[Int]]").unwrap();
    assert_eq!(
        parsed,
        DataType::Iterator(Box::new(DataType::Iterator(Box::new(DataType::Int))))
    );
}

#[test]
fn array_of_iter_parses() {
    let parsed = DataType::from_string("[Iter[Int]]").unwrap();
    assert_eq!(
        parsed,
        DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int))))
    );
}

#[test]
fn bare_iter_without_brackets_is_rejected() {
    // `Iter` alone is not a valid type — the bracket is mandatory.
    assert!(DataType::from_string("Iter").is_err());
}

// ============================================================================
// `contains_iterator` helper
// ============================================================================

#[test]
fn contains_iterator_recognizes_direct() {
    assert!(contains_iterator(&DataType::Iterator(Box::new(
        DataType::Int
    ))));
}

#[test]
fn contains_iterator_recurses_into_array() {
    let t = DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int))));
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_recurses_into_function_param() {
    let t = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Iterator(Box::new(DataType::Int))],
        output_type: Box::new(DataType::Int),
    });
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_recurses_into_function_return() {
    let t = DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Iterator(Box::new(DataType::Int))),
    });
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_recurses_into_anonymous_record() {
    let t = DataType::Record(RecordType::Anonymous(vec![(
        "field".to_string(),
        DataType::Iterator(Box::new(DataType::Int)),
    )]));
    assert!(contains_iterator(&t));
}

#[test]
fn contains_iterator_returns_false_for_iter_free_types() {
    assert!(!contains_iterator(&DataType::Int));
    assert!(!contains_iterator(&DataType::Array(Box::new(
        DataType::Float
    ))));
    assert!(!contains_iterator(&DataType::Function(FunctionType {
        parameter_types: vec![DataType::Int],
        output_type: Box::new(DataType::Float),
    })));
}

// ============================================================================
// Closure-capture restriction (validator)
// ============================================================================

mod closure_capture_validation {
    use super::*;
    use glam::f64::DVec2;
    use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
    use rust_lib_flutter_cad::structure_designer::nodes::expr::ExprData;
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

    fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
        let mut designer = StructureDesigner::new();
        designer.add_node_network(network_name);
        designer.set_active_node_network_name(Some(network_name.to_string()));
        designer
    }

    /// Override an `expr` node's parameter list with a single parameter of
    /// the given `data_type`. We do this programmatically rather than through
    /// the text-format editor because the text-format `data_type:` literal
    /// only accepts a single identifier — `Iter[Int]` cannot be expressed
    /// there in v1 (this could be relaxed in a follow-up).
    fn set_expr_single_parameter(
        designer: &mut StructureDesigner,
        network_name: &str,
        node_id: u64,
        param_name: &str,
        data_type: DataType,
    ) {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut(network_name).unwrap();
        let node = network.nodes.get_mut(&node_id).unwrap();
        if let Some(expr_data) = node.data.as_any_mut().downcast_mut::<ExprData>() {
            expr_data.parameters = vec![
                rust_lib_flutter_cad::structure_designer::nodes::expr::ExprParameter {
                    id: Some(1),
                    name: param_name.to_string(),
                    data_type,
                    data_type_str: None,
                },
            ];
        }
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    /// Wire `(src_node_id, src_pin_index)` into `dst_node`'s argument index
    /// `dst_arg_index`.
    fn connect(
        designer: &mut StructureDesigner,
        network_name: &str,
        src_node_id: u64,
        src_pin_index: i32,
        dst_node_id: u64,
        dst_arg_index: usize,
    ) {
        let registry = &mut designer.node_type_registry;
        let network = registry.node_networks.get_mut(network_name).unwrap();
        let dst_node = network.nodes.get_mut(&dst_node_id).unwrap();
        // Make sure the argument vec is long enough.
        while dst_node.arguments.len() <= dst_arg_index {
            dst_node.arguments.push(Default::default());
        }
        dst_node.arguments[dst_arg_index]
            .argument_output_pins
            .insert(src_node_id, src_pin_index);
    }

    /// An `expr` whose parameter is `Iter[Int]`. Wire the `expr`'s function
    /// pin (-1) into `map.f`. The validator should reject the function-pin
    /// wire because the captured value-pin type contains `Iter[T]`.
    ///
    /// Phase 4 note: prior to flipping `map.xs` to `Iter[T]`, the test also
    /// wired `range → map.xs` as scaffolding. Post-Phase 4, that wire would
    /// be a different type mismatch (`Iter[Int] → Iter[Float]`) which fires
    /// before the closure-capture check; we drop it so the closure-capture
    /// error is the one that surfaces.
    #[test]
    fn function_pin_rejects_iter_t_capture_via_value_pin() {
        let mut designer = setup_designer_with_network("net_iter_capture");

        let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));
        let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));

        // Set the expr's parameter type to Iter[Int]. (The expression body
        // doesn't matter for the validator's structural check.)
        set_expr_single_parameter(
            &mut designer,
            "net_iter_capture",
            expr_id,
            "x",
            DataType::Iterator(Box::new(DataType::Int)),
        );

        // Connect expr's function pin (-1) → map.f (param 1). map.xs is left
        // unconnected on purpose (see header comment).
        connect(&mut designer, "net_iter_capture", expr_id, -1, map_id, 1);

        designer.validate_active_network();

        let network = designer
            .node_type_registry
            .node_networks
            .get("net_iter_capture")
            .unwrap();
        assert!(
            !network.valid,
            "network with Iter[T] captured into a function pin should be invalid; \
             errors={:?}",
            network
                .validation_errors
                .iter()
                .map(|e| e.error_text.clone())
                .collect::<Vec<_>>()
        );
        let has_iter_capture_error = network
            .validation_errors
            .iter()
            .any(|e| e.error_text.contains("Iter") && e.error_text.contains("collect"));
        assert!(
            has_iter_capture_error,
            "expected an error mentioning `Iter` and `collect`, got: {:?}",
            network
                .validation_errors
                .iter()
                .map(|e| e.error_text.clone())
                .collect::<Vec<_>>()
        );
    }

    /// Same as above but the captured value-pin type is `[Iter[Int]]` (array
    /// of iterators). The `contains_iterator` predicate must recurse through
    /// arrays, so this is also rejected.
    #[test]
    fn function_pin_rejects_array_of_iter_capture() {
        let mut designer = setup_designer_with_network("net_array_iter_capture");

        let range_id = designer.add_node("range", DVec2::new(0.0, 0.0));
        let expr_id = designer.add_node("expr", DVec2::new(100.0, 0.0));
        let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));

        set_expr_single_parameter(
            &mut designer,
            "net_array_iter_capture",
            expr_id,
            "y",
            DataType::Array(Box::new(DataType::Iterator(Box::new(DataType::Int)))),
        );

        connect(
            &mut designer,
            "net_array_iter_capture",
            range_id,
            0,
            map_id,
            0,
        );
        connect(
            &mut designer,
            "net_array_iter_capture",
            expr_id,
            -1,
            map_id,
            1,
        );

        designer.validate_active_network();

        let network = designer
            .node_type_registry
            .node_networks
            .get("net_array_iter_capture")
            .unwrap();
        assert!(
            !network.valid,
            "captured `[Iter[Int]]` should also be rejected"
        );
    }
}

// ============================================================================
// Top-level parameter rejection (CLI binding layer)
// ============================================================================
//
// `cli_runner::parse_cli_parameters` is the binding layer. It is `pub(crate)`
// (no `pub` qualifier on its `mod`-level `fn`), so we exercise the rule
// indirectly: a network whose parameter has type `Iter[Int]` is set up, and
// we re-run the equivalent `contains_iterator(&param.data_type)` predicate
// that the CLI uses. If `contains_iterator` returns `true`, the CLI runner
// rejects the parameter with the documented error message.
//
// The end-to-end CLI run is exercised in CLI integration tests; here we
// verify the predicate that gates the rejection.

#[test]
fn cli_top_level_parameter_with_iter_type_is_flagged() {
    // The predicate that gates CLI parameter rejection is `contains_iterator`.
    // `cli_runner::parse_cli_parameters` checks `contains_iterator(&param_def.data_type)`
    // before parsing and returns an explanatory error mentioning `Iter[T]` and
    // `collect`. If `contains_iterator` ever returns false for a declared
    // `Iter` type, the CLI rejection silently disappears — so lock the
    // predicate's behavior in here.
    assert!(contains_iterator(&DataType::Iterator(Box::new(
        DataType::Int
    ))));
    assert!(contains_iterator(&DataType::Array(Box::new(
        DataType::Iterator(Box::new(DataType::Int))
    ))));
    assert!(!contains_iterator(&DataType::Int));
    assert!(!contains_iterator(&DataType::Array(Box::new(
        DataType::Float
    ))));
}
