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
//!
//! ## F6 — healing already-saved corrupted files
//!
//! The two `f6_*` tests cover the load-time de-duplication of `param_id`s left in
//! files saved by the buggy build (`dedupe_param_ids_in_network`): the core pass
//! (reassign duplicates, keep first, idempotent) and the end-to-end heal (a project
//! saved with duplicate ids loads healed, reports the repair, and a subsequent
//! parameter add no longer re-jumbles).

use atomcad_structure_designer::data_type::DataType;
use atomcad_structure_designer::invariants::{InvariantKind, check_network_invariants};
use atomcad_structure_designer::network_validator::dedupe_param_ids_in_network;
use atomcad_structure_designer::node_data::NodeData;
use atomcad_structure_designer::nodes::parameter::ParameterData;
use atomcad_structure_designer::serialization::node_networks_serialization::save_node_networks_to_file;
use atomcad_structure_designer::structure_designer::StructureDesigner;
use atomcad_structure_designer::text_format::TextValue;
use glam::f64::DVec2;
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
// REGRESSION TESTS (green since F1) — reproduce the bug; must stay green.
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
// F6 — healing already-saved files with duplicate param_ids (Damage A).
// ###########################################################################

/// Force a parameter node's `param_id` directly (simulates a file saved by the
/// buggy build, where two params ended up sharing an id).
fn force_param_id(designer: &mut StructureDesigner, network: &str, node_id: u64, id: u64) {
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut(network)
        .unwrap();
    let node = net.nodes.get_mut(&node_id).unwrap();
    if let Some(p) = node.data.as_any_mut().downcast_mut::<ParameterData>() {
        p.param_id = Some(id);
    }
}

fn param_id_of(designer: &StructureDesigner, network: &str, node_id: u64) -> Option<u64> {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    let node = net.nodes.get(&node_id).unwrap();
    node.data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .and_then(|p| p.param_id)
}

/// F6 core: `dedupe_param_ids_in_network` reassigns duplicates, keeps the first
/// occurrence, and is idempotent.
#[test]
fn f6_dedupe_reassigns_duplicate_param_ids() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);
    let (pa, pb) = (ids[0], ids[1]);

    // Collide: both parameter nodes share id 1.
    force_param_id(&mut designer, "Filt", pa, 1);
    force_param_id(&mut designer, "Filt", pb, 1);

    let net = designer
        .node_type_registry
        .node_networks
        .get_mut("Filt")
        .unwrap();
    let fixes = dedupe_param_ids_in_network(net);
    assert_eq!(fixes.len(), 1, "exactly one duplicate should be reassigned");

    let a = param_id_of(&designer, "Filt", pa);
    let b = param_id_of(&designer, "Filt", pb);
    assert!(
        a.is_some() && b.is_some() && a != b,
        "param ids must be distinct after dedupe, got {a:?} {b:?}"
    );

    // Idempotent: a second pass is a no-op.
    let net = designer
        .node_type_registry
        .node_networks
        .get_mut("Filt")
        .unwrap();
    assert!(
        dedupe_param_ids_in_network(net).is_empty(),
        "dedupe must be idempotent on already-unique ids"
    );
}

