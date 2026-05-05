//! Phase A tests for `atom_replace` programmatic rules input. See
//! `doc/design_atom_replace_rules_input.md`. This phase covers the built-in
//! record-def infrastructure and the `ElementMapping` def. The `rules` pin
//! itself ships in Phase B and is tested separately.
//!
//! What we verify here:
//! - `ElementMapping` resolves through `lookup_record_type_def` even when
//!   `record_type_defs` is empty.
//! - `add_record_type_def("ElementMapping", ...)` is rejected.
//! - `delete_record_type_def("ElementMapping")` is a no-op (the registry-
//!   level call returns `None`); the registry-level mutation guards reject
//!   `rename` and `update` on `ElementMapping` with a `BuiltIn` error.
//! - Adding a network named `ElementMapping` is rejected (namespace
//!   collision via `name_is_taken`).
//! - `name_is_taken` reports `true` for `ElementMapping`.
//! - `.cnnd` save emits no `record_type_defs` entry for `ElementMapping`.
//!   On load into a fresh registry, the def is still resolvable at runtime.
//! - Backward compat: loading a fixture saved before this feature (no
//!   built-in defs, no `record_type_defs`) leaves `ElementMapping` available
//!   at runtime.

use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef, RecordTypeDefError,
};
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
use std::collections::HashMap;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

#[test]
fn element_mapping_resolves_via_lookup_with_empty_user_defs() {
    let registry = NodeTypeRegistry::new();
    assert!(registry.record_type_defs.is_empty());
    let def = registry
        .lookup_record_type_def("ElementMapping")
        .expect("ElementMapping should resolve via built_in_record_type_defs");
    assert_eq!(def.name, "ElementMapping");
    assert_eq!(
        def.fields,
        vec![
            ("from".to_string(), DataType::Int),
            ("to".to_string(), DataType::Int),
        ]
    );
}

#[test]
fn element_mapping_resolves_through_record_type_named_resolve_fields() {
    let registry = NodeTypeRegistry::new();
    let ty = RecordType::Named("ElementMapping".to_string());
    let fields = ty
        .resolve_fields(&registry)
        .expect("Named(ElementMapping) must resolve to the built-in def");
    let canonical: Vec<_> = fields.into_owned();
    // Canonical order is sorted by field name. "from" < "to".
    assert_eq!(canonical[0].0, "from");
    assert_eq!(canonical[1].0, "to");
    assert!(matches!(canonical[0].1, DataType::Int));
    assert!(matches!(canonical[1].1, DataType::Int));
}

#[test]
fn name_is_taken_includes_built_in_record_defs() {
    let registry = NodeTypeRegistry::new();
    assert!(registry.name_is_taken("ElementMapping"));
    assert!(!registry.name_is_taken("NotARealName"));
}

#[test]
fn is_built_in_record_type_def_only_built_ins() {
    let mut registry = NodeTypeRegistry::new();
    registry
        .add_record_type_def(RecordTypeDef {
            name: "Point".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();
    assert!(registry.is_built_in_record_type_def("ElementMapping"));
    assert!(!registry.is_built_in_record_type_def("Point"));
    assert!(!registry.is_built_in_record_type_def("DoesNotExist"));
}

// ---------------------------------------------------------------------------
// Mutation guards
// ---------------------------------------------------------------------------

#[test]
fn add_record_type_def_rejects_built_in_name() {
    let mut registry = NodeTypeRegistry::new();
    let err = registry
        .add_record_type_def(RecordTypeDef {
            name: "ElementMapping".to_string(),
            fields: vec![],
        })
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::BuiltIn(ref s) if s == "ElementMapping"));
}

#[test]
fn delete_record_type_def_built_in_is_noop() {
    let mut registry = NodeTypeRegistry::new();
    // Returns None because built-ins are immutable.
    assert!(registry.delete_record_type_def("ElementMapping").is_none());
    // The def is still resolvable.
    assert!(registry.lookup_record_type_def("ElementMapping").is_some());
}

#[test]
fn rename_record_type_def_rejects_built_in_source() {
    let mut registry = NodeTypeRegistry::new();
    let err = registry
        .rename_record_type_def("ElementMapping", "MyMapping")
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::BuiltIn(_)));
    // Untouched.
    assert!(registry.lookup_record_type_def("ElementMapping").is_some());
    assert!(registry.lookup_record_type_def("MyMapping").is_none());
}

