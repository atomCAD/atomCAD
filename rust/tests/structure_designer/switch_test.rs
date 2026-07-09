//! Phase 1 tests for the `switch` node (`doc/design_switch_node.md`).
//!
//! Covers: Int/String selector matching, default fallback, optional-pin
//! inertness, laziness (untaken branch never evaluated), structural value
//! pass-through, selector error propagation, derived pin-name generation, and
//! the hand-authored-duplicate first-match-wins behavior.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::array_at::ArrayAtData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::sequence::SequenceData;
use rust_lib_flutter_cad::structure_designer::nodes::string::StringData;
use rust_lib_flutter_cad::structure_designer::nodes::switch::{
    SwitchCase, SwitchCaseValue, SwitchData,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers (mirror if_test.rs)
// ============================================================================

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

fn evaluate_node(designer: &StructureDesigner, network_name: &str, node_id: u64) -> NetworkResult {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let evaluator = NetworkEvaluator::new();
    let mut context = NetworkEvaluationContext::new();
    let network_stack = vec![NetworkStackElement {
        node_network: network,
        node_id: 0,
    }];
    evaluator.evaluate(&network_stack, node_id, 0, registry, false, &mut context)
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

fn add_int(designer: &mut StructureDesigner, network: &str, value: i32, y: f64) -> u64 {
    let id = designer.add_node("int", DVec2::new(0.0, y));
    set_node_data(designer, network, id, Box::new(IntData { value }));
    id
}

fn add_string(designer: &mut StructureDesigner, network: &str, value: &str, y: f64) -> u64 {
    let id = designer.add_node("string", DVec2::new(0.0, y));
    set_node_data(
        designer,
        network,
        id,
        Box::new(StringData {
            value: value.to_string(),
        }),
    );
    id
}

/// Build a `SwitchData` with sequential case ids (1..=N) — the shape supported
/// edit paths produce.
fn switch_data(
    selector_type: DataType,
    value_type: DataType,
    values: Vec<SwitchCaseValue>,
) -> SwitchData {
    let cases: Vec<SwitchCase> = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| SwitchCase {
            id: Some((i + 1) as u64),
            value,
        })
        .collect();
    let next_case_id = cases.len() as u64 + 1;
    SwitchData {
        selector_type,
        value_type,
        cases,
        next_case_id,
    }
}

fn add_switch(designer: &mut StructureDesigner, network: &str, data: SwitchData, x: f64) -> u64 {
    let id = designer.add_node("switch", DVec2::new(x, 0.0));
    set_node_data(designer, network, id, Box::new(data));
    id
}

/// An `array_at` reading index 0 of an empty typed array → an out-of-bounds
/// error. Used to prove a branch is *not* evaluated: if it were, the switch
/// output would be that error. Mirrors `if_test.rs`.
fn add_erroring_int_source(designer: &mut StructureDesigner, network: &str, y: f64) -> u64 {
    let seq_id = designer.add_node("sequence", DVec2::new(-200.0, y));
    set_node_data(
        designer,
        network,
        seq_id,
        Box::new(SequenceData {
            element_type: DataType::Int,
            input_count: 1,
        }),
    );
    let at_id = designer.add_node("array_at", DVec2::new(-100.0, y));
    set_node_data(
        designer,
        network,
        at_id,
        Box::new(ArrayAtData {
            element_type: DataType::Int,
            index: 0,
        }),
    );
    designer.validate_active_network();
    designer.connect_nodes(seq_id, 0, at_id, 0);
    at_id
}

// ============================================================================
// Registration & defaults
// ============================================================================

#[test]
fn test_switch_default() {
    let data = SwitchData::default();
    assert_eq!(data.selector_type, DataType::Int);
    assert_eq!(data.value_type, DataType::Float);
    assert_eq!(data.cases.len(), 2);
    assert_eq!(data.cases[0].value, SwitchCaseValue::Int(0));
    assert_eq!(data.cases[1].value, SwitchCaseValue::Int(1));
    assert_eq!(data.next_case_id, 3);
}

#[test]
fn test_switch_registered_in_registry() {
    let registry = NodeTypeRegistry::new();
    let nt = registry.get_node_type("switch").expect("switch registered");
    assert_eq!(nt.name, "switch");
    assert!(nt.public);
    // value + 2 cases + default
    assert_eq!(nt.parameters.len(), 4);
    assert_eq!(nt.parameters[0].name, "selector");
    assert_eq!(nt.parameters[0].data_type, DataType::Int);
    assert_eq!(nt.parameters[1].name, "case_0");
    assert_eq!(nt.parameters[2].name, "case_1");
    assert_eq!(nt.parameters[3].name, "default");
    assert_eq!(nt.output_pins.len(), 1);
}

// ============================================================================
// calculate_custom_node_type
// ============================================================================

#[test]
fn test_switch_custom_type_shape() {
    let registry = NodeTypeRegistry::new();
    let base = registry.get_node_type("switch").unwrap();
    let data = switch_data(
        DataType::Int,
        DataType::Crystal,
        vec![
            SwitchCaseValue::Int(3),
            SwitchCaseValue::Int(-3),
            SwitchCaseValue::Int(7),
        ],
    );
    let custom = data.calculate_custom_node_type(base).unwrap();

    // selector (Int) + 3 cases (Crystal) + default (Crystal).
    assert_eq!(custom.parameters.len(), 5);
    assert_eq!(custom.parameters[0].name, "selector");
    assert_eq!(custom.parameters[0].data_type, DataType::Int);
    assert_eq!(custom.parameters[1].name, "case_3");
    assert_eq!(custom.parameters[2].name, "case_neg3");
    assert_eq!(custom.parameters[3].name, "case_7");
    assert_eq!(custom.parameters[4].name, "default");
    for i in 1..=4 {
        assert_eq!(custom.parameters[i].data_type, DataType::Crystal);
    }
    // Case pins carry their stable ids; selector / default do not.
    assert_eq!(custom.parameters[1].id, Some(1));
    assert_eq!(custom.parameters[2].id, Some(2));
    assert_eq!(custom.parameters[3].id, Some(3));
    assert_eq!(custom.parameters[0].id, None);
    assert_eq!(custom.parameters[4].id, None);
    assert_eq!(*custom.output_type(), DataType::Crystal);
}

// ============================================================================
// Evaluation: Int selector matching + default + optional pins
// ============================================================================

#[test]
fn test_switch_int_matches_case() {
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 1, 0.0);
    let a = add_int(&mut designer, "test", 100, 100.0);
    let b = add_int(&mut designer, "test", 200, 200.0);
    let d = add_int(&mut designer, "test", 999, 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0); // selector
    designer.connect_nodes(a, 0, sw, 1); // case_0
    designer.connect_nodes(b, 0, sw, 2); // case_1
    designer.connect_nodes(d, 0, sw, 3); // default

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 200),
        other => panic!("Expected Int(200), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_switch_no_match_selects_default() {
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 5, 0.0);
    let a = add_int(&mut designer, "test", 100, 100.0);
    let b = add_int(&mut designer, "test", 200, 200.0);
    let d = add_int(&mut designer, "test", 999, 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(a, 0, sw, 1);
    designer.connect_nodes(b, 0, sw, 2);
    designer.connect_nodes(d, 0, sw, 3);

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 999),
        other => panic!("Expected Int(999), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_switch_no_match_unwired_default_is_none() {
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 5, 0.0);
    let a = add_int(&mut designer, "test", 100, 100.0);
    let b = add_int(&mut designer, "test", 200, 200.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(a, 0, sw, 1);
    designer.connect_nodes(b, 0, sw, 2);
    // default (pin 3) unwired

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_switch_unwired_selector_is_inert() {
    let mut designer = setup_designer_with_network("test");
    let a = add_int(&mut designer, "test", 100, 100.0);
    let b = add_int(&mut designer, "test", 200, 200.0);
    let d = add_int(&mut designer, "test", 999, 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    // selector (pin 0) unwired, everything else wired
    designer.connect_nodes(a, 0, sw, 1);
    designer.connect_nodes(b, 0, sw, 2);
    designer.connect_nodes(d, 0, sw, 3);

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::None => {}
        other => panic!("Expected None, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Evaluation: String selector (exact, case-sensitive)
// ============================================================================

#[test]
fn test_switch_string_exact_case_sensitive() {
    let mut designer = setup_designer_with_network("test");
    let sel = add_string(&mut designer, "test", "beta", 0.0);
    let a = add_int(&mut designer, "test", 10, 100.0);
    let b = add_int(&mut designer, "test", 20, 200.0);
    let d = add_int(&mut designer, "test", 99, 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::String,
            DataType::Int,
            vec![
                SwitchCaseValue::String("alpha".to_string()),
                SwitchCaseValue::String("beta".to_string()),
            ],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(a, 0, sw, 1);
    designer.connect_nodes(b, 0, sw, 2);
    designer.connect_nodes(d, 0, sw, 3);

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 20),
        other => panic!("Expected Int(20), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_switch_string_case_mismatch_falls_to_default() {
    let mut designer = setup_designer_with_network("test");
    let sel = add_string(&mut designer, "test", "Beta", 0.0); // wrong case
    let a = add_int(&mut designer, "test", 10, 100.0);
    let b = add_int(&mut designer, "test", 20, 200.0);
    let d = add_int(&mut designer, "test", 99, 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::String,
            DataType::Int,
            vec![
                SwitchCaseValue::String("alpha".to_string()),
                SwitchCaseValue::String("beta".to_string()),
            ],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(a, 0, sw, 1);
    designer.connect_nodes(b, 0, sw, 2);
    designer.connect_nodes(d, 0, sw, 3);

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 99),
        other => panic!(
            "Expected Int(99) (case-sensitive miss), got {:?}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Evaluation: laziness (untaken case pin never evaluated)
// ============================================================================

#[test]
fn test_switch_untaken_case_not_evaluated() {
    // case_1's source errors; selecting case_0 must not touch it.
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 0, 0.0);
    let good = add_int(&mut designer, "test", 7, 100.0);
    let bad = add_erroring_int_source(&mut designer, "test", 200.0);
    let d = add_int(&mut designer, "test", 999, 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(good, 0, sw, 1); // case_0 (taken)
    designer.connect_nodes(bad, 0, sw, 2); // case_1 (untaken, erroring)
    designer.connect_nodes(d, 0, sw, 3);

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 7),
        other => panic!(
            "Expected Int(7) (untaken case not evaluated), got {:?}",
            other.to_display_string()
        ),
    }
}

#[test]
fn test_switch_untaken_default_not_evaluated() {
    // default source errors; a matching case must not touch it.
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 1, 0.0);
    let a = add_int(&mut designer, "test", 100, 100.0);
    let b = add_int(&mut designer, "test", 200, 150.0);
    let bad = add_erroring_int_source(&mut designer, "test", 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(a, 0, sw, 1);
    designer.connect_nodes(b, 0, sw, 2); // taken
    designer.connect_nodes(bad, 0, sw, 3); // default (untaken, erroring)

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 200),
        other => panic!(
            "Expected Int(200) (untaken default not evaluated), got {:?}",
            other.to_display_string()
        ),
    }
}

// ============================================================================
// Evaluation: structural value_type flows through intact
// ============================================================================

#[test]
fn test_switch_structural_value_passes_through() {
    // value_type = Blueprint: a matched structural value flows through the
    // switch unchanged (exactly the case `expr` cannot handle).
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 0, 0.0);
    let cuboid = designer.add_node("cuboid", DVec2::new(0.0, 100.0));
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Blueprint,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(cuboid, 0, sw, 1); // case_0 (taken)

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Blueprint(_) => {}
        other => panic!("Expected Blueprint, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Evaluation: selector error propagates
// ============================================================================

#[test]
fn test_switch_selector_error_propagates() {
    let mut designer = setup_designer_with_network("test");
    let bad_sel = add_erroring_int_source(&mut designer, "test", 0.0); // Int error source
    let a = add_int(&mut designer, "test", 100, 100.0);
    let b = add_int(&mut designer, "test", 200, 200.0);
    let d = add_int(&mut designer, "test", 999, 300.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(bad_sel, 0, sw, 0);
    designer.connect_nodes(a, 0, sw, 1);
    designer.connect_nodes(b, 0, sw, 2);
    designer.connect_nodes(d, 0, sw, 3);

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Error(_) => {}
        other => panic!("Expected Error, got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// Derived pin names
// ============================================================================

#[test]
fn test_derived_pin_names_int() {
    let data = switch_data(
        DataType::Int,
        DataType::Float,
        vec![
            SwitchCaseValue::Int(5),
            SwitchCaseValue::Int(-3),
            SwitchCaseValue::Int(0),
        ],
    );
    assert_eq!(
        data.derived_case_pin_names(),
        vec!["case_5", "case_neg3", "case_0"]
    );
}

#[test]
fn test_derived_pin_names_string_sanitize_truncate_dedup() {
    let data = switch_data(
        DataType::String,
        DataType::Float,
        vec![
            SwitchCaseValue::String("a b".to_string()),
            SwitchCaseValue::String("a_b".to_string()),
            SwitchCaseValue::String("slot-1".to_string()),
            SwitchCaseValue::String("this_is_a_very_long_case_name_exceeding_limit".to_string()),
            SwitchCaseValue::String("!@#".to_string()),
        ],
    );
    let names = data.derived_case_pin_names();
    // "a b" sanitizes to "a_b" → base "case_a_b"; "a_b" also → "case_a_b",
    // deduped to "case_a_b__2".
    assert_eq!(names[0], "case_a_b");
    assert_eq!(names[1], "case_a_b__2");
    // "slot-1" → "slot_1".
    assert_eq!(names[2], "case_slot_1");
    // Truncation: 24 chars of the sanitized value kept after the `case_` prefix.
    assert_eq!(names[3], "case_this_is_a_very_long_case");
    assert_eq!(names[3].len(), "case_".len() + 24);
    // "!@#" sanitizes to "___".
    assert_eq!(names[4], "case____");
}

// ============================================================================
// Hand-authored duplicate case values: first match wins, no panic
// ============================================================================

#[test]
fn test_switch_hand_built_duplicate_first_match_wins() {
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 5, 0.0);
    let first = add_int(&mut designer, "test", 111, 100.0);
    let second = add_int(&mut designer, "test", 222, 200.0);
    let d = add_int(&mut designer, "test", 999, 300.0);
    // Bypass the setters: two cases with the same value 5. Derived names dedup
    // to case_5 / case_5__2, so both pins exist and are wirable.
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(5), SwitchCaseValue::Int(5)],
        ),
        400.0,
    );
    designer.validate_active_network();

    designer.connect_nodes(sel, 0, sw, 0);
    designer.connect_nodes(first, 0, sw, 1); // case_5
    designer.connect_nodes(second, 0, sw, 2); // case_5__2
    designer.connect_nodes(d, 0, sw, 3);

    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 111, "first duplicate match wins"),
        other => panic!("Expected Int(111), got {:?}", other.to_display_string()),
    }
}

// ============================================================================
// merge_cases (used by set_text_properties; core value-keyed id merge)
// ============================================================================

#[test]
fn test_merge_cases_removes_middle_keeps_ids_by_value() {
    let mut data = switch_data(
        DataType::Int,
        DataType::Float,
        vec![
            SwitchCaseValue::Int(1),
            SwitchCaseValue::Int(2),
            SwitchCaseValue::Int(3),
        ],
    );
    // Drop the middle case.
    data.merge_cases(vec![SwitchCaseValue::Int(1), SwitchCaseValue::Int(3)])
        .unwrap();
    assert_eq!(data.cases.len(), 2);
    assert_eq!(data.cases[0].value, SwitchCaseValue::Int(1));
    assert_eq!(data.cases[0].id, Some(1)); // kept by value
    assert_eq!(data.cases[1].value, SwitchCaseValue::Int(3));
    assert_eq!(data.cases[1].id, Some(3)); // kept by value
}

#[test]
fn test_merge_cases_edit_in_place_keeps_id_positionally() {
    let mut data = switch_data(
        DataType::Int,
        DataType::Float,
        vec![
            SwitchCaseValue::Int(1),
            SwitchCaseValue::Int(2),
            SwitchCaseValue::Int(3),
        ],
    );
    // Edit the middle value 2 → 5: 1 and 3 match by value, 5 inherits 2's id.
    data.merge_cases(vec![
        SwitchCaseValue::Int(1),
        SwitchCaseValue::Int(5),
        SwitchCaseValue::Int(3),
    ])
    .unwrap();
    assert_eq!(data.cases[1].value, SwitchCaseValue::Int(5));
    assert_eq!(data.cases[1].id, Some(2)); // positional fallback keeps the wire
}

#[test]
fn test_merge_cases_no_positional_steal() {
    // [1,2] -> [3,1]: value-match must resolve case 1's id BEFORE any
    // positional fallback, else 3 steals case 1's id. A correct two-pass merge
    // keeps 1's id on 1 and mints a fresh id for 3.
    let mut data = switch_data(
        DataType::Int,
        DataType::Float,
        vec![SwitchCaseValue::Int(1), SwitchCaseValue::Int(2)],
    );
    let next_before = data.next_case_id;
    data.merge_cases(vec![SwitchCaseValue::Int(3), SwitchCaseValue::Int(1)])
        .unwrap();
    // case 1 kept its original id 1.
    let case1 = data
        .cases
        .iter()
        .find(|c| c.value == SwitchCaseValue::Int(1))
        .unwrap();
    assert_eq!(case1.id, Some(1));
    // case 3 got a freshly minted id (not 1, not 2), from next_case_id.
    let case3 = data
        .cases
        .iter()
        .find(|c| c.value == SwitchCaseValue::Int(3))
        .unwrap();
    assert_eq!(case3.id, Some(next_before));
    assert_eq!(data.next_case_id, next_before + 1);
}

#[test]
fn test_merge_cases_rejects_duplicates_and_empty() {
    let mut data = SwitchData::default();
    let before = data.cases.clone();
    assert!(
        data.merge_cases(vec![SwitchCaseValue::Int(5), SwitchCaseValue::Int(5)])
            .is_err()
    );
    // node unchanged on error
    assert_eq!(data.cases.len(), before.len());
    assert!(data.merge_cases(vec![]).is_err());
}

// ============================================================================
// convert_selector_type (Phase 2)
// ============================================================================

#[test]
fn test_convert_selector_int_to_string_stringifies_keeps_ids() {
    let mut data = switch_data(
        DataType::Int,
        DataType::Float,
        vec![SwitchCaseValue::Int(5), SwitchCaseValue::Int(-3)],
    );
    data.convert_selector_type(&DataType::String).unwrap();
    assert_eq!(data.selector_type, DataType::String);
    assert_eq!(
        data.cases[0].value,
        SwitchCaseValue::String("5".to_string())
    );
    assert_eq!(
        data.cases[1].value,
        SwitchCaseValue::String("-3".to_string())
    );
    // Ids untouched.
    assert_eq!(data.cases[0].id, Some(1));
    assert_eq!(data.cases[1].id, Some(2));
}

#[test]
fn test_convert_selector_string_to_int_parses_keeps_ids() {
    let mut data = switch_data(
        DataType::String,
        DataType::Float,
        vec![
            SwitchCaseValue::String("5".to_string()),
            SwitchCaseValue::String("-3".to_string()),
        ],
    );
    data.convert_selector_type(&DataType::Int).unwrap();
    assert_eq!(data.selector_type, DataType::Int);
    assert_eq!(data.cases[0].value, SwitchCaseValue::Int(5));
    assert_eq!(data.cases[1].value, SwitchCaseValue::Int(-3));
    assert_eq!(data.cases[0].id, Some(1));
    assert_eq!(data.cases[1].id, Some(2));
}

#[test]
fn test_convert_selector_string_to_int_unparseable_rejects_atomically() {
    let mut data = switch_data(
        DataType::String,
        DataType::Float,
        vec![
            SwitchCaseValue::String("5".to_string()),
            SwitchCaseValue::String("notanint".to_string()),
        ],
    );
    let before = data.cases.clone();
    assert!(data.convert_selector_type(&DataType::Int).is_err());
    // Node completely unchanged: still String, values intact.
    assert_eq!(data.selector_type, DataType::String);
    assert_eq!(data.cases.len(), before.len());
    assert_eq!(data.cases[0].value, before[0].value);
    assert_eq!(data.cases[1].value, before[1].value);
}

#[test]
fn test_convert_selector_string_to_int_collision_rejects_atomically() {
    // "5" and "05" both parse to 5 — the flip must be rejected, not smuggle in
    // a duplicate the other edit paths forbid.
    let mut data = switch_data(
        DataType::String,
        DataType::Float,
        vec![
            SwitchCaseValue::String("5".to_string()),
            SwitchCaseValue::String("05".to_string()),
        ],
    );
    assert!(data.convert_selector_type(&DataType::Int).is_err());
    assert_eq!(data.selector_type, DataType::String);
    assert_eq!(
        data.cases[0].value,
        SwitchCaseValue::String("5".to_string())
    );
    assert_eq!(
        data.cases[1].value,
        SwitchCaseValue::String("05".to_string())
    );
}

// ============================================================================
// set_switch_data (Phase 2): StructureDesigner-level op + wire fallout
// ============================================================================

/// Fetch a node's live arguments (indexed by pin) from the active network.
fn switch_arg_source(
    designer: &StructureDesigner,
    network: &str,
    node_id: u64,
    pin: usize,
) -> Option<u64> {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    net.nodes.get(&node_id).unwrap().arguments[pin].get_node_id()
}

fn switch_has_source(designer: &StructureDesigner, network: &str, node_id: u64, src: u64) -> bool {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    net.nodes
        .get(&node_id)
        .unwrap()
        .arguments
        .iter()
        .any(|a| a.has_source(src))
}

fn switch_case_pin_names(designer: &StructureDesigner, network: &str, node_id: u64) -> Vec<String> {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    net.nodes
        .get(&node_id)
        .unwrap()
        .custom_node_type
        .as_ref()
        .unwrap()
        .parameters
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

/// Wire up a 3-case Int switch: selector + case_10/20/30 sources + default,
/// value_type Int. Returns (sw, sel, s10, s20, s30, d).
fn build_three_case_int_switch(
    designer: &mut StructureDesigner,
    selector_value: i32,
) -> (u64, u64, u64, u64, u64, u64) {
    let sel = add_int(designer, "test", selector_value, 0.0);
    let s10 = add_int(designer, "test", 110, 100.0);
    let s20 = add_int(designer, "test", 120, 200.0);
    let s30 = add_int(designer, "test", 130, 300.0);
    let d = add_int(designer, "test", 999, 400.0);
    let sw = add_switch(
        designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![
                SwitchCaseValue::Int(10),
                SwitchCaseValue::Int(20),
                SwitchCaseValue::Int(30),
            ],
        ),
        500.0,
    );
    designer.validate_active_network();
    designer.connect_nodes(sel, 0, sw, 0); // selector
    designer.connect_nodes(s10, 0, sw, 1); // case_10
    designer.connect_nodes(s20, 0, sw, 2); // case_20
    designer.connect_nodes(s30, 0, sw, 3); // case_30
    designer.connect_nodes(d, 0, sw, 4); // default
    (sw, sel, s10, s20, s30, d)
}

#[test]
fn test_set_switch_data_remove_middle_keeps_wires() {
    let mut designer = setup_designer_with_network("test");
    let (sw, _sel, s10, s20, s30, _d) = build_three_case_int_switch(&mut designer, 30);

    // Sanity: selector 30 selects case_30's source (130).
    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 130),
        other => panic!("Expected Int(130), got {:?}", other.to_display_string()),
    }

    // Remove the middle case (20).
    designer
        .set_switch_data(
            &[],
            sw,
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(10), SwitchCaseValue::Int(30)],
        )
        .expect("middle-case removal must succeed");

    // Pins renumber: value, case_10, case_30, default.
    let names = switch_case_pin_names(&designer, "test", sw);
    assert_eq!(names, vec!["selector", "case_10", "case_30", "default"]);

    // case_10 keeps its wire at pin 1; case_30's wire follows its id onto the
    // renumbered pin 2; the removed case_20's wire is gone.
    assert_eq!(switch_arg_source(&designer, "test", sw, 1), Some(s10));
    assert_eq!(
        switch_arg_source(&designer, "test", sw, 2),
        Some(s30),
        "case_30's wire must follow its id onto the renumbered pin"
    );
    assert!(
        !switch_has_source(&designer, "test", sw, s20),
        "removed case_20's wire must be dropped"
    );

    // Evaluation unchanged for the surviving selection.
    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 130),
        other => panic!("Expected Int(130), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_set_switch_data_edit_in_place_keeps_wire() {
    let mut designer = setup_designer_with_network("test");
    let (sw, _sel, _s10, s20, _s30, _d) = build_three_case_int_switch(&mut designer, 20);

    // Edit case 20 → 25 in place: 10 and 30 match by value, 25 inherits 20's id
    // positionally, so s20's wire survives on the renamed pin.
    designer
        .set_switch_data(
            &[],
            sw,
            DataType::Int,
            DataType::Int,
            vec![
                SwitchCaseValue::Int(10),
                SwitchCaseValue::Int(25),
                SwitchCaseValue::Int(30),
            ],
        )
        .expect("in-place edit must succeed");

    let names = switch_case_pin_names(&designer, "test", sw);
    assert_eq!(
        names,
        vec!["selector", "case_10", "case_25", "case_30", "default"]
    );
    assert_eq!(
        switch_arg_source(&designer, "test", sw, 2),
        Some(s20),
        "the wire must follow the positional-fallback id onto the renamed pin"
    );

    // Selector 20 no longer matches (renamed to 25) → falls to default.
    // Re-point the selector to 25 and confirm s20's value flows through.
    // (selector node still emits 20; instead evaluate by matching case 25.)
    let sel25 = add_int(&mut designer, "test", 25, 0.0);
    designer.connect_nodes(sel25, 0, sw, 0);
    match evaluate_node(&designer, "test", sw) {
        NetworkResult::Int(v) => assert_eq!(v, 120, "s20's wire survived on case_25"),
        other => panic!("Expected Int(120), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_set_switch_data_no_positional_steal_wires() {
    // [1,2] -> [3,1]: case 1's wire must stay on case 1 (its id), and a fresh id
    // is minted for case 3 — a single-pass merge would steal case 1's id.
    let mut designer = setup_designer_with_network("test");
    let s1 = add_int(&mut designer, "test", 111, 100.0);
    let s2 = add_int(&mut designer, "test", 222, 200.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(1), SwitchCaseValue::Int(2)],
        ),
        500.0,
    );
    designer.validate_active_network();
    designer.connect_nodes(s1, 0, sw, 1); // case_1
    designer.connect_nodes(s2, 0, sw, 2); // case_2

    designer
        .set_switch_data(
            &[],
            sw,
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(3), SwitchCaseValue::Int(1)],
        )
        .expect("reorder-insert must succeed");

    // New pin order: selector, case_3, case_1, default.
    let names = switch_case_pin_names(&designer, "test", sw);
    assert_eq!(names, vec!["selector", "case_3", "case_1", "default"]);
    // case_1 (pin 2) keeps s1; case_3 (pin 1) is fresh/unwired; s2 dropped.
    assert_eq!(switch_arg_source(&designer, "test", sw, 2), Some(s1));
    assert_eq!(switch_arg_source(&designer, "test", sw, 1), None);
    assert!(!switch_has_source(&designer, "test", sw, s2));
}