/// F6 end-to-end: a project saved with duplicate param_ids loads healed, reports
/// the repair, keeps its existing wires, and a subsequent parameter add no longer
/// re-jumbles (which it WOULD without the heal).
#[test]
fn f6_load_heals_duplicates_and_prevents_rejumble() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);
    let (pa, pb) = (ids[0], ids[1]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);

    // Simulate the buggy save: two params share id 1.
    force_param_id(&mut designer, "Filt", pa, 1);
    force_param_id(&mut designer, "Filt", pb, 1);

    let path = temp_path("pws_f6_heal.cnnd");
    save_node_networks_to_file(
        &mut designer.node_type_registry,
        &path,
        false,
        &HashMap::new(),
    )
    .unwrap();

    let mut loaded = StructureDesigner::new();
    loaded.load_node_networks(path.to_str().unwrap()).unwrap();

    // The heal happened and was reported (and drains).
    let repairs = loaded.take_load_param_id_repairs();
    assert_eq!(
        repairs.len(),
        1,
        "one repair message expected, got {repairs:?}"
    );
    assert!(
        loaded.take_load_param_id_repairs().is_empty(),
        "repair report should drain on read"
    );

    // Ids are now distinct.
    let a = param_id_of(&loaded, "Filt", pa);
    let b = param_id_of(&loaded, "Filt", pb);
    assert!(
        a.is_some() && b.is_some() && a != b,
        "loaded param ids must be distinct after heal, got {a:?} {b:?}"
    );

    // Existing wires intact (the heal moves no connection).
    assert_eq!(srcs(&loaded, "main", f, 0), vec![i1], "post-load pin0<-i1");
    assert_eq!(srcs(&loaded, "main", f, 1), vec![i2], "post-load pin1<-i2");

    // The payoff: a subsequent parameter add does NOT re-jumble.
    loaded.set_active_node_network_name(Some("Filt".to_string()));
    let p3 = loaded.add_node("parameter", DVec2::new(0.0, 120.0));
    set_parameter_props(&mut loaded, "Filt", p3, "third", DataType::Int, 2);

    assert_eq!(
        srcs(&loaded, "main", f, 0),
        vec![i1],
        "after-heal add: pin0<-i1"
    );
    assert_eq!(
        srcs(&loaded, "main", f, 1),
        vec![i2],
        "after-heal add: pin1<-i2"
    );
    assert_eq!(
        srcs(&loaded, "main", f, 2),
        Vec::<u64>::new(),
        "after-heal add: new pin empty (no clone)"
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

// ###########################################################################
// #96 — duplicating / pasting a parameter node must mint a fresh identity.
// ###########################################################################
//
// `add_node` hands every new `parameter` node a fresh `param_id` from the
// network's `next_param_id` counter, but `NodeNetwork::duplicate_node` and
// `copy_nodes_from` clone the node data verbatim — including `param_id`. The
// result is the same "Damage A" state this file's F6 section heals on load,
// except reached by a plain in-session edit:
//
//  - `check_network_invariants` reports `DuplicateParamId` (fatal, so the debug
//    wrapper panics at the next `validate_network`), and
//  - in release, `repair_call_sites_for_network` builds `param_id -> old_index`
//    as a `HashMap`, so the two colliding params collapse onto one entry and the
//    duplicate silently inherits its twin's wire at every call site.
//
// `sort_order` is deliberately NOT touched by the fix: `compare_parameters`
// tiebreaks equal sort orders on `node_id`, so the copy lands directly after its
// original — which is where a user duplicating a parameter expects it.

/// Current `param_name` of a parameter node.
fn param_name_of(designer: &StructureDesigner, network: &str, node_id: u64) -> String {
    designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap()
        .nodes
        .get(&node_id)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
        .param_name
        .clone()
}

/// Every `DuplicateParamId` violation currently present in `network`.
fn duplicate_param_id_violations(designer: &StructureDesigner, network: &str) -> Vec<String> {
    let net = designer
        .node_type_registry
        .node_networks
        .get(network)
        .unwrap();
    check_network_invariants(net, &designer.node_type_registry)
        .into_iter()
        .filter(|v| v.kind == InvariantKind::DuplicateParamId)
        .map(|v| v.detail)
        .collect()
}

/// D1: the duplicate gets its own `param_id`, so the per-network uniqueness
/// invariant (B3) still holds. Checked before any validation runs, so a failure
/// surfaces as the assertion rather than the debug invariant panic.
#[test]
fn regression_duplicate_param_node_mints_fresh_param_id() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let dup = designer.duplicate_node(ids[0]);
    assert_ne!(dup, 0, "duplicate_node should succeed");

    let original_id = param_id_of(&designer, "Filt", ids[0]);
    let dup_id = param_id_of(&designer, "Filt", dup);
    assert!(dup_id.is_some(), "duplicate must carry a param_id");
    assert_ne!(
        original_id, dup_id,
        "duplicated parameter must get a fresh param_id, not a clone of the original's"
    );
    assert_eq!(
        duplicate_param_id_violations(&designer, "Filt"),
        Vec::<String>::new(),
        "duplicating a parameter must not violate the param_id uniqueness invariant"
    );
}

