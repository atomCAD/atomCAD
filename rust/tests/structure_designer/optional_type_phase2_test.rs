//! Phase 2 tests for the `Optional[T]` data type (see
//! `doc/design_optional_type.md`).
//!
//! Phase 2 wires `Optional` into the record nodes:
//! - `record_construct`: an `Optional[T]` field gets a plain-`T` input pin;
//!   `eval` keeps an explicit `None` for an unwired/unset Optional field
//!   instead of collapsing the whole record; a required field unwired still
//!   collapses; a stored literal still overrides (resolution order unchanged).
//! - `record_destructure`: an `Optional[T]` field projects onto a plain-`T`
//!   output pin and passes `None`/payload straight through.
//! - Flipping a field's Optional-ness keeps the pin type `T` (no wire
//!   disconnect), only the eval-time collapse behavior changes.
//! - `.cnnd` round-trip of a def carrying Optional fields.

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_evaluator::{
    NetworkEvaluationContext, NetworkEvaluator, NetworkStackElement,
};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};
use rust_lib_flutter_cad::structure_designer::nodes::bool::BoolData;
use rust_lib_flutter_cad::structure_designer::nodes::int::IntData;
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::nodes::record_destructure::RecordDestructureData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ============================================================================
// Helpers (mirrors record_types_phase3_test.rs)
// ============================================================================

fn opt(inner: DataType) -> DataType {
    DataType::Optional(Box::new(inner))
}

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

/// `Settings = { count: Int (required), flag: Optional[Bool] }`. Authored order
/// is non-alphabetical relative to canonical (`count` < `flag`) only at the
/// margins; we mainly want a required field and an Optional field side by side.
fn settings_def() -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        "Settings".to_string(),
        vec![
            ("count".to_string(), DataType::Int),
            ("flag".to_string(), opt(DataType::Bool)),
        ],
    )
}

// ============================================================================
// Pin types: Optional[T] field exposes a plain T pin
// ============================================================================

#[test]
fn construct_optional_field_pin_is_plain_inner_type() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(settings_def())
        .unwrap();

    let id = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordConstructData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    // Authored order [count, flag]. The Optional[Bool] field's pin is plain Bool.
    let pins: Vec<(&str, &DataType)> = nt
        .parameters
        .iter()
        .map(|p| (p.name.as_str(), &p.data_type))
        .collect();
    assert_eq!(pins[0], ("count", &DataType::Int));
    assert_eq!(
        pins[1],
        ("flag", &DataType::Bool),
        "Optional[Bool] field must expose a plain Bool input pin, not Optional[Bool]"
    );
}

