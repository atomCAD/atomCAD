//! Phase 8 tests for record types (see `doc/design_record_types.md`).
//!
//! Phase 8 introduces the `product` node — cartesian product of N input
//! arrays into `Array[Record(target)]`. Pin layout follows the target def's
//! authored field order; iteration order is **rightmost field varies
//! fastest**.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::product::ProductData;
use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

/// Phase 6: `product` produces `Iter[Record(target)]`. Drain the walker into a
/// concrete `Vec<NetworkResult>` for assertions. Mirrors the pattern used in
/// `filter_test::extract_int_array`.
fn drain_iter(designer: &StructureDesigner, result: NetworkResult) -> Vec<NetworkResult> {
    match result {
        NetworkResult::Iterator(mut walker) => {
            let registry = &designer.node_type_registry;
            let evaluator = NetworkEvaluator::new();
            let mut out = Vec::new();
            loop {
                match walker.next(&evaluator, registry, &mut NetworkEvaluationContext::new()) {
                    None => break,
                    Some(NetworkResult::Error(e)) => panic!("walker yielded Error: {}", e),
                    Some(v) => out.push(v),
                }
            }
            out
        }
        NetworkResult::Array(items) => items,
        other => panic!(
            "expected Iterator or Array, got {}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn evaluate_node_pin(
    designer: &StructureDesigner,
    network_name: &str,
    node_id: u64,
    output_pin_index: i32,
) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        is_zone_body: false,
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(
        &network_stack,
        node_id,
        output_pin_index,
        registry,
        false,
        &mut context,
    )
}

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// Build a `sequence` node and wire `int` nodes carrying the given values
/// into its input pins. Returns the sequence node's id.
fn add_int_sequence(designer: &mut StructureDesigner, values: Vec<i32>) -> u64 {
    let n = values.len();
    let seq = designer.add_node("sequence", DVec2::new(0.0, 0.0));
    set_node_data(
        designer,
        "test",
        seq,
        Box::new(SequenceData {
            element_type: DataType::Int,
            input_count: n.max(1),
        }),
    );
    if n == 0 {
        // 0-element sequence: leave one input pin unconnected. The sequence
        // node skips unconnected inputs, so the resulting array is empty.
        return seq;
    }
    for (i, v) in values.into_iter().enumerate() {
        let int_id = designer.add_node("int", DVec2::new(0.0, (i as f64) * 50.0));
        set_node_data(designer, "test", int_id, Box::new(IntData { value: v }));
        designer.connect_nodes(int_id, 0, seq, i);
    }
    seq
}

fn record_field_int(rec: &NetworkResult, field: &str) -> i32 {
    rec.extract_record_field(field)
        .and_then(|v| {
            if let NetworkResult::Int(i) = v {
                Some(*i)
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            panic!(
                "missing or non-int field `{}` on {}",
                field,
                rec.to_display_string()
            )
        })
}

// `Date = {year, month, day}` — non-alphabetical authored order so the
// authored-vs-canonical distinction shows up in pin-layout assertions.
fn date_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Date".to_string(),
        vec![
            ("year".to_string(), DataType::Int),
            ("month".to_string(), DataType::Int),
            ("day".to_string(), DataType::Int),
        ],
    )
}

fn pair_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Pair".to_string(),
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Int),
        ],
    )
}

// ============================================================================
// Registration / pin layout
// ============================================================================

#[test]
fn product_registered() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("product").expect("registered");
    assert_eq!(nt.name, "product");
    assert!(nt.public);
    // Base type has zero parameters; the cache populator fills them in
    // per-instance from the `target` property.
    assert_eq!(nt.parameters.len(), 0);
    assert_eq!(nt.output_pins.len(), 1);
}

