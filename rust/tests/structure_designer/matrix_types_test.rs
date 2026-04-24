//! Phase 1 tests for IMat3 / Mat3 core types.
//!
//! Covers:
//! 1. DataType: parse round-trip, IMat3 ↔ Mat3 conversion rules.
//! 2. NetworkResult: extractors, infer_data_type, convert_to coercions.
//! 3. TextValue: serde round-trip, accessors, to_network_result coercions.
//! 4. Text-format parser: nested-tuple `((..),(..),(..))` literal recognition.

use glam::DMat3;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::{
    NetworkResult, dmat3_to_rows, imat3_rows_to_dmat3, rows_to_dmat3,
};
use rust_lib_flutter_cad::structure_designer::text_format::{
    Lexer, Parser, PropertyValue, Statement, TextValue,
};

// ---------------------------------------------------------------------------
// DataType
// ---------------------------------------------------------------------------

#[test]
fn data_type_display_round_trip_imat3() {
    assert_eq!(DataType::IMat3.to_string(), "IMat3");
    assert_eq!(DataType::from_string("IMat3").unwrap(), DataType::IMat3);
}

#[test]
fn data_type_display_round_trip_mat3() {
    assert_eq!(DataType::Mat3.to_string(), "Mat3");
    assert_eq!(DataType::from_string("Mat3").unwrap(), DataType::Mat3);
}

#[test]
fn imat3_can_convert_to_mat3_and_back() {
    assert!(DataType::can_be_converted_to(
        &DataType::IMat3,
        &DataType::Mat3
    ));
    assert!(DataType::can_be_converted_to(
        &DataType::Mat3,
        &DataType::IMat3
    ));
}

#[test]
fn imat3_does_not_freely_convert_from_ivec3_or_other_types() {
    // D4: no auto-promotion from IVec3 to diagonal IMat3.
    assert!(!DataType::can_be_converted_to(
        &DataType::IVec3,
        &DataType::IMat3
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::Vec3,
        &DataType::Mat3
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::Float,
        &DataType::Mat3
    ));
    assert!(!DataType::can_be_converted_to(
        &DataType::Int,
        &DataType::IMat3
    ));
}

#[test]
fn matrix_to_array_broadcast_works() {
    // Standard `T -> [T]` broadcasting still applies to matrices.
    let array_of_imat3 = DataType::Array(Box::new(DataType::IMat3));
    assert!(DataType::can_be_converted_to(
        &DataType::IMat3,
        &array_of_imat3
    ));
}

// ---------------------------------------------------------------------------
// NetworkResult
// ---------------------------------------------------------------------------

#[test]
fn network_result_imat3_infer_and_extract() {
    let m = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
    let r = NetworkResult::IMat3(m);
    assert_eq!(r.infer_data_type(), Some(DataType::IMat3));
    assert_eq!(r.extract_imat3(), Some(m));
}

