use glam::{DVec2, DVec3, IVec3};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::nodes::lattice_move::LatticeMoveData;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeTypeRegistryNetworks, node_network_to_serializable,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::edit_network as text_edit_network;
use rust_lib_flutter_cad::structure_designer::undo::{
    UndoCommand, UndoContext, UndoRefreshMode, UndoStack,
};
use serde_json::Value;
use std::fmt::Debug;

// --- Test Helpers ---

fn setup_designer_with_network(network_name: &str) -> StructureDesigner {
    let mut designer = StructureDesigner::new();
    designer.add_node_network(network_name);
    designer.set_active_node_network_name(Some(network_name.to_string()));
    designer
}

/// Serialize all networks to a comparable JSON Value.
/// Uses the same codepath as CNND file saving, but returns the Value
/// instead of writing to disk.
/// Normalizes HashMap-derived arrays for deterministic comparison.
fn snapshot_all_networks(registry: &mut NodeTypeRegistry) -> Value {
    let mut serializable_networks = Vec::new();

    // Collect network names first to avoid borrow conflict
    let names: Vec<String> = registry.node_networks.keys().cloned().collect();

    for name in names {
        // Split borrow: built_in_node_types and node_networks
        let (built_in_types, node_networks) =
            (&registry.built_in_node_types, &mut registry.node_networks);

        let network = node_networks.get_mut(&name).unwrap();
        let serializable = node_network_to_serializable(network, built_in_types, None).unwrap();
        serializable_networks.push((name, serializable));
    }

    // Sort by name for deterministic comparison
    serializable_networks.sort_by(|a, b| a.0.cmp(&b.0));

    let container = SerializableNodeTypeRegistryNetworks {
        node_networks: serializable_networks,
        version: 2,
        direct_editing_mode: false,
    };

    let mut value = serde_json::to_value(&container).unwrap();
    // Normalize HashMap-derived arrays for deterministic comparison
    normalize_json(&mut value);
    value
}

/// Sort arrays that come from HashMap iteration (displayed_node_ids, nodes)
/// so that comparison is deterministic regardless of insertion order.
fn normalize_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if key == "displayed_node_ids" {
                    // Sort by node_id (first element of each inner array)
                    if let Value::Array(arr) = val {
                        arr.sort_by(|a, b| {
                            let id_a = a
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let id_b = b
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            id_a.cmp(&id_b)
                        });
                    }
                } else if key == "nodes" {
                    // Sort nodes by id for deterministic comparison
                    // (HashMap iteration order differs after deserialize+reserialize)
                    if let Value::Array(arr) = val {
                        arr.sort_by(|a, b| {
                            let id_a = a
                                .as_object()
                                .and_then(|o| o.get("id"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let id_b = b
                                .as_object()
                                .and_then(|o| o.get("id"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            id_a.cmp(&id_b)
                        });
                    }
                    normalize_json(val);
                } else {
                    normalize_json(val);
                }
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                normalize_json(val);
            }
        }
        _ => {}
    }
}

/// Captures state, runs an action, then verifies undo/redo invariants.
fn assert_undo_redo_roundtrip(
    designer: &mut StructureDesigner,
    action: impl FnOnce(&mut StructureDesigner),
) {
    let before = snapshot_all_networks(&mut designer.node_type_registry);

    action(designer);

    let after = snapshot_all_networks(&mut designer.node_type_registry);

    // Property 1: do + undo = identity
    let undone = designer.undo();
    assert!(undone, "undo() should return true after a command");
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        before, after_undo,
        "State after undo should match state before action"
    );

    // Property 2: do + undo + redo = do
    let redone = designer.redo();
    assert!(redone, "redo() should return true after an undo");
    let after_redo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        after, after_redo,
        "State after redo should match state after action"
    );

    // Undo again to leave designer in original state for composability
    designer.undo();
}

// --- Dummy command for UndoStack unit tests ---

#[derive(Debug)]
struct DummyCommand {
    description: String,
}

impl DummyCommand {
    fn new(desc: &str) -> Self {
        Self {
            description: desc.to_string(),
        }
    }
}

impl UndoCommand for DummyCommand {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&self, _ctx: &mut UndoContext) {
        // no-op for testing
    }

    fn redo(&self, _ctx: &mut UndoContext) {
        // no-op for testing
    }

    fn refresh_mode(&self) -> UndoRefreshMode {
        UndoRefreshMode::Lightweight
    }
}

// ===== UndoStack Unit Tests =====

#[test]
fn undo_stack_empty_stack_returns_none() {
    let mut stack = UndoStack::default();
    let mut designer = setup_designer_with_network("test");
    let mut ctx = UndoContext {
        node_type_registry: &mut designer.node_type_registry,
        active_network_name: &mut designer.active_node_network_name,
    };

    assert!(!stack.can_undo());
    assert!(!stack.can_redo());
    assert!(stack.undo(&mut ctx).is_none());
    assert!(stack.redo(&mut ctx).is_none());
}

