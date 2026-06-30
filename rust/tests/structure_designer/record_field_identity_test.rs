//! Phase R1 of `doc/design_record_field_identity.md` — forward-looking unit
//! tests for the field-identity *infrastructure* (no wire-behaviour change yet;
//! the input-pin builders still emit `id: None`). They cover the three things
//! R1 introduces:
//!
//!   1. the `FieldId` allocator discipline (allocate-then-bump, never recycle),
//!   2. the per-def Phase-0 invariants (`DuplicateFieldId` / `FieldIdFloor`),
//!   3. the on-load id assignment / `next_field_id` floor recompute, with the
//!      `.cnnd` on-disk shape unchanged (no ids, no `next_field_id`).
//!
//! R1 has no pre-existing behaviour to break, so there is no red-first step
//! here (see the design doc §5/§7).

use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::invariants::{
    InvariantKind, check_document_invariants,
};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef,
};

fn field(name: &str, ty: DataType) -> (String, DataType) {
    (name.to_string(), ty)
}

// ---------------------------------------------------------------------------
// Constructor / allocator discipline
// ---------------------------------------------------------------------------

#[test]
fn from_named_fields_assigns_sequential_ids_and_floors_counter() {
    let def = RecordTypeDef::from_named_fields(
        "R",
        vec![
            field("a", DataType::Int),
            field("b", DataType::Float),
            field("c", DataType::Bool),
        ],
    );
    let ids: Vec<u64> = def.fields.iter().map(|f| f.id.0).collect();
    assert_eq!(ids, vec![0, 1, 2], "ids assigned in authored order");
    assert_eq!(def.next_field_id, 3, "counter floors at max(id)+1 = len");
}

#[test]
fn allocate_field_id_bumps_monotonically_and_never_recycles() {
    let mut def = RecordTypeDef::new("R");
    assert_eq!(def.next_field_id, 0);
    let a = def.allocate_field_id();
    let b = def.allocate_field_id();
    let c = def.allocate_field_id();
    assert_eq!((a.0, b.0, c.0), (0, 1, 2));
    assert_eq!(def.next_field_id, 3);
    // Even with no fields stored, the counter only moves forward.
    assert!(def.fields.is_empty());
}

#[test]
fn recompute_next_field_id_raises_floor_but_never_lowers() {
    let mut def = RecordTypeDef::from_named_fields("R", vec![field("a", DataType::Int)]);
    // Pretend a higher id slipped in (e.g. a migrated def).
    def.fields[0].id = rust_lib_flutter_cad::structure_designer::node_type_registry::FieldId(7);
    def.next_field_id = 0;
    def.recompute_next_field_id();
    assert_eq!(def.next_field_id, 8, "floor = max id (7) + 1");
    // Idempotent / never lowers a higher counter.
    def.next_field_id = 100;
    def.recompute_next_field_id();
    assert_eq!(def.next_field_id, 100);
}

/// The load-bearing allocator invariant: add → delete → re-add a field of the
/// same name must NOT recycle the deleted field's id. Drives the registry's
/// `update_record_type_def` (R1's name-based diff).
#[test]
fn field_ids_never_recycled_across_delete_and_readd() {
    let mut registry = NodeTypeRegistry::new();
    registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "R",
            vec![field("a", DataType::Int), field("b", DataType::Int)],
        ))
        .unwrap();
    let id_a = registry.record_type_defs["R"].fields[0].id;
    let id_b = registry.record_type_defs["R"].fields[1].id;
    assert_ne!(id_a, id_b);

    // Delete `b`.
    registry
        .update_record_type_def("R", vec![field("a", DataType::Int)])
        .unwrap();
    assert_eq!(registry.record_type_defs["R"].fields.len(), 1);
    assert_eq!(
        registry.record_type_defs["R"].fields[0].id, id_a,
        "surviving field keeps its id"
    );

    // Re-add a field named `b`.
    registry
        .update_record_type_def(
            "R",
            vec![field("a", DataType::Int), field("b", DataType::Int)],
        )
        .unwrap();
    let def = &registry.record_type_defs["R"];
    assert_eq!(def.fields[0].id, id_a, "`a` keeps its id across all edits");
    assert_ne!(
        def.fields[1].id, id_b,
        "re-added `b` must get a fresh id — never recycle the deleted one"
    );
    // And the counter still floors above every live id.
    let max = def.fields.iter().map(|f| f.id.0).max().unwrap();
    assert!(
        def.next_field_id > max,
        "next_field_id ({}) must exceed max live id ({})",
        def.next_field_id,
        max
    );
}

