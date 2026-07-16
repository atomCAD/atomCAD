//! Phases 1–2 of `doc/design_array_node_and_field_hints.md` (Part A) —
//! record-def field **editor hints**: a purely presentational annotation telling
//! a generic literal editor which widget to render.
//!
//! The load-bearing property under test is the invariant that hints are
//! **cosmetic**: they must be rejected at every def-mutation site when
//! ill-formed, but must never fail a `.cnnd` load — a corrupt or
//! future-versioned hint drops and the file opens. The rest is plumbing: the
//! serde shape (2-element entries stay byte-identical, hinted fields become
//! 3-element) and the hints reaching `APILiteralField` through
//! `lookup_record_type_def`.

use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    FieldEditorHint, FieldId, NodeTypeRegistry, RecordFieldEdit, RecordTypeDef, RecordTypeDefError,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

fn hinted(
    name: &str,
    ty: DataType,
    hint: Option<FieldEditorHint>,
) -> (String, DataType, Option<FieldEditorHint>) {
    (name.to_string(), ty, hint)
}

fn opt(ty: DataType) -> DataType {
    DataType::Optional(Box::new(ty))
}

fn enum_hint() -> FieldEditorHint {
    FieldEditorHint::Enum(vec!["a".to_string(), "b".to_string()])
}

fn range_hint() -> FieldEditorHint {
    FieldEditorHint::Range { min: 0.0, max: 1.0 }
}

/// Add a one-field def carrying `hint` on a field of type `ty`.
fn try_add_hinted_def(
    registry: &mut NodeTypeRegistry,
    ty: DataType,
    hint: FieldEditorHint,
) -> Result<(), RecordTypeDefError> {
    let mut def = RecordTypeDef::from_named_fields("R", vec![("f".to_string(), ty)]);
    // Set the hint directly rather than through `from_hinted_fields`, which
    // *drops* ill-formed hints (it is the load path). We want the strict
    // add-time gate under test here.
    def.fields[0].hint = Some(hint);
    registry.add_record_type_def(def)
}

// ---------------------------------------------------------------------------
// Applicability — every valid (hint, type) pair, including through Optional
// ---------------------------------------------------------------------------

#[test]
fn every_valid_hint_type_pair_is_accepted_including_through_optional() {
    let cases: Vec<(DataType, FieldEditorHint)> = vec![
        (DataType::Int, FieldEditorHint::Element),
        (opt(DataType::Int), FieldEditorHint::Element),
        (DataType::Vec3, FieldEditorHint::Color),
        (opt(DataType::Vec3), FieldEditorHint::Color),
        (DataType::String, enum_hint()),
        (opt(DataType::String), enum_hint()),
        (DataType::Float, range_hint()),
        (opt(DataType::Float), range_hint()),
        (DataType::Int, range_hint()),
        (opt(DataType::Int), range_hint()),
    ];
    for (ty, hint) in cases {
        let mut registry = NodeTypeRegistry::new();
        let result = try_add_hinted_def(&mut registry, ty.clone(), hint.clone());
        assert!(
            result.is_ok(),
            "hint {:?} must be accepted on field type {}: {:?}",
            hint,
            ty,
            result
        );
        assert_eq!(
            registry.lookup_record_type_def("R").unwrap().fields[0].hint,
            Some(hint),
            "the accepted hint must be stored verbatim",
        );
    }
}

#[test]
fn mismatched_hint_type_pairs_are_rejected_with_a_clear_error() {
    let cases: Vec<(DataType, FieldEditorHint)> = vec![
        (DataType::String, FieldEditorHint::Element),
        (DataType::Float, FieldEditorHint::Element),
        (DataType::Vec3, FieldEditorHint::Element),
        (DataType::Int, FieldEditorHint::Color),
        (DataType::Vec2, FieldEditorHint::Color),
        (DataType::Int, enum_hint()),
        (opt(DataType::Bool), enum_hint()),
        (DataType::String, range_hint()),
        (DataType::Vec3, range_hint()),
        // The hint describes the *inner* value, so a hint on an Array of the
        // right leaf type is still a mismatch.
        (
            DataType::Array(Box::new(DataType::Int)),
            FieldEditorHint::Element,
        ),
        // Optional is peeled exactly one level — a hint never reaches deeper.
        (
            opt(DataType::Array(Box::new(DataType::Int))),
            FieldEditorHint::Element,
        ),
    ];
    for (ty, hint) in cases {
        let mut registry = NodeTypeRegistry::new();
        let err = try_add_hinted_def(&mut registry, ty.clone(), hint.clone())
            .expect_err(&format!("hint {:?} must be rejected on {}", hint, ty));
        match &err {
            RecordTypeDefError::IllFormedHint(def, field, message) => {
                assert_eq!(def, "R");
                assert_eq!(field, "f");
                assert!(
                    message.contains("does not apply"),
                    "message should name the mismatch, got: {message}"
                );
            }
            other => panic!("expected IllFormedHint, got {other:?}"),
        }
        assert!(
            registry.lookup_record_type_def("R").is_none(),
            "a rejected def must not be inserted",
        );
    }
}

