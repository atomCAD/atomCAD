//! `DataType::from_string` reads anonymous record types — the `{a: T, ..}`
//! spelling `Display` prints for `RecordType::Anonymous`
//! (`doc/design_wire_declared_inputs.md`, Phase 0).

use atomcad_structure_designer::data_type::{DataType, FunctionType, RecordType};

fn anon(fields: &[(&str, DataType)]) -> DataType {
    DataType::Record(RecordType::anonymous(
        fields
            .iter()
            .map(|(n, t)| (n.to_string(), t.clone()))
            .collect(),
    ))
}

/// `t` prints as `printed`, and `printed` parses back to `t`.
fn assert_round_trip(t: DataType, printed: &str) {
    assert_eq!(t.to_string(), printed);
    assert_eq!(
        DataType::from_string(printed).unwrap_or_else(|e| panic!("{printed}: {e}")),
        t
    );
}

#[test]
fn empty_record_round_trips() {
    assert_round_trip(anon(&[]), "{}");
}

#[test]
fn flat_record_round_trips() {
    assert_round_trip(
        anon(&[("x", DataType::Int), ("y", DataType::Float)]),
        "{x: Int, y: Float}",
    );
}

#[test]
fn nested_record_round_trips() {
    assert_round_trip(
        anon(&[("a", DataType::Float), ("p", anon(&[("x", DataType::Int)]))]),
        "{a: Float, p: {x: Int}}",
    );
}

#[test]
fn array_of_records_round_trips() {
    assert_round_trip(
        DataType::Array(Box::new(anon(&[("a", DataType::Int)]))),
        "[{a: Int}]",
    );
}

#[test]
fn record_fields_hold_any_type() {
    assert_round_trip(
        anon(&[
            (
                "box",
                DataType::Record(RecordType::Named("Box".to_string())),
            ),
            ("items", DataType::Array(Box::new(DataType::Vec3))),
            ("opt", DataType::Optional(Box::new(DataType::Int))),
        ]),
        "{box: Record(Box), items: [Vec3], opt: Optional[Int]}",
    );
}

#[test]
fn function_type_with_a_record_parameter_round_trips() {
    let rec = anon(&[("a", DataType::Float), ("b", DataType::Int)]);
    assert_round_trip(
        DataType::Function(FunctionType::new(vec![rec.clone()], DataType::Float)),
        "{a: Float, b: Int} -> Float",
    );
    assert_round_trip(
        DataType::Function(FunctionType::new(
            vec![rec.clone(), DataType::Int],
            rec.clone(),
        )),
        "({a: Float, b: Int},Int) -> {a: Float, b: Int}",
    );
    // A record whose field is itself a function.
    assert_round_trip(
        anon(&[(
            "f",
            DataType::Function(FunctionType::new(vec![DataType::Int], DataType::Int)),
        )]),
        "{f: Int -> Int}",
    );
}

#[test]
fn field_order_canonicalizes() {
    assert_eq!(
        DataType::from_string("{b: Int, a: Int}").unwrap(),
        DataType::from_string("{a: Int, b: Int}").unwrap()
    );
    assert_eq!(
        DataType::from_string("{b: Int, a: Int}")
            .unwrap()
            .to_string(),
        "{a: Int, b: Int}"
    );
}

#[test]
fn whitespace_is_insignificant() {
    assert_eq!(
        DataType::from_string("{ a :Int ,b: { c:Float } }").unwrap(),
        anon(&[("a", DataType::Int), ("b", anon(&[("c", DataType::Float)])),])
    );
}

#[test]
fn duplicate_field_names_are_rejected() {
    let err = DataType::from_string("{a: Int, a: Float}").unwrap_err();
    assert!(err.contains("Duplicate field name"), "{err}");
    assert!(DataType::from_string("{p: {x: Int, x: Int}}").is_err());
}

#[test]
fn malformed_records_are_rejected() {
    for bad in [
        "{",
        "{a: Int",
        "{a Int}",
        "{a:}",
        "{: Int}",
        "{a: Int,}",
        "{a: Int b: Int}",
        "{1a: Int}",
        "{`a b`: Int}",
        "{a: Int}}",
        "}",
        ":",
    ] {
        assert!(
            DataType::from_string(bad).is_err(),
            "`{bad}` should not parse"
        );
    }
}
