//! Tests for empty folders (doc/design_empty_folders.md).
//!
//! Coverage:
//! - `add_folder` creates a marker; collision rejection
//! - prune-on-add: an entity / subfolder absorbs its ancestor empty-folder
//!   markers ("vanishes when emptied" falls out because removal never re-adds)
//! - rename / delete of an empty folder
//! - undo: add folder; entity-add restores the pruned ancestor marker
//! - `prune_redundant_folders` reconcile
//! - serialization roundtrip preserves an empty folder

use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    load_node_networks_from_file, save_node_networks_to_file,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use std::collections::HashMap;

#[test]
fn add_folder_creates_marker() {
    let mut d = StructureDesigner::new();
    d.add_folder("Physics").expect("create folder");
    assert!(d.node_type_registry.folders.contains("Physics"));
    assert_eq!(
        d.node_type_registry.get_folder_names(),
        vec!["Physics".to_string()]
    );
}

#[test]
fn add_folder_rejects_collision_with_network() {
    let mut d = StructureDesigner::new();
    d.add_node_network("Foo");
    assert!(
        d.add_folder("Foo").is_err(),
        "a folder may not share a name with a network"
    );
}

#[test]
fn add_folder_rejects_duplicate_folder() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create folder");
    assert!(d.add_folder("A").is_err(), "duplicate folder rejected");
}

#[test]
fn entity_add_prunes_ancestor_marker() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create folder");
    assert!(d.node_type_registry.folders.contains("A"));

    // Adding a network into A gives A content, so the marker is pruned.
    let name = d.add_new_node_network_in_namespace("A");
    assert_eq!(name, "A.UNTITLED");
    assert!(
        !d.node_type_registry.folders.contains("A"),
        "marker pruned once the folder has content"
    );
}

#[test]
fn record_add_prunes_ancestor_marker() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create folder");
    d.add_new_record_type_def_in_namespace("A")
        .expect("create record def");
    assert!(!d.node_type_registry.folders.contains("A"));
}

#[test]
fn subfolder_prunes_parent_marker() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create A");
    d.add_folder("A.B").expect("create A.B");
    assert!(!d.node_type_registry.folders.contains("A"));
    assert!(d.node_type_registry.folders.contains("A.B"));
}

#[test]
fn delete_empty_folder() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create folder");
    d.delete_namespace("A").expect("delete empty folder");
    assert!(d.node_type_registry.folders.is_empty());
}

#[test]
fn rename_empty_folder() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create folder");
    assert!(d.rename_namespace("A", "B"), "rename should apply");
    assert!(!d.node_type_registry.folders.contains("A"));
    assert!(d.node_type_registry.folders.contains("B"));
}

#[test]
fn rename_empty_subfolder_via_parent_namespace() {
    let mut d = StructureDesigner::new();
    d.add_folder("A.Empty").expect("create A.Empty");
    // Renaming the parent namespace A → C must carry the empty subfolder.
    assert!(d.rename_namespace("A", "C"));
    assert!(d.node_type_registry.folders.contains("C.Empty"));
    assert!(!d.node_type_registry.folders.contains("A.Empty"));
}

#[test]
fn undo_redo_add_folder() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create folder");
    d.undo();
    assert!(
        !d.node_type_registry.folders.contains("A"),
        "undo removes the folder"
    );
    d.redo();
    assert!(
        d.node_type_registry.folders.contains("A"),
        "redo re-creates the folder"
    );
}

#[test]
fn undo_entity_add_restores_pruned_folder() {
    let mut d = StructureDesigner::new();
    d.add_folder("A").expect("create folder");
    d.add_new_node_network_in_namespace("A");
    assert!(!d.node_type_registry.folders.contains("A"));

    // Undoing the network creation must bring back the empty folder it filled.
    d.undo();
    assert!(
        d.node_type_registry.folders.contains("A"),
        "undo of entity add restores the absorbed empty-folder marker"
    );
}

#[test]
fn prune_redundant_folders_drops_covered_markers() {
    let mut d = StructureDesigner::new();
    // Inject a redundant marker directly (as a hand-edited file might) alongside
    // an entity that lives under it.
    d.node_type_registry.folders.insert("A".to_string());
    d.node_type_registry.folders.insert("A.B".to_string());
    d.add_node_network("A.Spring"); // also prunes "A" via add_node_network
    // "A" is covered by A.Spring; "A.B" is a genuinely empty subfolder, kept.
    d.node_type_registry.prune_redundant_folders();
    assert!(!d.node_type_registry.folders.contains("A"));
    assert!(d.node_type_registry.folders.contains("A.B"));
}

#[test]
fn serialization_roundtrip_preserves_empty_folder() {
    let mut d = StructureDesigner::new();
    d.add_folder("Physics.Mechanics").expect("create folder");

    let mut path = std::env::temp_dir();
    path.push("atomcad_empty_folders_roundtrip.cnnd");

    save_node_networks_to_file(&mut d.node_type_registry, &path, false, &HashMap::new())
        .expect("save");

    let mut loaded = NodeTypeRegistry::new();
    load_node_networks_from_file(&mut loaded, path.to_str().unwrap()).expect("load");

    assert!(
        loaded.folders.contains("Physics.Mechanics"),
        "empty folder survives a save/load roundtrip"
    );

    let _ = std::fs::remove_file(&path);
}
