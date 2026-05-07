// Tests for string template literals in the `expr` language.
// See doc/design_string_template_literals_in_expr.md.

use rust_lib_flutter_cad::expr::expr::{Expr, TemplatePart};
use rust_lib_flutter_cad::expr::lexer::{TemplateLexError, Token, TokenTemplatePart, tokenize};
use rust_lib_flutter_cad::expr::parser::parse;
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

mod lexer_tests {
    use super::*;

    fn template_token(input: &str) -> Token {
        // We only care about the first token. After a lex error, the lexer's
        // position can land mid-string and produce trailing tokens — that's
        // fine for the parser (it surfaces the first error) and fine here
        // because we only test the leading template's verdict.
        let toks = tokenize(input);
        assert!(!toks.is_empty(), "tokenize produced no tokens");
        toks.into_iter().next().unwrap()
    }

    fn parts_ok(input: &str) -> Vec<TokenTemplatePart> {
        match template_token(input) {
            Token::Template(Ok(parts)) => parts,
            other => panic!("expected Template(Ok), got {:?}", other),
        }
    }

    fn lex_err(input: &str) -> TemplateLexError {
        match template_token(input) {
            Token::Template(Err(e)) => e,
            other => panic!("expected Template(Err), got {:?}", other),
        }
    }

    #[test]
    fn test_lex_text_only() {
        let parts = parts_ok("`hello`");
        assert_eq!(parts, vec![TokenTemplatePart::Text("hello".to_string())]);
    }

    #[test]
    fn test_lex_empty() {
        // Empty backtick literal: no parts at all.
        let parts = parts_ok("``");
        assert!(parts.is_empty());
    }

    #[test]
    fn test_lex_single_interpolation() {
        let parts = parts_ok("`${x}`");
        assert_eq!(parts, vec![TokenTemplatePart::Expr("x".to_string())]);
    }

    #[test]
    fn test_lex_text_then_expr_then_text() {
        let parts = parts_ok("`a${x}b`");
        assert_eq!(
            parts,
            vec![
                TokenTemplatePart::Text("a".to_string()),
                TokenTemplatePart::Expr("x".to_string()),
                TokenTemplatePart::Text("b".to_string()),
            ]
        );
    }

    #[test]
    fn test_lex_adjacent_interpolations_no_separator() {
        let parts = parts_ok("`${a}${b}`");
        assert_eq!(
            parts,
            vec![
                TokenTemplatePart::Expr("a".to_string()),
                TokenTemplatePart::Expr("b".to_string()),
            ]
        );
    }

    #[test]
    fn test_lex_bare_dollar_is_literal() {
        // `$` not followed by `{` is treated as a normal character.
        let parts = parts_ok("`cost: $5`");
        assert_eq!(parts, vec![TokenTemplatePart::Text("cost: $5".to_string())]);
    }

    #[test]
    fn test_lex_escaped_dollar_disables_interpolation() {
        let parts = parts_ok("`\\${x}`");
        assert_eq!(parts, vec![TokenTemplatePart::Text("${x}".to_string())]);
    }

    #[test]
    fn test_lex_escaped_backticks() {
        let parts = parts_ok("`\\`back\\``");
        assert_eq!(parts, vec![TokenTemplatePart::Text("`back`".to_string())]);
    }

    #[test]
    fn test_lex_escape_newline_tab_carriage_return() {
        let parts = parts_ok("`a\\nb\\tc\\rd`");
        assert_eq!(
            parts,
            vec![TokenTemplatePart::Text("a\nb\tc\rd".to_string())]
        );
    }

    #[test]
    fn test_lex_raw_newline_allowed_in_text() {
        let parts = parts_ok("`line1\nline2`");
        assert_eq!(
            parts,
            vec![TokenTemplatePart::Text("line1\nline2".to_string())]
        );
    }

    #[test]
    fn test_lex_brace_depth_tracked_in_interpolation() {
        let parts = parts_ok("`${ {x: 1} }`");
        assert_eq!(parts, vec![TokenTemplatePart::Expr(" {x: 1} ".to_string())]);
    }