#[test]
fn undo_stack_push_undo_redo_cursor_behavior() {
    let mut stack = UndoStack::default();
    let mut designer = setup_designer_with_network("test");

    // Push 3 commands
    stack.push(Box::new(DummyCommand::new("cmd1")));
    stack.push(Box::new(DummyCommand::new("cmd2")));
    stack.push(Box::new(DummyCommand::new("cmd3")));

    assert!(stack.can_undo());
    assert!(!stack.can_redo());
    assert_eq!(stack.undo_description(), Some("cmd3"));

    // Undo one
    let mut ctx = UndoContext {
        node_type_registry: &mut designer.node_type_registry,
        active_network_name: &mut designer.active_node_network_name,
    };
    assert!(stack.undo(&mut ctx).is_some());

    assert!(stack.can_undo());
    assert!(stack.can_redo());
    assert_eq!(stack.undo_description(), Some("cmd2"));
    assert_eq!(stack.redo_description(), Some("cmd3"));

    // Undo all remaining
    assert!(stack.undo(&mut ctx).is_some());
    assert!(stack.undo(&mut ctx).is_some());
    assert!(!stack.can_undo());
    assert!(stack.can_redo());
    assert!(stack.undo(&mut ctx).is_none());

    // Redo all
    assert!(stack.redo(&mut ctx).is_some());
    assert!(stack.redo(&mut ctx).is_some());
    assert!(stack.redo(&mut ctx).is_some());
    assert!(stack.can_undo());
    assert!(!stack.can_redo());
    assert!(stack.redo(&mut ctx).is_none());
}

#[test]
fn undo_stack_redo_tail_truncation_on_push() {
    let mut stack = UndoStack::default();
    let mut designer = setup_designer_with_network("test");
    let mut ctx = UndoContext {
        node_type_registry: &mut designer.node_type_registry,
        active_network_name: &mut designer.active_node_network_name,
    };

    stack.push(Box::new(DummyCommand::new("cmd1")));
    stack.push(Box::new(DummyCommand::new("cmd2")));
    stack.push(Box::new(DummyCommand::new("cmd3")));

    // Undo 2
    stack.undo(&mut ctx);
    stack.undo(&mut ctx);

    // Push a new command — should truncate cmd2 and cmd3
    stack.push(Box::new(DummyCommand::new("cmd4")));

    assert!(stack.can_undo());
    assert!(!stack.can_redo()); // cmd2 and cmd3 are gone

    // Undo all: should be cmd4 then cmd1
    assert_eq!(stack.undo_description(), Some("cmd4"));
    stack.undo(&mut ctx);
    assert_eq!(stack.undo_description(), Some("cmd1"));
    stack.undo(&mut ctx);
    assert!(!stack.can_undo());
}

#[test]
fn undo_stack_max_history_eviction() {
    let mut stack = UndoStack::default();
    stack.max_history = 3;

    stack.push(Box::new(DummyCommand::new("cmd1")));
    stack.push(Box::new(DummyCommand::new("cmd2")));
    stack.push(Box::new(DummyCommand::new("cmd3")));
    stack.push(Box::new(DummyCommand::new("cmd4"))); // cmd1 should be evicted

    let mut designer = setup_designer_with_network("test");
    let mut ctx = UndoContext {
        node_type_registry: &mut designer.node_type_registry,
        active_network_name: &mut designer.active_node_network_name,
    };

    // Can only undo 3 times (cmd4, cmd3, cmd2), not 4
    assert!(stack.undo(&mut ctx).is_some()); // undo cmd4
    assert!(stack.undo(&mut ctx).is_some()); // undo cmd3
    assert!(stack.undo(&mut ctx).is_some()); // undo cmd2
    assert!(stack.undo(&mut ctx).is_none()); // cmd1 was evicted
}

#[test]
fn undo_stack_clear() {
    let mut stack = UndoStack::default();
    stack.push(Box::new(DummyCommand::new("cmd1")));
    stack.push(Box::new(DummyCommand::new("cmd2")));

    stack.clear();

    assert!(!stack.can_undo());
    assert!(!stack.can_redo());
}

#[test]
fn undo_stack_suppression() {
    let mut stack = UndoStack::default();

    stack.push(Box::new(DummyCommand::new("cmd1")));

    stack.suppress_recording();
    stack.push(Box::new(DummyCommand::new("suppressed")));
    stack.resume_recording();

    stack.push(Box::new(DummyCommand::new("cmd2")));

    // Only cmd1 and cmd2 should be in the stack (suppressed was ignored)
    assert_eq!(stack.undo_description(), Some("cmd2"));

    let mut designer = setup_designer_with_network("test");
    let mut ctx = UndoContext {
        node_type_registry: &mut designer.node_type_registry,
        active_network_name: &mut designer.active_node_network_name,
    };

    stack.undo(&mut ctx);
    assert_eq!(stack.undo_description(), Some("cmd1"));
    stack.undo(&mut ctx);
    assert!(!stack.can_undo());
}

// ===== StructureDesigner undo/redo method tests =====

#[test]
fn undo_on_empty_stack_returns_false() {
    let mut designer = setup_designer_with_network("test");
    assert!(!designer.undo());
    assert!(!designer.redo());
}

#[test]
fn undo_stack_cleared_on_new_project() {
    let mut designer = setup_designer_with_network("test");
    // Push a dummy command
    designer.push_command(DummyCommand::new("some edit"));
    assert!(designer.undo_stack.can_undo());

    designer.new_project();

    assert!(!designer.undo_stack.can_undo());
    assert!(!designer.undo_stack.can_redo());
}

// ===== Snapshot helper test =====

#[test]
fn snapshot_all_networks_deterministic() {
    let mut designer = setup_designer_with_network("test");
    let snap1 = snapshot_all_networks(&mut designer.node_type_registry);
    let snap2 = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(snap1, snap2, "Consecutive snapshots should be identical");
}

// ===== SetNodeData command tests =====

#[test]
fn undo_set_node_data_float() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("float", glam::DVec2::ZERO);
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        let new_data = Box::new(FloatData { value: 42.0 });
        d.set_node_network_data(node_id, new_data);
    });
}

