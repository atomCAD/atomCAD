//! Phase 1 tests for hierarchical record type defs — move/rename parity with
//! networks (see `doc/design_hierarchical_records.md`).
//!
//! Coverage:
//! - `user_type_kind` dispatch
//! - Helper 1 (`rename_record_type_def_unchecked`) infallibility
//! - Helper 2 (`repair_all_networks`) pin-layout refresh
//! - `compute_namespace_rename` / `delete_namespace` sweeping both maps
//! - `check_record_delete_references` matrix
//! - kind-tagged `RenameNamespaceCommand` / record-aware `DeleteNamespaceCommand`
//!   undo/redo, incl. the #1 (active record def) and #3 (pin layout) regressions
//! - migrated per-record commands (standalone rename/delete) remap/restore the
//!   active record def and repair pin layouts

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::{DataType, RecordType};
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::node_type_registry::{
    NodeTypeRegistry, RecordTypeDef, RecordTypeDefError, UserTypeKind,
};
use rust_lib_flutter_cad::structure_designer::nodes::record_construct::RecordConstructData;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;

// ============================================================================
// Helpers
// ============================================================================

fn def(name: &str, fields: Vec<(&str, DataType)>) -> RecordTypeDef {
    RecordTypeDef::from_named_fields(
        name.to_string(),
        fields
            .into_iter()
            .map(|(n, t)| (n.to_string(), t))
            .collect(),
    )
}

fn named(name: &str) -> DataType {
    DataType::Record(RecordType::Named(name.to_string()))
}

fn set_node_data(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    data: Box<dyn NodeData>,
) {
    let registry = &mut designer.node_type_registry;
    let network = registry.node_networks.get_mut(network_name).unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    node.data = data;
    NodeTypeRegistry::populate_custom_node_type_cache_with_types(
        &registry.built_in_node_types,
        &registry.record_type_defs,
        &registry.built_in_record_type_defs,
        node,
        true,
    );
}

/// Add a `record_construct` node in `network_name` whose `schema` references
/// `schema_name`. Returns the node id.
fn add_record_construct(
    designer: &mut StructureDesigner,
    network_name: &str,
    schema_name: &str,
) -> u64 {
    let id = designer.add_node("record_construct", DVec2::new(0.0, 0.0));
    set_node_data(
        designer,
        network_name,
        id,
        Box::new(RecordConstructData {
            schema: schema_name.to_string(),
            ..Default::default()
        }),
    );
    id
}

/// The authored parameter-pin names of a node, resolved through the registry
/// (the derived `custom_node_type`). Used to assert pin-layout repair.
fn node_pin_names(designer: &StructureDesigner, network_name: &str, node_id: u64) -> Vec<String> {
    let registry = &designer.node_type_registry;
    let network = registry.node_networks.get(network_name).unwrap();
    let node = network.nodes.get(&node_id).unwrap();
    let nt = registry.get_node_type_for_node(node).unwrap();
    nt.parameters.iter().map(|p| p.name.clone()).collect()
}

fn schema_of(designer: &StructureDesigner, network_name: &str, node_id: u64) -> String {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = network.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<RecordConstructData>()
        .unwrap()
        .schema
        .clone()
}

fn has_record(designer: &StructureDesigner, name: &str) -> bool {
    designer
        .node_type_registry
        .record_type_defs
        .contains_key(name)
}

fn has_network(designer: &StructureDesigner, name: &str) -> bool {
    designer.node_type_registry.node_networks.contains_key(name)
}

// ============================================================================
// user_type_kind
// ============================================================================

#[test]
fn user_type_kind_distinguishes_networks_records_and_unknowns() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Net");
    designer
        .node_type_registry
        .add_record_type_def(def("Rec", vec![("a", DataType::Int)]))
        .unwrap();

    let reg = &designer.node_type_registry;
    assert_eq!(reg.user_type_kind("Net"), Some(UserTypeKind::Network));
    assert_eq!(reg.user_type_kind("Rec"), Some(UserTypeKind::Record));
    assert_eq!(reg.user_type_kind("Missing"), None);
    // Built-in record defs are not part of the movable hierarchy.
    assert_eq!(reg.user_type_kind("ElementMapping"), None);
}

// ============================================================================
// Helper 1 — rename_record_type_def_unchecked
// ============================================================================