/// Updating a def preserves the `FieldId` of fields whose **name** is unchanged
/// (R1's name-based diff), even when other fields are added/removed/reordered.
#[test]
fn unchanged_name_fields_keep_id_across_update() {
    let mut registry = NodeTypeRegistry::new();
    registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "R",
            vec![field("a", DataType::Int), field("b", DataType::Int)],
        ))
        .unwrap();
    let id_a = registry.record_type_defs["R"].fields[0].id;
    let id_b = registry.record_type_defs["R"].fields[1].id;

    // Reorder (b, a) + add c, retype a to Float — names `a`/`b` survive.
    registry
        .update_record_type_def(
            "R",
            vec![
                field("b", DataType::Int),
                field("a", DataType::Float),
                field("c", DataType::Bool),
            ],
        )
        .unwrap();
    let def = &registry.record_type_defs["R"];
    let by_name = |n: &str| def.fields.iter().find(|f| f.name == n).unwrap().id;
    assert_eq!(by_name("a"), id_a, "`a` keeps its id across reorder+retype");
    assert_eq!(by_name("b"), id_b, "`b` keeps its id across reorder");
    assert_ne!(by_name("c"), id_a);
    assert_ne!(by_name("c"), id_b);
}

// ---------------------------------------------------------------------------
// Phase-0 invariants
// ---------------------------------------------------------------------------

#[test]
fn duplicate_field_id_invariant_fires_and_is_fatal() {
    let mut registry = NodeTypeRegistry::new();
    let mut def = RecordTypeDef::from_named_fields(
        "R",
        vec![field("a", DataType::Int), field("b", DataType::Int)],
    );
    // Corrupt: two fields share an id (the bug shape the invariant guards).
    def.fields[1].id = def.fields[0].id;
    registry.record_type_defs.insert("R".to_string(), def);

    let violations = check_document_invariants(&registry);
    let v = violations
        .iter()
        .find(|x| x.kind == InvariantKind::DuplicateFieldId)
        .expect("DuplicateFieldId must be reported");
    assert!(v.is_fatal(), "DuplicateFieldId is Tier 1 (always fatal)");
}

#[test]
fn field_id_floor_invariant_fires_and_is_fatal() {
    let mut registry = NodeTypeRegistry::new();
    let mut def = RecordTypeDef::from_named_fields(
        "R",
        vec![field("a", DataType::Int), field("b", DataType::Int)],
    );
    // Drop the counter to/below the max live id — recycling would collide.
    def.next_field_id = 1; // max id is 1
    registry.record_type_defs.insert("R".to_string(), def);

    let violations = check_document_invariants(&registry);
    let v = violations
        .iter()
        .find(|x| x.kind == InvariantKind::FieldIdFloor)
        .expect("FieldIdFloor must be reported");
    assert!(v.is_fatal());
}

#[test]
fn healthy_def_reports_no_field_identity_violations() {
    let mut registry = NodeTypeRegistry::new();
    registry
        .add_record_type_def(RecordTypeDef::from_named_fields(
            "R",
            vec![field("a", DataType::Int), field("b", DataType::Float)],
        ))
        .unwrap();
    let violations = check_document_invariants(&registry);
    assert!(
        !violations.iter().any(|x| matches!(
            x.kind,
            InvariantKind::DuplicateFieldId | InvariantKind::FieldIdFloor
        )),
        "a freshly-built def must satisfy both field-identity invariants: {:#?}",
        violations
    );
}

// ---------------------------------------------------------------------------
// Serialization: ids are reassigned on load, never persisted (no .cnnd change)
// ---------------------------------------------------------------------------

#[test]
fn record_type_def_on_disk_shape_omits_ids() {
    let def = RecordTypeDef::from_named_fields(
        "R",
        vec![field("a", DataType::Int), field("b", DataType::Float)],
    );
    let json = serde_json::to_string(&def).unwrap();
    assert!(
        !json.contains("next_field_id"),
        "next_field_id must not be serialized: {json}"
    );
    // The on-disk shape is the pre-identity `{name, fields:[[name,type],...]}`.
    // Field `id`s are not written; deserializing the SAME bytes a second time
    // must reproduce identical ids (deterministic, authored-order assignment).
    let again: RecordTypeDef = serde_json::from_str(&json).unwrap();
    let ids: Vec<u64> = again.fields.iter().map(|f| f.id.0).collect();
    assert_eq!(ids, vec![0, 1]);
}

#[test]
fn loading_assigns_sequential_ids_and_recomputes_floor() {
    // A hand-written, pre-identity `.cnnd`-shaped def (no ids, no next_field_id).
    let old_format =
        r#"{ "name": "R", "fields": [ ["a", "Int"], ["b", "Float"], ["c", "Bool"] ] }"#;
    let def: RecordTypeDef = serde_json::from_str(old_format).unwrap();
    assert_eq!(def.name, "R");
    let ids: Vec<u64> = def.fields.iter().map(|f| f.id.0).collect();
    assert_eq!(ids, vec![0, 1, 2], "ids assigned in authored order on load");
    assert_eq!(def.next_field_id, 3, "floor recomputed to max(id)+1");
    // Names/types survive unchanged.
    assert_eq!(def.fields[0].name, "a");
    assert_eq!(def.fields[1].data_type, DataType::Float);
    assert_eq!(def.fields[2].data_type, DataType::Bool);
}