#[test]
fn undo_set_node_data_vec3() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("vec3", glam::DVec2::ZERO);
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        let new_data = Box::new(Vec3Data {
            value: DVec3::new(1.0, 2.0, 3.0),
        });
        d.set_node_network_data(node_id, new_data);
    });
}

#[test]
fn undo_set_node_data_multiple_edits() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("float", glam::DVec2::ZERO);
    designer.undo_stack.clear();

    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Edit 1
    designer.set_node_network_data(node_id, Box::new(FloatData { value: 10.0 }));
    let after_edit1 = snapshot_all_networks(&mut designer.node_type_registry);

    // Edit 2
    designer.set_node_network_data(node_id, Box::new(FloatData { value: 20.0 }));
    let after_edit2 = snapshot_all_networks(&mut designer.node_type_registry);

    // Edit 3
    designer.set_node_network_data(node_id, Box::new(FloatData { value: 30.0 }));

    // Undo all 3 edits
    assert!(designer.undo()); // undo edit 3
    let state = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(after_edit2, state);

    assert!(designer.undo()); // undo edit 2
    let state = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(after_edit1, state);

    assert!(designer.undo()); // undo edit 1
    let state = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, state);

    assert!(!designer.undo()); // nothing left
}

#[test]
fn undo_set_node_data_no_change_produces_no_command() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("float", glam::DVec2::ZERO);
    designer.undo_stack.clear();

    // Set the same default value — should produce no command since old == new
    designer.set_node_network_data(node_id, Box::new(FloatData { value: 0.0 }));

    assert!(
        !designer.undo_stack.can_undo(),
        "No command should be pushed when data doesn't change"
    );
}

// ===== AddNode command tests =====

#[test]
fn undo_add_node() {
    let mut designer = setup_designer_with_network("test");
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.add_node("sphere", glam::DVec2::new(100.0, 50.0));
    });
}

#[test]
fn undo_add_node_verifies_empty_after_undo() {
    let mut designer = setup_designer_with_network("test");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    designer.add_node("sphere", glam::DVec2::ZERO);
    assert!(designer.undo());

    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        initial, after_undo,
        "Network should be empty after undoing add_node"
    );
    assert!(!designer.undo()); // nothing left
}

// ===== DeleteNodes command tests =====

#[test]
fn undo_delete_single_node() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("sphere", glam::DVec2::ZERO);
    // Select the node for deletion
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(node_id);
    }
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.delete_selected();
    });
}

#[test]
fn undo_delete_connected_nodes() {
    let mut designer = setup_designer_with_network("test");
    let float_id = designer.add_node("float", glam::DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", glam::DVec2::new(200.0, 0.0));
    let cuboid_id = designer.add_node("cuboid", glam::DVec2::new(200.0, 200.0));
    // Connect float -> sphere (param 0) and float -> cuboid (param 0)
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    designer.connect_nodes(float_id, 0, cuboid_id, 0);

    // Select the float node (which has wires to other nodes)
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(float_id);
    }
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.delete_selected();
    });
}

#[test]
fn undo_delete_wires_only() {
    let mut designer = setup_designer_with_network("test");
    let float_id = designer.add_node("float", glam::DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", glam::DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);

    // Select the wire (not the node)
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_wire(float_id, 0, sphere_id, 0);
    }
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.delete_selected();
    });
}

#[test]
fn undo_delete_return_node() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", glam::DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));

    // Select the return node for deletion
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(sphere_id);
    }
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.delete_selected();
    });
}

// ===== DuplicateNode command tests =====

#[test]
fn undo_duplicate_node() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("sphere", glam::DVec2::ZERO);
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.duplicate_node(node_id);
    });
}

#[test]
fn undo_duplicate_node_only_original_remains() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("sphere", glam::DVec2::ZERO);
    designer.undo_stack.clear();

    let before = snapshot_all_networks(&mut designer.node_type_registry);

    let new_id = designer.duplicate_node(node_id);
    assert!(new_id != 0);

    // Undo should remove the duplicate
    assert!(designer.undo());
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        before, after_undo,
        "Only original node should remain after undoing duplicate"
    );
}

// ===== Sequence tests (add + delete) =====

#[test]
fn undo_sequence_add_delete() {
    let mut designer = setup_designer_with_network("test");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Add 3 nodes
    let id1 = designer.add_node("sphere", glam::DVec2::ZERO);
    let id2 = designer.add_node("cuboid", glam::DVec2::new(200.0, 0.0));
    let _id3 = designer.add_node("float", glam::DVec2::new(0.0, 200.0));

    // Delete 2 of them
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id1);
        network.toggle_node_selection(id2);
    }
    designer.delete_selected();

    // Undo all 4 operations (delete, add, add, add)
    for _ in 0..4 {
        assert!(designer.undo());
    }
    assert!(!designer.undo()); // nothing left

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo_all);
}

// ===== Phase 4: ConnectWire Tests =====

#[test]
fn undo_connect_wire() {
    let mut designer = setup_designer_with_network("test");
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.connect_nodes(float_id, 0, sphere_id, 0);
    });
}

#[test]
fn undo_connect_wire_that_replaced_existing() {
    let mut designer = setup_designer_with_network("test");
    let float1 = designer.add_node("float", DVec2::ZERO);
    let float2 = designer.add_node("float", DVec2::new(0.0, 100.0));
    let sphere = designer.add_node("sphere", DVec2::new(200.0, 0.0));

    // Connect float1 → sphere pin 0
    designer.connect_nodes(float1, 0, sphere, 0);
    designer.undo_stack.clear();

    // Connect float2 → sphere pin 0 (replaces float1's wire)
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.connect_nodes(float2, 0, sphere, 0);
    });
    // After undo: float1's wire should be restored
}