#[test]
fn test_set_switch_data_selector_flip_keeps_wires() {
    // Int→String flip: convert_selector_type runs first, so the same-domain
    // value match keeps every wire.
    let mut designer = setup_designer_with_network("test");
    let (sw, _sel, s10, s20, s30, _d) = build_three_case_int_switch(&mut designer, 10);

    designer
        .set_switch_data(
            &[],
            sw,
            DataType::String,
            DataType::Int,
            vec![
                SwitchCaseValue::String("10".to_string()),
                SwitchCaseValue::String("20".to_string()),
                SwitchCaseValue::String("30".to_string()),
            ],
        )
        .expect("Int→String flip must succeed");

    let names = switch_case_pin_names(&designer, "test", sw);
    assert_eq!(
        names,
        vec!["selector", "case_10", "case_20", "case_30", "default"]
    );
    // Every case wire survived (value match after conversion).
    assert_eq!(switch_arg_source(&designer, "test", sw, 1), Some(s10));
    assert_eq!(switch_arg_source(&designer, "test", sw, 2), Some(s20));
    assert_eq!(switch_arg_source(&designer, "test", sw, 3), Some(s30));
}

#[test]
fn test_set_switch_data_rejects_leave_unchanged() {
    let mut designer = setup_designer_with_network("test");
    let (sw, _sel, s10, s20, s30, _d) = build_three_case_int_switch(&mut designer, 10);

    let before_names = switch_case_pin_names(&designer, "test", sw);

    // Each of these must be rejected AND leave the node completely untouched.
    let rejected_edits: Vec<(DataType, DataType, Vec<SwitchCaseValue>)> = vec![
        // Duplicate values.
        (
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(10), SwitchCaseValue::Int(10)],
        ),
        // Empty case list.
        (DataType::Int, DataType::Int, vec![]),
        // Domain mismatch: a String value under an Int selector.
        (
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::String("10".to_string())],
        ),
        // Invalid selector type.
        (
            DataType::Float,
            DataType::Int,
            vec![SwitchCaseValue::Int(10)],
        ),
    ];

    for (sel_t, val_t, values) in rejected_edits {
        assert!(
            designer
                .set_switch_data(&[], sw, sel_t, val_t, values)
                .is_err(),
            "edit should be rejected"
        );
        // Node unchanged: pins, wires, and value type all intact.
        assert_eq!(switch_case_pin_names(&designer, "test", sw), before_names);
        assert!(switch_has_source(&designer, "test", sw, s10));
        assert!(switch_has_source(&designer, "test", sw, s20));
        assert!(switch_has_source(&designer, "test", sw, s30));
    }
}