// ---------------------------------------------------------------------------
// Hint well-formedness — Enum list rules, Range bounds
// ---------------------------------------------------------------------------

#[test]
fn enum_list_rules_are_enforced() {
    let bad: Vec<(FieldEditorHint, &str)> = vec![
        (FieldEditorHint::Enum(vec![]), "at least one entry"),
        (
            FieldEditorHint::Enum(vec!["a".to_string(), String::new()]),
            "must not be empty",
        ),
        (FieldEditorHint::Enum(vec![" a".to_string()]), "whitespace"),
        (FieldEditorHint::Enum(vec!["a ".to_string()]), "whitespace"),
        (
            FieldEditorHint::Enum(vec!["a".to_string(), "a".to_string()]),
            "duplicate",
        ),
    ];
    for (hint, expected_fragment) in bad {
        let mut registry = NodeTypeRegistry::new();
        let err = try_add_hinted_def(&mut registry, DataType::String, hint.clone())
            .expect_err(&format!("{hint:?} must be rejected"));
        let RecordTypeDefError::IllFormedHint(_, _, message) = &err else {
            panic!("expected IllFormedHint, got {err:?}");
        };
        assert!(
            message.contains(expected_fragment),
            "message '{message}' should mention '{expected_fragment}'"
        );
    }
}

#[test]
fn range_requires_min_below_max_and_finite_bounds() {
    let bad = vec![
        FieldEditorHint::Range { min: 1.0, max: 1.0 },
        FieldEditorHint::Range { min: 2.0, max: 1.0 },
        FieldEditorHint::Range {
            min: f64::NAN,
            max: 1.0,
        },
        FieldEditorHint::Range {
            min: 0.0,
            max: f64::INFINITY,
        },
    ];
    for hint in bad {
        let mut registry = NodeTypeRegistry::new();
        try_add_hinted_def(&mut registry, DataType::Float, hint.clone())
            .expect_err(&format!("{hint:?} must be rejected"));
    }
}

// ---------------------------------------------------------------------------
// update_record_type_def{,_with_edits}
// ---------------------------------------------------------------------------

#[test]
fn update_with_edits_accepts_a_valid_hint_and_rejects_a_mismatched_one() {
    let mut registry = NodeTypeRegistry::new();
    registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "R",
            vec![("f".to_string(), DataType::Int)],
        ))
        .unwrap();
    let id = registry.lookup_record_type_def("R").unwrap().fields[0].id;

    registry
        .update_record_type_def_with_edits(
            "R",
            vec![RecordFieldEdit {
                id: Some(id),
                name: "f".to_string(),
                data_type: DataType::Int,
                hint: Some(FieldEditorHint::Element),
            }],
        )
        .expect("Element on Int is valid");
    assert_eq!(
        registry.lookup_record_type_def("R").unwrap().fields[0].hint,
        Some(FieldEditorHint::Element)
    );

    // Retyping the field while keeping the now-stale hint in the same update is
    // rejected, and nothing is committed (the def keeps its Int field).
    let err = registry
        .update_record_type_def_with_edits(
            "R",
            vec![RecordFieldEdit {
                id: Some(id),
                name: "f".to_string(),
                data_type: DataType::String,
                hint: Some(FieldEditorHint::Element),
            }],
        )
        .expect_err("Element on String must be rejected");
    assert!(matches!(err, RecordTypeDefError::IllFormedHint(..)));
    let def = registry.lookup_record_type_def("R").unwrap();
    assert_eq!(def.fields[0].data_type, DataType::Int, "no partial commit");
    assert_eq!(def.fields[0].hint, Some(FieldEditorHint::Element));
}