// ===== Phase 4: MoveNodes Tests =====

#[test]
fn undo_move_nodes() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id);
    }
    designer.undo_stack.clear();

    // Move uses begin/end grouping
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.begin_move_nodes();
        d.move_selected_nodes(DVec2::new(10.0, 0.0));
        d.move_selected_nodes(DVec2::new(10.0, 0.0));
        d.move_selected_nodes(DVec2::new(10.0, 0.0));
        d.end_move_nodes();
    });
}

#[test]
fn move_without_actual_movement_creates_no_command() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id);
    }
    designer.undo_stack.clear();

    designer.begin_move_nodes();
    // No move_selected_nodes calls — click without drag
    designer.end_move_nodes();

    assert!(!designer.undo()); // No command was created
}

#[test]
fn undo_move_multiple_drags_are_separate_commands() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id);
    }
    designer.undo_stack.clear();

    // First drag
    designer.begin_move_nodes();
    designer.move_selected_nodes(DVec2::new(50.0, 0.0));
    designer.end_move_nodes();

    // Second drag
    designer.begin_move_nodes();
    designer.move_selected_nodes(DVec2::new(0.0, 50.0));
    designer.end_move_nodes();

    // Should be 2 separate undo steps
    assert!(designer.undo()); // undo second drag
    assert!(designer.undo()); // undo first drag
    assert!(!designer.undo()); // nothing left
}

// ===== Phase 4: SetReturnNode Tests =====

#[test]
fn undo_set_return_node() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.set_return_node_id(Some(sphere_id));
    });
}

#[test]
fn undo_clear_return_node() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.set_return_node_id(None);
    });
}

// ===== Phase 4: SetNodeDisplay Tests =====

#[test]
fn undo_set_node_display() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    // Clear stack (add_node may have set display)
    designer.undo_stack.clear();

    // Check current display state and toggle it
    let is_displayed = designer
        .node_type_registry
        .node_networks
        .get("test")
        .map(|net| net.is_node_displayed(sphere_id))
        .unwrap_or(false);

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.set_node_display(sphere_id, !is_displayed);
    });
}

#[test]
fn undo_set_node_display_toggle_twice() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);

    // Ensure node starts displayed, then turn it off so we have a known starting state
    designer.set_node_display(sphere_id, false);
    designer.undo_stack.clear();

    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Display on, then off — two state changes
    designer.set_node_display(sphere_id, true);
    designer.set_node_display(sphere_id, false);

    // Undo both
    assert!(designer.undo());
    assert!(designer.undo());

    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo);
}

// ===== Phase 5: PasteNodes Tests =====

#[test]
fn undo_paste_nodes() {
    let mut designer = setup_designer_with_network("test");

    // Add a node and select it for copy
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(sphere_id);
    }

    // Copy selection
    designer.copy_selection();
    designer.undo_stack.clear();

    // Paste and verify undo/redo roundtrip
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.paste_at_position(DVec2::new(200.0, 100.0));
    });
}

#[test]
fn undo_paste_connected_nodes() {
    let mut designer = setup_designer_with_network("test");

    // Build a small graph: float -> sphere
    let float_id = designer.add_node("float", DVec2::ZERO);
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));
    designer.connect_nodes(float_id, 0, sphere_id, 0);

    // Select both nodes
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(float_id);
        network.select_node(sphere_id);
    }

    // Copy selection
    designer.copy_selection();
    designer.undo_stack.clear();

    // Paste and verify undo/redo roundtrip (wires between pasted nodes should be preserved)
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.paste_at_position(DVec2::new(0.0, 200.0));
    });
}

#[test]
fn undo_cut_is_single_step() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(sphere_id);
    }
    designer.undo_stack.clear();

    // Cut = copy + delete → single undo step (only delete pushes a command)
    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.cut_selection();
    });
}

#[test]
fn undo_paste_multiple_times() {
    let mut designer = setup_designer_with_network("test");

    // Add a node and copy it
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(sphere_id);
    }
    designer.copy_selection();
    designer.undo_stack.clear();

    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Paste 3 times
    designer.paste_at_position(DVec2::new(100.0, 0.0));
    designer.paste_at_position(DVec2::new(200.0, 0.0));
    designer.paste_at_position(DVec2::new(300.0, 0.0));

    // Undo all 3 pastes
    assert!(designer.undo());
    assert!(designer.undo());
    assert!(designer.undo());
    assert!(!designer.undo()); // Nothing left

    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo);
}

// ===== Phase 6: Network-Level Command Tests =====

#[test]
fn undo_add_network() {
    let mut designer = setup_designer_with_network("main");
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.add_new_node_network();
    });
}

#[test]
fn undo_add_network_restores_active() {
    let mut designer = setup_designer_with_network("main");
    designer.undo_stack.clear();

    assert_eq!(designer.active_node_network_name, Some("main".to_string()));

    // add_new_node_network does NOT change active_node_network_name on its own
    // (only the API layer calls set_active_node_network_name)
    designer.add_new_node_network();
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("UNTITLED")
    );

    // Undo: network removed, active unchanged
    designer.undo();
    assert!(
        !designer
            .node_type_registry
            .node_networks
            .contains_key("UNTITLED")
    );
    assert_eq!(designer.active_node_network_name, Some("main".to_string()));
}

