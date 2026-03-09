use glam::{DVec2, DVec3};
use rust_lib_flutter_cad::structure_designer::node_type_registry::NodeTypeRegistry;
use rust_lib_flutter_cad::structure_designer::nodes::float::FloatData;
use rust_lib_flutter_cad::structure_designer::nodes::vec3::Vec3Data;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::{
    SerializableNodeTypeRegistryNetworks, node_network_to_serializable,
};
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
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
    };

    let mut value = serde_json::to_value(&container).unwrap();
    // Normalize HashMap-derived arrays for deterministic comparison
    normalize_json(&mut value);
    value
}

/// Sort arrays that come from HashMap iteration (displayed_node_ids)
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