#[test]
fn test_set_switch_data_string_to_int_collision_flip_rejects() {
    // A switch whose stored String cases parse to a collision ("5"/"05").
    // Flipping the selector to Int must reject atomically (convert runs before
    // merge), leaving the node a String switch with both wires intact.
    let mut designer = setup_designer_with_network("test");
    let s5 = add_int(&mut designer, "test", 55, 100.0);
    let s05 = add_int(&mut designer, "test", 66, 200.0);
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::String,
            DataType::Int,
            vec![
                SwitchCaseValue::String("5".to_string()),
                SwitchCaseValue::String("05".to_string()),
            ],
        ),
        500.0,
    );
    designer.validate_active_network();
    designer.connect_nodes(s5, 0, sw, 1); // case_5
    designer.connect_nodes(s05, 0, sw, 2); // case_05

    // The Int case_values passed are irrelevant — the stored-case conversion
    // fails first.
    assert!(
        designer
            .set_switch_data(
                &[],
                sw,
                DataType::Int,
                DataType::Int,
                vec![SwitchCaseValue::Int(5), SwitchCaseValue::Int(6)],
            )
            .is_err()
    );

    // Still a String switch, both wires intact.
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let data = net
        .nodes
        .get(&sw)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<SwitchData>()
        .unwrap();
    assert_eq!(data.selector_type, DataType::String);
    assert!(switch_has_source(&designer, "test", sw, s5));
    assert!(switch_has_source(&designer, "test", sw, s05));
}

