//! Phase 1 tests for the `IMat2` core type.
//!
//! `IMat2` is a strict 2D mirror of `IMat3` with one scoping difference: there
//! is **no** float `Mat2` partner, so `IMat2`'s only conversion is identity (no
//! `IMat2 ↔ Mat2` arms). See `doc/design_imat2_and_plane_tiling.md`.
//!
//! Covers:
//! 1. `DataType`: parse round-trip, identity-only conversion (no Mat2).
//! 2. `NetworkResult`: extractor, infer_data_type, identity convert_to.
//! 3. `TextValue`: serde round-trip, accessor, to_network_result identity.
//! 4. Text-format parser/serializer: `((a,b),(c,d))` literal recognition.
//! 5. `util::imat2::IMat2` value struct.

use glam::IVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::text_format::{
    Parser, PropertyValue, Statement, TextValue,
};
use rust_lib_flutter_cad::util::imat2::IMat2;

// ---------------------------------------------------------------------------
// DataType
// ---------------------------------------------------------------------------

#[test]
fn data_type_display_round_trip_imat2() {
    assert_eq!(DataType::IMat2.to_string(), "IMat2");
    assert_eq!(DataType::from_string("IMat2").unwrap(), DataType::IMat2);
}

#[test]
fn imat2_conversion_is_identity_only() {
    let registry = NodeTypeRegistry::new();
    // Identity always works.
    assert!(DataType::can_be_converted_to(
        &DataType::IMat2,
        &DataType::IMat2,
        &registry
    ));
    // No IMat2 <-> IMat3 / Mat3 / IVec2 etc. — IMat2 has no conversion partner.
    assert!(!DataType::can_be_converted_to(
        &DataType::IMat2,
        &DataType::IMat3,
        &registry
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::IMat3,
        &DataType::IMat2,
        &registry
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::IMat2,
        &DataType::Mat3,
        &registry
    ));
    // D4 analog: no auto-promotion from IVec2 to a diagonal IMat2.
    assert!(!DataType::can_be_converted_to(
        &DataType::IVec2,
        &DataType::IMat2,
        &registry
    ));
}

#[test]
fn imat2_to_array_broadcast_works() {
    let registry = NodeTypeRegistry::new();
    // Standard `T -> [T]` broadcasting still applies.
    let array_of_imat2 = DataType::Array(Box::new(DataType::IMat2));
    assert!(DataType::can_be_converted_to(
        &DataType::IMat2,
        &array_of_imat2,
        &registry
    ));
}

// ---------------------------------------------------------------------------
// NetworkResult
// ---------------------------------------------------------------------------

#[test]
fn network_result_imat2_infer_and_extract() {
    let m = [[1, 2], [3, 4]];
    let r = NetworkResult::IMat2(m);
    assert_eq!(r.infer_data_type(), Some(DataType::IMat2));
    assert_eq!(r.extract_imat2(), Some(m));
}

#[test]
fn network_result_imat2_extract_wrong_variant_is_none() {
    assert_eq!(NetworkResult::Int(7).extract_imat2(), None);
}

#[test]
fn network_result_convert_imat2_identity() {
    let registry = NodeTypeRegistry::new();
    let m = [[1, 0], [0, 1]];
    let r = NetworkResult::IMat2(m).convert_to(&DataType::IMat2, &DataType::IMat2, &registry);
    assert_eq!(r.extract_imat2(), Some(m));
}

#[test]
fn network_result_imat2_display_string() {
    let r = NetworkResult::IMat2([[1, 2], [3, 4]]);
    assert_eq!(r.to_display_string(), "((1, 2), (3, 4))");
}

// ---------------------------------------------------------------------------
// TextValue serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn text_value_imat2_serde_round_trip() {
    let tv = TextValue::IMat2([[1, 2], [3, 4]]);
    let json = serde_json::to_string(&tv).expect("serialize");
    let back: TextValue = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(tv, back);
    assert!(json.contains("\"IMat2\""));
}

#[test]
fn text_value_inferred_data_type_for_imat2() {
    assert_eq!(
        TextValue::IMat2([[1, 0], [0, 1]]).inferred_data_type(),
        DataType::IMat2
    );
}

#[test]
fn text_value_to_network_result_imat2_identity() {
    let tv = TextValue::IMat2([[2, -1], [-1, 1]]);
    let nr = tv.to_network_result(&DataType::IMat2).expect("coerced");
    assert_eq!(nr.extract_imat2(), Some([[2, -1], [-1, 1]]));
}

#[test]
fn text_value_as_imat2_accessor() {
    let tv = TextValue::IMat2([[1, 0], [0, 1]]);
    assert_eq!(tv.as_imat2(), Some([[1, 0], [0, 1]]));
    // No truncation arm — a non-IMat2 source yields None.
    assert_eq!(TextValue::Int(3).as_imat2(), None);
}

