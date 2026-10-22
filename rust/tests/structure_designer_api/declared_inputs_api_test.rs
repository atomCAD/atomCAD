//! The FFI surface of declared inputs (`doc/design_wire_declared_inputs.md`,
//! Phases 1 and 2). The `#[frb(sync)]` wrappers need the GPU-backed `CADInstance`;
//! what they hand Dart is built by the `view_builders` helpers tested here:
//! `NodeView::has_open_input_port`, the `APIAddedInput` of a drop, and the
//! offer list of `get_wire_type_offers`. The behavior behind them is tested in
//! the crate (`declared_inputs_test.rs`).

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::node_network::SourcePin;
use atomcad_structure_designer::node_type_registry::RecordTypeDef;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use glam::f64::DVec2;
use rust_lib_flutter_cad::api::structure_designer::structure_designer_api_types::APIDataTypeBase;
use rust_lib_flutter_cad::api::structure_designer::view_builders::{
    added_input_view, has_open_input_port, wire_type_offers_view,
};

const OUT: SourcePin = SourcePin::NodeOutput { pin_index: 0 };

fn setup(code: &str) -> StructureDesigner {
    let mut d = StructureDesigner::new();
    d.add_node_network("Main");
    d.set_active_node_network_name(Some("Main".to_string()));
    let outcome = d.ai_text_edit(code, true);
    assert!(
        outcome.result.errors.is_empty(),
        "{:?}",
        outcome.result.errors
    );
    d.validate_active_network();
    d
}

fn id(d: &StructureDesigner, name: &str) -> u64 {
    d.get_scope_network(&[])
        .unwrap()
        .nodes
        .values()
        .find(|n| n.custom_name.as_deref() == Some(name))
        .unwrap()
        .id
}

fn open_port(d: &StructureDesigner, node_id: u64) -> bool {
    has_open_input_port(&d.get_scope_network(&[]).unwrap().nodes[&node_id])
}

/// The open port is exactly `declared_inputs().is_some()`: `expr` and an
/// anonymous `record_construct` have one; a named-mode `record_construct`
/// (its fields are its def's), a custom-node instance and ordinary nodes do
/// not.
#[test]
fn open_input_ports_follow_declared_inputs() {
    let mut d = StructureDesigner::new();
    d.add_node_network("Sub");
    d.add_node_network("Main");
    d.set_active_node_network_name(Some("Main".to_string()));
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Box",
        vec![("a".to_string(), DataType::Float)],
    ))
    .unwrap();
    let e = d.add_node("expr", DVec2::ZERO);
    let int = d.add_node("int", DVec2::ZERO);
    let sphere = d.add_node("sphere", DVec2::ZERO);
    let map = d.add_node("map", DVec2::ZERO);
    let instance = d.add_node("Sub", DVec2::ZERO);
    let anonymous = d.add_node("record_construct", DVec2::ZERO);
    let named = d.add_node("record_construct", DVec2::ZERO);
    d.set_record_construct_schema(&[], named, "Box").unwrap();

    assert!(open_port(&d, e));
    assert!(open_port(&d, anonymous), "an anonymous record_construct");
    for (label, node) in [
        ("int", int),
        ("sphere", sphere),
        ("map", map),
        ("custom-node instance", instance),
        ("named-mode record_construct", named),
    ] {
        assert!(!open_port(&d, node), "{} has an open port", label);
    }

    // The port follows the mode both ways.
    d.set_record_construct_schema(&[], named, "").unwrap();
    assert!(open_port(&d, named));
    d.set_record_construct_schema(&[], anonymous, "Box")
        .unwrap();
    assert!(!open_port(&d, anonymous));
}

/// A named-mode `record_construct` offers no Take type from wire, even with
/// a mismatched wire on a field: a def's fields are retyped in the schema
/// editor.
#[test]
fn a_named_record_construct_crosses_no_offers() {
    let mut d = setup("v = vec3 { x: 1.0, y: 2.0, z: 3.0 }");
    d.add_record_type_def(RecordTypeDef::from_named_fields(
        "Box",
        vec![("a".to_string(), DataType::Float)],
    ))
    .unwrap();
    let r = d.add_node("record_construct", DVec2::ZERO);
    d.set_record_construct_schema(&[], r, "Box").unwrap();
    d.connect_nodes(id(&d, "v"), 0, r, 0);
    d.validate_active_network();
    assert!(
        d.get_scope_network(&[])
            .unwrap()
            .validation_errors
            .iter()
            .any(|e| e.node_id == Some(r)),
        "the Vec3 wire into the Float field is reported"
    );

    assert!(d.wire_type_offers(&[], r).is_empty());
    assert!(wire_type_offers_view(&d, &[], r).is_empty());
}

#[test]
fn an_added_input_crosses_with_its_row_index_id_and_name() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
e = expr { expression: \"1\", parameters: [{ name: \"a\", data_type: Int }] }
",
    );
    let (v, e) = (id(&d, "v"), id(&d, "e"));
    let added = d.add_input_from_wire(&[], e, v, OUT, 0).unwrap();
    let api = added_input_view(&added);
    assert_eq!(api.pin_index, 1);
    assert_eq!(api.pin_id, added.pin_id);
    assert_eq!(api.name, "v");
}

#[test]
fn offers_cross_one_per_input_pin_in_row_order() {
    let mut d = setup(
        "v = vec3 { x: 1.0, y: 2.0, z: 3.0 }
i = int { value: 1 }
e = expr { expression: \"1\", parameters: [{ name: \"a\", data_type: Int }, { name: \"b\", data_type: Float }, { name: \"c\", data_type: Float }], a: i }
",
    );
    let (v, e) = (id(&d, "v"), id(&d, "e"));
    d.connect_nodes(v, 0, e, 2);

    let offers = wire_type_offers_view(&d, &[], e);

    assert_eq!(offers.len(), 3, "one entry per input pin");
    assert!(offers[0].is_none(), "compatible");
    assert!(offers[1].is_none(), "unwired");
    let offer = offers[2].as_ref().expect("the stale row is offered");
    assert!(matches!(offer.data_type_base, APIDataTypeBase::Vec3));
    assert_eq!(
        d.wire_type_offers(&[], e)[2],
        Some(DataType::Vec3),
        "the API entry is the domain offer"
    );
}

#[test]
fn a_node_without_declared_inputs_crosses_an_empty_offer_list() {
    let d = setup("i = int { value: 1 }");
    assert!(wire_type_offers_view(&d, &[], id(&d, "i")).is_empty());
}
