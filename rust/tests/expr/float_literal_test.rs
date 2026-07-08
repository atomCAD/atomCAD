// Regression tests for https://github.com/atomCAD/atomCAD/issues/389
//
// Two related defects:
//   Fix A — a float-form literal like `2.0` was lexed to an f64 and then
//     reclassified by value in the parser (`n.fract() == 0.0` → Int), so
//     `2.0` became `Expr::Int(2)`. That broke `sqrt(2.0)` and silently turned
//     `2.0 / 4` into integer division (`0` instead of `0.5`).
//   Fix B — the float-taking math functions used the strict `extract_float`,
//     which rejects a genuine `Int` argument, so `sqrt(4)` / `sin(0)` errored
//     even though Int is convertible to Float.
#![allow(clippy::approx_constant)]

use rust_lib_flutter_cad::expr::expr::Expr;
use rust_lib_flutter_cad::expr::parser::parse;
use rust_lib_flutter_cad::expr::validation::get_function_implementations;
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

fn eval_str(src: &str) -> NetworkResult {
    let expr = parse(src).expect("should parse");
    expr.evaluate(&HashMap::new(), get_function_implementations())
}

// --- Fix A: float-form literals parse to Expr::Float ---

#[test]
fn test_decimal_literal_is_float() {
    assert!(
        matches!(parse("2.0").unwrap(), Expr::Float(v) if v == 2.0),
        "2.0 should parse as Expr::Float, not Int"
    );
}

#[test]
fn test_trailing_dot_literal_is_float() {
    assert!(
        matches!(parse("2.").unwrap(), Expr::Float(v) if v == 2.0),
        "2. should parse as Expr::Float"
    );
}

#[test]
fn test_exponent_literal_is_float() {
    assert!(
        matches!(parse("1e3").unwrap(), Expr::Float(v) if v == 1000.0),
        "1e3 should parse as Expr::Float"
    );
}

#[test]
fn test_plain_integer_literal_stays_int() {
    assert!(
        matches!(parse("2").unwrap(), Expr::Int(2)),
        "2 should still parse as Expr::Int"
    );
}

#[test]
fn test_large_integer_literal_is_float() {
    // Beyond i32::MAX with no decimal point → cannot be Int, stays Float.
    assert!(
        matches!(parse("4000000000").unwrap(), Expr::Float(_)),
        "an out-of-i32-range integer literal should be Float"
    );
}

#[test]
fn test_sqrt_of_float_literal() {
    match eval_str("sqrt(2.0)") {
        NetworkResult::Float(v) => assert!((v - std::f64::consts::SQRT_2).abs() < 1e-10),
        other => panic!("expected Float, got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_float_literal_division_is_not_integer_division() {
    // `2.0 / 4` must be 0.5 — before Fix A, `2.0` was Int(2) so this was 2/4 = 0.
    match eval_str("2.0 / 4") {
        NetworkResult::Float(v) => assert_eq!(v, 0.5),
        other => panic!("expected Float(0.5), got {:?}", other.to_display_string()),
    }
}

// --- Fix B: float-taking functions coerce a genuine Int argument ---

#[test]
fn test_sqrt_of_int_literal() {
    match eval_str("sqrt(4)") {
        NetworkResult::Float(v) => assert_eq!(v, 2.0),
        other => panic!("expected Float(2.0), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_sin_of_int_literal() {
    match eval_str("sin(0)") {
        NetworkResult::Float(v) => assert!(v.abs() < 1e-10),
        other => panic!("expected Float(0.0), got {:?}", other.to_display_string()),
    }
}

#[test]
fn test_rounding_functions_accept_int() {
    for src in ["floor(3)", "ceil(3)", "round(3)"] {
        match eval_str(src) {
            NetworkResult::Float(v) => assert_eq!(v, 3.0, "{}", src),
            other => panic!(
                "expected Float for {}, got {:?}",
                src,
                other.to_display_string()
            ),
        }
    }
}