/// D2: the headline damage — with a cloned `param_id` the new pin steals a
/// preceding pin's wire at every call site of the network.
#[test]
fn regression_duplicate_param_node_does_not_steal_call_site_wire() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.add_node_network("main");
    designer.set_active_node_network_name(Some("main".to_string()));
    let i1 = designer.add_node("int", DVec2::new(0.0, 0.0));
    let i2 = designer.add_node("int", DVec2::new(0.0, 80.0));
    let f = designer.add_node("Filt", DVec2::new(150.0, 0.0));
    designer.connect_nodes(i1, 0, f, 0);
    designer.connect_nodes(i2, 0, f, 1);
    assert_eq!(srcs(&designer, "main", f, 0), vec![i1], "setup pin0<-i1");
    assert_eq!(srcs(&designer, "main", f, 1), vec![i2], "setup pin1<-i2");

    // The user duplicates the `first` parameter node and renames the copy,
    // intending a third, initially unconnected pin.
    designer.set_active_node_network_name(Some("Filt".to_string()));
    let dup = designer.duplicate_node(ids[0]);
    set_parameter_props(&mut designer, "Filt", dup, "third", DataType::Int, 2);
    designer.validate_active_network();

    assert_eq!(
        arg_count(&designer, "main", f),
        3,
        "instance grew to 3 pins"
    );
    assert_eq!(srcs(&designer, "main", f, 0), vec![i1], "pin0 (first)<-i1");
    assert_eq!(srcs(&designer, "main", f, 1), vec![i2], "pin1 (last)<-i2");
    assert_eq!(
        srcs(&designer, "main", f, 2),
        Vec::<u64>::new(),
        "pin2 (third) must be unconnected, not a clone of pin0's wire"
    );
}

/// D3: the duplicate also gets a non-colliding `param_name`, so the network does
/// not land in a blocking "Duplicate parameter name" error state.
#[test]
fn regression_duplicate_param_node_gets_unique_name() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("radius", 0), ("last", 1)]);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let dup = designer.duplicate_node(ids[0]);
    designer.validate_active_network();

    assert_ne!(
        param_name_of(&designer, "Filt", ids[0]),
        param_name_of(&designer, "Filt", dup),
        "duplicated parameter must get a unique name"
    );
    let name_errors: Vec<String> = designer
        .node_type_registry
        .node_networks
        .get("Filt")
        .unwrap()
        .validation_errors
        .iter()
        .filter(|e| e.error_text.contains("Duplicate parameter name"))
        .map(|e| e.error_text.clone())
        .collect();
    assert_eq!(
        name_errors,
        Vec::<String>::new(),
        "duplicating a parameter must not leave the network in a duplicate-name error state"
    );
}

/// D4: copy/paste within the same network has the same identity hole as
/// duplicate — `copy_nodes_from` clones the node data verbatim too.
#[test]
fn regression_paste_param_node_mints_fresh_param_id() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    designer.select_nodes(vec![ids[0]]);
    assert!(designer.copy_selection(), "copy should succeed");
    let pasted = designer.paste_at_position(DVec2::new(0.0, 300.0));
    assert_eq!(pasted.len(), 1, "one node pasted");

    assert_ne!(
        param_id_of(&designer, "Filt", ids[0]),
        param_id_of(&designer, "Filt", pasted[0]),
        "pasted parameter must get a fresh param_id"
    );
    assert_eq!(
        duplicate_param_id_violations(&designer, "Filt"),
        Vec::<String>::new(),
        "pasting a parameter must not violate the param_id uniqueness invariant"
    );
}