#[test]
fn undo_delete_network_restores_contents() {
    let mut designer = setup_designer_with_network("main");

    // Build a network with content
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));
    designer.undo_stack.clear();

    let before_delete = snapshot_all_networks(&mut designer.node_type_registry);

    // Switch to main and delete helper
    designer.set_active_node_network_name(Some("main".to_string()));
    designer.delete_node_network("helper").unwrap();

    assert!(
        !designer
            .node_type_registry
            .node_networks
            .contains_key("helper")
    );

    // Undo: network and all contents should be restored
    designer.undo();
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(before_delete, after_undo);
}

#[test]
fn undo_delete_network_roundtrip() {
    let mut designer = setup_designer_with_network("main");

    // Create and populate a network
    designer.add_node_network("helper");
    designer.set_active_node_network_name(Some("helper".to_string()));
    designer.add_node("sphere", DVec2::ZERO);
    designer.add_node("float", DVec2::new(200.0, 0.0));
    designer.undo_stack.clear();

    // Switch to main to delete
    designer.set_active_node_network_name(Some("main".to_string()));

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.delete_node_network("helper").unwrap();
    });
}

#[test]
fn undo_rename_network() {
    let mut designer = setup_designer_with_network("alpha");
    designer.undo_stack.clear();

    assert_undo_redo_roundtrip(&mut designer, |d| {
        d.rename_node_network("alpha", "beta");
    });
}

#[test]
fn undo_rename_then_undo_earlier_commands() {
    let mut designer = setup_designer_with_network("alpha");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    designer.add_node("sphere", DVec2::ZERO); // targets "alpha"
    designer.rename_node_network("alpha", "beta"); // rename

    // Undo rename — network is "alpha" again
    assert!(designer.undo());
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("alpha")
    );
    assert!(
        !designer
            .node_type_registry
            .node_networks
            .contains_key("beta")
    );

    // Undo add_node — targets "alpha", which exists
    assert!(designer.undo());

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo_all);
}

#[test]
fn undo_across_network_switch() {
    let mut designer = setup_designer_with_network("net_a");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Work in net_a
    designer.add_node("sphere", DVec2::ZERO);

    // Add net_b and work in it
    designer.add_new_node_network(); // creates "UNTITLED"
    designer.set_active_node_network_name(Some("UNTITLED".to_string()));
    designer.add_node("cuboid", DVec2::ZERO);

    // Undo all (while active network is UNTITLED)
    // add_node(cuboid), add_network(UNTITLED), add_node(sphere)
    assert!(designer.undo()); // undo cuboid
    assert!(designer.undo()); // undo add network
    assert!(designer.undo()); // undo sphere

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo_all);
}

// ===== Phase 7: TextEditNetwork + FactorSelection =====

/// Helper to apply a text edit to a network (mirrors the API layer pattern).
/// Removes the network from the registry, applies the edit, puts it back,
/// and pushes an undo command.
fn apply_text_edit(designer: &mut StructureDesigner, network_name: &str, code: &str) {
    use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::node_network_to_serializable;
    use rust_lib_flutter_cad::structure_designer::undo::commands::text_edit_network::TextEditNetworkCommand;

    // Temporarily remove network from registry
    let mut network = designer
        .node_type_registry
        .node_networks
        .remove(network_name)
        .expect("Network not found");

    // Snapshot before
    let before_snapshot = node_network_to_serializable(
        &mut network,
        &designer.node_type_registry.built_in_node_types,
        None,
    )
    .ok();

    // Apply text edit in replace mode
    let result = text_edit_network(&mut network, &designer.node_type_registry, code, true);

    // Snapshot after
    let after_snapshot = node_network_to_serializable(
        &mut network,
        &designer.node_type_registry.built_in_node_types,
        None,
    )
    .ok();

    // Put network back
    designer
        .node_type_registry
        .node_networks
        .insert(network_name.to_string(), network);

    // Validate network
    designer.validate_active_network();

    // Push undo command if changes were made
    let made_changes = result.success
        && (!result.nodes_created.is_empty()
            || !result.nodes_updated.is_empty()
            || !result.nodes_deleted.is_empty()
            || !result.connections_made.is_empty());
    if made_changes {
        if let (Some(before), Some(after)) = (before_snapshot, after_snapshot) {
            designer.push_command(TextEditNetworkCommand {
                network_name: network_name.to_string(),
                before_snapshot: before,
                after_snapshot: after,
            });
        }
    }

    designer.mark_full_refresh();
    designer.set_dirty(true);
}

#[test]
fn undo_text_edit_network() {
    let mut designer = setup_designer_with_network("test");

    // Start with a sphere node
    designer.add_node("sphere", DVec2::new(100.0, 50.0));
    designer.undo_stack.clear();

    let before = snapshot_all_networks(&mut designer.node_type_registry);

    // Apply text edit that replaces the network content
    apply_text_edit(
        &mut designer,
        "test",
        "my_cuboid = cuboid { size: (10, 10, 10) }\noutput my_cuboid",
    );

    let after = snapshot_all_networks(&mut designer.node_type_registry);
    assert_ne!(before, after, "Text edit should have changed the network");

    // Undo should restore the original state
    assert!(designer.undo());
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        before, after_undo,
        "State after undo should match state before text edit"
    );

    // Redo should restore the text-edited state
    assert!(designer.redo());
    let after_redo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        after, after_redo,
        "State after redo should match state after text edit"
    );
}