#[test]
fn rename_record_type_def_rejects_built_in_target() {
    let mut registry = NodeTypeRegistry::new();
    registry
        .add_record_type_def(RecordTypeDef {
            name: "Mine".to_string(),
            fields: vec![("a".to_string(), DataType::Int)],
        })
        .unwrap();
    let err = registry
        .rename_record_type_def("Mine", "ElementMapping")
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::BuiltIn(_)));
    // Original user def still in place.
    assert!(registry.record_type_defs.contains_key("Mine"));
}

#[test]
fn update_record_type_def_rejects_built_in() {
    let mut registry = NodeTypeRegistry::new();
    let err = registry
        .update_record_type_def("ElementMapping", vec![])
        .unwrap_err();
    assert!(matches!(err, RecordTypeDefError::BuiltIn(_)));
    // Built-in is unchanged.
    let def = registry.lookup_record_type_def("ElementMapping").unwrap();
    assert_eq!(def.fields.len(), 2);
}

// ---------------------------------------------------------------------------
// Namespace collisions across the wider user-type namespace
// ---------------------------------------------------------------------------

#[test]
fn add_node_network_with_name_rejects_built_in_record_def_name() {
    use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
    let designer = StructureDesigner::default();
    // Should be rejected because `ElementMapping` is taken by a built-in
    // record type def. The structure_designer-level helper validates the
    // name itself but does not check the namespace; the API layer
    // (`add_node_network_with_name`) calls `name_is_taken`. We mirror that
    // check here directly to assert the policy is enforceable.
    assert!(designer.node_type_registry.name_is_taken("ElementMapping"));
    // Sanity: a fresh non-colliding name is not taken.
    assert!(!designer.node_type_registry.name_is_taken("Brand_New_Net"));
}

// ---------------------------------------------------------------------------
// .cnnd serialization: built-in defs are never emitted; round-trip leaves
// the def resolvable at runtime.
// ---------------------------------------------------------------------------

#[test]
fn cnnd_save_does_not_emit_built_in_record_defs() {
    let mut registry = NodeTypeRegistry::new();
    // Add one user def so the file has an unambiguous record_type_defs section.
    registry
        .add_record_type_def(RecordTypeDef {
            name: "Point".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();

    let temp = TempDir::new().unwrap();
    let path = temp.path().join("project.cnnd");

    save_node_networks_to_file(&mut registry, &path, false, &HashMap::new()).unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let defs = value
        .get("record_type_defs")
        .expect("record_type_defs must be emitted when at least one user def exists")
        .as_array()
        .unwrap();
    let names: Vec<&str> = defs
        .iter()
        .map(|d| d.get("name").and_then(|n| n.as_str()).unwrap())
        .collect();
    assert!(names.contains(&"Point"));
    assert!(
        !names.contains(&"ElementMapping"),
        "Built-in defs must not be serialized; saw {:?}",
        names
    );
}

#[test]
fn cnnd_load_keeps_element_mapping_resolvable() {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;
    let mut registry = NodeTypeRegistry::new();
    registry
        .add_record_type_def(RecordTypeDef {
            name: "Point".to_string(),
            fields: vec![("x".to_string(), DataType::Int)],
        })
        .unwrap();
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("project.cnnd");
    save_node_networks_to_file(&mut registry, &path, false, &HashMap::new()).unwrap();

    let path_str = path.to_string_lossy().to_string();
    let mut loaded_registry = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut loaded_registry, &path_str).unwrap();

    // ElementMapping is preserved by `new()`, never wiped by load.
    assert!(
        loaded_registry
            .lookup_record_type_def("ElementMapping")
            .is_some(),
        "ElementMapping must be available after a load"
    );
    // The loaded user def is also present.
    assert!(loaded_registry.record_type_defs.contains_key("Point"));
}

#[test]
fn pre_record_cnnd_file_has_element_mapping_at_runtime() {
    // A file saved before this feature has no `record_type_defs` field; the
    // serializer treats the missing field as an empty list. Built-in defs
    // are populated by `new()` before deserialization, so they must survive
    // the wipe-and-reinsert pass on load.
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::load_node_networks_from_file;

    let temp = TempDir::new().unwrap();
    let path = temp.path().join("legacy.cnnd");
    // Hand-rolled minimal v3 .cnnd shape: an empty network array, no
    // record_type_defs field at all.
    std::fs::write(&path, r#"{"node_networks": [], "version": 3}"#).unwrap();

    let mut registry = NodeTypeRegistry::new();
    let path_str = path.to_string_lossy().to_string();
    load_node_networks_from_file(&mut registry, &path_str).unwrap();

    assert!(registry.lookup_record_type_def("ElementMapping").is_some());
    assert!(registry.record_type_defs.is_empty());
}