#[test]
fn hint_free_update_preserves_a_still_applicable_hint_and_drops_a_stale_one() {
    let mut registry = NodeTypeRegistry::new();
    let mut def = RecordTypeDef::from_named_fields(
        "R",
        vec![
            ("keep".to_string(), DataType::Int),
            ("retype".to_string(), DataType::Int),
        ],
    );
    def.fields[0].hint = Some(FieldEditorHint::Element);
    def.fields[1].hint = Some(FieldEditorHint::Element);
    registry.add_record_type_def(def).unwrap();

    // `update_record_type_def` cannot express hints, so a surviving field keeps
    // its own — unless the new type makes it inapplicable, in which case it is
    // dropped rather than erroring (the caller never mentioned it).
    registry
        .update_record_type_def(
            "R",
            vec![
                ("keep".to_string(), DataType::Int),
                ("retype".to_string(), DataType::String),
            ],
        )
        .expect("a hint-free update must not fail on hint state it cannot see");
    let def = registry.lookup_record_type_def("R").unwrap();
    assert_eq!(def.fields[0].hint, Some(FieldEditorHint::Element));
    assert_eq!(def.fields[1].hint, None, "stale hint dropped on retype");
}

// ---------------------------------------------------------------------------
// Persistence — §Persistence of the design doc
// ---------------------------------------------------------------------------

#[test]
fn hint_free_save_is_byte_identical_to_the_pre_feature_format() {
    let def = RecordTypeDef::from_named_fields(
        "R",
        vec![
            ("a".to_string(), DataType::Int),
            ("b".to_string(), DataType::Float),
        ],
    );
    let json = serde_json::to_string(&def).unwrap();
    assert_eq!(
        json, r#"{"name":"R","fields":[["a","Int"],["b","Float"]]}"#,
        "a hint-free def must serialize to the exact pre-hint 2-element shape"
    );
}