#[test]
fn unchecked_rename_renames_and_rewrites_refs() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("Point", vec![("x", DataType::Int)]))
        .unwrap();
    let cons = add_record_construct(&mut designer, "Main", "Point");

    designer
        .node_type_registry
        .rename_record_type_def_unchecked("Point", "Vertex");

    assert!(!has_record(&designer, "Point"));
    assert!(has_record(&designer, "Vertex"));
    // The bare `schema` reference on the record_construct node is rewritten.
    assert_eq!(schema_of(&designer, "Main", cons), "Vertex");
}

#[test]
fn unchecked_rename_is_noop_on_same_name_and_missing() {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("Point", vec![("x", DataType::Int)]))
        .unwrap();

    // old == new: no-op.
    designer
        .node_type_registry
        .rename_record_type_def_unchecked("Point", "Point");
    assert!(has_record(&designer, "Point"));

    // Missing source: no panic, no-op.
    designer
        .node_type_registry
        .rename_record_type_def_unchecked("Ghost", "Whatever");
    assert!(!has_record(&designer, "Whatever"));
}

// ============================================================================
// Namespace rename — both maps + kind dispatch
// ============================================================================

#[test]
fn compute_namespace_rename_sweeps_records() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Physics.Spring");
    designer
        .node_type_registry
        .add_record_type_def(def("Physics.ElementMapping", vec![("from", DataType::Int)]))
        .unwrap();

    let plan = designer.compute_namespace_rename("Physics", "Mechanics");
    assert!(plan.is_applicable());
    // Both kinds are present, tagged correctly.
    let net = plan
        .items
        .iter()
        .find(|i| i.old_name == "Physics.Spring")
        .unwrap();
    assert_eq!(net.kind, UserTypeKind::Network);
    let rec = plan
        .items
        .iter()
        .find(|i| i.old_name == "Physics.ElementMapping")
        .unwrap();
    assert_eq!(rec.kind, UserTypeKind::Record);
}

#[test]
fn rename_namespace_record_only() {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.A", vec![("a", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.B", vec![("b", DataType::Int)]))
        .unwrap();

    assert!(designer.rename_namespace("NS", "Renamed"));
    assert!(has_record(&designer, "Renamed.A"));
    assert!(has_record(&designer, "Renamed.B"));
    assert!(!has_record(&designer, "NS.A"));
}

#[test]
fn rename_namespace_mixed_folder() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Physics.Spring");
    designer
        .node_type_registry
        .add_record_type_def(def("Physics.ElementMapping", vec![("from", DataType::Int)]))
        .unwrap();

    assert!(designer.rename_namespace("Physics", "Mechanics"));
    assert!(has_network(&designer, "Mechanics.Spring"));
    assert!(has_record(&designer, "Mechanics.ElementMapping"));
    assert!(!has_network(&designer, "Physics.Spring"));
    assert!(!has_record(&designer, "Physics.ElementMapping"));
}

#[test]
fn rename_namespace_record_collision_refuses() {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("A.Foo", vec![("a", DataType::Int)]))
        .unwrap();
    // A network already occupies the target name the record would move onto.
    designer.add_node_network("B.Foo");

    let plan = designer.compute_namespace_rename("A", "B");
    assert!(plan.has_conflicts());
    assert!(!plan.is_applicable());
    assert!(!designer.rename_namespace("A", "B"));
    // Nothing moved.
    assert!(has_record(&designer, "A.Foo"));
}

#[test]
fn rename_namespace_record_refs_follow_across_move() {
    // A record referenced from a surviving network has its reference rewritten
    // when the record moves namespaces.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    let cons = add_record_construct(&mut designer, "Main", "NS.Point");

    assert!(designer.rename_namespace("NS", "Geo"));
    assert_eq!(schema_of(&designer, "Main", cons), "Geo.Point");
    // Pin layout was repaired against the renamed def.
    assert_eq!(node_pin_names(&designer, "Main", cons), vec!["x"]);
}

// ============================================================================
// compute_leaf_rename — kind detection
// ============================================================================

#[test]
fn compute_leaf_rename_detects_record_kind() {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("a.b.Point", vec![("x", DataType::Int)]))
        .unwrap();

    let plan = designer.compute_leaf_rename("a.b.Point", "a.Point");
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.items[0].kind, UserTypeKind::Record);
    assert!(plan.is_applicable());

    // Unknown / built-in => empty plan.
    assert!(designer.compute_leaf_rename("nope", "x").is_empty());
    assert!(
        designer
            .compute_leaf_rename("ElementMapping", "x")
            .is_empty()
    );
}

