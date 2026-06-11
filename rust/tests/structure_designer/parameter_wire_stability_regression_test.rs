//! Regression tests for the cross-network parameter wire-jumbling bug.
//!
//! Design doc: `doc/design_parameter_wire_stability.md`.
//!
//! ## Confirmed root cause
//!
//! `NodeNetwork.next_param_id` (the per-network counter that hands out unique
//! `param_id`s for wire preservation) is **never serialized and never restored**:
//! `serialization::node_networks_serialization::serializable_to_node_network`
//! restores `next_node_id` but not `next_param_id`, and `duplicate_node_network`
//! copies via a serialize round-trip — so in both cases the counter resets to `1`
//! (`NodeNetwork::new`). The next parameter added to such a network is handed
//! `id = 1`, which **collides with the network's existing first parameter**
//! (ids start at 1). `network_validator::repair_call_sites_for_network` then
//! resolves the new param's id to the first param's old index and **clones that
//! wire onto the new pin** — the user's "new port connected to the same source as
//! a preceding input (despite a type error)". It only manifests after a project is
//! reopened (load) or a network is duplicated, which is why it reads as a
//! regression and every pure in-memory edit path is fine.
//!
//! ## Status: FIXED by F1 (these tests now pass and guard against re-introduction)
//!
//! The fix restores `next_param_id` in `serializable_to_node_network` — the single
//! deserialize chokepoint shared by `.cnnd` load, `duplicate_node_network`, and the
//! undo/snapshot-restore commands — by deriving it from the loaded parameter nodes
//! (`max(param_id) + 1`). The three `regression_*` tests below reproduce the bug
//! (they FAILED before F1); they now pass. Keep them green.
//!
//! ## Guards (must also stay green)
//!
//! The six `guard_*` tests document parameter-edit paths that were already correct
//! before F1 (HOF-body instances, reorder, in-memory save/load roundtrip, editing
//! an original after duplicating it, undo/redo, two-step add-then-reorder).

use glam::f64::DVec2;
use rust_lib_flutter_cad::structure_designer::data_type::DataType;
use rust_lib_flutter_cad::structure_designer::node_data::NodeData;
use rust_lib_flutter_cad::structure_designer::nodes::parameter::ParameterData;
use rust_lib_flutter_cad::structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
use rust_lib_flutter_cad::structure_designer::structure_designer::StructureDesigner;
use rust_lib_flutter_cad::structure_designer::text_format::TextValue;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn set_parameter_props(
    designer: &mut StructureDesigner,
    network_name: &str,
    node_id: u64,
    name: &str,
    data_type: DataType,
    sort_order: i32,
) {
    designer.set_active_node_network_name(Some(network_name.to_string()));
    let network = designer
        .node_type_registry
        .node_networks
        .get_mut(network_name)
        .unwrap();
    let node = network.nodes.get_mut(&node_id).unwrap();
    if let Some(param_data) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
        let mut props = HashMap::new();
        props.insert(
            "param_name".to_string(),
            TextValue::String(name.to_string()),
        );
        props.insert("data_type".to_string(), TextValue::DataType(data_type));
        props.insert("sort_order".to_string(), TextValue::Int(sort_order));
        props.insert(
            "param_index".to_string(),
            TextValue::Int(param_data.param_index as i32),
        );
        param_data.set_text_properties(&props).unwrap();
    }
    designer.validate_active_network();
}

/// Sorted list of source node ids wired into a TOP-LEVEL node's input pin.
fn srcs(
    designer: &StructureDesigner,
    network_name: &str,
    dest_node_id: u64,
    param_index: usize,
) -> Vec<u64> {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    let node = network.nodes.get(&dest_node_id).unwrap();
    let mut v: Vec<u64> = node
        .arguments
        .get(param_index)
        .map(|a| a.argument_output_pins().keys().copied().collect())
        .unwrap_or_default();
    v.sort_unstable();
    v
}

/// Number of input pins (arguments) the instance node currently has.
fn arg_count(designer: &StructureDesigner, network_name: &str, node_id: u64) -> usize {
    let network = designer
        .node_type_registry
        .node_networks
        .get(network_name)
        .unwrap();
    network.nodes.get(&node_id).unwrap().arguments.len()
}

/// Sorted list of source node ids wired into a node INSIDE an HOF body.
fn body_srcs(
    designer: &mut StructureDesigner,
    parent: &str,
    scope: &[u64],
    dest_node_id: u64,
    param_index: usize,
) -> Vec<u64> {
    designer.set_active_node_network_name(Some(parent.to_string()));
    let net = designer.get_scope_network(scope).unwrap();
    let node = net.nodes.get(&dest_node_id).unwrap();
    let mut v: Vec<u64> = node
        .arguments
        .get(param_index)
        .map(|a| a.argument_output_pins().keys().copied().collect())
        .unwrap_or_default();
    v.sort_unstable();
    v
}

