// Phase 7 of doc/design_record_types.md: expression-language record literals,
// generalized field access, and inline anonymous record-type expressions.
//
// Layered to mirror the existing array-literal test family:
//   - lexer_tests:  brace/colon tokens
//   - parser_tests: `{x: 1}` literal parsing, type-position parsing,
//                   error cases (duplicates, trailing commas, …)
//   - validation_tests: types of record literals, field access, vector/record
//                       member-name conflict resolution
//   - evaluation_tests: end-to-end parse → validate → evaluate round-trips

use rust_lib_flutter_cad::expr::expr::Expr;
use rust_lib_flutter_cad::expr::lexer::{Token, tokenize};
use rust_lib_flutter_cad::expr::parser::parse;
use rust_lib_flutter_cad::expr::validation::{
    get_function_implementations, get_function_signatures,
};
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::evaluator::network_result::NetworkResult;
use std::collections::HashMap;

mod lexer_tests {
    use super::*;

    #[test]
    fn test_tokenize_braces_and_colon() {
        let tokens = tokenize("{ x: 1 }");
        assert_eq!(
            tokens,
            vec![
                Token::LBrace,
                Token::Ident("x".to_string()),
                Token::Colon,
                Token::Number(1.0),
                Token::RBrace,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_tokenize_empty_record_literal() {
        let tokens = tokenize("{}");
        assert_eq!(tokens, vec![Token::LBrace, Token::RBrace, Token::Eof]);
    }
}

mod parser_tests {
    use super::*;

    #[test]
    fn test_parse_simple_record_literal() {
        let expr = parse("{x: 1, y: 2}").expect("parse should succeed");
        match expr {
            Expr::RecordLiteral(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "x");
                assert_eq!(fields[1].0, "y");
                assert!(matches!(fields[0].1, Expr::Int(1)));
                assert!(matches!(fields[1].1, Expr::Int(2)));
            }
            other => panic!("expected RecordLiteral, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_empty_record_literal() {
        let expr = parse("{}").expect("parse should succeed");
        match expr {
            Expr::RecordLiteral(fields) => assert!(fields.is_empty()),
            other => panic!("expected RecordLiteral, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_record_literal_preserves_authored_order() {
        // Source order in AST is preserved (canonicalization happens later);
        // makes error messages able to point back to the user's spelling.
        let expr = parse("{z: 3, a: 1, m: 2}").expect("parse should succeed");
        match expr {
            Expr::RecordLiteral(fields) => {
                let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                assert_eq!(names, vec!["z", "a", "m"]);
            }
            other => panic!("expected RecordLiteral, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_record_literal_with_expressions() {
        let expr = parse("{x: 1 + 2, y: 3 * 4}").expect("parse should succeed");
        match expr {
            Expr::RecordLiteral(fields) => {
                assert_eq!(fields.len(), 2);
                assert!(matches!(fields[0].1, Expr::Binary(..)));
                assert!(matches!(fields[1].1, Expr::Binary(..)));
            }
            other => panic!("expected RecordLiteral, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_record_literal() {
        let expr = parse("{outer: {inner: 1}}").expect("parse should succeed");
        match expr {
            Expr::RecordLiteral(outer) => {
                assert_eq!(outer.len(), 1);
                assert_eq!(outer[0].0, "outer");
                match &outer[0].1 {
                    Expr::RecordLiteral(inner) => {
                        assert_eq!(inner.len(), 1);
                        assert_eq!(inner[0].0, "inner");
                    }
                    other => panic!("expected nested RecordLiteral, got {:?}", other),
                }
            }
            other => panic!("expected RecordLiteral, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_field_access_on_record_literal() {
        let expr = parse("{x: 1, y: 2}.x").expect("parse should succeed");
        match expr {
            Expr::MemberAccess(rec, name) => {
                assert_eq!(name, "x");
                assert!(matches!(*rec, Expr::RecordLiteral(_)));
            }
            other => panic!("expected MemberAccess, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_duplicate_field_rejected() {
        let res = parse("{x: 1, x: 2}");
        assert!(res.is_err(), "expected duplicate-field error: {:?}", res);
    }

    #[test]
    fn test_parse_trailing_comma_rejected() {
        let res = parse("{x: 1, }");
        assert!(res.is_err(), "expected trailing-comma error: {:?}", res);
    }

    #[test]
    fn test_parse_missing_colon_rejected() {
        let res = parse("{x 1}");
        assert!(res.is_err(), "expected missing-colon error: {:?}", res);
    }

    #[test]
    fn test_parse_unclosed_record_rejected() {
        let res = parse("{x: 1, y: 2");
        assert!(res.is_err(), "expected unclosed-brace error: {:?}", res);
    }

    #[test]
    fn test_parse_record_literal_in_array() {
        let expr = parse("[{x: 1}, {x: 2}]").expect("parse should succeed");
        match expr {
            Expr::Array(elems) => {
                assert_eq!(elems.len(), 2);
                for e in &elems {
                    assert!(matches!(e, Expr::RecordLiteral(_)));
                }
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_anonymous_record_type_expr_after_empty_array() {
        // Inline anonymous record type expression in []TypeExpr position.
        let expr = parse("[]{x: Int, y: Int}").expect("parse should succeed");
        match expr {
            Expr::EmptyArray(DataType::Record(RecordType::Anonymous(fields))) => {
                // Anonymous canonicalizes (sorted by name); both fields are Int.
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "x");
                assert_eq!(fields[0].1, DataType::Int);
                assert_eq!(fields[1].0, "y");
                assert_eq!(fields[1].1, DataType::Int);
            }
            other => panic!("expected EmptyArray of anonymous record, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_named_record_type_expr_after_empty_array() {
        // Identifier in type position resolves first as a built-in, then as a
        // named-record reference. `Foo` is unknown → falls back to
        // `Record(Named("Foo"))`. Resolution of the dangling name happens at
        // the network layer.
        let expr = parse("[]Foo").expect("parse should succeed");
        match expr {
            Expr::EmptyArray(DataType::Record(RecordType::Named(n))) => {
                assert_eq!(n, "Foo");
            }
            other => panic!("expected EmptyArray of named record, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_empty_anonymous_record_type_expr() {
        // The empty record `{}` is the top of the record-subtype lattice.
        let expr = parse("[]{}").expect("parse should succeed");
        match expr {
            Expr::EmptyArray(DataType::Record(RecordType::Anonymous(fields))) => {
                assert!(fields.is_empty());
            }
            other => panic!("expected EmptyArray of empty record type, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_nested_record_type_expr() {
        // Anonymous record with a record-typed field.
        let expr = parse("[]{p: {x: Int, y: Int}}").expect("parse should succeed");
        match expr {
            Expr::EmptyArray(DataType::Record(RecordType::Anonymous(fields))) => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0, "p");
                match &fields[0].1 {
                    DataType::Record(RecordType::Anonymous(inner)) => {
                        assert_eq!(inner.len(), 2);
                    }
                    other => panic!("expected nested anonymous record, got {:?}", other),
                }
            }
            other => panic!("expected EmptyArray of anonymous record, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_anonymous_record_type_rejects_abstract_field() {
        // Abstract phase types are not legal element/field types in expr type
        // position (same rule as `[]HasAtoms`).
        let res = parse("[]{a: HasAtoms}");
        assert!(res.is_err(), "got {:?}", res);
    }
}

mod validation_tests {
    use super::*;

    fn validate(text: &str, vars: HashMap<String, DataType>) -> Result<DataType, String> {
        let expr = parse(text)?;
        expr.validate(&vars, get_function_signatures())
    }

    #[test]
    fn test_validate_simple_record_literal() {
        let ty = validate("{x: 1, y: 2}", HashMap::new()).expect("should validate");
        match ty {
            DataType::Record(RecordType::Anonymous(fields)) => {
                // Validation canonicalizes (sorted by name).
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "x");
                assert_eq!(fields[0].1, DataType::Int);
                assert_eq!(fields[1].0, "y");
                assert_eq!(fields[1].1, DataType::Int);
            }
            other => panic!("expected anonymous record type, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_record_literal_canonicalizes_order() {
        // Author order `{y, x}` validates to a canonicalized type `{x, y}` so
        // that derived `PartialEq` / `Hash` work — author order on the AST is
        // separate from type-level canonical order.
        // Note: the existing expr lexer collapses `2.0` to `Int(2)` (see
        // `Token::Number` handling in parser.rs); use `2.5` to keep the
        // float-ness through parsing.
        let ty = validate("{y: 2.5, x: 1}", HashMap::new()).expect("should validate");
        match ty {
            DataType::Record(RecordType::Anonymous(fields)) => {
                let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                assert_eq!(names, vec!["x", "y"]);
                assert_eq!(fields[0].1, DataType::Int);
                assert_eq!(fields[1].1, DataType::Float);
            }
            other => panic!("expected anonymous record type, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_empty_record_literal() {
        let ty = validate("{}", HashMap::new()).expect("should validate");
        match ty {
            DataType::Record(RecordType::Anonymous(fields)) => assert!(fields.is_empty()),
            other => panic!("expected empty anonymous record, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_field_access_returns_field_type() {
        let ty = validate("{x: 1, y: 2.5}.x", HashMap::new()).expect("should validate");
        assert_eq!(ty, DataType::Int);

        let ty = validate("{x: 1, y: 2.5}.y", HashMap::new()).expect("should validate");
        assert_eq!(ty, DataType::Float);
    }

    #[test]
    fn test_validate_nested_field_access() {
        let ty =
            validate("{outer: {inner: 1}}.outer.inner", HashMap::new()).expect("should validate");
        assert_eq!(ty, DataType::Int);
    }

    #[test]
    fn test_validate_field_access_unknown_field_rejected() {
        let res = validate("{x: 1}.y", HashMap::new());
        assert!(res.is_err(), "expected unknown-field error: {:?}", res);
    }

    #[test]
    fn test_validate_field_access_arithmetic() {
        // Round-trip from the design doc: `{x: 1, y: 2}.x + 1`.
        let ty = validate("{x: 1, y: 2}.x + 1", HashMap::new()).expect("should validate");
        assert_eq!(ty, DataType::Int);
    }

    #[test]
    fn test_validate_record_field_shadows_vector_member() {
        // A field literally named `x` on a record-typed receiver resolves
        // through the record's schema, NOT as a vector component. The field's
        // declared type is `Bool` here — which is impossible if `.x` were
        // routed through the Vec3 rule (that would force `Float`).
        let ty = validate("{x: true, y: 1, z: 2}.x", HashMap::new()).expect("should validate");
        assert_eq!(ty, DataType::Bool);
    }

    #[test]
    fn test_validate_record_takes_precedence_over_matrix_member() {
        // Same idea, but with a matrix-style accessor name `m00`. On a record
        // receiver `m00` is a normal field; this would fail under the matrix
        // rule because the receiver isn't a Mat3/IMat3.
        let ty = validate("{m00: 5}.m00", HashMap::new()).expect("should validate");
        assert_eq!(ty, DataType::Int);
    }

    #[test]
    fn test_validate_named_record_field_access_errors_clearly() {
        // Named-record field access cannot be resolved without a registry in
        // `Expr::validate`. The validator surfaces a clear error rather than
        // silently typing the access; resolution happens at the network
        // layer or via destructure nodes.
        let mut vars = HashMap::new();
        vars.insert(
            "r".to_string(),
            DataType::Record(RecordType::Named("Point".to_string())),
        );
        let res = validate("r.x", vars);
        assert!(res.is_err(), "expected named-record error: {:?}", res);
    }

    #[test]
    fn test_validate_anonymous_record_through_variable() {
        // A parameter typed as an anonymous record schema resolves field
        // access against the inline schema — no registry needed.
        let mut vars = HashMap::new();
        vars.insert(
            "p".to_string(),
            DataType::Record(RecordType::anonymous(vec![
                ("x".to_string(), DataType::Int),
                ("y".to_string(), DataType::Float),
            ])),
        );
        let ty = validate("p.x + 1", vars.clone()).expect("should validate");
        assert_eq!(ty, DataType::Int);
        let ty = validate("p.y", vars).expect("should validate");
        assert_eq!(ty, DataType::Float);
    }

    #[test]
    fn test_validate_array_of_record_literals() {
        let ty = validate("[{x: 1}, {x: 2}]", HashMap::new()).expect("should validate");
        match ty {
            DataType::Array(inner) => match *inner {
                DataType::Record(RecordType::Anonymous(fields)) => {
                    assert_eq!(fields.len(), 1);
                    assert_eq!(fields[0].0, "x");
                    assert_eq!(fields[0].1, DataType::Int);
                }
                other => panic!("expected record element type, got {:?}", other),
            },
            other => panic!("expected Array, got {:?}", other),
        }
    }
}

mod evaluation_tests {
    use super::*;

    fn evaluate(text: &str) -> NetworkResult {
        let expr = parse(text).expect("parse should succeed");
        expr.evaluate(&HashMap::new(), get_function_implementations())
    }

    #[test]
    fn test_evaluate_record_literal() {
        let v = evaluate("{x: 1, y: 2}");
        match v {
            NetworkResult::Record(fields) => {
                // NetworkResult::record(...) canonicalizes — sorted by name.
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "x");
                assert_eq!(fields[1].0, "y");
                assert!(matches!(fields[0].1, NetworkResult::Int(1)));
                assert!(matches!(fields[1].1, NetworkResult::Int(2)));
            }
            other => panic!("expected Record, got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_field_access_arithmetic() {
        // Round-trip from the design doc.
        let v = evaluate("{x: 1, y: 2}.x + 1");
        match v {
            NetworkResult::Int(n) => assert_eq!(n, 2),
            other => panic!("expected Int(2), got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_nested_field_access() {
        let v = evaluate("{outer: {inner: 7}}.outer.inner");
        match v {
            NetworkResult::Int(n) => assert_eq!(n, 7),
            other => panic!("expected Int(7), got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_record_canonicalizes_field_order() {
        let v = evaluate("{z: 3, a: 1, m: 2}");
        match v {
            NetworkResult::Record(fields) => {
                let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                assert_eq!(names, vec!["a", "m", "z"]);
            }
            other => panic!("expected Record, got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_record_field_shadows_vector_member() {
        // `.x` on a record receiver resolves to the record field even though
        // the field happens to be named the same as a Vec3 component.
        let v = evaluate("{x: true, y: 1, z: 2}.x");
        match v {
            NetworkResult::Bool(b) => assert!(b),
            other => panic!("expected Bool, got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_empty_record_literal() {
        let v = evaluate("{}");
        match v {
            NetworkResult::Record(fields) => assert!(fields.is_empty()),
            other => panic!("expected Record, got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_field_access_returns_typed_value() {
        let v = evaluate("{x: 1, y: 2.5}.y");
        match v {
            NetworkResult::Float(f) => assert!((f - 2.5).abs() < f64::EPSILON),
            other => panic!("expected Float, got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_array_of_records() {
        let v = evaluate("[{x: 1}, {x: 2}, {x: 3}]");
        match v {
            NetworkResult::Array(items) => {
                assert_eq!(items.len(), 3);
                for item in items {
                    assert!(matches!(item, NetworkResult::Record(_)));
                }
            }
            other => panic!("expected Array, got {:?}", other.infer_data_type()),
        }
    }

    #[test]
    fn test_evaluate_anonymous_record_type_matches_named_def_structurally() {
        // Phase 7 design: an anonymous record-type expression
        // (`{x: Int, y: Int}`) parses and matches structurally against a
        // named def with the same shape. Compatibility (subtyping) is
        // implemented by `DataType::can_be_converted_to` (Phase 4) — so
        // here we drive that through the parsed types directly.
        use rust_lib_flutter_cad::structure_designer::node_type_registry::{
            NodeTypeRegistry, RecordTypeDef,
        };

        // Parse `{x: Int, y: Int}` from an `expr`-style type position by
        // wrapping it in a `[]<TypeExpr>` literal.
        let parsed = match parse("[]{x: Int, y: Int}").expect("parse") {
            Expr::EmptyArray(DataType::Array(inner)) => *inner,
            Expr::EmptyArray(t) => t,
            other => panic!("unexpected: {:?}", other),
        };

        // Build a registry that defines `Point = {x: Int, y: Int}` and ask
        // whether the parsed anonymous schema is bidirectionally compatible
        // with the named def.
        let mut registry = NodeTypeRegistry::default();
        registry.record_type_defs.insert(
            "Point".to_string(),
            RecordTypeDef {
                name: "Point".to_string(),
                fields: vec![
                    ("x".to_string(), DataType::Int),
                    ("y".to_string(), DataType::Int),
                ],
            },
        );

        let named = DataType::Record(RecordType::Named("Point".to_string()));
        assert!(
            DataType::can_be_converted_to(&parsed, &named, &registry),
            "anonymous → named should pass width subtyping"
        );
        assert!(
            DataType::can_be_converted_to(&named, &parsed, &registry),
            "named → anonymous should pass width subtyping"
        );
    }
}