#[test]
fn test_set_switch_data_value_type_retype_changes_pins() {
    // Retype value_type Int → Crystal in one edit. NOTE: unlike the design
    // doc's aspirational test 9, the repair pass does NOT drop the now-
    // type-incompatible external wires — `set_custom_node_type`'s by-id argument
    // rebuild preserves them, and `validate_wires` flags the mismatch as a
    // *blocking* error rather than disconnecting. So the wires remain (feeding a
    // now-invalid network) and the case/output pins retype to Crystal.
    let mut designer = setup_designer_with_network("test");
    let (sw, _sel, s10, _s20, _s30, _d) = build_three_case_int_switch(&mut designer, 10);

    designer
        .set_switch_data(
            &[],
            sw,
            DataType::Int,
            DataType::Crystal,
            vec![
                SwitchCaseValue::Int(10),
                SwitchCaseValue::Int(20),
                SwitchCaseValue::Int(30),
            ],
        )
        .expect("value_type retype must succeed");

    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let ct = net
        .nodes
        .get(&sw)
        .unwrap()
        .custom_node_type
        .as_ref()
        .unwrap();
    assert_eq!(ct.parameters[1].data_type, DataType::Crystal);
    assert_eq!(*ct.output_type(), DataType::Crystal);
    // The Int wire is preserved (not type-pruned); the network is now invalid.
    assert!(switch_has_source(&designer, "test", sw, s10));
    assert!(
        !net.valid,
        "an Int source feeding a Crystal case pin makes the network invalid"
    );
}

