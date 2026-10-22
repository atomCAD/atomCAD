//! Field access on named records in `expr`
//! (`doc/design_anonymous_record_readers.md`, Part 3 / Phase 4): `r.x` on a
//! `Record(Named(Box))` resolves through the record defs, and without them it
//! is *pending* — never an error — through every arm of the validator. The
//! node-level half (repair, load, text edits) is in
//! `tests/structure_designer/expr_named_record_node_test.rs`.

use atomcad_structure_designer::data_type::{DataType, RecordType};
use atomcad_structure_designer::expr::expr::ExprTypeError;
use atomcad_structure_designer::expr::parser::parse;
use atomcad_structure_designer::expr::validation::get_function_signatures;
use atomcad_structure_designer::node_type_registry::{RecordDefs, RecordTypeDef};
use std::collections::HashMap;

/// `Box = { x: Float, n: Int, tag: String, flag: Optional[Bool] }`.
fn user_defs() -> HashMap<String, RecordTypeDef> {
    let mut defs = HashMap::new();
    defs.insert(
        "Box".to_string(),
        RecordTypeDef::from_named_fields(
            "Box",
            vec![
                ("x".to_string(), DataType::Float),
                ("n".to_string(), DataType::Int),
                ("tag".to_string(), DataType::String),
                (
                    "flag".to_string(),
                    DataType::Optional(Box::new(DataType::Bool)),
                ),
            ],
        ),
    );
    defs
}

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

/// The variables every case validates against: `r: Box`.
fn variables() -> HashMap<String, DataType> {
    HashMap::from([("r".to_string(), named("Box"))])
}

fn validate(text: &str, defs: Option<RecordDefs<'_>>) -> Result<DataType, ExprTypeError> {
    parse(text)
        .expect("parse")
        .validate_with_defs(&variables(), get_function_signatures(), defs)
}

fn invalid(result: Result<DataType, ExprTypeError>) -> String {
    match result {
        Err(ExprTypeError::Invalid(message)) => message,
        other => panic!("expected an error, got {:?}", other),
    }
}

/// Every operator, call and literal over `r.x`.
const COMPOUNDS: &[&str] = &[
    "r.x",
    "r.x * 2",
    "2 * r.x",
    "-r.x",
    "sqrt(r.x)",
    "if r.x > 0 then 1 else 2",
    "if true then r.x else 2.0",
    "{q: r.x}",
    "[r.x, 1.0]",
    "max(r.x, 1)",
    "`${r.x}`",
    "{q: r}.q.x",
];

#[test]
fn without_defs_a_named_field_access_is_pending_through_every_arm() {
    for text in COMPOUNDS {
        assert_eq!(
            validate(text, None),
            Err(ExprTypeError::Pending),
            "{}",
            text
        );
    }
}

#[test]
fn with_defs_the_same_expressions_type() {
    let user = user_defs();
    let built_in = HashMap::new();
    let defs = Some(RecordDefs::new(&user, &built_in));
    let expected = [
        DataType::Float,
        DataType::Float,
        DataType::Float,
        DataType::Float,
        DataType::Float,
        DataType::Int,
        DataType::Float,
        DataType::Record(RecordType::anonymous(vec![(
            "q".to_string(),
            DataType::Float,
        )])),
        DataType::Array(Box::new(DataType::Float)),
        DataType::Float,
        DataType::String,
        DataType::Float,
    ];
    for (text, expected) in COMPOUNDS.iter().zip(expected) {
        assert_eq!(validate(text, defs), Ok(expected), "{}", text);
    }
    assert_eq!(validate("r.n + 1", defs), Ok(DataType::Int));
}

#[test]
fn with_defs_a_wrongly_typed_field_gives_the_real_error() {
    let user = user_defs();
    let built_in = HashMap::new();
    let defs = Some(RecordDefs::new(&user, &built_in));
    assert!(invalid(validate("r.tag * 2", defs)).contains("not supported for types"));
    assert!(invalid(validate("sqrt(r.tag)", defs)).contains("expects type"));
    assert!(invalid(validate("if r.tag then 1 else 2", defs)).contains("must be boolean or int"));
    // ... and the same expressions are pending without the defs.
    for text in ["r.tag * 2", "sqrt(r.tag)", "if r.tag then 1 else 2"] {
        assert_eq!(
            validate(text, None),
            Err(ExprTypeError::Pending),
            "{}",
            text
        );
    }
}

#[test]
fn a_pending_operand_does_not_hide_an_error_elsewhere() {
    // The error does not depend on the pending field, so it is reported.
    assert!(invalid(validate("undefined_name + r.x", None)).contains("Unknown variable"));
}

#[test]
fn a_missing_field_names_the_def() {
    let user = user_defs();
    let built_in = HashMap::new();
    let defs = Some(RecordDefs::new(&user, &built_in));
    assert_eq!(
        invalid(validate("r.q", defs)),
        "Record `Box` has no field 'q'"
    );
}

#[test]
fn an_unknown_def_is_an_error_naming_it() {
    let user = user_defs();
    let built_in = HashMap::new();
    let defs = Some(RecordDefs::new(&user, &built_in));
    let variables = HashMap::from([("r".to_string(), named("Nope"))]);
    let result =
        parse("r.x")
            .unwrap()
            .validate_with_defs(&variables, get_function_signatures(), defs);
    assert_eq!(invalid(result), "Unknown record type `Nope`");
}

#[test]
fn an_optional_field_reads_as_its_inner_type() {
    let user = user_defs();
    let built_in = HashMap::new();
    let defs = Some(RecordDefs::new(&user, &built_in));
    assert_eq!(validate("r.flag", defs), Ok(DataType::Bool));
}

#[test]
fn a_built_in_def_resolves_too() {
    let user = HashMap::new();
    let mut built_in = HashMap::new();
    built_in.insert(
        "Pair".to_string(),
        RecordTypeDef::from_named_fields(
            "Pair",
            vec![
                ("from".to_string(), DataType::Int),
                ("to".to_string(), DataType::Int),
            ],
        ),
    );
    let variables = HashMap::from([("p".to_string(), named("Pair"))]);
    let result = parse("p.to - p.from").unwrap().validate_with_defs(
        &variables,
        get_function_signatures(),
        Some(RecordDefs::new(&user, &built_in)),
    );
    assert_eq!(result, Ok(DataType::Int));
}

#[test]
fn the_defs_free_validate_reports_a_named_field_access() {
    // `validate` is the convenience form for callers that never meet a named
    // record; it has nowhere to put "pending", so it reports it.
    let result = parse("r.x")
        .unwrap()
        .validate(&variables(), get_function_signatures());
    assert!(result.unwrap_err().contains("without the record defs"));
}