#[test]
fn destructure_optional_field_pin_is_plain_inner_type() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(settings_def())
        .unwrap();

    let id = designer.add_node("record_destructure", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        id,
        Box::new(RecordDestructureData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get("test").unwrap();
    let node = network.nodes.get(&id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();

    let out: Vec<(&str, Option<&DataType>)> = nt
        .output_pins
        .iter()
        .map(|p| (p.name.as_str(), p.fixed_type()))
        .collect();
    assert_eq!(out[0], ("count", Some(&DataType::Int)));
    assert_eq!(
        out[1],
        ("flag", Some(&DataType::Bool)),
        "Optional[Bool] field must project onto a plain Bool output pin"
    );
}

// ============================================================================
// Eval: collapse exemption for Optional fields
// ============================================================================

#[test]
fn unwired_optional_field_keeps_none_record_emitted() {
    // count wired, flag (Optional) left unwired & no literal → record is still
    // emitted with flag = None.
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(settings_def())
        .unwrap();

    let count = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", count, Box::new(IntData { value: 3 }));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    designer.validate_active_network();
    // Authored order [count, flag] → pin 0 = count, pin 1 = flag.
    designer.connect_nodes(count, 0, construct, 0);

    let result = evaluate_node_pin(&designer, "test", construct, 0);
    let NetworkResult::Record(fields) = result else {
        panic!(
            "expected Record (Optional field must not collapse it), got {:?}",
            result.to_display_string()
        );
    };
    // Canonical (sorted) order: [count, flag].
    let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["count", "flag"]);
    assert!(matches!(fields[0].1, NetworkResult::Int(3)));
    assert!(
        matches!(fields[1].1, NetworkResult::None),
        "unset Optional field must be an explicit None, got {:?}",
        fields[1].1.to_display_string()
    );
}

#[test]
fn unwired_required_field_collapses_record() {
    // flag (Optional) wired, count (required) left unwired → whole record None.
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(settings_def())
        .unwrap();

    let flag = designer.add_node("bool", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        flag,
        Box::new(BoolData { value: true }),
    );

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    designer.validate_active_network();
    // pin 1 = flag (Optional). count (pin 0, required) left unwired.
    designer.connect_nodes(flag, 0, construct, 1);

    let result = evaluate_node_pin(&designer, "test", construct, 0);
    assert!(
        matches!(result, NetworkResult::None),
        "a required field left unwired must collapse the record, got {:?}",
        result.to_display_string()
    );
}

#[test]
fn unwired_optional_field_with_literal_uses_literal() {
    // count wired, flag unwired but with a stored literal `true` → literal wins
    // (resolution order unchanged for Optional fields).
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(settings_def())
        .unwrap();

    let count = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", count, Box::new(IntData { value: 7 }));

    let mut literals = HashMap::new();
    literals.insert("flag".to_string(), TextValue::Bool(true));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Settings".to_string(),
            literal_values: literals,
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(count, 0, construct, 0);

    let result = evaluate_node_pin(&designer, "test", construct, 0);
    let NetworkResult::Record(fields) = result else {
        panic!("expected Record, got {:?}", result.to_display_string());
    };
    // [count, flag] canonical.
    assert!(matches!(fields[0].1, NetworkResult::Int(7)));
    assert!(
        matches!(fields[1].1, NetworkResult::Bool(true)),
        "stored literal must override the unset default, got {:?}",
        fields[1].1.to_display_string()
    );
}

// ============================================================================
// Construct → destructure round-trip: unset Optional flows None into a plain T
// ============================================================================

#[test]
fn construct_destructure_optional_none_flows_to_plain_consumer() {
    let mut designer = setup_designer_with_network("test");
    designer
        .node_type_registry
        .add_record_type_def(settings_def())
        .unwrap();

    let count = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", count, Box::new(IntData { value: 9 }));

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    let destructure = designer.add_node("record_destructure", DVec2::new(400.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        destructure,
        Box::new(RecordDestructureData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(count, 0, construct, 0); // count wired; flag unset
    designer.connect_nodes(construct, 0, destructure, 0);

    // Output pins in authored order [count, flag] → pin 0 = count, pin 1 = flag.
    let count_out = evaluate_node_pin(&designer, "test", destructure, 0);
    let flag_out = evaluate_node_pin(&designer, "test", destructure, 1);
    assert!(matches!(count_out, NetworkResult::Int(9)));
    assert!(
        matches!(flag_out, NetworkResult::None),
        "unset Optional field destructures to None on a plain Bool pin, got {:?}",
        flag_out.to_display_string()
    );

    // The wire from construct (Record(Settings)) to destructure (Record(Settings))
    // is statically valid (same named record), so the network stays valid.
    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    assert!(
        net.valid,
        "construct→destructure of same schema must stay valid"
    );
}

// ============================================================================
// Flipping a field's Optional-ness keeps the pin type T (no wire disconnect)
// ============================================================================

#[test]
fn flipping_optionalness_keeps_pin_type_and_wire() {
    let mut designer = setup_designer_with_network("test");
    // Start with `flag: Bool` (required), wire a bool into it, then flip the
    // def to `flag: Optional[Bool]` and confirm the pin stays Bool and the wire
    // survives.
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "Settings".to_string(),
            vec![
                ("count".to_string(), DataType::Int),
                ("flag".to_string(), DataType::Bool),
            ],
        ))
        .unwrap();

    let count = designer.add_node("int", DVec2::new(0.0, 0.0));
    set_node_data(&mut designer, "test", count, Box::new(IntData { value: 1 }));
    let flag = designer.add_node("bool", DVec2::new(0.0, 100.0));
    set_node_data(
        &mut designer,
        "test",
        flag,
        Box::new(BoolData { value: false }),
    );

    let construct = designer.add_node("record_construct", DVec2::new(200.0, 0.0));
    set_node_data(
        &mut designer,
        "test",
        construct,
        Box::new(RecordConstructData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    designer.validate_active_network();
    designer.connect_nodes(count, 0, construct, 0);
    designer.connect_nodes(flag, 0, construct, 1);

    // Flip flag → Optional[Bool].
    designer
        .update_record_type_def(
            "Settings",
            vec![
                ("count".to_string(), DataType::Int),
                ("flag".to_string(), opt(DataType::Bool)),
            ],
        )
        .unwrap();
    designer.validate_active_network();

    let net = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    let construct_node = net.nodes.get(&construct).unwrap();
    let registry = &designer.node_type_registry;
    let nt = registry.get_node_type_for_node(construct_node).unwrap();

    // Pin type is still plain Bool (flip did not retype the pin).
    assert_eq!(nt.parameters[1].name, "flag");
    assert_eq!(nt.parameters[1].data_type, DataType::Bool);
    // The flag wire (pin 1) survived the flip (no disconnect).
    assert_eq!(
        construct_node.arguments[1].argument_output_pins().len(),
        1,
        "flipping a field to Optional must not disconnect the existing wire"
    );
    assert!(
        net.valid,
        "network must stay valid after flipping Optional-ness"
    );
}

// ============================================================================
// .cnnd round-trip of a def with Optional fields
// ============================================================================

#[test]
fn optional_field_def_cnnd_roundtrip() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
        load_node_networks_from_file, save_node_networks_to_file,
    };
    use tempfile::tempdir;

    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(settings_def())
        .unwrap();

    let cons = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        &mut designer,
        "Main",
        cons,
        Box::new(RecordConstructData {
            schema: "Settings".to_string(),
            ..Default::default()
        }),
    );

    let temp_dir = tempdir().expect("temp dir");
    let temp_file = temp_dir.path().join("optional_def.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &temp_file,
        false,
        &HashMap::new(),
    )
    .expect("save");

    let mut registry2 = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut registry2, temp_file.to_str().unwrap()).expect("reload");

    // The Optional field survives serde round-trip with its variant intact.
    let def = registry2
        .record_type_defs
        .get("Settings")
        .expect("def missing after roundtrip");
    assert_eq!(
        def.fields
            .iter()
            .map(|f| (f.name.clone(), f.data_type.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("count".to_string(), DataType::Int),
            ("flag".to_string(), opt(DataType::Bool)),
        ]
    );

    // The derived construct pin layout still strips Optional → plain Bool.
    let network = registry2.node_networks.get("Main").unwrap();
    let node = network.nodes.get(&cons).unwrap();
    let nt = registry2.get_node_type_for_node(node).unwrap();
    assert_eq!(nt.parameters[1].name, "flag");
    assert_eq!(nt.parameters[1].data_type, DataType::Bool);
}