// ============================================================================
// Namespace delete — both maps
// ============================================================================

#[test]
fn delete_namespace_record_only() {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.A", vec![("a", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.B", vec![("b", DataType::Int)]))
        .unwrap();

    assert!(designer.delete_namespace("NS").is_ok());
    assert!(!has_record(&designer, "NS.A"));
    assert!(!has_record(&designer, "NS.B"));
}

#[test]
fn delete_namespace_mixed_folder() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("NS.Spring");
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Rec", vec![("a", DataType::Int)]))
        .unwrap();

    assert!(designer.delete_namespace("NS").is_ok());
    assert!(!has_network(&designer, "NS.Spring"));
    assert!(!has_record(&designer, "NS.Rec"));
}

#[test]
fn delete_namespace_empty_message_mentions_items() {
    let designer_setup = || {
        let mut d = StructureDesigner::new();
        d.add_node_network("Unrelated");
        d
    };
    let mut designer = designer_setup();
    let err = designer.delete_namespace("Missing").unwrap_err();
    assert!(err.contains("No items found"), "got: {}", err);
}

// ============================================================================
// check_record_delete_references matrix
// ============================================================================

#[test]
fn delete_namespace_blocked_by_surviving_network_ref() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    add_record_construct(&mut designer, "Main", "NS.Point");
    // Re-activate Main is fine; Main is NOT under NS, so it survives.

    let err = designer.delete_namespace("NS").unwrap_err();
    assert!(err.contains("references"), "got: {}", err);
    // Nothing was deleted.
    assert!(has_record(&designer, "NS.Point"));
}

#[test]
fn delete_namespace_blocked_by_surviving_record_field_ref() {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    // A surviving (not under NS) record def whose field references NS.Point.
    designer
        .node_type_registry
        .add_record_type_def(def("Box", vec![("p", named("NS.Point"))]))
        .unwrap();

    let err = designer.delete_namespace("NS").unwrap_err();
    assert!(err.contains("Box"), "got: {}", err);
    assert!(has_record(&designer, "NS.Point"));
}

#[test]
fn delete_namespace_allows_deleted_network_ref_to_deleted_record() {
    // A network under NS references a record under NS; both are deleted
    // together, so the intra-set reference does NOT block.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("NS.User");
    designer.set_active_node_network_name(Some("NS.User".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    add_record_construct(&mut designer, "NS.User", "NS.Point");

    assert!(designer.delete_namespace("NS").is_ok());
    assert!(!has_network(&designer, "NS.User"));
    assert!(!has_record(&designer, "NS.Point"));
}

#[test]
fn delete_namespace_blocked_combined_listing() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("Box", vec![("p", named("NS.Point"))]))
        .unwrap();
    add_record_construct(&mut designer, "Main", "NS.Point");

    let err = designer.delete_namespace("NS").unwrap_err();
    // Both blockers listed.
    assert!(err.contains("network 'Main'"), "got: {}", err);
    assert!(err.contains("record 'Box'"), "got: {}", err);
}

#[test]
fn delete_namespace_builtin_record_never_spuriously_blocks() {
    // A user network referencing the built-in ElementMapping must not block a
    // delete of an unrelated record namespace (the built-in is not a target).
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.A", vec![("a", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("Uses", vec![("m", named("ElementMapping"))]))
        .unwrap();

    assert!(designer.delete_namespace("NS").is_ok());
}

// ============================================================================
// Single-def delete_record_type_def reference checks
//
// The single-def delete path must block just like the batch `delete_namespace`
// path (they share `record_delete_blockers` / `collect_record_refs_in_network`).
// Regression: deleting a still-referenced record def used to silently succeed
// and leave a dangling `Record(Named(_))` behind (e.g. inside another def).
// ============================================================================