/// Build a custom network `name` with the given (param_name, sort_order) Int
/// params plus an `int` return node. Returns the param node ids in order.
fn make_filter(designer: &mut StructureDesigner, name: &str, params: &[(&str, i32)]) -> Vec<u64> {
    designer.add_node_network(name);
    designer.set_active_node_network_name(Some(name.to_string()));
    let mut ids = Vec::new();
    for (i, (pname, sort)) in params.iter().enumerate() {
        let pid = designer.add_node("parameter", DVec2::new(0.0, i as f64 * 60.0));
        set_parameter_props(designer, name, pid, pname, DataType::Int, *sort);
        ids.push(pid);
    }
    let ret = designer.add_node("int", DVec2::new(200.0, 0.0));
    designer.set_return_node_id(Some(ret));
    designer.validate_active_network();
    ids
}

fn temp_path(file: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(file);
    p
}

// ###########################################################################
// REGRESSION (currently RED) — the bug. Fixer threads must turn these green.
// ###########################################################################

/// R1: reopen a project (.cnnd load), then add a parameter to a network that has
/// instances elsewhere. The new pin clones an existing pin's wire (id collision
/// from the reset `next_param_id`).
#[test]
fn regression_load_then_add_param_clones_neighbor_wire() {
    let mut designer = StructureDesigner::new();
    make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);

    let path = temp_path("pws_r1_add_after_load.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    let mut loaded = StructureDesigner::new();
    loaded.load_node_networks(path.to_str().unwrap()).unwrap();

    // Sanity: wires intact immediately after load.
    assert_eq!(srcs(&loaded, "main", f, 0), vec![i1], "post-load pin0<-i1");
    assert_eq!(srcs(&loaded, "main", f, 1), vec![i2], "post-load pin1<-i2");

    // Reproduce: add a NEW parameter to the loaded network.
    loaded.set_active_node_network_name(Some("Filt".to_string()));
    let p3 = loaded.add_node("parameter", DVec2::new(0.0, 120.0));
    set_parameter_props(&mut loaded, "Filt", p3, "third", DataType::Int, 2);

    eprintln!(
        "R1 after-load add-param: pin0={:?} pin1={:?} pin2={:?} arg_count={}",
        srcs(&loaded, "main", f, 0),
        srcs(&loaded, "main", f, 1),
        srcs(&loaded, "main", f, 2),
        arg_count(&loaded, "main", f),
    );

    assert_eq!(
        arg_count(&loaded, "main", f),
        3,
        "instance should have 3 pins"
    );
    assert_eq!(
        srcs(&loaded, "main", f, 0),
        vec![i1],
        "pin0 (first) must STILL carry i1"
    );
    assert_eq!(
        srcs(&loaded, "main", f, 1),
        vec![i2],
        "pin1 (last) must STILL carry i2"
    );
    assert_eq!(
        srcs(&loaded, "main", f, 2),
        Vec::<u64>::new(),
        "new pin (third) must be EMPTY, not a clone of an existing pin's wire"
    );
}