#[test]
fn undo_text_edit_network_multiple_edits() {
    let mut designer = setup_designer_with_network("test");
    designer.undo_stack.clear();

    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // First text edit: add a sphere
    apply_text_edit(
        &mut designer,
        "test",
        "s = sphere { radius: 5.0 }\noutput s",
    );
    let after_first = snapshot_all_networks(&mut designer.node_type_registry);

    // Second text edit: replace with cuboid
    apply_text_edit(
        &mut designer,
        "test",
        "c = cuboid { size: (10, 10, 10) }\noutput c",
    );

    // Undo second edit
    assert!(designer.undo());
    let after_undo_second = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(after_first, after_undo_second);

    // Undo first edit
    assert!(designer.undo());
    let after_undo_first = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo_first);
}

#[test]
fn undo_factor_selection() {
    let mut designer = setup_designer_with_network("test");

    // Build a simple graph: just a sphere node (no external inputs when only it is selected)
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));

    // Select the sphere for factoring
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(sphere_id);
    }
    designer.undo_stack.clear();

    let before = snapshot_all_networks(&mut designer.node_type_registry);

    // Factor selection into a subnetwork (no external inputs for a standalone node)
    let result = designer.factor_selection_into_subnetwork("my_sub", vec![]);
    assert!(result.is_ok(), "Factor should succeed: {:?}", result);

    // Verify subnetwork was created
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("my_sub"),
        "Subnetwork should exist after factoring"
    );

    let after = snapshot_all_networks(&mut designer.node_type_registry);
    assert_ne!(before, after, "Factoring should have changed the state");

    // Undo should remove the subnetwork and restore the source network
    assert!(designer.undo());
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        before, after_undo,
        "State after undo should match state before factoring"
    );
    assert!(
        !designer
            .node_type_registry
            .node_networks
            .contains_key("my_sub"),
        "Subnetwork should be removed after undo"
    );

    // Redo should recreate the subnetwork
    assert!(designer.redo());
    let after_redo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        after, after_redo,
        "State after redo should match state after factoring"
    );
    assert!(
        designer
            .node_type_registry
            .node_networks
            .contains_key("my_sub"),
        "Subnetwork should exist again after redo"
    );
}

// ===== Phase 8: Integration / Sequence Tests =====

#[test]
fn undo_full_workflow() {
    // Add nodes, connect, edit data, move, delete, undo all → initial state
    let mut designer = setup_designer_with_network("test");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // 1. Add a sphere node
    let sphere_id = designer.add_node("sphere", DVec2::new(100.0, 50.0));
    // 2. Add a float node
    let float_id = designer.add_node("float", DVec2::new(0.0, 0.0));
    // 3. Connect float → sphere
    designer.connect_nodes(float_id, 0, sphere_id, 0);
    // 4. Edit float data
    designer.set_node_network_data(float_id, Box::new(FloatData { value: 42.0 }));
    // 5. Move nodes
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(sphere_id);
    }
    designer.begin_move_nodes();
    designer.move_selected_nodes(DVec2::new(50.0, 0.0));
    designer.end_move_nodes();
    // 6. Set return node
    designer.set_return_node_id(Some(sphere_id));

    // Undo all 6 operations
    for i in 0..6 {
        assert!(designer.undo(), "undo #{} should succeed", i + 1);
    }
    assert!(!designer.undo(), "Nothing left to undo");

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        initial, after_undo_all,
        "Undoing all should restore initial state"
    );

    // Redo all 6
    for i in 0..6 {
        assert!(designer.redo(), "redo #{} should succeed", i + 1);
    }
    assert!(!designer.redo(), "Nothing left to redo");

    // Undo all again to verify full cycle
    for _ in 0..6 {
        assert!(designer.undo());
    }
    let after_second_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        initial, after_second_undo,
        "Second undo-all should also restore initial state"
    );
}

#[test]
fn undo_redo_interleaved() {
    // Do 5 ops, undo 3, do 2 new ops, undo all → initial state
    let mut designer = setup_designer_with_network("test");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // 5 operations
    let sphere_id = designer.add_node("sphere", DVec2::new(0.0, 0.0)); // cmd 1
    let float_id = designer.add_node("float", DVec2::new(100.0, 0.0)); // cmd 2
    designer.add_node("cuboid", DVec2::new(200.0, 0.0)); // cmd 3
    designer.connect_nodes(float_id, 0, sphere_id, 0); // cmd 4
    designer.set_return_node_id(Some(sphere_id)); // cmd 5

    // Undo 3 (undo cmd 5, 4, 3)
    assert!(designer.undo());
    assert!(designer.undo());
    assert!(designer.undo());

    // Do 2 new ops (truncates redo tail)
    let f1 = designer.add_node("float", DVec2::new(300.0, 0.0)); // cmd 6
    designer.set_node_network_data(f1, Box::new(FloatData { value: 7.0 })); // cmd 7

    assert!(!designer.redo(), "Redo should be empty after new commands");

    // Undo all remaining: cmd 7, 6, 2, 1
    for i in 0..4 {
        assert!(designer.undo(), "undo #{} should succeed", i + 1);
    }
    assert!(!designer.undo(), "Nothing left to undo");

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        initial, after_undo_all,
        "Undoing all should restore initial state"
    );
}

#[test]
fn undo_max_history_eviction_with_real_commands() {
    let mut designer = setup_designer_with_network("test");
    designer.undo_stack.max_history = 3;

    designer.add_node("sphere", DVec2::ZERO); // cmd 1
    designer.add_node("cuboid", DVec2::ZERO); // cmd 2
    designer.add_node("extrude", DVec2::ZERO); // cmd 3
    designer.add_node("float", DVec2::ZERO); // cmd 4 — drops cmd 1

    // Can only undo 3 times, not 4
    assert!(designer.undo()); // undo cmd 4
    assert!(designer.undo()); // undo cmd 3
    assert!(designer.undo()); // undo cmd 2
    assert!(!designer.undo()); // cmd 1 was evicted

    // The network should still have the sphere from cmd 1 (which was evicted and can't be undone)
    let network = designer
        .node_type_registry
        .node_networks
        .get("test")
        .unwrap();
    assert_eq!(
        network.nodes.len(),
        1,
        "Only the evicted sphere node should remain"
    );
}