#[test]
fn product_pin_layout_follows_authored_order() {
    // `Date = {year, month, day}` — non-alphabetical authored order.
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(date_def())
        .unwrap();

    let id = designer.add_node("product", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(ProductData {
            target: "Date".to_string(),
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    // Pin order = authored order, NOT alphabetical.
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["year", "month", "day"]);
    // Phase 6: each input pin is Iter[FieldType].
    for p in &nt.parameters {
        assert!(matches!(p.data_type, DataType::Iterator(ref el) if **el == DataType::Int));
    }
    // Phase 6: output is Iter[Record(Named("Date"))].
    let expected_out = DataType::Iterator(Box::new(DataType::Record(RecordType::Named(
        "Date".to_string(),
    ))));
    assert_eq!(nt.output_pins[0].fixed_type(), Some(&expected_out));
}

// ============================================================================
// Pin-layout snapshot test
// ============================================================================

#[test]
fn product_pin_layout_snapshot() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(date_def())
        .unwrap();

    let id = designer.add_node("product", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(ProductData {
            target: "Date".to_string(),
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    let mut s = String::new();
    s.push_str("inputs:\n");
    for p in &nt.parameters {
        s.push_str(&format!("  {}: {}\n", p.name, p.data_type));
    }
    s.push_str("outputs:\n");
    for pin in &nt.output_pins {
        let ty = pin
            .fixed_type()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "<polymorphic>".to_string());
        s.push_str(&format!("  {}: {}\n", pin.name, ty));
    }
    insta::assert_snapshot!(s);
}

// ============================================================================
// 2-field product (smallest non-trivial case)
// ============================================================================

#[test]
fn product_two_fields_smallest_case() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(pair_def())
        .unwrap();

    let xs_a = add_int_sequence(&mut designer, vec![1, 2]);
    let xs_b = add_int_sequence(&mut designer, vec![10, 20]);

    let prod = designer.add_node("product", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Pair".to_string(),
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(xs_a, 0, prod, 0); // pin 0 = a
    designer.connect_nodes(xs_b, 0, prod, 1); // pin 1 = b

    let items = drain_iter(&designer, evaluate_node_pin(&designer, "test", prod, 0));
    // 2 × 2 = 4 records. Rightmost (b) varies fastest, so the order is:
    // (a=1,b=10), (a=1,b=20), (a=2,b=10), (a=2,b=20).
    assert_eq!(items.len(), 4);
    let pairs: Vec<(i32, i32)> = items
        .iter()
        .map(|r| (record_field_int(r, "a"), record_field_int(r, "b")))
        .collect();
    assert_eq!(pairs, vec![(1, 10), (1, 20), (2, 10), (2, 20)]);
}

// ============================================================================
// 3-field product — locks in iteration order (rightmost varies fastest)
// ============================================================================

#[test]
fn product_three_fields_rightmost_varies_fastest() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(date_def())
        .unwrap();

    let years = add_int_sequence(&mut designer, vec![2000, 2001]);
    let months = add_int_sequence(&mut designer, vec![1, 2]);
    let days = add_int_sequence(&mut designer, vec![10, 20]);

    let prod = designer.add_node("product", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Date".to_string(),
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(years, 0, prod, 0); // year (leftmost — varies slowest)
    designer.connect_nodes(months, 0, prod, 1);
    designer.connect_nodes(days, 0, prod, 2); // day (rightmost — varies fastest)

    let items = drain_iter(&designer, evaluate_node_pin(&designer, "test", prod, 0));
    // 2 × 2 × 2 = 8 records. Day (rightmost) varies fastest.
    assert_eq!(items.len(), 8);
    let triples: Vec<(i32, i32, i32)> = items
        .iter()
        .map(|r| {
            (
                record_field_int(r, "year"),
                record_field_int(r, "month"),
                record_field_int(r, "day"),
            )
        })
        .collect();
    assert_eq!(
        triples,
        vec![
            (2000, 1, 10),
            (2000, 1, 20),
            (2000, 2, 10),
            (2000, 2, 20),
            (2001, 1, 10),
            (2001, 1, 20),
            (2001, 2, 10),
            (2001, 2, 20),
        ]
    );
}

// ============================================================================
// Empty input on any axis → empty output
// ============================================================================

#[test]
fn product_empty_input_yields_empty_output() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(pair_def())
        .unwrap();

    let xs_a = add_int_sequence(&mut designer, vec![1, 2, 3]);
    let xs_b = add_int_sequence(&mut designer, vec![]); // empty axis

    let prod = designer.add_node("product", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Pair".to_string(),
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(xs_a, 0, prod, 0);
    designer.connect_nodes(xs_b, 0, prod, 1);

    let items = drain_iter(&designer, evaluate_node_pin(&designer, "test", prod, 0));
    assert!(items.is_empty());
}

// ============================================================================
// Cardinality math: |out| == ∏ |xs_i|
// ============================================================================

#[test]
fn product_large_cardinality() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(date_def())
        .unwrap();

    // 5 × 4 × 3 = 60.
    let years = add_int_sequence(&mut designer, (0..5).collect());
    let months = add_int_sequence(&mut designer, (0..4).collect());
    let days = add_int_sequence(&mut designer, (0..3).collect());

    let prod = designer.add_node("product", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Date".to_string(),
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(years, 0, prod, 0);
    designer.connect_nodes(months, 0, prod, 1);
    designer.connect_nodes(days, 0, prod, 2);

    let items = drain_iter(&designer, evaluate_node_pin(&designer, "test", prod, 0));
    assert_eq!(items.len(), 5 * 4 * 3);
}

// ============================================================================
// Target def with an `Array[T]` field (records of arrays — NOT recursive)
// ============================================================================

#[test]
fn product_target_with_array_field() {
    // `Bag = { tag: Int, items: Array[Int] }` — not a recursive def, just a
    // record whose field value happens to be an array. Each axis input is
    // therefore Array[Array[Int]], i.e. a list of arrays.
    let mut designer = setup_designer_with_network("test");
    let bag = RecordTypeDef::from_named_fields(
        "Bag".to_string(),
        vec![
            ("tag".to_string(), DataType::Int),
            (
                "items".to_string(),
                DataType::Array(Box::new(DataType::Int)),
            ),
        ],
    );
    designer
        .node_type_registry
        .add_record_type_def(bag)
        .unwrap();

    let prod = designer.add_node("product", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Bag".to_string(),
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&prod).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    // Phase 6: pin `tag` is Iter[Int]; pin `items` is Iter[Array[Int]].
    let names: Vec<&str> = nt.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["tag", "items"]);
    assert_eq!(
        nt.parameters[0].data_type,
        DataType::Iterator(Box::new(DataType::Int))
    );
    assert_eq!(
        nt.parameters[1].data_type,
        DataType::Iterator(Box::new(DataType::Array(Box::new(DataType::Int))))
    );
}

// ============================================================================
// Dangling target: empty / deleted def
// ============================================================================

#[test]
fn product_with_empty_target_has_no_input_pins() {
    let mut designer = setup_designer_with_network("test");
    let prod = designer.add_node("product", DVec2::new(0.0, 0.0));
    // Default target = "" — registry-aware cache populator gives zero pins.
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "".to_string(),
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&prod).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();
    assert_eq!(nt.parameters.len(), 0);
    // Output type is dangling Iter[Record(Named(""))] — fails subtyping
    // against any real consumer.
    assert_eq!(
        nt.output_pins[0].fixed_type(),
        Some(&DataType::Iterator(Box::new(DataType::Record(
            RecordType::Named(String::new())
        ))))
    );
}

#[test]
fn product_after_deleting_target_def_has_no_input_pins() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(pair_def())
        .unwrap();

    let prod = designer.add_node("product", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Pair".to_string(),
        }),
    );

    // Sanity: starts with 2 input pins.
    {
        let registry = &designer.node_type_registry;
        let network = registry.node_networks.get("test").unwrap();
        let node = network.nodes.get(&prod).unwrap();
        let nt = registry.get_node_type_for_node(node).unwrap();
        assert_eq!(nt.parameters.len(), 2);
    }

    // Delete the def, then re-run repair (mirrors the API path that runs
    // after a registry edit).
    designer.node_type_registry.delete_record_type_def("Pair");
    {
        let registry = &mut designer.node_type_registry;
        let mut network = registry.node_networks.remove("test").unwrap();
        registry.repair_node_network(&mut network);
        registry.node_networks.insert("test".to_string(), network);
    }

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&prod).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();
    // Dangling target — pins gone, output type still references the missing
    // name (will fail subtyping against any consumer). Phase 6: now Iter[..].
    assert_eq!(nt.parameters.len(), 0);
    assert_eq!(
        nt.output_pins[0].fixed_type(),
        Some(&DataType::Iterator(Box::new(DataType::Record(
            RecordType::Named("Pair".to_string())
        ))))
    );
}