#[test]
fn test_set_switch_data_noop_pushes_no_command() {
    let mut designer = setup_designer_with_network("test");
    let (sw, _sel, _s10, _s20, _s30, _d) = build_three_case_int_switch(&mut designer, 10);
    // A prior meaningful edit to establish a redo target.
    designer
        .set_switch_data(
            &[],
            sw,
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(10), SwitchCaseValue::Int(30)],
        )
        .unwrap();
    designer.undo();
    // Re-issue the identical current state (cases [10,20,30]) — a no-op edit
    // must NOT push a command, so the redo tail survives.
    designer
        .set_switch_data(
            &[],
            sw,
            DataType::Int,
            DataType::Int,
            vec![
                SwitchCaseValue::Int(10),
                SwitchCaseValue::Int(20),
                SwitchCaseValue::Int(30),
            ],
        )
        .unwrap();
    assert!(
        designer.redo(),
        "a no-op set_switch_data must not have pushed a command"
    );
}

// ============================================================================
// Phase 3 — `.cnnd` serialization round-trips + loader healing
//
// Round-trips a network through the serializable (`.cnnd`) form the file path
// uses (`node_network_to_serializable` -> JSON -> `serializable_to_node_network`,
// which invokes `switch_node_data_loader` + healing) and asserts the normalized
// JSON is byte-identical. Also drives the loader healing directly (missing ids,
// zero counter, selector/value-variant mismatch, post-conversion duplicates,
// all-dropped reset). Text-format round-trips live in
// `text_format_test.rs::switch_text_format_tests`.
// ============================================================================