#[test]
fn redo_tail_truncation_after_new_command() {
    let mut designer = setup_designer_with_network("test");

    designer.add_node("sphere", DVec2::ZERO); // cmd 1
    designer.add_node("cuboid", DVec2::ZERO); // cmd 2

    designer.undo(); // undo cmd 2, now cmd 2 is in redo tail
    assert!(
        designer.undo_stack.can_redo(),
        "cmd 2 should be in redo tail"
    );

    designer.add_node("extrude", DVec2::ZERO); // cmd 3 — truncates cmd 2

    assert!(
        !designer.undo_stack.can_redo(),
        "Redo tail should be truncated after new command"
    );
    assert!(designer.undo()); // undo cmd 3
    assert!(designer.undo()); // undo cmd 1
    assert!(!designer.undo()); // nothing left
}

#[test]
fn undo_sequence_restores_initial_state() {
    let mut designer = setup_designer_with_network("test");
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Perform a sequence of varied operations
    let sphere_id = designer.add_node("sphere", DVec2::ZERO); // cmd 1
    let float_id = designer.add_node("float", DVec2::new(200.0, 0.0)); // cmd 2
    designer.connect_nodes(float_id, 0, sphere_id, 0); // cmd 3
    designer.set_return_node_id(Some(sphere_id)); // cmd 4

    // Undo all 4
    for _ in 0..4 {
        assert!(designer.undo());
    }
    assert!(!designer.undo()); // Nothing left to undo

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo_all);

    // Redo all 4
    for _ in 0..4 {
        assert!(designer.redo());
    }
    assert!(!designer.redo()); // Nothing left to redo

    // Undo all again to verify full cycle
    for _ in 0..4 {
        assert!(designer.undo());
    }
    let after_second_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_second_undo_all);
}

#[test]
fn undo_factor_then_edit_then_undo_all() {
    let mut designer = setup_designer_with_network("test");

    // Build a graph: just a sphere node
    let sphere_id = designer.add_node("sphere", DVec2::new(200.0, 0.0));

    // Select the sphere and factor it
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(sphere_id);
    }
    designer.undo_stack.clear();
    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    designer
        .factor_selection_into_subnetwork("my_sub", vec![])
        .unwrap();

    // Switch to the subnetwork and add a node
    designer.set_active_node_network_name(Some("my_sub".to_string()));
    designer.add_node("cuboid", DVec2::new(100.0, 100.0));

    // Undo all: undo add_node, undo factor
    assert!(designer.undo()); // undo add cuboid in subnetwork
    assert!(designer.undo()); // undo factor

    // Need to switch back since undo of factor doesn't change active network
    designer.set_active_node_network_name(Some("test".to_string()));

    let after_undo_all = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(
        initial, after_undo_all,
        "Undoing all should restore to pre-factor state"
    );
}

// --- Gadget drag undo tests ---

#[test]
fn undo_gadget_drag_lattice_move() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("lattice_move", DVec2::ZERO);
    // Select the node so it becomes the active node
    designer.select_node(node_id);
    designer.undo_stack.clear();

    let initial = snapshot_all_networks(&mut designer.node_type_registry);

    // Simulate gadget drag: begin snapshot, modify data, end snapshot
    designer.begin_gadget_drag_snapshot();

    // Modify the node data as a gadget's sync_data() would
    let new_data = Box::new(LatticeMoveData {
        translation: IVec3::new(3, 0, 0),
        lattice_subdivision: 1,
        is_atomic_mode: false,
    });
    designer.set_node_network_data(node_id, new_data);

    designer.end_gadget_drag_snapshot();

    let after_drag = snapshot_all_networks(&mut designer.node_type_registry);
    assert_ne!(initial, after_drag, "Drag should change node data");

    // The undo stack should have 2 commands: 1 from set_node_network_data + 1 from gadget drag
    // But we want to test that the gadget drag command works.
    // Actually, set_node_network_data also pushes a SetNodeDataCommand (since it's not atom_edit).
    // So we have 2 commands. The gadget drag command is the last one.
    // Undo the gadget drag command
    assert!(designer.undo());
    // Undo the set_node_network_data command
    assert!(designer.undo());

    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(initial, after_undo, "Undo should restore initial state");
}

#[test]
fn undo_gadget_drag_simulated_no_change() {
    // If gadget drag doesn't actually change data, no command should be pushed
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("lattice_move", DVec2::ZERO);
    designer.select_node(node_id);
    designer.undo_stack.clear();

    // Begin and end without modifying data
    designer.begin_gadget_drag_snapshot();
    designer.end_gadget_drag_snapshot();

    // No command should be pushed
    assert!(
        !designer.undo_stack.can_undo(),
        "No undo command should exist when gadget drag didn't change data"
    );
}