/// D5 (guard): pasting a parameter into a DIFFERENT network keeps its name — the
/// rename is a collision remedy, not an unconditional renumbering. The `param_id`
/// still comes from the target network's counter, since ids are per-network.
#[test]
fn guard_paste_param_into_other_network_keeps_name() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Src", &[("radius", 0)]);
    make_filter(&mut designer, "Dst", &[("height", 0)]);

    designer.set_active_node_network_name(Some("Src".to_string()));
    designer.select_nodes(vec![ids[0]]);
    assert!(designer.copy_selection(), "copy should succeed");
    designer.set_active_node_network_name(Some("Dst".to_string()));
    let pasted = designer.paste_at_position(DVec2::new(0.0, 200.0));
    assert_eq!(pasted.len(), 1, "one node pasted");

    assert_eq!(
        param_name_of(&designer, "Dst", pasted[0]),
        "radius",
        "no name collision in the target network, so the name is preserved"
    );
    assert_eq!(
        duplicate_param_id_violations(&designer, "Dst"),
        Vec::<String>::new(),
        "pasted parameter must hold a param_id unique within the TARGET network"
    );
}

/// D6: minting a fresh id advances `next_param_id`, so undo has to restore it —
/// otherwise the counter drifts and (per this file's header) a later parameter
/// add can collide all over again.
#[test]
fn guard_duplicate_param_node_undo_restores_next_param_id() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let before = designer
        .node_type_registry
        .node_networks
        .get("Filt")
        .unwrap()
        .next_param_id;

    let dup = designer.duplicate_node(ids[0]);
    designer.validate_active_network();
    let after_dup = designer
        .node_type_registry
        .node_networks
        .get("Filt")
        .unwrap()
        .next_param_id;
    assert!(
        after_dup > before,
        "duplicating a parameter consumes a param_id ({} -> {})",
        before,
        after_dup
    );

    assert!(designer.undo(), "undo should succeed");
    assert!(
        !designer
            .node_type_registry
            .node_networks
            .get("Filt")
            .unwrap()
            .nodes
            .contains_key(&dup),
        "undo removes the duplicate"
    );
    assert_eq!(
        designer
            .node_type_registry
            .node_networks
            .get("Filt")
            .unwrap()
            .next_param_id,
        before,
        "undo must restore next_param_id"
    );

    // Redo must re-establish the fresh identity, not resurrect the collision.
    assert!(designer.redo(), "redo should succeed");
    designer.validate_active_network();
    assert_eq!(
        duplicate_param_id_violations(&designer, "Filt"),
        Vec::<String>::new(),
        "redo must restore the duplicate's fresh param_id"
    );
}

/// D7 (guard): `sort_order` is intentionally left alone. Equal sort orders are
/// tiebroken by `node_id`, so the copy lands immediately after its original — the
/// placement a user duplicating a parameter expects. This pins the decision taken
/// on issue #96 against a future "pick the lowest unoccupied sort order".
#[test]
fn guard_duplicate_param_node_keeps_sort_order_and_lands_after_original() {
    let mut designer = StructureDesigner::new();
    let ids = make_filter(&mut designer, "Filt", &[("first", 0), ("last", 1)]);

    designer.set_active_node_network_name(Some("Filt".to_string()));
    let dup = designer.duplicate_node(ids[0]);
    designer.validate_active_network();

    let net = designer
        .node_type_registry
        .node_networks
        .get("Filt")
        .unwrap();
    let dup_sort = net
        .nodes
        .get(&dup)
        .unwrap()
        .data
        .as_any_ref()
        .downcast_ref::<ParameterData>()
        .unwrap()
        .sort_order;
    assert_eq!(dup_sort, 0, "sort_order is cloned unchanged");

    let names: Vec<String> = net
        .node_type
        .parameters
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names.len(), 3, "three parameters");
    assert_eq!(names[0], "first", "original stays first (lower node id)");
    assert_eq!(
        names[2], "last",
        "the copy sorts before `last`, not after it"
    );
}