#[test]
fn hinted_def_round_trips_as_three_element_entries() {
    let def = RecordTypeDef::from_hinted_fields(
        "R",
        vec![
            hinted("e", DataType::Int, Some(FieldEditorHint::Element)),
            hinted("c", DataType::Vec3, Some(FieldEditorHint::Color)),
            hinted("s", DataType::String, Some(enum_hint())),
            hinted("r", DataType::Float, Some(range_hint())),
            hinted("plain", DataType::Bool, None),
        ],
    );
    let json = serde_json::to_string(&def).unwrap();
    assert!(
        json.contains(r#"["e","Int","Element"]"#),
        "hinted fields are 3-element entries: {json}"
    );
    assert!(
        json.contains(r#"["plain","Bool"]"#),
        "hint-free fields stay 2-element: {json}"
    );

    let back: RecordTypeDef = serde_json::from_str(&json).unwrap();
    assert_eq!(back, def, "hints round-trip through the .cnnd shape");
}

#[test]
fn old_file_with_two_element_entries_loads_hint_free() {
    let old_format = r#"{ "name": "R", "fields": [ ["a", "Int"], ["b", "Vec3"] ] }"#;
    let def: RecordTypeDef = serde_json::from_str(old_format).unwrap();
    assert!(def.fields.iter().all(|f| f.hint.is_none()));
    assert_eq!(def.fields[0].data_type, DataType::Int);
    assert_eq!(def.fields[1].data_type, DataType::Vec3);
}

#[test]
fn hand_corrupted_mismatched_hint_loads_with_the_hint_dropped() {
    // `Element` on a String field — ill-formed. Cosmetic data must never brick
    // a project file, so this loads (hint-free) rather than failing.
    let corrupted = r#"{ "name": "R", "fields": [ ["a", "String", "Element"] ] }"#;
    let def: RecordTypeDef = serde_json::from_str(corrupted).unwrap();
    assert_eq!(def.fields[0].data_type, DataType::String);
    assert_eq!(
        def.fields[0].hint, None,
        "the hint is dropped, not the file"
    );
}

#[test]
fn future_hint_kind_loads_with_the_hint_dropped_via_the_ignored_any_fallback() {
    // A third element this version cannot parse (a hint kind from a newer
    // version). The `IgnoredAny` last-resort wire variant swallows it.
    let future = r#"{ "name": "R", "fields": [ ["a", "Int", {"Gradient": {"stops": 4}}] ] }"#;
    let def: RecordTypeDef = serde_json::from_str(future).unwrap();
    assert_eq!(def.fields[0].name, "a");
    assert_eq!(def.fields[0].data_type, DataType::Int);
    assert_eq!(def.fields[0].hint, None);
}

#[test]
fn a_malformed_field_entry_is_still_a_load_error() {
    // The IgnoredAny fallback must not swallow *everything* — an entry whose
    // type is unparseable is genuine corruption, not a cosmetic annotation.
    let broken = r#"{ "name": "R", "fields": [ ["a", "NotAType"] ] }"#;
    assert!(serde_json::from_str::<RecordTypeDef>(broken).is_err());
}

// ---------------------------------------------------------------------------
// The shipped built-in annotations
// ---------------------------------------------------------------------------

#[test]
fn element_mapping_declares_element_hints_on_both_fields() {
    let registry = NodeTypeRegistry::new();
    let def = registry
        .lookup_record_type_def("ElementMapping")
        .expect("ElementMapping is a built-in def");
    for field in &def.fields {
        assert_eq!(
            field.hint,
            Some(FieldEditorHint::Element),
            "ElementMapping.{} should render an element dropdown",
            field.name
        );
    }
}

#[test]
fn style_rule_declares_all_four_hint_kinds() {
    let registry = NodeTypeRegistry::new();
    let def = registry
        .lookup_record_type_def("StyleRule")
        .expect("StyleRule is a built-in def");
    let hint = |name: &str| {
        def.fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("StyleRule has no field '{name}'"))
            .hint
            .clone()
    };
    assert_eq!(hint("element"), Some(FieldEditorHint::Element));
    assert_eq!(hint("color"), Some(FieldEditorHint::Color));
    assert_eq!(
        hint("alpha"),
        Some(FieldEditorHint::Range { min: 0.0, max: 1.0 })
    );
    assert_eq!(
        hint("render_style"),
        Some(FieldEditorHint::Enum(vec![
            "ball_and_stick".to_string(),
            "space_filling".to_string(),
            "default".to_string(),
        ])),
        "the Enum must list exactly the strings apply_style accepts",
    );
    assert_eq!(
        hint("tag"),
        None,
        "tag stays unhinted — the useful choices are runtime context",
    );
}

// ---------------------------------------------------------------------------
// The invariant: hints never touch the type system
// ---------------------------------------------------------------------------

#[test]
fn hints_do_not_affect_record_conversion() {
    let mut registry = NodeTypeRegistry::new();
    // Two structurally identical defs — one hinted, one not.
    registry
        .add_record_type_def(RecordTypeDef::from_hinted_fields(
            "Hinted",
            vec![hinted("f", DataType::Int, Some(FieldEditorHint::Element))],
        ))
        .unwrap();
    registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "Plain",
            vec![("f".to_string(), DataType::Int)],
        ))
        .unwrap();

    let hinted_ty = DataType::Record(RecordType::Named("Hinted".to_string()));
    let plain_ty = DataType::Record(RecordType::Named("Plain".to_string()));
    assert!(
        DataType::can_be_converted_to(&hinted_ty, &plain_ty, &registry),
        "a hint must not gate conversion"
    );
    assert!(
        DataType::can_be_converted_to(&plain_ty, &hinted_ty, &registry),
        "a hint must not gate conversion in either direction"
    );
}

// ---------------------------------------------------------------------------
// Phase 2 — the schema-editor path (`StructureDesigner::update_record_type_def_with_ids`)
//
// Phase 1's API row could not carry a hint, so the FRB shim re-attached each
// surviving field's hint by `FieldId` behind the caller's back. Phase 2 gives
// the row a `hint`, which makes the caller's list **authoritative** in both
// directions: a hint it names is written, and a hint it omits is *cleared*
// rather than resurrected. These tests pin that authority — and the fact that
// a rejected update commits nothing — at the level the shim delegates to.
// (The shim itself is thin plumbing; `rust/AGENTS.md` exempts it from tests.)
// ---------------------------------------------------------------------------

/// Fresh designer holding a one-field user def named `R`, plus that field's id.
fn designer_with_field(ty: DataType) -> (StructureDesigner, FieldId) {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "R",
            vec![("f".to_string(), ty)],
        ))
        .unwrap();
    let id = designer
        .node_type_registry
        .lookup_record_type_def("R")
        .unwrap()
        .fields[0]
        .id;
    (designer, id)
}

fn hint_of(designer: &StructureDesigner) -> Option<FieldEditorHint> {
    designer
        .node_type_registry
        .lookup_record_type_def("R")
        .unwrap()
        .fields[0]
        .hint
        .clone()
}