// ============================================================================
// Missing-input propagation
// ============================================================================

#[test]
fn product_missing_input_is_none() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(pair_def())
        .unwrap();

    let xs_a = add_int_sequence(&mut designer, vec![1, 2]);

    let prod = designer.add_node("product", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Pair".to_string(),
        }),
    );

    designer.validate_active_network();
    // Only connect `a`; `b` is left unconnected.
    designer.connect_nodes(xs_a, 0, prod, 0);

    let result = evaluate_node_pin(&designer, "test", prod, 0);
    assert!(
        matches!(result, NetworkResult::None),
        "got {:?}",
        result.to_display_string()
    );
}

// ============================================================================
// Single-value broadcast: a non-array source on an array-typed pin is
// auto-wrapped to a one-element array (matches existing array-pin semantics).
// ============================================================================

#[test]
fn product_broadcasts_single_value_input() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(pair_def())
        .unwrap();

    // `a` is a single Int; `b` is a 2-element array. The product's `a` pin
    // is Array[Int] — convert_to broadcasts the scalar into [a].
    let int_a = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", int_a, Box::new(IntData { value: 7 }));
    let xs_b = add_int_sequence(&mut designer, vec![100, 200]);

    let prod = designer.add_node("product", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        prod,
        Box::new(ProductData {
            target: "Pair".to_string(),
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(int_a, 0, prod, 0);
    designer.connect_nodes(xs_b, 0, prod, 1);

    let items = drain_iter(&designer, evaluate_node_pin(&designer, "test", prod, 0));
    assert_eq!(items.len(), 2);
    let pairs: Vec<(i32, i32)> = items
        .iter()
        .map(|r| (record_field_int(r, "a"), record_field_int(r, "b")))
        .collect();
    assert_eq!(pairs, vec![(7, 100), (7, 200)]);
}