/// R2: same as R1, but distinct parameter types so the cloned wire lands on a pin
/// of the WRONG type — the user's "despite type error" observation.
#[test]
fn regression_load_then_add_param_clones_wrong_typed_wire() {
    let mut designer = StructureDesigner::new();
    designer.add_node_network("Filt");
    designer.set_active_node_network_name(Some("Filt".to_string()));
    let pa = designer.add_node("parameter", DVec2::new(0.0, 0.0));
    set_parameter_props(&mut designer, "Filt", pa, "first", DataType::Bool, 0);
    let pb = designer.add_node("parameter", DVec2::new(0.0, 60.0));
    set_parameter_props(&mut designer, "Filt", pb, "last", DataType::Int, 1);
    let ret = designer.add_node("int", DVec2::new(200.0, 0.0));
    designer.set_return_node_id(Some(ret));
    designer.validate_active_network();

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let b = designer.add_node("bool", DVec2::new(0.0, 0.0)); // Bool source -> first
    let n = designer.add_node("int", DVec2::new(0.0, 80.0)); // Int source  -> last
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(b, 0, f, 0);
    designer.connect_nodes(n, 0, f, 1);

    let path = temp_path("pws_r2_add_after_load_typed.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    let mut loaded = StructureDesigner::new();
    loaded.load_node_networks(path.to_str().unwrap()).unwrap();

    loaded.set_active_node_network_name(Some("Filt".to_string()));
    let p3 = loaded.add_node("parameter", DVec2::new(0.0, 120.0));
    set_parameter_props(&mut loaded, "Filt", p3, "third", DataType::Int, 2);

    eprintln!(
        "R2 typed after-load: pin0={:?} pin1={:?} pin2={:?}",
        srcs(&loaded, "main", f, 0),
        srcs(&loaded, "main", f, 1),
        srcs(&loaded, "main", f, 2),
    );

    assert!(
        !srcs(&loaded, "main", f, 2).contains(&b),
        "new Int pin must NOT inherit the Bool source (wrong-typed phantom wire)"
    );
    assert_eq!(
        srcs(&loaded, "main", f, 2),
        Vec::<u64>::new(),
        "new pin (third) must be empty"
    );
}

/// R3: second trigger of the same root cause — add a parameter to a DUPLICATED
/// network (the copy's `next_param_id` also reset via the serialize round-trip).
#[test]
fn regression_duplicate_then_add_param_corrupts_instance_wires() {
    let mut designer = StructureDesigner::new();
    make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    let copy_name = designer.duplicate_node_network("Filt").unwrap();

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node(&copy_name, DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);
    assert_eq!(srcs(&designer, "main", f, 0), vec![i1], "pre pin0<-i1");
    assert_eq!(srcs(&designer, "main", f, 1), vec![i2], "pre pin1<-i2");

    designer.set_active_node_network_name(Some(copy_name.clone()));
    let p3 = designer.add_node("parameter", DVec2::new(0.0, 120.0));
    set_parameter_props(&mut designer, &copy_name, p3, "third", DataType::Int, 2);

    eprintln!(
        "R3 duplicate add-param: pin0={:?} pin1={:?} pin2={:?}",
        srcs(&designer, "main", f, 0),
        srcs(&designer, "main", f, 1),
        srcs(&designer, "main", f, 2),
    );

    assert_eq!(
        srcs(&designer, "main", f, 0),
        vec![i1],
        "pin0 must STILL carry i1"
    );
    assert_eq!(
        srcs(&designer, "main", f, 1),
        vec![i2],
        "pin1 must STILL carry i2"
    );
    assert_eq!(
        srcs(&designer, "main", f, 2),
        Vec::<u64>::new(),
        "new pin must be EMPTY, not a clone of an existing pin's wire"
    );
}

// ###########################################################################
// POSITIVE GUARDS (currently GREEN) — paths that already hold. Keep them green.
// ###########################################################################

/// G1: instance inside an HOF body — add parameter in the middle.
#[test]
fn guard_hof_body_add_parameter_in_middle() {
    let mut designer = StructureDesigner::new();
    make_filter(&mut designer, "Filt", &[("first", 0), ("last", 2)]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    let i1 = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let i2 = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 80.0), None);
    let f = designer.add_node_scoped(&[map_id], "Filt", DVec2::new(150.0, 0.0), None);
    designer.connect_nodes_scoped(&[map_id], i1, 0, f, 0);
    designer.connect_nodes_scoped(&[map_id], i2, 0, f, 1);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let mid = designer.add_node("parameter", DVec2::new(0.0, 30.0));
    set_parameter_props(&mut designer, "Filt", mid, "middle", DataType::Int, 1);

    assert_eq!(
        body_srcs(&mut designer, "main", &[map_id], f, 0),
        vec![i1],
        "pin0<-i1"
    );
    assert_eq!(
        body_srcs(&mut designer, "main", &[map_id], f, 1),
        Vec::<u64>::new(),
        "new middle pin empty"
    );
    assert_eq!(
        body_srcs(&mut designer, "main", &[map_id], f, 2),
        vec![i2],
        "pin2<-i2"
    );
}

/// G2: instance inside an HOF body — reorder parameters (swap).
#[test]
fn guard_hof_body_reorder_parameters() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("a", 0), ("b", 1)]);
    let (pa, pb) = (ids[0], ids[1]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let map_id = designer.add_node("map", DVec2::new(0.0, 0.0));
    let i1 = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 0.0), None);
    let i2 = designer.add_node_scoped(&[map_id], "int", DVec2::new(0.0, 80.0), None);
    let f = designer.add_node_scoped(&[map_id], "Filt", DVec2::new(150.0, 0.0), None);
    designer.connect_nodes_scoped(&[map_id], i1, 0, f, 0);
    designer.connect_nodes_scoped(&[map_id], i2, 0, f, 1);

    set_parameter_props(&mut designer, "Filt", pa, "a", DataType::Int, 1);
    set_parameter_props(&mut designer, "Filt", pb, "b", DataType::Int, 0);

    assert_eq!(
        body_srcs(&mut designer, "main", &[map_id], f, 0),
        vec![i2],
        "pin0 is 'b'<-i2"
    );
    assert_eq!(
        body_srcs(&mut designer, "main", &[map_id], f, 1),
        vec![i1],
        "pin1 is 'a'<-i1"
    );
}