#[test]
fn delete_record_def_blocked_by_referencing_record_def() {
    // The reported bug: A { inner: B }, deleting B must be blocked, not left
    // dangling inside A.
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("B", vec![("x", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("A", vec![("inner", named("B"))]))
        .unwrap();

    let err = designer.delete_record_type_def("B").unwrap_err();
    match &err {
        RecordTypeDefError::Referenced(name, refs) => {
            assert_eq!(name, "B");
            assert!(refs.contains("record 'A'"), "got: {refs}");
        }
        other => panic!("expected Referenced, got {other:?}"),
    }
    assert!(
        has_record(&designer, "B"),
        "B must survive a blocked delete"
    );
}

#[test]
fn delete_record_def_blocked_by_nested_container_field() {
    // Array[B] inside A also blocks (the ref walker recurses containers).
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("B", vec![("x", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def(
            "A",
            vec![("xs", DataType::Array(Box::new(named("B"))))],
        ))
        .unwrap();

    let err = designer.delete_record_type_def("B").unwrap_err();
    assert!(
        matches!(err, RecordTypeDefError::Referenced(..)),
        "got: {err:?}"
    );
    assert!(has_record(&designer, "B"));
}

#[test]
fn delete_record_def_blocked_by_node_in_network() {
    // A record_construct node referencing B blocks; message names the network.
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("B", vec![("x", DataType::Int)]))
        .unwrap();
    add_record_construct(&mut designer, "Main", "B");

    let err = designer.delete_record_type_def("B").unwrap_err();
    match &err {
        RecordTypeDefError::Referenced(_, refs) => {
            assert!(refs.contains("network 'Main'"), "got: {refs}");
        }
        other => panic!("expected Referenced, got {other:?}"),
    }
    assert!(has_record(&designer, "B"));
}

#[test]
fn delete_record_def_blocked_lists_all_references() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("B", vec![("x", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("A", vec![("inner", named("B"))]))
        .unwrap();
    add_record_construct(&mut designer, "Main", "B");

    let err = designer.delete_record_type_def("B").unwrap_err();
    let RecordTypeDefError::Referenced(_, refs) = &err else {
        panic!("expected Referenced, got {err:?}");
    };
    assert!(refs.contains("record 'A'"), "got: {refs}");
    assert!(refs.contains("network 'Main'"), "got: {refs}");
}

#[test]
fn delete_container_def_not_blocked_by_its_own_reference() {
    // A { inner: B } — deleting A is fine even though A references B; a def's
    // own outgoing references never block its own deletion.
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("B", vec![("x", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("A", vec![("inner", named("B"))]))
        .unwrap();

    assert!(designer.delete_record_type_def("A").is_ok());
    assert!(!has_record(&designer, "A"));
    assert!(has_record(&designer, "B"));
}

#[test]
fn delete_unreferenced_record_def_succeeds_and_undo_restores() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("B", vec![("x", DataType::Int)]))
        .unwrap();
    designer.undo_stack.clear();

    designer.delete_record_type_def("B").unwrap();
    assert!(!has_record(&designer, "B"));

    designer.undo();
    assert!(has_record(&designer, "B"), "undo restores the def");

    designer.redo();
    assert!(!has_record(&designer, "B"), "redo re-deletes");
}

// ============================================================================
// Undo / redo — namespace rename
// ============================================================================

#[test]
fn undo_redo_rename_namespace_record_only() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    let cons = add_record_construct(&mut designer, "Main", "NS.Point");
    designer.undo_stack.clear();

    assert!(designer.rename_namespace("NS", "Geo"));
    assert!(has_record(&designer, "Geo.Point"));
    assert_eq!(schema_of(&designer, "Main", cons), "Geo.Point");

    assert!(designer.undo());
    assert!(has_record(&designer, "NS.Point"));
    assert!(!has_record(&designer, "Geo.Point"));
    assert_eq!(schema_of(&designer, "Main", cons), "NS.Point");
    // #3 — pin layout repaired against the restored name.
    assert_eq!(node_pin_names(&designer, "Main", cons), vec!["x"]);

    assert!(designer.redo());
    assert!(has_record(&designer, "Geo.Point"));
    assert_eq!(schema_of(&designer, "Main", cons), "Geo.Point");
    assert_eq!(node_pin_names(&designer, "Main", cons), vec!["x"]);
}

#[test]
fn undo_redo_rename_namespace_mixed() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Physics.Spring");
    designer
        .node_type_registry
        .add_record_type_def(def("Physics.Rec", vec![("a", DataType::Int)]))
        .unwrap();
    designer.undo_stack.clear();

    assert!(designer.rename_namespace("Physics", "Mechanics"));
    assert!(designer.undo());
    assert!(has_network(&designer, "Physics.Spring"));
    assert!(has_record(&designer, "Physics.Rec"));
    assert!(!has_network(&designer, "Mechanics.Spring"));
    assert!(!has_record(&designer, "Mechanics.Rec"));

    assert!(designer.redo());
    assert!(has_network(&designer, "Mechanics.Spring"));
    assert!(has_record(&designer, "Mechanics.Rec"));
}

#[test]
fn rename_namespace_remaps_active_record_def_through_undo_redo() {
    // #1 — the active record def follows the move and survives undo/redo.
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    designer.set_active_record_def_name(Some("NS.Point".to_string()));
    designer.undo_stack.clear();

    designer.rename_namespace("NS", "Geo");
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("Geo.Point".to_string())
    );

    designer.undo();
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("NS.Point".to_string())
    );

    designer.redo();
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("Geo.Point".to_string())
    );
}