#[test]
fn schema_update_round_trips_every_hint_kind() {
    for (ty, hint) in [
        (DataType::Int, FieldEditorHint::Element),
        (DataType::Vec3, FieldEditorHint::Color),
        (DataType::String, enum_hint()),
        (DataType::Float, range_hint()),
        // Through an Optional wrapper — the hint describes the inner value.
        (opt(DataType::Int), FieldEditorHint::Element),
    ] {
        let (mut designer, id) = designer_with_field(ty.clone());
        designer
            .update_record_type_def_with_ids(
                "R",
                vec![RecordFieldEdit {
                    id: Some(id),
                    name: "f".to_string(),
                    data_type: ty.clone(),
                    hint: Some(hint.clone()),
                }],
            )
            .unwrap_or_else(|e| panic!("{:?} on {} must be accepted: {}", hint, ty, e));
        assert_eq!(hint_of(&designer), Some(hint.clone()), "on a {} field", ty);
    }
}

#[test]
fn schema_update_clears_a_hint_the_row_omits() {
    let (mut designer, id) = designer_with_field(DataType::Int);
    designer
        .update_record_type_def_with_ids(
            "R",
            vec![RecordFieldEdit {
                id: Some(id),
                name: "f".to_string(),
                data_type: DataType::Int,
                hint: Some(FieldEditorHint::Element),
            }],
        )
        .unwrap();

    // The row still names the same field (same id, same type) but no longer
    // names a hint — that is the user picking "None" in the hint dropdown, and
    // it must stick. Phase 1's carry-across would have re-attached Element here.
    designer
        .update_record_type_def_with_ids(
            "R",
            vec![RecordFieldEdit {
                id: Some(id),
                name: "f".to_string(),
                data_type: DataType::Int,
                hint: None,
            }],
        )
        .unwrap();
    assert_eq!(
        hint_of(&designer),
        None,
        "an omitted hint is a cleared hint"
    );
}

#[test]
fn schema_update_rejects_a_mismatched_hint_and_commits_nothing() {
    let (mut designer, id) = designer_with_field(DataType::Int);

    // Retyping Int -> String while keeping the stale Element hint. Reachable
    // only from a direct API caller (the SchemaEditor clears the hint in the
    // same update it sends), so it is a hard reject, not a silent drop.
    let err = designer
        .update_record_type_def_with_ids(
            "R",
            vec![RecordFieldEdit {
                id: Some(id),
                name: "f".to_string(),
                data_type: DataType::String,
                hint: Some(FieldEditorHint::Element),
            }],
        )
        .expect_err("Element on String must be rejected");
    assert!(
        matches!(err, RecordTypeDefError::IllFormedHint(..)),
        "got {:?}",
        err
    );
    assert!(
        err.to_string().contains("Element"),
        "the error must name the offending hint: {}",
        err
    );

    let def = designer
        .node_type_registry
        .lookup_record_type_def("R")
        .unwrap();
    assert_eq!(def.fields[0].data_type, DataType::Int, "no partial commit");
    assert_eq!(def.fields[0].hint, None);
}

#[test]
fn schema_update_rejects_an_ill_formed_enum_list() {
    let (mut designer, id) = designer_with_field(DataType::String);
    let err = designer
        .update_record_type_def_with_ids(
            "R",
            vec![RecordFieldEdit {
                id: Some(id),
                name: "f".to_string(),
                data_type: DataType::String,
                hint: Some(FieldEditorHint::Enum(vec![
                    "a".to_string(),
                    "a".to_string(),
                ])),
            }],
        )
        .expect_err("a duplicate Enum entry must be rejected");
    assert!(matches!(err, RecordTypeDefError::IllFormedHint(..)));
    assert_eq!(hint_of(&designer), None, "no partial commit");
}

/// Hints ride on `FieldId`, so a rename — the one edit that changes a field's
/// *name* identity — must carry them across untouched.
#[test]
fn schema_update_keeps_the_hint_across_a_field_rename() {
    let (mut designer, id) = designer_with_field(DataType::Int);
    designer
        .update_record_type_def_with_ids(
            "R",
            vec![RecordFieldEdit {
                id: Some(id),
                name: "renamed".to_string(),
                data_type: DataType::Int,
                hint: Some(FieldEditorHint::Element),
            }],
        )
        .unwrap();
    let def = designer
        .node_type_registry
        .lookup_record_type_def("R")
        .unwrap();
    assert_eq!(def.fields[0].name, "renamed");
    assert_eq!(def.fields[0].id, id, "rename keeps the editing identity");
    assert_eq!(def.fields[0].hint, Some(FieldEditorHint::Element));
}