use rust_lib_flutter_cad::structure_designer::nodes::map::MapData;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeNetwork, node_network_to_serializable, serializable_to_node_network,
};
use serde_json::Value;

/// Normalized whole-network JSON (sorts the HashMap-derived `nodes` /
/// `displayed_*` arrays so the compare is order-independent — same shape as
/// `undo_test.rs::normalize_json` / `zip_with_test.rs`).
fn network_json(designer: &mut StructureDesigner, name: &str) -> Value {
    let snapshot = designer
        .snapshot_network(name)
        .expect("network must snapshot");
    let mut value = serde_json::to_value(&snapshot).unwrap();
    normalize_network_json(&mut value);
    value
}

fn normalize_network_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if key == "displayed_node_ids" || key == "displayed_output_pins" {
                    if let Value::Array(arr) = val {
                        arr.sort_by(|a, b| {
                            let id_a = a
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let id_b = b
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            id_a.cmp(&id_b)
                        });
                    }
                } else if key == "nodes"
                    && let Value::Array(arr) = val
                {
                    arr.sort_by(|a, b| {
                        let id_a = a
                            .as_object()
                            .and_then(|o| o.get("id"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let id_b = b
                            .as_object()
                            .and_then(|o| o.get("id"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        id_a.cmp(&id_b)
                    });
                }
                normalize_network_json(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                normalize_network_json(val);
            }
        }
        _ => {}
    }
}

/// saver -> JSON -> loader (`serializable_to_node_network`, which runs
/// `switch_node_data_loader` + healing) -> saver again; the normalized JSON must
/// be byte-identical.
fn assert_cnnd_roundtrip(designer: &mut StructureDesigner, name: &str) {
    let before = network_json(designer, name);

    let snap = designer.snapshot_network(name).expect("snapshot network");
    let json = serde_json::to_string(&snap).unwrap();
    let reloaded: SerializableNodeNetwork = serde_json::from_str(&json).unwrap();

    let built_in = &designer.node_type_registry.built_in_node_types;
    let mut net2 =
        serializable_to_node_network(&reloaded, built_in, None).expect("load network back");
    let snap2 = node_network_to_serializable(&mut net2, built_in, None).unwrap();
    let mut after = serde_json::to_value(&snap2).unwrap();
    normalize_network_json(&mut after);

    assert_eq!(
        before, after,
        "cnnd roundtrip changed the network JSON for '{}'",
        name
    );
}

/// Test 4: a top-level `switch` with wired cases and a **structural**
/// `value_type` (Blueprint) round-trips exactly through the `.cnnd` form.
#[test]
fn switch_phase3_cnnd_roundtrip_wired_cases_structural_value_type() {
    let mut designer = setup_designer_with_network("test");
    let sel = add_int(&mut designer, "test", 0, 0.0);
    let cuboid0 = designer.add_node("cuboid", DVec2::new(0.0, 100.0));
    let cuboid_default = designer.add_node("cuboid", DVec2::new(0.0, 200.0));
    let sw = add_switch(
        &mut designer,
        "test",
        switch_data(
            DataType::Int,
            DataType::Blueprint,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        400.0,
    );
    designer.validate_active_network();
    designer.connect_nodes(sel, 0, sw, 0); // selector
    designer.connect_nodes(cuboid0, 0, sw, 1); // case_0 (Blueprint)
    designer.connect_nodes(cuboid_default, 0, sw, 3); // default (Blueprint)

    assert_cnnd_roundtrip(&mut designer, "test");
}

/// Test 4b: a **body-internal** `switch` (inside a `map` zone body) survives the
/// round-trip — the recursive body serialization + the healing loader run on a
/// body node.
#[test]
fn switch_phase3_cnnd_roundtrip_body_internal_switch() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );

    // Insert a switch into the map's inline zone body.
    let sw_id = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .expect("map body must init");
        body.add_node(
            "switch",
            DVec2::new(0.0, 0.0),
            4,
            Box::new(switch_data(
                DataType::Int,
                DataType::Int,
                vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
            )),
        )
    };
    // Populate the body switch's custom-type cache (mirrors set_node_data, but
    // resolved through the body network).
    {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&sw_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }

    assert_cnnd_roundtrip(&mut designer, "main");
}