    #[test]
    fn test_lex_unterminated_template() {
        assert_eq!(lex_err("`abc"), TemplateLexError::Unterminated);
    }

    #[test]
    fn test_lex_unterminated_interpolation() {
        assert_eq!(lex_err("`${x"), TemplateLexError::UnterminatedInterpolation);
    }

    #[test]
    fn test_lex_unknown_escape() {
        assert_eq!(lex_err("`\\q`"), TemplateLexError::UnknownEscape('q'));
    }

    #[test]
    fn test_lex_empty_interpolation() {
        assert_eq!(lex_err("`${}`"), TemplateLexError::EmptyInterpolation);
    }

    #[test]
    fn test_lex_whitespace_only_interpolation_treated_as_empty() {
        // Whitespace-only `${   }` carries no expression, so we surface it as
        // EmptyInterpolation rather than handing the parser a blank source.
        assert_eq!(lex_err("`${   }`"), TemplateLexError::EmptyInterpolation);
    }

    #[test]
    fn test_lex_nested_template_rejected() {
        assert_eq!(
            lex_err("`${`inner`}`"),
            TemplateLexError::NestedTemplateNotSupported
        );
    }
}

mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_text_only_template() {
        let expr = parse("`hello`").expect("should parse");
        match expr {
            Expr::Template(parts) => {
                assert_eq!(parts.len(), 1);
                assert!(matches!(&parts[0], TemplatePart::Text(s) if s == "hello"));
            }
            other => panic!("expected Template, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_empty_template() {
        let expr = parse("``").expect("should parse");
        match expr {
            Expr::Template(parts) => assert!(parts.is_empty()),
            other => panic!("expected Template, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_single_interpolation_with_arithmetic() {
        let expr = parse("`${x + 1}`").expect("should parse");
        match expr {
            Expr::Template(parts) => {
                assert_eq!(parts.len(), 1);
                match &parts[0] {
                    TemplatePart::Expr(inner) => {
                        assert!(matches!(**inner, Expr::Binary(..)));
                    }
                    _ => panic!("expected Expr part"),
                }
            }
            other => panic!("expected Template, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_member_access_in_interpolation() {
        let expr = parse("`${v.species}`").expect("should parse");
        match expr {
            Expr::Template(parts) => match &parts[0] {
                TemplatePart::Expr(inner) => {
                    assert!(matches!(**inner, Expr::MemberAccess(..)));
                }
                _ => panic!("expected Expr part"),
            },
            other => panic!("expected Template, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_interpolation_parse_error_is_wrapped() {
        // Inner parse error should be wrapped with the interpolation source so
        // users can identify which `${...}` failed.
        let err = parse("`${x +}`").expect_err("should fail");
        assert!(
            err.contains("template interpolation"),
            "expected wrap message; got: {}",
            err
        );
        assert!(err.contains("x +"), "expected raw source in error: {}", err);
    }

    #[test]
    fn test_parse_lex_errors_surface_as_messages() {
        let err = parse("`abc").expect_err("should fail");
        assert!(
            err.contains("unterminated template literal"),
            "got: {}",
            err
        );

        let err = parse("`${}`").expect_err("should fail");
        assert!(err.contains("empty `${}`"), "got: {}", err);

        let err = parse("`${`inner`}`").expect_err("should fail");
        assert!(err.contains("nested template literals"), "got: {}", err);
    }

    #[test]
    fn test_parse_template_to_prefix_string_round_trip() {
        let expr = parse("`a${x}b`").expect("should parse");
        let s = expr.to_prefix_string();
        // Sanity check on the debug form; we don't lock the exact spelling
        // beyond the feature's identity.
        assert!(s.starts_with("(template "), "got: {}", s);
        assert!(s.contains("(text "), "got: {}", s);
        assert!(s.contains("(expr x)"), "got: {}", s);
    }
}

mod validation_tests {
    use super::*;

    fn validate(text: &str, vars: HashMap<String, DataType>) -> Result<DataType, String> {
        let expr = parse(text)?;
        expr.validate(&vars, get_function_signatures())
    }

    #[test]
    fn test_validate_text_only_is_string() {
        assert_eq!(
            validate("`hello`", HashMap::new()).unwrap(),
            DataType::String
        );
    }

    #[test]
    fn test_validate_empty_template_is_string() {
        assert_eq!(validate("``", HashMap::new()).unwrap(), DataType::String);
    }

    #[test]
    fn test_validate_string_int_float_bool_interpolations_accepted() {
        for ty in [
            DataType::String,
            DataType::Int,
            DataType::Float,
            DataType::Bool,
        ] {
            let mut vars = HashMap::new();
            vars.insert("x".to_string(), ty.clone());
            let out = validate("`${x}`", vars)
                .unwrap_or_else(|e| panic!("{:?} should validate: {}", ty, e));
            assert_eq!(out, DataType::String);
        }
    }

    #[test]
    fn test_validate_rejects_non_stringable_types() {
        let cases: Vec<(DataType, &str)> = vec![
            (DataType::Vec3, "Vec3"),
            (DataType::Vec2, "Vec2"),
            (DataType::IVec3, "IVec3"),
            (DataType::Array(Box::new(DataType::Int)), "Array[Int]"),
            (
                DataType::Record(RecordType::anonymous(vec![(
                    "x".to_string(),
                    DataType::Int,
                )])),
                "Record",
            ),
        ];
        for (ty, label) in cases {
            let mut vars = HashMap::new();
            vars.insert("x".to_string(), ty);
            let res = validate("`${x}`", vars);
            assert!(res.is_err(), "{} should fail validation: {:?}", label, res);
        }
    }

    #[test]
    fn test_validate_record_field_access_in_template() {
        let mut vars = HashMap::new();
        vars.insert(
            "v".to_string(),
            DataType::Record(RecordType::anonymous(vec![
                ("species".to_string(), DataType::String),
                ("size".to_string(), DataType::Int),
            ])),
        );
        let ty = validate("`${v.species}_size${v.size}.xyz`", vars).unwrap();
        assert_eq!(ty, DataType::String);
    }

    #[test]
    fn test_validate_conditional_interpolation() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), DataType::Int);
        let ty = validate("`${if x > 0 then 1 else 2}`", vars).unwrap();
        assert_eq!(ty, DataType::String);
    }
}

mod evaluation_tests {
    use super::*;

    fn eval_with(text: &str, vars: HashMap<String, NetworkResult>) -> NetworkResult {
        let expr = parse(text).expect("parse should succeed");
        expr.evaluate(&vars, get_function_implementations())
    }

    fn eval(text: &str) -> NetworkResult {
        eval_with(text, HashMap::new())
    }

    fn assert_string(v: NetworkResult, expected: &str) {
        match v {
            NetworkResult::String(s) => assert_eq!(s, expected),
            other => panic!(
                "expected String({:?}), got type {:?}",
                expected,
                other.infer_data_type()
            ),
        }
    }

    #[test]
    fn test_evaluate_text_only() {
        assert_string(eval("`hello`"), "hello");
    }

    #[test]
    fn test_evaluate_empty_template() {
        assert_string(eval("``"), "");
    }

    #[test]
    fn test_evaluate_int_interpolation() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::Int(42));
        assert_string(eval_with("`${x}`", vars), "42");
    }

    #[test]
    fn test_evaluate_float_trims_trailing_zeros() {
        // Path-friendly Float formatting: 1.0 → "1", 0.1 → "0.1".
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::Float(1.0));
        assert_string(eval_with("`${x}`", vars), "1");

        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::Float(0.1));
        assert_string(eval_with("`${x}`", vars), "0.1");
    }

    #[test]
    fn test_evaluate_float_nan_rejected() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::Float(f64::NAN));
        match eval_with("`${x}`", vars) {
            NetworkResult::Error(msg) => assert!(
                msg.contains("non-finite"),
                "expected non-finite error, got: {}",
                msg
            ),
            other => panic!("expected Error, got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_float_infinity_rejected() {
        for f in [f64::INFINITY, f64::NEG_INFINITY] {
            let mut vars = HashMap::new();
            vars.insert("x".to_string(), NetworkResult::Float(f));
            match eval_with("`${x}`", vars) {
                NetworkResult::Error(_) => {}
                other => panic!(
                    "expected Error for {}, got {:?}",
                    f,
                    other.infer_data_type()
                ),
            }
        }
    }

    #[test]
    fn test_evaluate_bool_interpolation() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::Bool(true));
        assert_string(eval_with("`${x}`", vars), "true");
    }

    #[test]
    fn test_evaluate_string_interpolation_passthrough() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::String("abc".to_string()));
        assert_string(eval_with("`${x}`", vars), "abc");
    }

    #[test]
    fn test_evaluate_mixed_text_and_interpolation() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::Int(7));
        assert_string(eval_with("`a${x}b`", vars), "a7b");
    }

    #[test]
    fn test_evaluate_adjacent_interpolations() {
        let mut vars = HashMap::new();
        vars.insert("a".to_string(), NetworkResult::Int(1));
        vars.insert("b".to_string(), NetworkResult::Int(2));
        assert_string(eval_with("`${a}${b}`", vars), "12");
    }

    #[test]
    fn test_evaluate_record_field_interpolation() {
        // Build a Record(species: String("Si"), size: Int(5)) variable.
        let mut vars = HashMap::new();
        let rec = NetworkResult::record(vec![
            (
                "species".to_string(),
                NetworkResult::String("Si".to_string()),
            ),
            ("size".to_string(), NetworkResult::Int(5)),
        ]);
        vars.insert("v".to_string(), rec);
        assert_string(eval_with("`${v.species}_${v.size}.xyz`", vars), "Si_5.xyz");
    }

    #[test]
    fn test_evaluate_escaped_dollar_is_literal() {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), NetworkResult::Int(7));
        assert_string(eval_with("`\\${x}`", vars), "${x}");
    }

    #[test]
    fn test_evaluate_bare_dollar_is_literal() {
        assert_string(eval("`cost: $5`"), "cost: $5");
    }
}