#[test]
fn text_value_from_imat2_constructor() {
    assert_eq!(
        TextValue::from_imat2([[5, 6], [7, 8]]),
        TextValue::IMat2([[5, 6], [7, 8]])
    );
}

// ---------------------------------------------------------------------------
// Text-format parser/serializer: nested-tuple 2x2 literals
// ---------------------------------------------------------------------------

/// Parse a single property value out of `name = node { p: <literal> }`.
fn parse_single_literal(literal_src: &str) -> PropertyValue {
    let src = format!("n = node {{ p: {} }}", literal_src);
    let stmts = Parser::parse(&src).expect("parse");
    match stmts.into_iter().next().expect("one statement") {
        Statement::Assignment { properties, .. } => {
            properties.into_iter().next().expect("one property").1
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn parser_recognizes_imat2_literal() {
    let pv = parse_single_literal("((1, 2), (3, 4))");
    match pv {
        PropertyValue::Literal(TextValue::IMat2(m)) => {
            assert_eq!(m, [[1, 2], [3, 4]]);
        }
        other => panic!("expected IMat2 literal, got {:?}", other),
    }
}

#[test]
fn parser_imat2_literal_with_negatives() {
    let pv = parse_single_literal("((-1, 1), (2, -1))");
    match pv {
        PropertyValue::Literal(TextValue::IMat2(m)) => {
            assert_eq!(m, [[-1, 1], [2, -1]]);
        }
        other => panic!("expected IMat2 literal, got {:?}", other),
    }
}

#[test]
fn parser_2x2_float_literal_truncates_to_imat2() {
    // There is no float Mat2, so a 2x2 literal always resolves to IMat2 even
    // if a component looks like a float (truncated toward zero).
    let pv = parse_single_literal("((1.9, 2.0), (3.0, 4.0))");
    match pv {
        PropertyValue::Literal(TextValue::IMat2(m)) => {
            assert_eq!(m, [[1, 2], [3, 4]]);
        }
        other => panic!("expected IMat2 literal, got {:?}", other),
    }
}

#[test]
fn serializer_round_trips_imat2_text() {
    let m = [[1, -2], [3, 4]];
    let tv = TextValue::IMat2(m);
    let text = tv.to_text();
    assert_eq!(text, "((1, -2), (3, 4))");
    let pv = parse_single_literal(&text);
    match pv {
        PropertyValue::Literal(TextValue::IMat2(parsed)) => assert_eq!(parsed, m),
        other => panic!("expected IMat2 round-trip, got {:?}", other),
    }
}

#[test]
fn parser_still_parses_imat3_after_generalization() {
    // Regression: 3x3 literals must still resolve to IMat3.
    let pv = parse_single_literal("((1, 0, 0), (0, 1, 0), (0, 0, 1))");
    assert!(matches!(
        pv,
        PropertyValue::Literal(TextValue::IMat3(_))
    ));
}

// ---------------------------------------------------------------------------
// util::imat2::IMat2 value struct
// ---------------------------------------------------------------------------

#[test]
fn imat2_struct_identity_and_mul() {
    let id = IMat2::identity();
    let v = IVec2::new(3, 5);
    assert_eq!(id.mul(&v), v);
}

#[test]
fn imat2_struct_mul_vector() {
    // Columns are basis images: col0 = (2, 0), col1 = (0, 3) -> scales axes.
    let m = IMat2::new(&IVec2::new(2, 0), &IVec2::new(0, 3));
    assert_eq!(m.mul(&IVec2::new(1, 1)), IVec2::new(2, 3));
}

#[test]
fn imat2_struct_mul_imat2_associates_with_vector() {
    let a = IMat2::new(&IVec2::new(2, 0), &IVec2::new(0, 2));
    let b = IMat2::new(&IVec2::new(1, 1), &IVec2::new(-1, 1));
    let v = IVec2::new(3, 4);
    let ab = a.mul_imat2(&b);
    // (A*B)*v == A*(B*v)
    assert_eq!(ab.mul(&v), a.mul(&b.mul(&v)));
}

#[test]
fn imat2_struct_as_dmat2() {
    let m = IMat2::new(&IVec2::new(1, 2), &IVec2::new(3, 4));
    let d = m.as_dmat2();
    // glam DMat2 is column-major: col(0) = (1, 2), col(1) = (3, 4).
    assert_eq!(d.col(0).x, 1.0);
    assert_eq!(d.col(0).y, 2.0);
    assert_eq!(d.col(1).x, 3.0);
    assert_eq!(d.col(1).y, 4.0);
}