#[test]
fn network_result_mat3_infer_and_extract() {
    let dmat = imat3_rows_to_dmat3(&[[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
    let r = NetworkResult::Mat3(dmat);
    assert_eq!(r.infer_data_type(), Some(DataType::Mat3));
    assert_eq!(r.extract_mat3(), Some(dmat));
}

#[test]
fn imat3_rows_to_dmat3_preserves_row_major_semantics() {
    // After construction, `.row(i).col(j)` of the row-major source equals the
    // (i, j) entry retrieved via `dmat3_to_rows` (which transposes back).
    let m = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
    let d = imat3_rows_to_dmat3(&m);
    let back = dmat3_to_rows(&d);
    for i in 0..3 {
        for j in 0..3 {
            assert_eq!(back[i][j] as i32, m[i][j], "mismatch at ({}, {})", i, j);
        }
    }
}

#[test]
fn network_result_convert_imat3_to_mat3_lossless() {
    let m = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
    let r = NetworkResult::IMat3(m).convert_to(&DataType::IMat3, &DataType::Mat3);
    let dmat = r.extract_mat3().expect("should be Mat3");
    let rows = dmat3_to_rows(&dmat);
    for i in 0..3 {
        for j in 0..3 {
            assert_eq!(rows[i][j], m[i][j] as f64);
        }
    }
}

#[test]
fn network_result_convert_mat3_to_imat3_truncates() {
    let rows = [[1.7, 2.3, 3.5], [-1.6, 0.9, 4.99], [10.1, -5.5, 0.0]];
    let dmat = rows_to_dmat3(&rows);
    let r = NetworkResult::Mat3(dmat).convert_to(&DataType::Mat3, &DataType::IMat3);
    let imat = r.extract_imat3().expect("should be IMat3");
    // `as i32` truncates toward zero.
    assert_eq!(imat[0], [1, 2, 3]);
    assert_eq!(imat[1], [-1, 0, 4]);
    assert_eq!(imat[2], [10, -5, 0]);
}

#[test]
fn network_result_convert_identity_when_target_matches() {
    let m = [[1, 0, 0], [0, 1, 0], [0, 0, 1]];
    let r = NetworkResult::IMat3(m).convert_to(&DataType::IMat3, &DataType::IMat3);
    assert_eq!(r.extract_imat3(), Some(m));
}

// ---------------------------------------------------------------------------
// TextValue serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn text_value_imat3_serde_round_trip() {
    let tv = TextValue::IMat3([[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
    let json = serde_json::to_string(&tv).expect("serialize");
    let back: TextValue = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(tv, back);
    // Sanity: type tag is "IMat3".
    assert!(json.contains("\"IMat3\""));
}

#[test]
fn text_value_mat3_serde_round_trip() {
    let tv = TextValue::Mat3([[1.5, 2.5, 3.5], [-1.0, 0.0, 4.25], [10.125, -5.0, 0.0]]);
    let json = serde_json::to_string(&tv).expect("serialize");
    let back: TextValue = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(tv, back);
    assert!(json.contains("\"Mat3\""));
}

#[test]
fn text_value_inferred_data_type_for_matrices() {
    assert_eq!(
        TextValue::IMat3([[1, 0, 0], [0, 1, 0], [0, 0, 1]]).inferred_data_type(),
        DataType::IMat3
    );
    assert_eq!(
        TextValue::Mat3([[0.0; 3]; 3]).inferred_data_type(),
        DataType::Mat3
    );
}

#[test]
fn text_value_to_network_result_imat3_to_mat3_coerces() {
    let tv = TextValue::IMat3([[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
    let nr = tv.to_network_result(&DataType::Mat3).expect("coerced");
    let dmat = nr.extract_mat3().expect("Mat3");
    let rows = dmat3_to_rows(&dmat);
    for i in 0..3 {
        for j in 0..3 {
            assert_eq!(rows[i][j], (i * 3 + j + 1) as f64);
        }
    }
}

#[test]
fn text_value_to_network_result_mat3_to_imat3_truncates() {
    let tv = TextValue::Mat3([[1.9, 2.1, -3.5], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]]);
    let nr = tv.to_network_result(&DataType::IMat3).expect("coerced");
    let m = nr.extract_imat3().expect("IMat3");
    assert_eq!(m[0], [1, 2, -3]);
}

#[test]
fn text_value_as_imat3_and_as_mat3_accessors() {
    let tv_i = TextValue::IMat3([[1, 0, 0], [0, 1, 0], [0, 0, 1]]);
    assert_eq!(tv_i.as_imat3(), Some([[1, 0, 0], [0, 1, 0], [0, 0, 1]]));
    assert_eq!(
        tv_i.as_mat3(),
        Some([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
    );

    let tv_f = TextValue::Mat3([[1.7, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    assert_eq!(
        tv_f.as_mat3(),
        Some([[1.7, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
    );
    // as_imat3 truncates Mat3 source.
    assert_eq!(tv_f.as_imat3(), Some([[1, 0, 0], [0, 1, 0], [0, 0, 1]]));
}

// ---------------------------------------------------------------------------
// Text-format parser: nested-tuple matrix literals
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
fn parser_recognizes_imat3_literal_when_all_components_int() {
    let pv = parse_single_literal("((1, 2, 3), (4, 5, 6), (7, 8, 9))");
    match pv {
        PropertyValue::Literal(TextValue::IMat3(m)) => {
            assert_eq!(m, [[1, 2, 3], [4, 5, 6], [7, 8, 9]]);
        }
        other => panic!("expected IMat3 literal, got {:?}", other),
    }
}

#[test]
fn parser_recognizes_mat3_literal_when_any_component_float() {
    let pv = parse_single_literal("((1, 2, 3), (4, 5.0, 6), (7, 8, 9))");
    match pv {
        PropertyValue::Literal(TextValue::Mat3(m)) => {
            assert_eq!(m[0], [1.0, 2.0, 3.0]);
            assert_eq!(m[1], [4.0, 5.0, 6.0]);
            assert_eq!(m[2], [7.0, 8.0, 9.0]);
        }
        other => panic!("expected Mat3 literal, got {:?}", other),
    }
}

#[test]
fn parser_rejects_matrix_with_wrong_row_count() {
    let src = "n = node { p: ((1, 2, 3), (4, 5, 6)) }";
    let err = Parser::parse(src).expect_err("should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("Matrix literal must have 3 rows"),
        "unexpected error: {}",
        msg
    );
}

#[test]
fn parser_rejects_matrix_with_wrong_row_width() {
    let src = "n = node { p: ((1, 2), (3, 4), (5, 6)) }";
    let err = Parser::parse(src).expect_err("should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("Matrix row must have 3 components"),
        "unexpected error: {}",
        msg
    );
}

#[test]
fn parser_still_parses_plain_vec3_literal() {
    // Regression: pre-existing `(x, y, z)` syntax must not be misclassified.
    let pv = parse_single_literal("(1, 2, 3)");
    match pv {
        PropertyValue::Literal(TextValue::IVec3(v)) => {
            assert_eq!(v.x, 1);
            assert_eq!(v.y, 2);
            assert_eq!(v.z, 3);
        }
        other => panic!("expected IVec3 literal, got {:?}", other),
    }
}

#[test]
fn serializer_round_trips_imat3_text() {
    let m = [[1, -2, 3], [4, 5, -6], [7, 8, 9]];
    let tv = TextValue::IMat3(m);
    let text = tv.to_text();
    assert_eq!(text, "((1, -2, 3), (4, 5, -6), (7, 8, 9))");
    let pv = parse_single_literal(&text);
    match pv {
        PropertyValue::Literal(TextValue::IMat3(parsed)) => assert_eq!(parsed, m),
        other => panic!("expected IMat3 round-trip, got {:?}", other),
    }
}

#[test]
fn serializer_round_trips_mat3_text() {
    let m = [[1.5, -2.0, 0.25], [4.0, 5.0, -6.0], [7.0, 8.0, 9.0]];
    let tv = TextValue::Mat3(m);
    let text = tv.to_text();
    let pv = parse_single_literal(&text);
    match pv {
        PropertyValue::Literal(TextValue::Mat3(parsed)) => {
            for i in 0..3 {
                for j in 0..3 {
                    assert!((parsed[i][j] - m[i][j]).abs() < 1e-12);
                }
            }
        }
        other => panic!("expected Mat3 round-trip, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Glam interop sanity check
// ---------------------------------------------------------------------------

#[test]
fn rows_to_dmat3_round_trip_is_identity() {
    let rows = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
    let d = rows_to_dmat3(&rows);
    let back = dmat3_to_rows(&d);
    assert_eq!(rows, back);
}

#[test]
fn dmat3_default_constructor_does_not_collide_with_our_layer() {
    // Ensure NetworkResult::Mat3 stores DMat3 directly.
    let id = DMat3::IDENTITY;
    let r = NetworkResult::Mat3(id);
    assert_eq!(r.extract_mat3(), Some(DMat3::IDENTITY));
}

#[test]
fn lexer_includes_dot_for_pin_refs_unaffected_by_matrix_changes() {
    // Sanity: matrix-literal changes did not break the existing dot-handling.
    let toks = Lexer::tokenize("a.b").expect("tokenize");
    // Sequence: ident("a"), dot, ident("b"), eof
    assert_eq!(toks.len(), 4);
}