/// Coverage test: enumerates all `DataType` variants and asserts whether each
/// is allowed as a `${...}` interpolation result. When a new variant is added,
/// this test forces an explicit accept-or-reject decision in
/// `Expr::Template`'s validation arm.
#[test]
fn template_interpolation_accepts_only_documented_stringable_types() {
    let int_arr = || DataType::Array(Box::new(DataType::Int));
    let cases: Vec<(DataType, bool)> = vec![
        // accepted
        (DataType::String, true),
        (DataType::Int, true),
        (DataType::Float, true),
        (DataType::Bool, true),
        // rejected: vectors / matrices
        (DataType::Vec2, false),
        (DataType::Vec3, false),
        (DataType::IVec2, false),
        (DataType::IVec3, false),
        (DataType::Mat3, false),
        (DataType::IMat3, false),
        // rejected: composite & domain
        (int_arr(), false),
        (DataType::Iterator(Box::new(DataType::Int)), false),
        (
            DataType::Record(RecordType::Named("Foo".to_string())),
            false,
        ),
        (
            DataType::Record(RecordType::anonymous(vec![(
                "x".to_string(),
                DataType::Int,
            )])),
            false,
        ),
        (DataType::Structure, false),
        (DataType::Blueprint, false),
        (DataType::Crystal, false),
        (DataType::Molecule, false),
        (DataType::LatticeVecs, false),
        (DataType::DrawingPlane, false),
        (DataType::Geometry2D, false),
        (DataType::Motif, false),
    ];

    for (ty, accepted) in cases {
        let mut vars = HashMap::new();
        vars.insert("x".to_string(), ty.clone());
        let parsed = parse("`${x}`").expect("template parse should succeed");
        let res = parsed.validate(&vars, get_function_signatures());
        if accepted {
            assert!(
                res.is_ok(),
                "{:?} should be accepted in template interpolation, got {:?}",
                ty,
                res
            );
        } else {
            assert!(
                res.is_err(),
                "{:?} should be rejected in template interpolation, got {:?}",
                ty,
                res
            );
        }
    }
}