/// Null every case `id` and zero `next_case_id` on the switch node's serialized
/// `data` blob — the shape a hand-authored `.cnnd` file has.
fn strip_switch_case_ids(network_json: &mut Value, switch_id: u64) {
    let nodes = network_json
        .get_mut("nodes")
        .and_then(|v| v.as_array_mut())
        .expect("nodes array");
    for node in nodes {
        if node.get("id").and_then(|v| v.as_u64()) == Some(switch_id) {
            let data = node.get_mut("data").expect("node data");
            data["next_case_id"] = Value::from(0u64);
            for case in data
                .get_mut("cases")
                .and_then(|v| v.as_array_mut())
                .expect("cases array")
            {
                case["id"] = Value::Null;
            }
        }
    }
}

/// Test 5a: a hand-authored file with null case ids, a zero counter, and Int
/// case values stored under a **String** selector loads sanely — the loader
/// stringifies the values into the selector domain, mints fresh distinct ids,
/// advances the counter past them, and the external (positional) case wires
/// survive.
#[test]
fn switch_phase3_load_heals_missing_ids_and_selector_mismatch() {
    let mut designer = setup_designer_with_network("test");
    let s1 = add_int(&mut designer, "test", 100, 100.0);
    let s2 = add_int(&mut designer, "test", 200, 200.0);
    let s3 = add_int(&mut designer, "test", 300, 300.0);
    // A String selector holding Int case values (constructed directly, bypassing
    // the setters, to mimic hand-authored corruption). value_type Int, so the
    // case pins accept the int sources.
    let sw = add_switch(
        &mut designer,
        "test",
        SwitchData {
            selector_type: DataType::String,
            value_type: DataType::Int,
            cases: vec![
                SwitchCase {
                    id: Some(1),
                    value: SwitchCaseValue::Int(1),
                },
                SwitchCase {
                    id: Some(2),
                    value: SwitchCaseValue::Int(2),
                },
                SwitchCase {
                    id: Some(3),
                    value: SwitchCaseValue::Int(3),
                },
            ],
            next_case_id: 4,
        },
        500.0,
    );
    designer.validate_active_network();
    designer.connect_nodes(s1, 0, sw, 1); // case pin 1
    designer.connect_nodes(s2, 0, sw, 2); // case pin 2
    designer.connect_nodes(s3, 0, sw, 3); // case pin 3

    let snap = designer.snapshot_network("test").unwrap();
    let mut json = serde_json::to_value(&snap).unwrap();
    strip_switch_case_ids(&mut json, sw);

    let reloaded: SerializableNodeNetwork = serde_json::from_value(json).unwrap();
    let built_in = &designer.node_type_registry.built_in_node_types;
    let net2 = serializable_to_node_network(&reloaded, built_in, None).expect("load heals");

    let sw_node = net2.nodes.get(&sw).unwrap();
    let data = sw_node
        .data
        .as_any_ref()
        .downcast_ref::<SwitchData>()
        .unwrap();
    // Selector is a valid domain; values were stringified into it.
    assert_eq!(data.selector_type, DataType::String);
    assert_eq!(
        data.cases
            .iter()
            .map(|c| c.value.clone())
            .collect::<Vec<_>>(),
        vec![
            SwitchCaseValue::String("1".to_string()),
            SwitchCaseValue::String("2".to_string()),
            SwitchCaseValue::String("3".to_string()),
        ]
    );
    // Fresh distinct ids, counter past every id.
    let ids: Vec<u64> = data
        .cases
        .iter()
        .map(|c| c.id.expect("healed id"))
        .collect();
    let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "healed case ids must be distinct");
    assert!(data.next_case_id > *ids.iter().max().unwrap());
    // Positional case wires survive the id-less load.
    assert!(
        !sw_node.arguments[1].is_empty()
            && !sw_node.arguments[2].is_empty()
            && !sw_node.arguments[3].is_empty(),
        "external case wires must survive an id-less load"
    );
}

/// Test 5b: `heal` drops a post-conversion duplicate. Two distinct strings
/// ("5"/"05") that parse to the same Int under an Int selector — the second is
/// dropped (never a duplicate, never smuggled past the setters' uniqueness rule).
#[test]
fn switch_phase3_heal_drops_post_conversion_duplicate() {
    let mut data = SwitchData {
        selector_type: DataType::Int,
        value_type: DataType::Int,
        cases: vec![
            SwitchCase {
                id: Some(1),
                value: SwitchCaseValue::String("5".to_string()),
            },
            SwitchCase {
                id: Some(2),
                value: SwitchCaseValue::String("05".to_string()),
            },
            SwitchCase {
                id: Some(3),
                value: SwitchCaseValue::String("7".to_string()),
            },
        ],
        next_case_id: 0,
    };
    data.heal();

    // "05" collides with "5" after parsing and is dropped; "5" and "7" survive.
    assert_eq!(
        data.cases
            .iter()
            .map(|c| c.value.clone())
            .collect::<Vec<_>>(),
        vec![SwitchCaseValue::Int(5), SwitchCaseValue::Int(7)]
    );
    let ids: Vec<u64> = data.cases.iter().map(|c| c.id.unwrap()).collect();
    let unique: std::collections::HashSet<u64> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len(), "surviving ids stay distinct");
    assert!(data.next_case_id > *ids.iter().max().unwrap());
}