/// G3: in-memory edit then save/load roundtrip preserves the (already-repaired) wires.
#[test]
fn guard_save_load_roundtrip_preserves_wires() {
    let mut designer = StructureDesigner::new();
    make_filter(&mut designer, "Filt", &[("first", 0), ("last", 2)]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let mid = designer.add_node("parameter", DVec2::new(0.0, 30.0));
    set_parameter_props(&mut designer, "Filt", mid, "middle", DataType::Int, 1);

    let path = temp_path("pws_g3_roundtrip.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();
    let mut loaded = StructureDesigner::new();
    loaded.load_node_networks(path.to_str().unwrap()).unwrap();

    assert_eq!(
        arg_count(&loaded, "main", f),
        3,
        "loaded instance has 3 pins"
    );
    assert_eq!(srcs(&loaded, "main", f, 0), vec![i1], "loaded pin0<-i1");
    assert_eq!(
        srcs(&loaded, "main", f, 1),
        Vec::<u64>::new(),
        "loaded middle empty"
    );
    assert_eq!(srcs(&loaded, "main", f, 2), vec![i2], "loaded pin2<-i2");
}

/// G4: editing the ORIGINAL after duplicating it repairs instances of the original.
#[test]
fn guard_duplicate_then_edit_original() {
    let mut designer = StructureDesigner::new();
    make_filter(&mut designer, "Filt", &[("first", 0), ("last", 2)]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);

    let _copy = designer.duplicate_node_network("Filt").unwrap();

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let mid = designer.add_node("parameter", DVec2::new(0.0, 30.0));
    set_parameter_props(&mut designer, "Filt", mid, "middle", DataType::Int, 1);

    assert_eq!(srcs(&designer, "main", f, 0), vec![i1], "pin0<-i1");
    assert_eq!(
        srcs(&designer, "main", f, 1),
        Vec::<u64>::new(),
        "middle empty"
    );
    assert_eq!(srcs(&designer, "main", f, 2), vec![i2], "pin2<-i2");
}

/// G5: add a parameter, undo, redo — wires survive.
#[test]
fn guard_undo_redo_add_parameter() {
    let mut designer = StructureDesigner::new();
    make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let p3 = designer.add_node("parameter", DVec2::new(0.0, 120.0));
    set_parameter_props(&mut designer, "Filt", p3, "third", DataType::Int, 2);

    designer.undo();
    designer.redo();

    assert_eq!(
        srcs(&designer, "main", f, 0),
        vec![i1],
        "pin0<-i1 after undo/redo"
    );
    assert_eq!(
        srcs(&designer, "main", f, 1),
        vec![i2],
        "pin1<-i2 after undo/redo"
    );
}

/// G6: realistic add-at-end then drag-to-middle (two-step), top level.
#[test]
fn guard_add_at_end_then_reorder_to_middle() {
    let mut designer = StructureDesigner::new();
    make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let p3 = designer.add_node("parameter", DVec2::new(0.0, 120.0));
    set_parameter_props(&mut designer, "Filt", p3, "mid", DataType::Int, 2);
    assert_eq!(srcs(&designer, "main", f, 0), vec![i1], "step1 pin0<-i1");
    assert_eq!(srcs(&designer, "main", f, 1), vec![i2], "step1 pin1<-i2");

    set_parameter_props(&mut designer, "Filt", p3, "mid", DataType::Int, 1);
    let last_pid = designer
        .node_type_registry
        .node_networks
        .get("Filt")
        .unwrap()
        .nodes
        .iter()
        .find_map(|(id, n)| {
            n.data
                .as_any_ref()
                .downcast_ref::<ParameterData>()
                .filter(|p| p.param_name == "last")
                .map(|_| *id)
        })
        .unwrap();
    set_parameter_props(&mut designer, "Filt", last_pid, "last", DataType::Int, 2);

    assert_eq!(
        srcs(&designer, "main", f, 0),
        vec![i1],
        "final pin0 (first)<-i1"
    );
    assert_eq!(
        srcs(&designer, "main", f, 1),
        Vec::<u64>::new(),
        "final pin1 (mid) empty"
    );
    assert_eq!(
        srcs(&designer, "main", f, 2),
        vec![i2],
        "final pin2 (last)<-i2"
    );
}