#[test]
fn undo_redo_gadget_drag_roundtrip() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("vec3", DVec2::ZERO);
    designer.select_node(node_id);
    designer.undo_stack.clear();

    // Simulate a gadget drag that changes vec3 data (as if a viewport gadget moved it)
    // Use begin/end_gadget_drag_snapshot directly to isolate the gadget undo mechanism
    // without the double-push from set_node_network_data.
    let before = snapshot_all_networks(&mut designer.node_type_registry);

    designer.begin_gadget_drag_snapshot();

    // Directly set node data on the network (bypassing set_node_network_data to avoid
    // its own undo push — simulating what sync_gadget_data does internally)
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.set_node_network_data(
            node_id,
            Box::new(Vec3Data {
                value: DVec3::new(5.0, 10.0, 15.0),
            }),
        );
    }

    designer.end_gadget_drag_snapshot();

    let after = snapshot_all_networks(&mut designer.node_type_registry);
    assert_ne!(before, after, "Drag should change data");

    // Exactly one undo command from the gadget drag
    assert!(designer.undo_stack.can_undo());

    // Undo
    assert!(designer.undo());
    let after_undo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(before, after_undo, "Undo should restore initial state");

    // Redo
    assert!(designer.redo());
    let after_redo = snapshot_all_networks(&mut designer.node_type_registry);
    assert_eq!(after, after_redo, "Redo should restore post-drag state");

    // No more redo
    assert!(!designer.undo_stack.can_redo());
}

// ===== Undo Description Tests =====

#[test]
fn undo_description_add_node() {
    let mut designer = setup_designer_with_network("test");
    designer.add_node("sphere", DVec2::ZERO);
    assert_eq!(designer.undo_stack.undo_description(), Some("Add sphere"));
}

#[test]
fn undo_description_add_node_cuboid() {
    let mut designer = setup_designer_with_network("test");
    designer.add_node("cuboid", DVec2::ZERO);
    assert_eq!(designer.undo_stack.undo_description(), Some("Add cuboid"));
}

#[test]
fn undo_description_edit_node() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("float", DVec2::ZERO);
    designer.undo_stack.clear();

    designer.set_node_network_data(node_id, Box::new(FloatData { value: 42.0 }));
    assert_eq!(designer.undo_stack.undo_description(), Some("Edit float"));
}

#[test]
fn undo_description_edit_node_vec3() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("vec3", DVec2::ZERO);
    designer.undo_stack.clear();

    designer.set_node_network_data(
        node_id,
        Box::new(Vec3Data {
            value: DVec3::new(1.0, 2.0, 3.0),
        }),
    );
    assert_eq!(designer.undo_stack.undo_description(), Some("Edit vec3"));
}

#[test]
fn undo_description_duplicate_node() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("sphere", DVec2::ZERO);
    designer.undo_stack.clear();

    designer.duplicate_node(node_id);
    assert_eq!(
        designer.undo_stack.undo_description(),
        Some("Duplicate sphere")
    );
}

#[test]
fn undo_description_delete_single_node() {
    let mut designer = setup_designer_with_network("test");
    let node_id = designer.add_node("cuboid", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(node_id);
    }
    designer.undo_stack.clear();

    designer.delete_selected();
    assert_eq!(
        designer.undo_stack.undo_description(),
        Some("Delete cuboid")
    );
}

#[test]
fn undo_description_delete_multiple_nodes() {
    let mut designer = setup_designer_with_network("test");
    let id1 = designer.add_node("sphere", DVec2::ZERO);
    let id2 = designer.add_node("cuboid", DVec2::new(200.0, 0.0));
    let id3 = designer.add_node("float", DVec2::new(0.0, 200.0));
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id1);
        network.toggle_node_selection(id2);
        network.toggle_node_selection(id3);
    }
    designer.undo_stack.clear();

    designer.delete_selected();
    assert_eq!(
        designer.undo_stack.undo_description(),
        Some("Delete 3 nodes")
    );
}

#[test]
fn undo_description_move_single_node() {
    let mut designer = setup_designer_with_network("test");
    let id = designer.add_node("sphere", DVec2::ZERO);
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id);
    }
    designer.undo_stack.clear();

    designer.begin_move_nodes();
    designer.move_selected_nodes(DVec2::new(10.0, 0.0));
    designer.end_move_nodes();
    assert_eq!(designer.undo_stack.undo_description(), Some("Move sphere"));
}

#[test]
fn undo_description_move_multiple_nodes() {
    let mut designer = setup_designer_with_network("test");
    let id1 = designer.add_node("sphere", DVec2::ZERO);
    let id2 = designer.add_node("cuboid", DVec2::new(200.0, 0.0));
    if let Some(network) = designer.node_type_registry.node_networks.get_mut("test") {
        network.select_node(id1);
        network.toggle_node_selection(id2);
    }
    designer.undo_stack.clear();

    designer.begin_move_nodes();
    designer.move_selected_nodes(DVec2::new(10.0, 0.0));
    designer.end_move_nodes();
    assert_eq!(designer.undo_stack.undo_description(), Some("Move 2 nodes"));
}

#[test]
fn undo_description_set_return_node() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.undo_stack.clear();

    designer.set_return_node_id(Some(sphere_id));
    assert_eq!(
        designer.undo_stack.undo_description(),
        Some("Set sphere as return node")
    );
}

#[test]
fn undo_description_clear_return_node() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_return_node_id(Some(sphere_id));
    designer.undo_stack.clear();

    designer.set_return_node_id(None);
    assert_eq!(
        designer.undo_stack.undo_description(),
        Some("Clear sphere return node")
    );
}

#[test]
fn undo_description_toggle_node_display() {
    let mut designer = setup_designer_with_network("test");
    let sphere_id = designer.add_node("sphere", DVec2::ZERO);
    designer.set_node_display(sphere_id, false);
    designer.undo_stack.clear();

    designer.set_node_display(sphere_id, true);
    assert_eq!(
        designer.undo_stack.undo_description(),
        Some("Toggle sphere display")
    );
}