/// Test 5c: `heal` resets an all-dropped case list to the default two cases
/// (never empty), and repairs a selector type outside {Int, String}.
#[test]
fn switch_phase3_heal_resets_all_dropped_and_bad_selector() {
    // Every case value is an unparseable string under an Int selector -> all
    // dropped -> reset to the default [0, 1].
    let mut data = SwitchData {
        selector_type: DataType::Int,
        value_type: DataType::Int,
        cases: vec![
            SwitchCase {
                id: Some(1),
                value: SwitchCaseValue::String("x".to_string()),
            },
            SwitchCase {
                id: Some(2),
                value: SwitchCaseValue::String("y".to_string()),
            },
        ],
        next_case_id: 3,
    };
    data.heal();
    assert_eq!(
        data.cases
            .iter()
            .map(|c| c.value.clone())
            .collect::<Vec<_>>(),
        vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)]
    );
    assert!(data.cases.iter().all(|c| c.id.is_some()));

    // A selector_type outside {Int, String} resets to Int.
    let mut bad_sel = SwitchData {
        selector_type: DataType::Float,
        value_type: DataType::Int,
        cases: vec![SwitchCase {
            id: Some(1),
            value: SwitchCaseValue::Int(7),
        }],
        next_case_id: 2,
    };
    bad_sel.heal();
    assert_eq!(bad_sel.selector_type, DataType::Int);
    assert_eq!(bad_sel.cases.len(), 1);
    assert_eq!(bad_sel.cases[0].value, SwitchCaseValue::Int(7));
}

/// Test: `heal` is a no-op on a well-formed switch (the round-trip of a good
/// file must not perturb it — the loader runs `heal` on every load).
#[test]
fn switch_phase3_heal_is_noop_on_valid_data() {
    let good = switch_data(
        DataType::Int,
        DataType::Crystal,
        vec![
            SwitchCaseValue::Int(3),
            SwitchCaseValue::Int(-7),
            SwitchCaseValue::Int(42),
        ],
    );
    let mut healed = good.clone();
    healed.heal();
    assert_eq!(healed.selector_type, good.selector_type);
    assert_eq!(healed.value_type, good.value_type);
    assert_eq!(healed.next_case_id, good.next_case_id);
    assert_eq!(
        healed.cases.iter().map(|c| c.id).collect::<Vec<_>>(),
        good.cases.iter().map(|c| c.id).collect::<Vec<_>>()
    );
    assert_eq!(
        healed
            .cases
            .iter()
            .map(|c| c.value.clone())
            .collect::<Vec<_>>(),
        good.cases
            .iter()
            .map(|c| c.value.clone())
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Phase 4 (API): the scope-aware setter reaches a body-internal switch.
//
// The API layer is a thin wrapper (tested via the underlying core function per
// `rust/AGENTS.md`), but the property-panel-wrong-node bug class — a setter
// resolving a body node by bare id and hitting a same-id top-level node — must
// be pinned by a `scope_path`-driven test of the `StructureDesigner` op the API
// forwards to.
// ============================================================================

/// Read the `SwitchData` of a node living inside `map_id`'s zone body.
fn body_switch_data<'a>(
    designer: &'a StructureDesigner,
    network: &str,
    map_id: u64,
    sw_id: u64,
) -> &'a SwitchData {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&map_id)
        .unwrap()
        .zone
        .as_ref()
        .expect("map body")
        .nodes
        .get(&sw_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<SwitchData>()
        .expect("body node is a switch")
}

/// `set_switch_data` with a non-empty `scope_path` edits the switch inside a
/// `map` body, not a same-id node in the active top-level network.
#[test]
fn test_set_switch_data_targets_body_node_via_scope_path() {
    let mut designer = setup_designer_with_network("main");
    let map_id = designer.add_node("map", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "main",
        map_id,
        Box::new(MapData {
            input_type: DataType::Int,
            output_type: DataType::Int,
        }),
    );

    // A body switch with id 4, plus a top-level switch that happens to share the
    // numeric id 4 would be the collision — but top-level ids are auto-assigned,
    // so instead we plant a top-level switch with distinct cases and prove the
    // scoped edit leaves it untouched while the body switch changes.
    let top_sw = add_switch(
        &mut designer,
        "main",
        switch_data(
            DataType::Int,
            DataType::Int,
            vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        ),
        600.0,
    );

    let sw_id = {
        let body = designer
            .node_type_registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .expect("map body must init");
        body.add_node(
            "switch",
            DVec2::new(0.0, 0.0),
            top_sw as usize, // same numeric id as the top-level switch — the bug class
            Box::new(switch_data(
                DataType::Int,
                DataType::Int,
                vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
            )),
        )
    };
    // Populate the body switch's custom-type cache (mirrors set_node_data,
    // resolved through the body network).
    {
        let registry = &mut designer.node_type_registry;
        let node = registry
            .node_networks
            .get_mut("main")
            .unwrap()
            .nodes
            .get_mut(&map_id)
            .unwrap()
            .zone_mut()
            .unwrap()
            .nodes
            .get_mut(&sw_id)
            .unwrap();
        NodeTypeRegistry::populate_custom_node_type_cache_with_types(
            &registry.built_in_node_types,
            &registry.record_type_defs,
            &registry.built_in_record_type_defs,
            node,
            true,
        );
    }
    designer.validate_active_network();

    // Edit the body switch through its scope path.
    designer
        .set_switch_data(
            &[map_id],
            sw_id,
            DataType::Int,
            DataType::Int,
            vec![
                SwitchCaseValue::Int(0),
                SwitchCaseValue::Int(1),
                SwitchCaseValue::Int(2),
            ],
        )
        .expect("scoped body switch edit must succeed");

    // The body switch got the new third case…
    let body = body_switch_data(&designer, "main", map_id, sw_id);
    assert_eq!(
        body.cases
            .iter()
            .map(|c| c.value.clone())
            .collect::<Vec<_>>(),
        vec![
            SwitchCaseValue::Int(0),
            SwitchCaseValue::Int(1),
            SwitchCaseValue::Int(2),
        ],
        "the body switch (targeted by scope_path) must have gained the new case"
    );

    // …while the same-id top-level switch is untouched.
    let top = designer
        .node_type_registry
        .node_networks
        .get("main")
        .unwrap()
        .nodes
        .get(&top_sw)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<SwitchData>()
        .unwrap();
    assert_eq!(
        top.cases
            .iter()
            .map(|c| c.value.clone())
            .collect::<Vec<_>>(),
        vec![SwitchCaseValue::Int(0), SwitchCaseValue::Int(1)],
        "the same-id top-level switch must NOT be affected by the scoped edit"
    );
}