// ============================================================================
// Undo / redo — namespace delete
// ============================================================================

#[test]
fn undo_redo_delete_namespace_with_records() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("NS.Spring");
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Rec", vec![("a", DataType::Int)]))
        .unwrap();
    designer.undo_stack.clear();

    designer.delete_namespace("NS").unwrap();
    assert!(!has_network(&designer, "NS.Spring"));
    assert!(!has_record(&designer, "NS.Rec"));

    designer.undo();
    assert!(has_network(&designer, "NS.Spring"));
    assert!(has_record(&designer, "NS.Rec"));

    designer.redo();
    assert!(!has_network(&designer, "NS.Spring"));
    assert!(!has_record(&designer, "NS.Rec"));
}

#[test]
fn delete_namespace_clears_and_restores_active_record_def() {
    // #1 — namespace delete clears the active record def on redo and restores
    // it on undo.
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Rec", vec![("a", DataType::Int)]))
        .unwrap();
    designer.set_active_record_def_name(Some("NS.Rec".to_string()));
    designer.undo_stack.clear();

    designer.delete_namespace("NS").unwrap();
    assert_eq!(designer.get_active_record_def_name(), None);

    designer.undo();
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("NS.Rec".to_string())
    );

    designer.redo();
    assert_eq!(designer.get_active_record_def_name(), None);
}

// ============================================================================
// Migrated per-record commands (standalone rename/delete)
// ============================================================================

#[test]
fn standalone_rename_remaps_active_record_def_and_repairs_pins() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("Point", vec![("x", DataType::Int)]))
        .unwrap();
    let cons = add_record_construct(&mut designer, "Main", "Point");
    designer.set_active_record_def_name(Some("Point".to_string()));
    designer.undo_stack.clear();

    designer.rename_record_type_def("Point", "Vertex").unwrap();
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("Vertex".to_string())
    );

    designer.undo();
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("Point".to_string())
    );
    assert_eq!(schema_of(&designer, "Main", cons), "Point");
    assert_eq!(node_pin_names(&designer, "Main", cons), vec!["x"]);

    designer.redo();
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("Vertex".to_string())
    );
    assert_eq!(schema_of(&designer, "Main", cons), "Vertex");
    assert_eq!(node_pin_names(&designer, "Main", cons), vec!["x"]);
}

#[test]
fn standalone_delete_clears_and_restores_active_record_def() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Main");
    designer.set_active_node_network_name(Some("Main".to_string()));
    designer
        .node_type_registry
        .add_record_type_def(def("Point", vec![("x", DataType::Int)]))
        .unwrap();
    designer.set_active_record_def_name(Some("Point".to_string()));
    designer.undo_stack.clear();

    designer.delete_record_type_def("Point").unwrap();
    assert_eq!(designer.get_active_record_def_name(), None);

    designer.undo();
    assert_eq!(
        designer.get_active_record_def_name(),
        Some("Point".to_string())
    );
    assert!(has_record(&designer, "Point"));

    designer.redo();
    assert_eq!(designer.get_active_record_def_name(), None);
    assert!(!has_record(&designer, "Point"));
}

// ============================================================================
// Folder move where a record references another moved record
// ============================================================================

#[test]
fn rename_namespace_moved_record_refs_another_moved_record() {
    let mut designer = StructureDesigner::new();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Point", vec![("x", DataType::Int)]))
        .unwrap();
    designer
        .node_type_registry
        .add_record_type_def(def("NS.Box", vec![("p", named("NS.Point"))]))
        .unwrap();

    assert!(designer.rename_namespace("NS", "Geo"));
    assert!(has_record(&designer, "Geo.Point"));
    assert!(has_record(&designer, "Geo.Box"));
    // The referrer's field follows to the new name.
    let box_def = designer
        .node_type_registry
        .record_type_defs
        .get("Geo.Box")
        .unwrap();
    assert_eq!(box_def.fields[0].data_type, named("Geo.Point"));
}
